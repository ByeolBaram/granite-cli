// Third Party
use anyhow::Result;
use dialoguer::{Confirm, Input};

// Local
use crate::commands::ProviderCommands;
use crate::dependency::{self, DependsOn, Requirement};
use crate::models::{MODEL_REGISTRY, ModelType};
use crate::providers::{Provider, ProviderMetadata, ProviderSource};

pub struct ModelCommands;

/*-- Model -> Provider dependency --------------------------------------------*/

/// What a model variant needs from a provider: support for its format and
/// precision. Concrete `Requirement`/`DependsOn` pairing for the abstract
/// dependency-resolution framework in `dependency::mod`.
#[derive(Clone)]
struct VariantRequirement {
    format: String,
    precision: String,
}

impl Requirement<dyn Provider> for VariantRequirement {
    fn admits_type(&self, metadata: &ProviderMetadata) -> bool {
        metadata
            .supported_formats
            .iter()
            .any(|f| f.to_string().eq_ignore_ascii_case(&self.format))
            && metadata
                .supported_precisions
                .iter()
                .any(|p| p.eq_ignore_ascii_case(&self.precision))
    }

    fn admits_instance(&self, instance: &dyn Provider) -> bool {
        instance.can_run_model(&self.format, &self.precision)
    }
}

impl DependsOn<dyn Provider> for VariantRequirement {
    type Requirement = Self;

    fn requirement(&self) -> Self {
        self.clone()
    }
}

impl ModelCommands {
    pub fn catalog(ctx: &crate::AppContext, filter_type: Option<ModelType>) -> Result<()> {
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
        println!("{:<35} {:<18} {:<8} {:<12} {:<12}", "ID", "FAMILY", "SIZE", "CONTEXT", "TYPE");
        println!("{:<35} {:<18} {:<8} {:<12} {:<12}", "----", "------", "----", "-------", "----");

        for (model_id, model) in &filtered {
            println!(
                "{:<35} {:<18} {:<8} {:<12} {:<12}",
                model_id,
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

    pub fn list(ctx: &crate::AppContext, filter_type: Option<ModelType>) -> Result<()> {

        println!();
        println!("{:<35} {:<18} {:<8} {:<12} {:<12} {}", "ID", "FAMILY", "SIZE", "CONTEXT", "TYPE", "PROVIDER");
        println!("{:<35} {:<18} {:<8} {:<12} {:<12} {}", "----", "------", "----", "-------", "----", "--------");

        let mut valid = 0;
        for (model_id, model_config) in ctx.config.models.clone() {
            if let Some(model_md) = MODEL_REGISTRY.get(&model_id) {
                if let Some(ref t) = filter_type {
                    if model_md.model_type != *t {
                        continue;
                    }
                }
                valid += 1;
                println!(
                    "{:<35} {:<18} {:<8} {:<12} {:<12} {}",
                    model_id,
                    model_md.family,
                    format!("{}B", model_md.size / 1_000_000_000),
                    format!("{}", model_md.context_length),
                    model_md.model_type.to_string(),
                    match model_config.provider_id {
                        Some(p_id) => p_id,
                        None => "None".to_string(),
                    },
                );
            }
        }

        println!();
        println!("Total: {} models", valid);
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

    pub async fn setup(ctx: &mut crate::AppContext, model_id: &str) -> Result<()> {
        match MODEL_REGISTRY.get(model_id) {
            Some(model) => {
                println!("\nSetting up model: {}", model_id);
                println!("{}", model.description.as_deref().unwrap_or("No description available."));
                println!();
                println!("Size: {}B params, {} context", model.size / 1_000_000_000, model.context_length);
                println!("Type: {}", model.model_type);
                println!();

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

                let requirement = VariantRequirement {
                    format: selected_variant.format.clone(),
                    precision: selected_variant.precision.clone(),
                };
                let source = ProviderSource::from_config(&ctx.config);
                let resolution = dependency::resolve(&requirement, &source);

                let provider_id = Self::select_provider(ctx, &resolution).await?;

                let model_config = crate::config::ModelConfig {
                    model_id: model_id.to_string(),
                    provider_id,
                    variant: Some(format!("{}/{}", selected_variant.format, selected_variant.precision)),
                    enabled: true,
                };

                ctx.config.insert_model(model_id, model_config);

                println!("\nModel '{}' configured successfully!", model_id);

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

    /// Resolve which provider instance to use for a model variant, prompting
    /// to configure a new one (with its own instance nickname, distinct from
    /// its catalog type) when no existing instance satisfies it.
    async fn select_provider(
        ctx: &mut crate::AppContext,
        resolution: &dependency::Resolution,
    ) -> Result<Option<String>> {
        if resolution.is_unsatisfiable() {
            println!("\nNo provider supports this variant's format/precision yet.");
            println!("Configure a provider later, then set its id on this model.");
            return Ok(None);
        }

        const CONFIGURE_NEW: &str = "Configure a new provider...";
        let mut options = resolution.existing_instances.clone();
        if !resolution.configurable_types.is_empty() {
            options.push(CONFIGURE_NEW.to_string());
        }

        let choice = if options.len() == 1 {
            0
        } else {
            dialoguer::Select::new()
                .with_prompt("Select a provider for this model")
                .items(&options)
                .default(0)
                .interact()?
        };

        if options[choice] != CONFIGURE_NEW {
            return Ok(Some(options[choice].clone()));
        }

        let provider_type = if resolution.configurable_types.len() == 1 {
            resolution.configurable_types[0]
        } else {
            let type_index = dialoguer::Select::new()
                .with_prompt("Select a provider type to configure")
                .items(&resolution.configurable_types)
                .default(0)
                .interact()?;
            resolution.configurable_types[type_index]
        };

        let nickname: String = Input::new()
            .with_prompt("Name this provider instance")
            .with_initial_text(provider_type)
            .interact_text()?;

        ProviderCommands::setup(ctx, provider_type, Some(&nickname)).await?;

        Ok(Some(nickname))
    }
}

/*-- tests -----------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ModelFormat, ProviderType};
    use crate::registry::ConfigConstructable;

    fn metadata_supporting(formats: Vec<ModelFormat>, precisions: Vec<&str>) -> ProviderMetadata {
        ProviderMetadata {
            name: "Test Provider".to_string(),
            description: "".to_string(),
            provider_type: ProviderType::Local,
            default_endpoint: "http://localhost".to_string(),
            supported_api_types: vec![],
            default_function_endpoints: std::collections::HashMap::new(),
            supported_formats: formats,
            supported_precisions: precisions.into_iter().map(String::from).collect(),
            authentication: vec![],
            tags: vec![],
        }
    }

    #[test]
    fn admits_type_matches_format_and_precision_case_insensitively() {
        let requirement = VariantRequirement { format: "GGUF".to_string(), precision: "FP16".to_string() };
        let metadata = metadata_supporting(vec![ModelFormat::GGUF], vec!["fp16", "fp32"]);
        assert!(requirement.admits_type(&metadata));
    }

    #[test]
    fn admits_type_rejects_unsupported_format() {
        let requirement = VariantRequirement { format: "gguf".to_string(), precision: "fp16".to_string() };
        let metadata = metadata_supporting(vec![ModelFormat::Safetensors], vec!["fp16"]);
        assert!(!requirement.admits_type(&metadata));
    }

    #[test]
    fn admits_type_rejects_unsupported_precision() {
        let requirement = VariantRequirement { format: "gguf".to_string(), precision: "bfloat16".to_string() };
        let metadata = metadata_supporting(vec![ModelFormat::GGUF], vec!["fp16", "fp32"]);
        assert!(!requirement.admits_type(&metadata));
    }

    #[test]
    fn admits_instance_defers_to_the_provider_instance() {
        let requirement = VariantRequirement { format: "safetensors".to_string(), precision: "bfloat16".to_string() };
        let provider = crate::providers::OpenAIProvider::new(&serde_json::json!({ "base_url": "http://localhost:8080" }));
        assert!(requirement.admits_instance(&provider));
    }
}
