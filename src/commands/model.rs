// Third Party
use anyhow::Result;

// Local
use crate::commands::ProviderCommands;
use crate::dependency::{self, DependsOn, Requirement};
use crate::models::{MODEL_REGISTRY, ModelType};
use crate::providers::{Provider, ProviderMetadata, ProviderSource};
use crate::utils::Searchable;

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
    pub fn search(ctx: &crate::AppContext, query: &str) -> Result<()> {
        let q = query.to_lowercase();
        let models = MODEL_REGISTRY.entries();

        let mut rows: Vec<Vec<String>> = models
            .iter()
            .filter(|(id, m)| {
                id.to_lowercase().contains(&q)
                    || m.search_fields().iter().any(|f| f.to_lowercase().contains(&q))
            })
            .map(|(id, m)| vec![
                id.to_string(),
                m.family.clone(),
                format!("{}B", m.size / 1_000_000_000),
                m.context_length.to_string(),
                m.model_type.to_string(),
            ])
            .collect();
        rows.sort_by(|a, b| a[0].cmp(&b[0]));

        if rows.is_empty() {
            ctx.ui.info(&format!("No models found matching '{}'.", query));
            return Ok(());
        }

        ctx.ui.table(
            &format!("Search results for '{}' ({} models)", query, rows.len()),
            &["ID", "FAMILY", "SIZE", "CONTEXT", "TYPE"],
            &rows,
        );
        Ok(())
    }

    pub fn catalog(ctx: &crate::AppContext, filter_type: Option<ModelType>) -> Result<()> {
        let models = MODEL_REGISTRY.entries();

        let filtered: std::collections::HashMap<_, _> = match filter_type {
            Some(ref t) => models.into_iter().filter(|(_, m)| m.model_type == *t).collect(),
            None => models.into_iter().collect(),
        };

        if filtered.is_empty() {
            ctx.ui.info(&format!(
                "No models found{}.",
                filter_type.as_ref().map(|t| format!(" matching type: {}", t)).unwrap_or_default()
            ));
            return Ok(());
        }

        let mut rows: Vec<Vec<String>> = filtered.iter().map(|(model_id, model)| {
            vec![
                model_id.to_string(),
                model.family.clone(),
                format!("{}B", model.size / 1_000_000_000),
                model.context_length.to_string(),
                model.model_type.to_string(),
            ]
        }).collect();
        rows.sort_by(|a, b| a[0].cmp(&b[0]));

        ctx.ui.table(
            &format!("Model Catalog ({} models)", filtered.len()),
            &["ID", "FAMILY", "SIZE", "CONTEXT", "TYPE"],
            &rows,
        );
        Ok(())
    }

    pub fn list(ctx: &crate::AppContext, filter_type: Option<ModelType>) -> Result<()> {
        let mut rows: Vec<Vec<String>> = Vec::new();

        for (model_id, model_config) in &ctx.config.models {
            if let Some(model_md) = MODEL_REGISTRY.get(model_id) {
                if let Some(ref t) = filter_type {
                    if model_md.model_type != *t {
                        continue;
                    }
                }
                rows.push(vec![
                    model_id.clone(),
                    model_md.family.clone(),
                    format!("{}B", model_md.size / 1_000_000_000),
                    model_md.context_length.to_string(),
                    model_md.model_type.to_string(),
                    model_config.provider_id.clone().unwrap_or_else(|| "None".to_string()),
                ]);
            }
        }
        rows.sort_by(|a, b| a[0].cmp(&b[0]));

        ctx.ui.table(
            &format!("Configured Models ({} models)", rows.len()),
            &["ID", "FAMILY", "SIZE", "CONTEXT", "TYPE", "PROVIDER"],
            &rows,
        );
        Ok(())
    }

    pub fn info(ctx: &crate::AppContext, model_id: &str) -> Result<()> {
        match MODEL_REGISTRY.get(model_id) {
            Some(model) => {
                let mut fields: Vec<(&str, String)> = vec![
                    ("Family",        model.family.clone()),
                    ("Version",       model.version.clone()),
                    ("Size",          format!("{}B parameters ({:.2}B)", model.size, model.size as f64 / 1_000_000_000.0)),
                    ("Context Length", format!("{} tokens", model.context_length)),
                    ("Type",          model.model_type.to_string()),
                    ("Hugging Face",  model.huggingface_repo.clone()),
                ];

                if let Some(desc) = &model.description {
                    fields.push(("Description", desc.clone()));
                }
                if !model.tags.is_empty() {
                    fields.push(("Tags", model.tags.join(", ")));
                }

                let variants_str = model.variants.iter()
                    .map(|v| format!("{} / {} ({:.1} GB)", v.format, v.precision, v.size_gb))
                    .collect::<Vec<_>>()
                    .join(", ");
                fields.push(("Variants", variants_str));

                let funcs_str = model.supported_functions.iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                fields.push(("Supported Functions", funcs_str));

                if let Some(configured) = ctx.config.get_model(model_id) {
                    fields.push(("Config: Provider", format!("{:?}", configured.provider_id)));
                    fields.push(("Config: Variant",  format!("{:?}", configured.variant)));
                    fields.push(("Config: Enabled",  configured.enabled.to_string()));
                }

                ctx.ui.detail(model_id, &fields);
                Ok(())
            }
            None => {
                ctx.ui.error(&format!("Model '{}' not found in registry.", model_id));
                let available: Vec<_> = MODEL_REGISTRY.entries().keys().map(|k| k.to_string()).collect();
                ctx.ui.info(&format!("Available models: {}", available.join(", ")));
                anyhow::bail!("Model not found");
            }
        }
    }

    pub async fn setup(ctx: &mut crate::AppContext, model_id: &str) -> Result<()> {
        match MODEL_REGISTRY.get(model_id) {
            Some(model) => {
                ctx.ui.info(&format!("\nSetting up model: {}", model_id));
                ctx.ui.info(&format!("{}", model.description.as_deref().unwrap_or("No description available.")));
                ctx.ui.info("");
                ctx.ui.info(&format!("Size: {}B params, {} context", model.size / 1_000_000_000, model.context_length));
                ctx.ui.info(&format!("Type: {}", model.model_type));
                ctx.ui.info("");

                let variant_options: Vec<_> = model.variants.iter()
                    .map(|v| format!("{} / {} ({:.1} GB)", v.format, v.precision, v.size_gb))
                    .collect();

                let variant_index = ctx.ui.select("Select model variant:", &variant_options, 0)?;

                let selected_variant = &model.variants[variant_index];
                ctx.ui.info(&format!("\nSelected: {} / {}", selected_variant.format, selected_variant.precision));

                if let Some(existing) = ctx.config.get_model(model_id) {
                    if existing.enabled {
                        let overwrite = ctx.ui.confirm(
                            &format!("Model '{}' is already configured. Overwrite?", model_id),
                            false,
                        )?;
                        if !overwrite {
                            ctx.ui.info("Model setup skipped.");
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

                ctx.ui.info(&format!("\nModel '{}' configured successfully!", model_id));

                Ok(())
            }
            None => {
                ctx.ui.error(&format!("Model '{}' not found in registry.", model_id));
                let available: Vec<_> = MODEL_REGISTRY.entries().keys().map(|k| k.to_string()).collect();
                ctx.ui.info(&format!("Available models: {}", available.join(", ")));
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
            ctx.ui.info("\nNo provider supports this variant's format/precision yet.");
            ctx.ui.info("Configure a provider later, then set its id on this model.");
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
            ctx.ui.select("Select a provider for this model", &options, 0)?
        };

        if options[choice] != CONFIGURE_NEW {
            return Ok(Some(options[choice].clone()));
        }

        let provider_type = if resolution.configurable_types.len() == 1 {
            resolution.configurable_types[0]
        } else {
            let type_options: Vec<String> = resolution.configurable_types.iter().map(|s| s.to_string()).collect();
            let type_index = ctx.ui.select("Select a provider type to configure", &type_options, 0)?;
            resolution.configurable_types[type_index]
        };

        let nickname = ctx.ui.text("Name this provider instance", provider_type)?;

        ProviderCommands::setup(ctx, provider_type, Some(&nickname)).await?;

        Ok(Some(nickname))
    }
}

/*-- tests -----------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ModelConfig};
    use crate::providers::{ModelFormat, ProviderType};
    use crate::registry::ConfigConstructable;
    use crate::utils::ui::base::tests::CaptureUi;

    fn empty_ctx() -> crate::AppContext {
        crate::AppContext {
            config: Config::default(),
            ui: Box::new(CaptureUi::default()),
        }
    }

    macro_rules! tables {
        ($ctx:expr) => {
            (&*($ctx.ui) as &dyn std::any::Any).downcast_ref::<CaptureUi>().unwrap().tables.borrow()
        };
    }

    macro_rules! details {
        ($ctx:expr) => {
            (&*($ctx.ui) as &dyn std::any::Any).downcast_ref::<CaptureUi>().unwrap().details.borrow()
        };
    }

    macro_rules! errors {
        ($ctx:expr) => {
            (&*($ctx.ui) as &dyn std::any::Any).downcast_ref::<CaptureUi>().unwrap().errors.borrow()
        };
    }

    macro_rules! infos {
        ($ctx:expr) => {
            (&*($ctx.ui) as &dyn std::any::Any).downcast_ref::<CaptureUi>().unwrap().infos.borrow()
        };
    }

    fn ctx_with_model(id: &str, provider_id: Option<&str>) -> crate::AppContext {
        let mut ctx = empty_ctx();
        ctx.config.models.insert(id.to_string(), ModelConfig {
            model_id: id.to_string(),
            provider_id: provider_id.map(String::from),
            variant: None,
            enabled: true,
        });
        ctx
    }

    // -- catalog --------------------------------------------------------------

    #[test]
    fn catalog_table_has_correct_column_headers() {
        let ctx = empty_ctx();
        ModelCommands::catalog(&ctx, None).unwrap();
        let tables = tables!(ctx);
        assert_eq!(tables.len(), 1);
        let (_, headers, _) = &tables[0];
        assert!(headers.contains(&"ID".to_string()));
        assert!(headers.contains(&"FAMILY".to_string()));
        assert!(headers.contains(&"SIZE".to_string()));
        assert!(headers.contains(&"CONTEXT".to_string()));
        assert!(headers.contains(&"TYPE".to_string()));
    }

    #[test]
    fn catalog_no_filter_returns_all_models() {
        let ctx = empty_ctx();
        ModelCommands::catalog(&ctx, None).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert!(!rows.is_empty(), "expected at least one model in catalog");
    }

    #[test]
    fn catalog_text_filter_returns_only_text_models() {
        let ctx = empty_ctx();
        ModelCommands::catalog(&ctx, Some(ModelType::Text)).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        for row in rows {
            assert_eq!(row[4], "Text", "all filtered rows should be Text type");
        }
    }

    #[test]
    fn catalog_vision_filter_returns_only_vision_models() {
        let ctx = empty_ctx();
        ModelCommands::catalog(&ctx, Some(ModelType::Vision)).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        for row in rows {
            assert_eq!(row[4], "Vision");
        }
    }

    #[test]
    fn catalog_speech_filter_returns_only_speech_models() {
        let ctx = empty_ctx();
        ModelCommands::catalog(&ctx, Some(ModelType::Speech)).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        for row in rows {
            assert_eq!(row[4], "Speech");
        }
    }

    // -- list -----------------------------------------------------------------

    #[test]
    fn list_empty_config_renders_zero_rows() {
        let ctx = empty_ctx();
        ModelCommands::list(&ctx, None).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert_eq!(rows.len(), 0);
    }

    #[test]
    fn list_configured_model_shows_provider_id() {
        let ctx = ctx_with_model("granite-3.1-8b-instruct", Some("my-ollama"));
        ModelCommands::list(&ctx, None).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert_eq!(rows.len(), 1);
        assert!(rows[0].iter().any(|c| c == "my-ollama"));
    }

    #[test]
    fn list_configured_model_without_provider_shows_none() {
        let ctx = ctx_with_model("granite-3.1-8b-instruct", None);
        ModelCommands::list(&ctx, None).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert!(rows[0].iter().any(|c| c == "None"));
    }

    #[test]
    fn list_unknown_model_id_in_config_is_skipped() {
        let ctx = ctx_with_model("this-model-does-not-exist", Some("p1"));
        ModelCommands::list(&ctx, None).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        // The unknown id is not in MODEL_REGISTRY, so it should be skipped
        assert_eq!(rows.len(), 0);
    }

    // -- info -----------------------------------------------------------------

    #[test]
    fn info_known_model_renders_detail_with_key_fields() {
        let ctx = empty_ctx();
        ModelCommands::info(&ctx, "granite-3.1-8b-instruct").unwrap();
        let details = details!(ctx);
        assert_eq!(details.len(), 1);
        let (title, fields) = &details[0];
        assert_eq!(title, "granite-3.1-8b-instruct");
        assert!(fields.iter().any(|(k, _)| k == "Family"));
        assert!(fields.iter().any(|(k, _)| k == "Context Length"));
        assert!(fields.iter().any(|(k, _)| k == "Supported Functions"));
    }

    #[test]
    fn info_unknown_model_returns_err_and_emits_error() {
        let ctx = empty_ctx();
        let result = ModelCommands::info(&ctx, "does-not-exist");
        assert!(result.is_err());
        assert!(!errors!(ctx).is_empty());
    }


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

    // -- search ---------------------------------------------------------------

    #[test]
    fn search_returns_matching_models_by_id() {
        let ctx = empty_ctx();
        // Use a query unique enough that it only appears in matching IDs, not in descriptions
        ModelCommands::search(&ctx, "granite-3.1-8b-instruct").unwrap();
        let tables = tables!(ctx);
        assert_eq!(tables.len(), 1);
        let (_, _, rows) = &tables[0];
        assert!(!rows.is_empty());
        // Every returned row must have matched; verify at least one row has the exact model ID
        assert!(rows.iter().any(|r| r[0] == "granite-3.1-8b-instruct"));
    }

    #[test]
    fn search_is_case_insensitive() {
        let ctx = empty_ctx();
        ModelCommands::search(&ctx, "GRANITE").unwrap();
        let tables = tables!(ctx);
        assert!(!tables.is_empty());
        let (_, _, rows) = &tables[0];
        assert!(!rows.is_empty());
    }

    #[test]
    fn search_no_match_emits_info_not_table() {
        let ctx = empty_ctx();
        ModelCommands::search(&ctx, "zzznomatch").unwrap();
        assert!(tables!(ctx).is_empty());
        assert!(!infos!(ctx).is_empty());
    }

    #[test]
    fn search_family_match_returns_rows() {
        let ctx = empty_ctx();
        // "Granite 3.3" is a family name
        ModelCommands::search(&ctx, "3.3").unwrap();
        let tables = tables!(ctx);
        assert!(!tables.is_empty());
        let (_, _, rows) = &tables[0];
        assert!(!rows.is_empty());
    }
}
