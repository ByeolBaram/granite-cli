use crate::registry::{self, Registry, ModelType};
use anyhow::Result;
use dialoguer::{Confirm, Input};

pub struct ModelCommands;

impl ModelCommands {
    pub fn list(filter_type: Option<ModelType>) -> Result<()> {
        let registry = &*registry::MODEL_REGISTRY;
        let models = registry.list();

        let filtered: Vec<_> = match filter_type {
            Some(ref t) => models.into_iter().filter(|m| m.model_type == *t).collect(),
            None => models.into_iter().collect(),
        };

        if filtered.is_empty() {
            println!("No models found{}.", filter_type.as_ref().map(|t| format!(" matching type: {}", t)).unwrap_or_default());
            return Ok(());
        }

        println!();
        println!("{:<35} {:<18} {:<8} {:<12} {:<12}", "ID", "FAMILY", "SIZE", "CONTEXT", "TYPE");
        println!("{:<35} {:<18} {:<8} {:<12} {:<12}", "----", "------", "----", "-------", "----");

        for model in &filtered {
            println!(
                "{:<35} {:<18} {:<8} {:<12} {:<12}",
                model.id,
                model.family,
                format!("{}B", model.size / 1_000_000_000),
                format!("{}", model.context_length),
                model.model_type.to_string(),
            );
        }

        println!();
        println!("Total: {} models", filtered.len());
        Ok(())
    }

    pub fn info(model_id: &str) -> Result<()> {
        let registry = &*registry::MODEL_REGISTRY;

        match registry.get(model_id) {
            Some(model) => {
                println!();
                println!("Model: {}", model.id);
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
                        variant.huggingface_path
                    );
                }

                println!("\nRequired Provider Capabilities:");
                for cap in &model.required_provider_capabilities {
                    println!("  - {}", cap);
                }

                Ok(())
            }
            None => {
                eprintln!("Error: Model '{}' not found in registry.", model_id);
                println!("\nAvailable models:");
                for model in registry.list() {
                    println!("  - {}", model.id);
                }
                anyhow::bail!("Model not found");
            }
        }
    }

    pub fn setup(model_id: &str) -> Result<()> {
        let registry = &*registry::MODEL_REGISTRY;

        match registry.get(model_id) {
            Some(model) => {
                println!("\nSetting up model: {}", model.id);
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

                let overwrite = if let Ok(Some(existing)) = crate::config::Config::load_model(model_id) {
                    if existing.enabled {
                        Confirm::new()
                            .with_prompt(&format!("Model '{}' is already configured. Overwrite?", model_id))
                            .default(false)
                            .interact()?
                    } else {
                        true
                    }
                } else {
                    true
                };

                if !overwrite {
                    println!("Model setup skipped.");
                    return Ok(());
                }

                let model_config = crate::config::ModelConfig {
                    model_id: model.id.clone(),
                    provider_id: if provider_id.is_empty() { None } else { Some(provider_id) },
                    variant: Some(format!("{}/{}", selected_variant.format, selected_variant.precision)),
                    endpoint: None,
                    api_key: None,
                    enabled: true,
                };

                let config = crate::config::Config::load()?;
                config.save_model(model_id, &model_config)?;

                println!("\nModel '{}' configured successfully!", model.id);
                println!("Note: Provider selection will be completed in the next phase.");

                Ok(())
            }
            None => {
                eprintln!("Error: Model '{}' not found in registry.", model_id);
                println!("\nAvailable models:");
                for model in registry.list() {
                    println!("  - {}", model.id);
                }
                anyhow::bail!("Model not found");
            }
        }
    }
}
