// Third Party
use anyhow::Result;
use dialoguer::{Confirm, Input};

// Local
use crate::models::{MODEL_REGISTRY, ModelType};

pub struct ModelCommands;

impl ModelCommands {
    pub fn list(ctx: &crate::AppContext, filter_type: Option<ModelType>) -> Result<()> {
        let models = MODEL_REGISTRY.entries();

        let filtered: std::collections::HashMap<_, _> = match filter_type {
            Some(ref t) => models.into_iter().filter(|(_, m)| m.model_type == *t).collect(),
            None => models.into_iter().collect(),
        };

        if filtered.is_empty() {
            println!("No models found{}.", filter_type.as_ref().map(|t| format!(" matching type: {}", t)).unwrap_or_default());
            return Ok(());
        }

        println!();
        println!("{:<35} {:<18} {:<8} {:<12} {:<12} {}", "ID", "FAMILY", "SIZE", "CONTEXT", "TYPE", "STATUS");
        println!("{:<35} {:<18} {:<8} {:<12} {:<12} {}", "----", "------", "----", "-------", "----", "------");

        for (model_id, model) in &filtered {
            let status = if ctx.config.models.contains_key(*model_id) {
                "CONFIGURED"
            } else {
                "BUNDLED"
            };
            println!(
                "{:<35} {:<18} {:<8} {:<12} {:<12} {}",
                model_id,
                model.family,
                format!("{}B", model.size / 1_000_000_000),
                format!("{}", model.context_length),
                model.model_type.to_string(),
                status,
            );
        }

        // Show configured models not in the registry
        let mut extra_configured = Vec::new();
        for id in ctx.config.models.keys() {
            if MODEL_REGISTRY.get(id).is_none() {
                extra_configured.push(id.clone());
            }
        }
        extra_configured.sort();

        if !extra_configured.is_empty() {
            println!();
            println!("Additional configured models:");
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
        println!("Total: {} models{}", filtered.len(), extra_suffix);
        Ok(())
    }

    pub fn info(ctx: &crate::AppContext, model_id: &str) -> Result<()> {
        match MODEL_REGISTRY.get(model_id) {
            Some(model) => {
                println!();
                println!("Model: {}", model_id);
                println!("Family: {}", model.family);
                println!("Version: {}", model.version);
                println!("Size: {}B parameters ({:.2}B)", model.size, model.size as f64 / 1_000_000_000.0);
                println!("Context Length: {} tokens", model.context_length);
                println!("Type: {}", model.model_type);
                println!("Hugging Face: {}", model.huggingface_repo);

                if let Some(desc) = &model.description {
                    println!("\nDescription:");
                    println!("  {}", desc);
                }

                if !model.tags.is_empty() {
                    println!("\nTags: {}", model.tags.join(", "));
                }

                println!("\nAvailable Variants:");
                for variant in &model.variants {
                    println!(
                        "  - {} / {} ({:.1} GB) -> {}",
                        variant.format,
                        variant.precision,
                        variant.size_gb,
                        variant.url
                    );
                }

                println!("\nSupported Functions:");
                for func in &model.supported_functions {
                    println!("  - {}", func);
                }

                // Show configuration state
                if let Some(configured) = ctx.config.get_model(model_id) {
                    println!("\nConfiguration:");
                    println!("  Provider: {:?}", configured.provider_id);
                    println!("  Variant: {:?}", configured.variant);
                    println!("  Enabled: {}", configured.enabled);
                    println!("  API Key: {}", masked(configured.api_key.as_deref()));
                }

                Ok(())
            }
            None => {
                eprintln!("Error: Model '{}' not found in registry.", model_id);
                println!("\nAvailable models:");
                for (model_reg_id, _) in MODEL_REGISTRY.entries() {
                    println!("  - {}", model_reg_id);
                }
                anyhow::bail!("Model not found");
            }
        }
    }

    pub fn setup(ctx: &mut crate::AppContext, model_id: &str) -> Result<()> {
        match MODEL_REGISTRY.get(model_id) {
            Some(model) => {
                println!("\nSetting up model: {}", model_id);
                println!("{}", model.description.as_deref().unwrap_or("No description available."));
                println!();
                println!("Size: {}B params, {} context", model.size / 1_000_000_000, model.context_length);
                println!("Type: {}", model.model_type);
                println!();

                let provider_id = Input::new()
                    .with_prompt("Provider ID (leave empty for now, will be configured later)")
                    .default(String::new())
                    .interact_text()?;

                let variant_options: Vec<_> = model.variants.iter()
                    .map(|v| format!("{} / {} ({:.1} GB)", v.format, v.precision, v.size_gb))
                    .collect();

                let variant_index = dialoguer::Select::new()
                    .with_prompt("Select model variant:")
                    .items(&variant_options)
                    .default(0)
                    .interact()?;

                let selected_variant = &model.variants[variant_index];
                println!("\nSelected: {} / {}", selected_variant.format, selected_variant.precision);

                if let Some(existing) = ctx.config.get_model(model_id) {
                    if existing.enabled {
                        let overwrite = Confirm::new()
                            .with_prompt(&format!("Model '{}' is already configured. Overwrite?", model_id))
                            .default(false)
                            .interact()?;
                        if !overwrite {
                            println!("Model setup skipped.");
                            return Ok(());
                        }
                    }
                }

                let model_config = crate::config::ModelConfig {
                    model_id: model_id.to_string(),
                    provider_id: if provider_id.is_empty() { None } else { Some(provider_id) },
                    variant: Some(format!("{}/{}", selected_variant.format, selected_variant.precision)),
                    endpoint: None,
                    api_key: None,
                    enabled: true,
                };

                ctx.config.insert_model(model_id, model_config);

                println!("\nModel '{}' configured successfully!", model_id);
                println!("Configure a provider to complete setup:");
                println!("  granite-cli provider setup <provider-id>");
                println!("Then set the provider_id in models.yaml, or run:");
                println!("  granite-cli configure <tool-id>");

                Ok(())
            }
            None => {
                eprintln!("Error: Model '{}' not found in registry.", model_id);
                println!("\nAvailable models:");
                for (model_reg_id, _) in MODEL_REGISTRY.entries() {
                    println!("  - {}", model_reg_id);
                }
                anyhow::bail!("Model not found");
            }
        }
    }
}

fn masked(api_key: Option<&str>) -> String {
    match api_key {
        Some(key) if key.len() > 8 => format!("{}****{}", &key[..4], &key[key.len() - 4..]),
        Some(_) => "****".to_string(),
        None => "(not set)".to_string(),
    }
}
