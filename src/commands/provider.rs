// Third Party
use anyhow::Result;
use dialoguer::{Confirm, Input};

// Local
use crate::providers::{PROVIDER_REGISTRY, AuthType, HealthStatus};

pub struct ProviderCommands;

impl ProviderCommands {
    /// List all providers from the static registry, indicating which are configured.
    pub fn list(ctx: &crate::AppContext) -> Result<()> {
        let providers = PROVIDER_REGISTRY.list();

        println!();
        println!("{:<20} {:<10} {:<35} {}", "ID", "TYPE", "ENDPOINT", "STATUS");
        println!("{:<20} {:<10} {:<35} {}", "----", "----", "--------", "------");

        for provider in &providers {
            let status = if ctx.config.providers.contains_key(&provider.id) {
                "CONFIGURED"
            } else {
                "BUNDLED"
            };

            println!(
                "{:<20} {:<10} {:<35} {}",
                provider.id,
                provider.provider_type,
                provider.default_endpoint,
                status,
            );
        }

        // Show configured providers not in the registry
        let mut extra_configured = Vec::<String>::new();
        for id in ctx.config.providers.keys() {
            if PROVIDER_REGISTRY.get(id).is_none() {
                extra_configured.push(id.clone());
            }
        }
        extra_configured.sort();

        if !extra_configured.is_empty() {
            println!();
            println!("Additional configured providers:");
            for id in &extra_configured {
                println!("  - {} (CONFIGURED)", id);
            }
        }

        println!();
        let extra_suffix = if extra_configured.is_empty() {
            String::new()
        } else {
            format!(", {} additional configured", extra_configured.len())
        };
        println!("Total: {} providers{}", providers.len(), extra_suffix);
        Ok(())
    }

     /// Interactive provider setup wizard.
    pub async fn setup(ctx: &mut crate::AppContext, provider_id: &str) -> Result<()> {
        let provider_def = match PROVIDER_REGISTRY.get(provider_id) {
            Some(def) => def,
            None => {
                eprintln!("Error: Provider '{}' not found in registry.", provider_id);
                println!("\nAvailable providers:");
                for p in PROVIDER_REGISTRY.list() {
                    println!("  - {} ({})", p.id, p.name);
                }
                anyhow::bail!("Provider not found");
            }
        };

        println!("\nSetting up provider: {}", provider_def.name);
        println!("{}", provider_def.description);
        println!();
        println!("Type: {}", provider_def.provider_type);
        println!("Default endpoint: {}", provider_def.default_endpoint);

        if !provider_def.authentication.is_empty() {
            print!("Authentication: ");
            for (i, auth) in provider_def.authentication.iter().enumerate() {
                if i > 0 { print!(", "); }
                print!("{}", auth);
            }
            println!();
        }

        // Prompt for endpoint
        let endpoint = Input::new()
            .with_prompt("Endpoint URL")
            .default(provider_def.default_endpoint.clone())
            .interact_text()?;

        // Prompt for API key if required
        let api_key = if provider_def.authentication.iter().any(|a| matches!(a, AuthType::ApiKey)) {
            let key: String = Input::new()
                .with_prompt("API Key (not shown)")
                .show_default(false)
                .interact_text()?;
            if key.is_empty() {
                println!("Warning: No API key provided. Provider may not work without one.");
                None
            } else {
                Some(key)
            }
        } else {
            None
        };

        // Check if already configured
        if let Some(_existing) = ctx.config.get_provider(provider_id) {
            let overwrite = Confirm::new()
                .with_prompt(&format!("Provider '{}' is already configured. Overwrite?", provider_id))
                .default(false)
                .interact()?;
            if !overwrite {
                println!("Provider setup skipped.");
                return Ok(());
            }
        }

        let provider_config = crate::config::ProviderConfig {
            provider_id: provider_id.to_string(),
            name: provider_def.name.clone(),
            provider_type: format!("{}", provider_def.provider_type),
            endpoint,
            api_key,
            enabled: true,
        };

        ctx.config.insert_provider(provider_id, provider_config);

        // Health check
        println!("\nRunning health check...");
        match Self::check_provider_health(ctx, provider_id).await {
            Ok(status) => {
                if status.healthy {
                    println!("Provider '{}' is healthy!", provider_id);
                } else {
                    println!("Warning: Provider '{}' health check failed. It may need to be started or configured differently.", provider_id);
                }
            }
            Err(e) => {
                println!("Warning: Could not run health check: {}", e);
            }
        }

        println!("\nProvider '{}' configured successfully!", provider_id);
        println!("Supported APIs:");
        for (func, endpoints) in &provider_def.default_function_endpoints {
            let endpoint_strs: Vec<String> = endpoints.iter()
                .map(|ep| format!("{} ({})", ep.api_type(), ep.path()))
                .collect();
            println!("  - {} -> {}", func, endpoint_strs.join(", "));
        }

        Ok(())
    }

    /// Check health of a provider or all configured providers.
    pub async fn health(ctx: &mut crate::AppContext, provider_id: Option<&str>) -> Result<()> {
        let providers_to_check: Vec<String> = match provider_id {
            Some(id) => vec![id.to_string()],
            None => ctx.config.providers.keys().cloned().collect(),
        };

        if providers_to_check.is_empty() {
            println!("No configured providers to check.");
            return Ok(());
        }

        println!();
        println!("{:<20} {:<12} {:<10} {}", "PROVIDER", "LATENCY", "HEALTHY", "ERROR");
        println!("{:<20} {:<12} {:<10} {}", "--------", "-------", "-------", "-----");

        for id in &providers_to_check {
            match Self::check_provider_health(ctx, id).await {
                Ok(status) => {
                    let latency_str = if status.latency.as_millis() < 1000 {
                        format!("{}ms", status.latency.as_millis())
                    } else {
                        format!("{:.2}s", status.latency.as_secs_f64())
                    };

                    println!(
                        "{:<20} {:<12} {:<10} {}",
                        id,
                        latency_str,
                        status.healthy,
                        status.error.as_deref().unwrap_or("-")
                    );
                }
                Err(e) => {
                    println!(
                        "{:<20} {:<12} {:<10} {}",
                        id,
                        "N/A",
                        false,
                        e.to_string()
                    );
                }
            }
        }

        println!();
        Ok(())
    }

    async fn check_provider_health(ctx: &crate::AppContext, provider_id: &str) -> Result<HealthStatus> {
        let provider_config = ctx.config.get_provider(provider_id)
            .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found in configuration", provider_id))?;

        let endpoint = &provider_config.endpoint;

        // Determine which provider implementation to use
        let factory_id = match provider_config.name.to_lowercase().as_str() {
            "ollama" => "ollama",
            "anthropic" => "anthropic",
            "openai" => "openai",
            "ibm watsonx.ai" => "watsonx",
            _ => {
                if provider_config.provider_type.to_lowercase() == "local" {
                    "ollama"
                } else {
                    "openai"
                }
            }
        };

        // Create a temporary provider config for health check
        let temp_config = crate::providers::ProviderMetadata {
            id: provider_id.to_string(),
            name: provider_config.name.clone(),
            description: String::new(),
            provider_type: if provider_config.provider_type.to_lowercase() == "local" {
                crate::providers::ProviderType::Local
            } else {
                crate::providers::ProviderType::Hosted
            },
            default_endpoint: endpoint.clone(),
            supported_api_types: vec![],
            default_function_endpoints: std::collections::HashMap::new(),
            supported_formats: vec![],
            supported_precisions: vec![],
            authentication: vec![],
            tags: vec![],
        };

        // TODO: This is probably wrong and at minimum inefficient!
        let provider = crate::providers::PROVIDER_REGISTRY
            .construct(factory_id, &serde_json::to_value(temp_config)?)
            .map_err(|e| anyhow::anyhow!("Failed to create provider: {}", e))?;

        let status = provider.health_check().await?;

        Ok(status)
    }
}
