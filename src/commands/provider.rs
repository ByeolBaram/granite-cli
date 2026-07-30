// Third Party
use anyhow::Result;

// Local
use crate::providers::{PROVIDER_REGISTRY, HealthStatus};
use crate::utils::prompt_from_schema;

pub struct ProviderCommands;

impl ProviderCommands {
    pub fn catalog(ctx: &crate::AppContext) -> Result<()> {
        let providers = PROVIDER_REGISTRY.entries();

        let mut rows: Vec<Vec<String>> = providers.iter().map(|(id, p)| {
            vec![id.to_string(), p.default_endpoint.clone()]
        }).collect();
        rows.sort_by(|a, b| a[0].cmp(&b[0]));

        ctx.ui.table(
            &format!("Provider Catalog ({} providers)", providers.len()),
            &["ID", "DEFAULT ENDPOINT"],
            &rows,
        );
        Ok(())
    }

    pub fn list(ctx: &crate::AppContext) -> Result<()> {
        let mut rows: Vec<Vec<String>> = ctx.config.providers.iter().map(|(id, cfg)| {
            let base_url = cfg.config.get("base_url")
                .and_then(|v| v.as_str())
                .unwrap_or("-")
                .to_string();
            vec![id.clone(), cfg.provider_type.clone(), cfg.enabled.to_string(), base_url]
        }).collect();
        rows.sort_by(|a, b| {
            let type_cmp = a[1].cmp(&b[1]);
            if type_cmp != std::cmp::Ordering::Equal {
                return type_cmp;
            }
            a[0].cmp(&b[0])
        });

        ctx.ui.table(
            &format!("Configured Providers ({} providers)", rows.len()),
            &["ID", "TYPE", "ENABLED", "BASE URL"],
            &rows,
        );
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
                ctx.ui.error(&format!("Provider type '{}' not found in registry.", provider_type));
                let available: Vec<_> = PROVIDER_REGISTRY.entries().iter().map(|(p_id, p)| format!("{} ({})", p_id, p.name)).collect();
                ctx.ui.info(&format!("Available provider types: {}", available.join(", ")));
                anyhow::bail!("Provider type not found");
            }
        };

        ctx.ui.info(&format!("\nSetting up provider instance: {}", provider_type));
        ctx.ui.info(&provider_def.description);
        ctx.ui.info("");
        ctx.ui.info(&format!("Type: {}", provider_def.provider_type));

        // Get a name for this instance
        let instance_id = match instance_id {
            Some(instance_id_arg) => instance_id_arg.to_string(),
            _ => ctx.ui.text("Instance name: ", provider_type)?,
        };

        if !provider_def.authentication.is_empty() {
            let auths = provider_def.authentication.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ");
            ctx.ui.info(&format!("Authentication: {}", auths));
        }

        // Check if this instance is already configured
        let existing_config = ctx.config.get_provider(&instance_id);
        if existing_config.is_some() {
            let overwrite = ctx.ui.confirm(
                &format!("Provider instance '{}' is already configured. Overwrite?", instance_id),
                false,
            )?;
            if !overwrite {
                ctx.ui.info("Provider setup skipped.");
                return Ok(());
            }
        }

        let schema = PROVIDER_REGISTRY.config_schema(provider_type)
            .ok_or_else(|| anyhow::anyhow!("No config schema registered for provider type '{}'", provider_type))?;
        let defaults = existing_config
            .map(|c| c.config.clone())
            .or_else(|| PROVIDER_REGISTRY.default_config(provider_type))
            .unwrap_or_else(|| serde_json::json!({}));

        let config = prompt_from_schema(&*ctx.ui, &schema, &defaults)?;

        let provider_config = crate::config::ProviderConfig {
            provider_id: instance_id.clone(),
            provider_type: provider_type.to_string(),
            config,
            enabled: true,
        };

        if let Err(e) = ctx.config.insert_provider(&instance_id, provider_config) {
            ctx.ui.warn(&format!("failed to save provider config: {}", e));
        }

        // Health check
        ctx.ui.info("\nRunning health check...");
        match Self::check_provider_health(ctx, &instance_id).await {
            Ok(status) => {
                if status.healthy {
                    ctx.ui.info(&format!("Provider '{}' is healthy!", instance_id));
                } else {
                    ctx.ui.warn(&format!("Provider '{}' health check failed. It may need to be started or configured differently.", instance_id));
                }
            }
            Err(e) => {
                ctx.ui.warn(&format!("Could not run health check: {}", e));
            }
        }

        ctx.ui.info(&format!("\nProvider instance '{}' configured successfully!", instance_id));
        ctx.ui.info("Supported APIs:");
        for (func, endpoints) in &provider_def.default_function_endpoints {
            let endpoint_strs: Vec<String> = endpoints.iter()
                .map(|ep| format!("{} ({})", ep.api_type(), ep.path()))
                .collect();
            ctx.ui.info(&format!("  - {} -> {}", func, endpoint_strs.join(", ")));
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
            ctx.ui.info("No configured providers to check.");
            return Ok(());
        }

        for id in &providers_to_check {
            match Self::check_provider_health(ctx, id).await {
                Ok(status) => {
                    let detail = if let Some(ref e) = status.error {
                        format!("{} — {}", status.latency.as_millis(), e)
                    } else {
                        format!("{}ms", status.latency.as_millis())
                    };
                    ctx.ui.status(id, status.healthy, &detail);
                }
                Err(e) => {
                    ctx.ui.status(id, false, &e.to_string());
                }
            }
        }

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

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ProviderConfig};
    use crate::utils::ui::base::tests::CaptureUi;

    fn test_ctx() -> crate::AppContext {
        crate::AppContext {
            config: Config::default(),
            ui: Box::new(CaptureUi::default()),
        }
    }

    fn ctx_with_provider(id: &str, url: &str) -> crate::AppContext {
        let mut ctx = test_ctx();
        ctx.config.providers.insert(id.to_string(), ProviderConfig {
            provider_id: id.to_string(),
            provider_type: "openai-compatible".to_string(),
            config: serde_json::json!({ "base_url": url }),
            enabled: true,
        });
        ctx
    }

    macro_rules! tables {
        ($ctx:expr) => {
            (&*($ctx.ui) as &dyn std::any::Any).downcast_ref::<CaptureUi>().unwrap().tables.borrow()
        };
    }

    macro_rules! infos {
        ($ctx:expr) => {
            (&*($ctx.ui) as &dyn std::any::Any).downcast_ref::<CaptureUi>().unwrap().infos.borrow()
        };
    }

    macro_rules! statuses {
        ($ctx:expr) => {
            (&*($ctx.ui) as &dyn std::any::Any).downcast_ref::<CaptureUi>().unwrap().statuses.borrow()
        };
    }

    // -- catalog --------------------------------------------------------------

    #[test]
    fn catalog_table_has_id_endpoint_columns() {
        let ctx = test_ctx();
        ProviderCommands::catalog(&ctx).unwrap();
        let tables = tables!(ctx);
        assert_eq!(tables.len(), 1);
        let (_, headers, _) = &tables[0];
        assert!(headers.contains(&"ID".to_string()));
        assert!(headers.contains(&"DEFAULT ENDPOINT".to_string()));
        assert!(!headers.contains(&"TYPE".to_string()));
    }

    #[test]
    fn catalog_contains_openai_compatible_entry() {
        let ctx = test_ctx();
        ProviderCommands::catalog(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert!(rows.iter().any(|r| r[0] == "openai-compatible"));
    }

    // -- list -----------------------------------------------------------------

    #[test]
    fn list_empty_config_has_zero_rows() {
        let ctx = test_ctx();
        ProviderCommands::list(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert_eq!(rows.len(), 0);
    }

    #[test]
    fn list_configured_provider_shows_base_url() {
        let ctx = ctx_with_provider("my-ollama", "http://localhost:11434");
        ProviderCommands::list(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert_eq!(rows.len(), 1);
        assert!(rows[0].iter().any(|c| c.contains("11434")));
    }

    #[test]
    fn list_disabled_provider_still_appears() {
        let mut ctx = ctx_with_provider("my-ollama", "http://localhost:11434");
        ctx.config.providers.get_mut("my-ollama").unwrap().enabled = false;
        ProviderCommands::list(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert_eq!(rows.len(), 1);
        assert!(rows[0].iter().any(|c| c == "false"));
    }

    #[test]
    fn list_sorted_by_type_then_id() {
        let mut ctx = test_ctx();
        ctx.config.providers.insert("prod-openai".to_string(), ProviderConfig {
            provider_id: "prod-openai".to_string(),
            provider_type: "openai-compatible".to_string(),
            config: serde_json::json!({ "base_url": "http://prod" }),
            enabled: true,
        });
        ctx.config.providers.insert("local-ollama".to_string(), ProviderConfig {
            provider_id: "local-ollama".to_string(),
            provider_type: "ollama".to_string(),
            config: serde_json::json!({ "base_url": "http://localhost:11434" }),
            enabled: true,
        });
        ctx.config.providers.insert("dev-openai".to_string(), ProviderConfig {
            provider_id: "dev-openai".to_string(),
            provider_type: "openai-compatible".to_string(),
            config: serde_json::json!({ "base_url": "http://dev" }),
            enabled: true,
        });
        ProviderCommands::list(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0][1], "ollama");
        assert_eq!(rows[1][1], "openai-compatible");
        assert_eq!(rows[2][1], "openai-compatible");
        assert_eq!(rows[1][0], "dev-openai");
        assert_eq!(rows[2][0], "prod-openai");
    }

    // -- health ----------------------------------------------------------------

    #[tokio::test]
    async fn health_no_providers_emits_info_message() {
        let mut ctx = test_ctx();
        ProviderCommands::health(&mut ctx, None).await.unwrap();
        assert!(!infos!(ctx).is_empty());
        assert!(statuses!(ctx).is_empty());
    }
}
