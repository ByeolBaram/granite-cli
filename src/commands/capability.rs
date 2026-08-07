// Third Party
use anyhow::Result;

// Local
use crate::capabilities::{CAPABILITY_REGISTRY, Dependency, ModelRequirement, ProviderRequirement};
use crate::dependency::{self, Configured};
use crate::utils::prompt_from_schema;
use crate::utils::ui::Ui;

pub struct CapabilityCommands;

impl CapabilityCommands {
    pub fn catalog(ctx: &crate::AppContext) -> Result<()> {
        let capabilities = CAPABILITY_REGISTRY.entries();

        let mut rows: Vec<Vec<String>> = capabilities
            .iter()
            .map(|(cap_id, cap)| {
                let deps: Vec<_> = cap.dependencies.iter().map(|d| d.to_string()).collect();
                let deps_str = if deps.is_empty() {
                    "None".to_string()
                } else {
                    deps.join(", ")
                };
                vec![cap_id.to_string(), cap.name.clone(), deps_str]
            })
            .collect();
        rows.sort_by(|a, b| a[0].cmp(&b[0]));

        ctx.ui.table(
            &format!("Capability Catalog ({} capabilities)", capabilities.len()),
            &["ID", "NAME", "DEPENDENCIES"],
            &rows,
        );
        Ok(())
    }

    pub fn list(ctx: &crate::AppContext) -> Result<()> {
        let mut rows: Vec<Vec<String>> = ctx
            .config
            .capabilities
            .iter()
            .map(|(id, cfg)| vec![id.clone(), cfg.capability_type.clone()])
            .collect();
        rows.sort_by(|a, b| {
            let type_cmp = a[1].cmp(&b[1]);
            if type_cmp != std::cmp::Ordering::Equal {
                return type_cmp;
            }
            a[0].cmp(&b[0])
        });

        ctx.ui.table(
            &format!("Configured Capabilities ({} capabilities)", rows.len()),
            &["ID", "TYPE"],
            &rows,
        );
        Ok(())
    }

    pub fn info(ctx: &crate::AppContext, capability_id: &str) -> Result<()> {
        let configured = ctx.config.get_capability(capability_id);

        let catalog_entry = configured
            .and_then(|c| CAPABILITY_REGISTRY.get(&c.capability_type))
            .or_else(|| CAPABILITY_REGISTRY.get(capability_id));

        match catalog_entry {
            Some(cap) => {
                let mut fields: Vec<(&str, String)> = vec![
                    ("Name", cap.name.clone()),
                    ("Description", cap.description.clone()),
                ];

                if !cap.tags.is_empty() {
                    fields.push(("Tags", cap.tags.join(", ")));
                }

                if let Some(configured) = configured {
                    fields.push(("Type", configured.capability_type.clone()));
                    if let Some(obj) = configured.config.as_object() {
                        for (k, v) in obj {
                            fields.push(("Config", format!("{k} = {v}")));
                        }
                    }
                }

                ctx.ui.detail(capability_id, &fields);
                Ok(())
            }
            None => {
                if configured.is_some() {
                    let fields: Vec<(&str, String)> = vec![(
                        "Note",
                        "Configured but its type is not found in the bundled registry.".to_string(),
                    )];
                    ctx.ui.detail(capability_id, &fields);
                    Ok(())
                } else {
                    ctx.ui.error(&format!(
                        "Capability '{capability_id}' not found in registry."
                    ));
                    anyhow::bail!("Capability not found");
                }
            }
        }
    }

    /// Interactive capability setup wizard.
    ///
    /// `capability_type` is the catalog/registry key (e.g. `agent-model`).
    /// `instance_id` is the nickname for this instance; defaults to
    /// `capability_type` when not given.
    pub async fn setup(
        ctx: &mut crate::AppContext,
        capability_type: &str,
        instance_id: Option<&str>,
    ) -> Result<()> {
        let cap_def = match CAPABILITY_REGISTRY.get(capability_type) {
            Some(def) => def,
            None => {
                ctx.ui.error(&format!(
                    "Capability type '{capability_type}' not found in registry."
                ));
                let available: Vec<String> = {
                    let mut entries: Vec<String> = CAPABILITY_REGISTRY
                        .entries()
                        .iter()
                        .map(|(id, c)| format!("{} ({})", id, c.name))
                        .collect();
                    entries.sort();
                    entries
                };
                ctx.ui
                    .info(&format!("Available types: {}", available.join(", ")));
                anyhow::bail!("Capability type not found");
            }
        };

        ctx.ui
            .info(&format!("\nSetting up capability: {capability_type}"));
        ctx.ui.info(&cap_def.description);

        let instance_id = match instance_id {
            Some(id) => id.to_string(),
            None => ctx.ui.text("Instance name: ", capability_type)?,
        };

        let existing_config = ctx.config.get_capability(&instance_id);
        if existing_config.is_some() {
            let overwrite = ctx.ui.confirm(
                &format!("Capability '{instance_id}' is already configured. Overwrite?"),
                false,
            )?;
            if !overwrite {
                ctx.ui.info("Capability setup skipped.");
                return Ok(());
            }
        }

        let mut schema = CAPABILITY_REGISTRY
            .config_schema(capability_type)
            .ok_or_else(|| {
                anyhow::anyhow!("No config schema registered for capability type '{capability_type}'")
            })?;
        let defaults = existing_config
            .map(|c| c.config.clone())
            .or_else(|| CAPABILITY_REGISTRY.default_config(capability_type))
            .unwrap_or_else(|| serde_json::json!({}));

        // Phase A: prompt for everything except dependency-resolved fields --
        // those are picked from configured instances below, never free-typed.
        let dependency_keys: std::collections::HashSet<&str> = cap_def
            .dependencies
            .iter()
            .filter_map(|d| match d {
                Dependency::Model { config_key, .. } | Dependency::Provider { config_key, .. } => {
                    Some(config_key.as_str())
                }
                Dependency::ExternalTool { .. } => None,
            })
            .collect();
        if let Some(serde_json::Value::Object(props)) = schema.get_mut("properties") {
            props.retain(|k, _| !dependency_keys.contains(k.as_str()));
        }
        if let Some(serde_json::Value::Array(req)) = schema.get_mut("required") {
            req.retain(|v| !v.as_str().is_some_and(|s| dependency_keys.contains(s)));
        }
        let mut config = prompt_from_schema(&*ctx.ui, &schema, &defaults)?;

        // Phase B: build a preview instance from what's been collected so
        // far, then resolve its actual (possibly narrowed) dependencies
        // against currently configured models/providers.
        let preview = CAPABILITY_REGISTRY
            .construct(capability_type, &config)
            .map_err(|e| anyhow::anyhow!(e))?;
        for dep in preview.dependencies() {
            match dep {
                Dependency::Model {
                    config_key,
                    requirement,
                    required,
                    ..
                } => {
                    if let Some(id) = Self::resolve_model_dependency(ctx, &requirement, required)?
                    {
                        config
                            .as_object_mut()
                            .unwrap()
                            .insert(config_key, serde_json::Value::String(id));
                    }
                }
                Dependency::Provider {
                    config_key,
                    requirement,
                    required,
                    ..
                } => {
                    if let Some(id) =
                        Self::resolve_provider_dependency(ctx, &requirement, required)?
                    {
                        config
                            .as_object_mut()
                            .unwrap()
                            .insert(config_key, serde_json::Value::String(id));
                    }
                }
                Dependency::ExternalTool {
                    requirement,
                    required,
                } => {
                    if required && !requirement.is_satisfied() {
                        anyhow::bail!(
                            "Required external command '{}' is not available.",
                            requirement.command
                        );
                    }
                }
            }
        }

        let capability_config = crate::config::CapabilityConfig {
            capability_id: instance_id.clone(),
            capability_type: capability_type.to_string(),
            config,
        };

        if let Err(e) = ctx
            .config
            .insert_capability(&instance_id, capability_config)
        {
            ctx.ui
                .warn(&format!("failed to save capability config: {e}"));
        }

        ctx.ui.info(&format!(
            "\nCapability '{instance_id}' configured successfully!"
        ));

        Ok(())
    }

    /// Resolve a capability's model dependency against currently configured
    /// models, narrowed to those whose attached provider also supports every
    /// function the requirement asks for. Returns the chosen model id, or
    /// `None` if the dependency isn't required and nothing satisfies it.
    fn resolve_model_dependency(
        ctx: &crate::AppContext,
        requirement: &ModelRequirement,
        required: bool,
    ) -> Result<Option<String>> {
        let source = crate::models::ModelSource::from_config(&ctx.config);
        let resolution = dependency::resolve(requirement, &source);
        let instances = source.instances();
        let usable: Vec<String> = resolution
            .existing_instances
            .into_iter()
            .filter(|id| {
                instances
                    .iter()
                    .find(|(i, _)| i == id)
                    .is_some_and(|(_, model)| match model.provider() {
                        Ok(p) => requirement
                            .supported_functions
                            .iter()
                            .all(|f| p.supports_function(f)),
                        Err(_) => false,
                    })
            })
            .collect();
        Self::pick_dependency(&*ctx.ui, &usable, required, "model")
    }

    /// Resolve a capability's provider dependency against currently
    /// configured providers. Returns the chosen provider id, or `None` if
    /// the dependency isn't required and nothing satisfies it.
    fn resolve_provider_dependency(
        ctx: &crate::AppContext,
        requirement: &ProviderRequirement,
        required: bool,
    ) -> Result<Option<String>> {
        let source = crate::providers::ProviderSource::from_config(&ctx.config);
        let resolution = dependency::resolve(requirement, &source);
        Self::pick_dependency(&*ctx.ui, &resolution.existing_instances, required, "provider")
    }

    /// Pick one candidate id: error if required and none exist, auto-select
    /// the sole candidate, or prompt when there's a choice.
    fn pick_dependency(
        ui: &dyn Ui,
        candidates: &[String],
        required: bool,
        what: &str,
    ) -> Result<Option<String>> {
        if candidates.is_empty() {
            if required {
                anyhow::bail!(
                    "No configured {what} satisfies this capability's requirements yet. Configure one first."
                );
            }
            return Ok(None);
        }
        if candidates.len() == 1 {
            return Ok(Some(candidates[0].clone()));
        }
        let index = ui.select(&format!("Select a {what} for this capability:"), candidates, 0)?;
        Ok(Some(candidates[index].clone()))
    }

    /// Remove a configured capability instance by ID.
    ///
    /// Deletes the capability's config file and removes it from the
    /// in-memory config. After this call `capability list` will no longer
    /// show the entry.
    pub fn remove(ctx: &mut crate::AppContext, capability_id: &str) -> Result<()> {
        if ctx.config.get_capability(capability_id).is_none() {
            anyhow::bail!("No capability configured with id '{capability_id}'. Nothing to remove.");
        }

        if let Err(e) = ctx.config.remove_capability(capability_id) {
            ctx.ui
                .warn(&format!("failed to persist capability removal: {e}"));
        }
        ctx.ui
            .info(&format!("Capability '{capability_id}' removed."));
        Ok(())
    }
}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CapabilityConfig, Config};
    use crate::utils::ui::base::tests::CaptureUi;
    use std::sync::Arc;

    fn test_ctx() -> crate::AppContext {
        crate::AppContext {
            config: Config::default(),
            ui: Arc::new(CaptureUi::default()),
        }
    }

    fn ctx_with_capability(id: &str, capability_type: &str) -> crate::AppContext {
        let mut ctx = test_ctx();
        ctx.config.capabilities.insert(
            id.to_string(),
            CapabilityConfig {
                capability_id: id.to_string(),
                capability_type: capability_type.to_string(),
                config: serde_json::json!({}),
            },
        );
        ctx
    }

    macro_rules! tables {
        ($ctx:expr) => {
            (&*($ctx.ui) as &dyn std::any::Any)
                .downcast_ref::<CaptureUi>()
                .unwrap()
                .tables
                .borrow()
        };
    }

    macro_rules! details {
        ($ctx:expr) => {
            (&*($ctx.ui) as &dyn std::any::Any)
                .downcast_ref::<CaptureUi>()
                .unwrap()
                .details
                .borrow()
        };
    }

    macro_rules! infos {
        ($ctx:expr) => {
            (&*($ctx.ui) as &dyn std::any::Any)
                .downcast_ref::<CaptureUi>()
                .unwrap()
                .infos
                .borrow()
        };
    }

    // -- catalog --------------------------------------------------------------

    #[test]
    fn catalog_table_has_id_name_dependencies_columns() {
        let ctx = test_ctx();
        CapabilityCommands::catalog(&ctx).unwrap();
        let tables = tables!(ctx);
        assert_eq!(tables.len(), 1);
        let (_, headers, _) = &tables[0];
        assert!(headers.contains(&"ID".to_string()));
        assert!(headers.contains(&"NAME".to_string()));
        assert!(headers.contains(&"DEPENDENCIES".to_string()));
    }

    #[test]
    fn catalog_contains_agent_model() {
        let ctx = test_ctx();
        CapabilityCommands::catalog(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert!(rows.iter().any(|r| r[0] == "agent-model"));
    }

    // -- list -----------------------------------------------------------------

    #[test]
    fn list_empty_config_has_zero_rows() {
        let ctx = test_ctx();
        CapabilityCommands::list(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert_eq!(rows.len(), 0);
    }

    #[test]
    fn list_configured_capability_shows_row() {
        let ctx = ctx_with_capability("my-cap", "agent-model");
        CapabilityCommands::list(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0], "my-cap");
        assert_eq!(rows[0][1], "agent-model");
    }

    // -- info -----------------------------------------------------------------

    #[test]
    fn info_unknown_capability_returns_err() {
        let ctx = test_ctx();
        let result = CapabilityCommands::info(&ctx, "does-not-exist");
        assert!(result.is_err());
    }

    #[test]
    fn info_configured_only_capability_renders_detail_not_err() {
        let ctx = ctx_with_capability("custom-cap", "not-a-real-type");
        let result = CapabilityCommands::info(&ctx, "custom-cap");
        assert!(result.is_ok());
        assert!(!details!(ctx).is_empty());
    }

    #[test]
    fn info_configured_agent_model_resolves_via_catalog_type() {
        let ctx = ctx_with_capability("chat", "agent-model");
        let result = CapabilityCommands::info(&ctx, "chat");
        assert!(result.is_ok());
        assert!(!details!(ctx).is_empty());
    }

    // -- setup ------------------------------------------------------------------

    #[tokio::test]
    async fn setup_unknown_type_returns_err() {
        let mut ctx = test_ctx();
        let result = CapabilityCommands::setup(&mut ctx, "no-such-type", Some("test")).await;
        assert!(result.is_err());
    }

    fn ctx_with_chat_capable_model() -> crate::AppContext {
        use crate::config::{ModelConfig, ProviderConfig};

        let mut ctx = test_ctx();
        ctx.config.providers.insert(
            "ollama".to_string(),
            ProviderConfig {
                provider_id: "ollama".to_string(),
                provider_type: "ollama".to_string(),
                config: serde_json::json!({}),
            },
        );
        ctx.config.models.insert(
            "granite-3.1-8b-instruct".to_string(),
            ModelConfig {
                model_id: "granite-3.1-8b-instruct".to_string(),
                provider_id: Some("ollama".to_string()),
                variant: None,
            },
        );
        ctx
    }

    #[tokio::test]
    async fn setup_agent_model_persists_config() {
        let mut ctx = ctx_with_chat_capable_model();
        // CaptureUi's text() echoes back the default when prompted; here we
        // pass an explicit instance id so no prompt is needed. Exactly one
        // configured model satisfies the Chat requirement, so it's picked
        // automatically without a select prompt.
        let result = CapabilityCommands::setup(&mut ctx, "agent-model", Some("chat")).await;
        assert!(result.is_ok());
        let configured = ctx.config.get_capability("chat").unwrap();
        assert_eq!(
            configured.config.get("model_id").and_then(|v| v.as_str()),
            Some("granite-3.1-8b-instruct")
        );
        let infos = infos!(ctx);
        assert!(
            infos
                .iter()
                .any(|m| m.contains("chat") && m.contains("configured successfully"))
        );
    }

    #[tokio::test]
    async fn setup_agent_model_fails_when_no_model_configured() {
        let mut ctx = test_ctx();
        let result = CapabilityCommands::setup(&mut ctx, "agent-model", Some("chat")).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No configured model satisfies")
        );
        assert!(ctx.config.get_capability("chat").is_none());
    }

    #[tokio::test]
    async fn setup_agent_model_excludes_providerless_model() {
        use crate::config::ModelConfig;

        let mut ctx = test_ctx();
        // Configured model with no provider_id -- Model::provider() errs, so
        // it must not be offered as a candidate.
        ctx.config.models.insert(
            "granite-3.1-8b-instruct".to_string(),
            ModelConfig {
                model_id: "granite-3.1-8b-instruct".to_string(),
                provider_id: None,
                variant: None,
            },
        );

        let result = CapabilityCommands::setup(&mut ctx, "agent-model", Some("chat")).await;
        assert!(result.is_err());
        assert!(ctx.config.get_capability("chat").is_none());
    }

    // -- remove -----------------------------------------------------------------

    #[test]
    fn remove_existing_capability_succeeds_and_disappears_from_list() {
        let mut ctx = ctx_with_capability("my-cap", "agent-model");
        assert!(ctx.config.get_capability("my-cap").is_some());

        CapabilityCommands::remove(&mut ctx, "my-cap").unwrap();

        assert!(ctx.config.get_capability("my-cap").is_none());
        let infos = infos!(ctx);
        assert!(
            infos
                .iter()
                .any(|m| m.contains("my-cap") && m.contains("removed"))
        );
    }

    #[test]
    fn remove_nonexistent_capability_returns_err() {
        let mut ctx = test_ctx();
        let result = CapabilityCommands::remove(&mut ctx, "doesnt-exist");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Nothing to remove")
        );
    }

    #[test]
    fn list_does_not_show_removed_capability() {
        let mut ctx = ctx_with_capability("my-cap", "agent-model");
        CapabilityCommands::remove(&mut ctx, "my-cap").unwrap();
        CapabilityCommands::list(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert!(rows.is_empty());
    }
}
