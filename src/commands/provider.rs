// Third Party
use anyhow::Result;
use dialoguer::Confirm;

// Local
use crate::providers::{PROVIDER_REGISTRY, HealthStatus};
use crate::utils::prompt_from_schema;

pub struct ProviderCommands;

impl ProviderCommands {
    pub fn catalog(_ctx: &crate::AppContext) -> Result<()> {
        let providers = PROVIDER_REGISTRY.entries();

        let filtered = providers.len();

        println!();
        println!("{:<20} {:<10} {:<35}", "ID", "TYPE", "ENDPOINT");
        println!("{:<20} {:<10} {:<35}", "----", "----", "--------");

        for (provider_id, provider) in &providers {
            println!(
                "{:<20} {:<10} {:<35}",
                provider_id,
                provider.provider_type,
                provider.default_endpoint,
            );
        }

        println!();
        println!("Total: {} providers", filtered);
        Ok(())
    }

    pub fn list(_ctx: &crate::AppContext) -> Result<()> {

        println!();
        println!("{:<20} {:<20} {:<10} {:<35}", "ID", "TYPE", "ENABLED", "BASE URL");
        println!("{:<20} {:<20} {:<10} {:<35}", "----", "----", "-------", "--------");

        let mut valid = 0;
        for (provider_id, provider_config) in _ctx.config.providers.clone() {
            valid += 1;
            let base_url = provider_config.config.get("base_url")
                .and_then(|v| v.as_str())
                .unwrap_or("-");
            println!(
                "{:<20} {:<20} {:<10} {:<35}",
                provider_id,
                provider_config.provider_type,
                provider_config.enabled,
                base_url,
            );
        }

        println!();
        println!("Total: {} providers", valid);
        Ok(())
    }

     /// Interactive provider setup wizard.
    ///
    /// `provider_type` is the catalog/registry key (e.g. `openai-compatible`).
    /// `instance_id` is this instance's nickname, distinct from its type --
    /// defaults to `provider_type` when not given, but a caller may pass a
    /// different value to configure multiple named instances of one type
    /// (e.g. `openai-compatible` backing `llama-cpp`, `ollama`, `lm-studio`).
    pub async fn setup(
        ctx: &mut crate::AppContext,
        provider_type: &str,
        instance_id: Option<&str>,
    ) -> Result<()> {
        let provider_def = match PROVIDER_REGISTRY.get(provider_type) {
            Some(def) => def,
            None => {
                eprintln!("Error: Provider type '{}' not found in registry.", provider_type);
                println!("\nAvailable provider types:");
                for (p_id, p) in PROVIDER_REGISTRY.entries() {
                    println!("  - {} ({})", p_id, p.name);
                }
                anyhow::bail!("Provider type not found");
            }
        };

        let instance_id = instance_id.unwrap_or(provider_type).to_string();

        println!("\nSetting up provider instance: {}", instance_id);
        println!("{}", provider_def.name);
        println!("{}", provider_def.description);
        println!();
        println!("Type: {}", provider_def.provider_type);

        if !provider_def.authentication.is_empty() {
            print!("Authentication: ");
            for (i, auth) in provider_def.authentication.iter().enumerate() {
                if i > 0 { print!(", "); }
                print!("{}", auth);
            }
            println!();
        }

        // Check if this instance is already configured
        let existing_config = ctx.config.get_provider(&instance_id);
        if existing_config.is_some() {
            let overwrite = Confirm::new()
                .with_prompt(&format!("Provider instance '{}' is already configured. Overwrite?", instance_id))
                .default(false)
                .interact()?;
            if !overwrite {
                println!("Provider setup skipped.");
                return Ok(());
            }
        }

        let schema = PROVIDER_REGISTRY.config_schema(provider_type)
            .ok_or_else(|| anyhow::anyhow!("No config schema registered for provider type '{}'", provider_type))?;
        let defaults = existing_config
            .map(|c| c.config.clone())
            .or_else(|| PROVIDER_REGISTRY.default_config(provider_type))
            .unwrap_or_else(|| serde_json::json!({}));

        println!();
        let config = prompt_from_schema(&schema, &defaults)?;

        let provider_config = crate::config::ProviderConfig {
            provider_id: instance_id.clone(),
            provider_type: provider_type.to_string(),
            config,
            enabled: true,
        };

        ctx.config.insert_provider(&instance_id, provider_config);

        // Health check
        println!("\nRunning health check...");
        match Self::check_provider_health(ctx, &instance_id).await {
            Ok(status) => {
                if status.healthy {
                    println!("Provider '{}' is healthy!", instance_id);
                } else {
                    println!("Warning: Provider '{}' health check failed. It may need to be started or configured differently.", instance_id);
                }
            }
            Err(e) => {
                println!("Warning: Could not run health check: {}", e);
            }
        }

        println!("\nProvider instance '{}' configured successfully!", instance_id);
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

        let provider = PROVIDER_REGISTRY
            .construct(&provider_config.provider_type, &provider_config.config)
            .map_err(|e| anyhow::anyhow!("Failed to create provider: {}", e))?;

        let status = provider.health_check().await?;

        Ok(status)
    }
}
