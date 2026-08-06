// Standard
use std::collections::HashMap;

// Third Party
use anyhow::Result;

// Local
use crate::capabilities::CAPABILITY_REGISTRY;

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
            .keys()
            .map(|id| {
                let name = CAPABILITY_REGISTRY
                    .get(id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| id.clone());
                vec![id.clone(), name]
            })
            .collect();
        rows.sort_by(|a, b| a[0].cmp(&b[0]));

        ctx.ui.table(
            &format!("Configured Capabilities ({} capabilities)", rows.len()),
            &["ID", "NAME"],
            &rows,
        );
        Ok(())
    }

    pub fn info(ctx: &crate::AppContext, capability_id: &str) -> Result<()> {
        match CAPABILITY_REGISTRY.get(capability_id) {
            Some(cap) => {
                let mut fields: Vec<(&str, String)> = vec![
                    ("Name", cap.name.clone()),
                    ("Description", cap.description.clone()),
                ];

                if !cap.tags.is_empty() {
                    fields.push(("Tags", cap.tags.join(", ")));
                }

                fields.push(("Execution Hooks", "on_setup, on_configure, on_pre_launch, on_post_launch, on_shutdown, runtime_bindings".to_string()));

                if let Some(configured) = ctx.config.get_capability(capability_id) {
                    for (k, v) in &configured.config {
                        fields.push(("Config", format!("{k} = {v}")));
                    }
                }

                ctx.ui.detail(capability_id, &fields);
                Ok(())
            }
            None => {
                if let Some(_configured) = ctx.config.get_capability(capability_id) {
                    let fields: Vec<(&str, String)> = vec![(
                        "Note",
                        "Configured but not found in bundled registry.".to_string(),
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

    pub async fn setup(ctx: &mut crate::AppContext, capability_id: &str) -> Result<()> {
        match CAPABILITY_REGISTRY.get(capability_id) {
            Some(cap) => {
                ctx.ui
                    .info(&format!("\nSetting up capability: {capability_id}"));
                ctx.ui.info(&format!("Name: {}", cap.name));
                ctx.ui.info(&format!("Description: {}", cap.description));
                ctx.ui.info("");

                // Check dependencies
                ctx.ui.info("Checking dependencies:");
                // let mut all_satisfied = true;

                // for dep in &cap.dependencies {
                //     let status = Self::check_dep_status(ctx, dep);
                //     println!("  - {} {}", dep, status);
                //     if status == " [MISSING]" {
                //         all_satisfied = false;
                //     }
                // }

                // Use DI factory to validate dependencies
                // TODO: Recursively resolve dependencies

                // if !all_satisfied {
                //     ctx.ui.info("\nSome dependencies are missing. You may want to:");
                //     ctx.ui.info("  - Configure required models: granite-cli model setup <model-id>");
                //     ctx.ui.info("  - Configure required providers: granite-cli provider setup <provider-id>");
                //     ctx.ui.info("  - Install required external tools");
                //     ctx.ui.info("");

                //     let continue_anyway = ctx.ui.confirm("Continue with setup anyway?", false)?;

                //     if !continue_anyway {
                //         ctx.ui.info("Capability setup cancelled.");
                //         return Ok(());
                //     }
                // }

                // Run on_setup hook
                // TODO: Recursively set up dependencies
                // println!("\nRunning setup hooks...");
                // if let Ok(capability) = crate::capabilities::resolve_capability_from_registry(capability_id) {
                //     let result = capability.on_setup(&factory).await;
                //     if let Err(e) = result {
                //         println!("Warning: on_setup hook failed: {}", e);
                //     }
                // }

                // Prompt for capability-specific configuration
                let config_map = HashMap::new();

                ctx.ui.info(&format!(
                    "\nCapability {} will be available at tool launch time.",
                    cap.name
                ));

                let capability_config = crate::config::CapabilityConfig {
                    capability_id: capability_id.to_string(),
                    config: config_map,
                };

                if let Err(e) = ctx
                    .config
                    .insert_capability(capability_id, capability_config)
                {
                    ctx.ui
                        .warn(&format!("failed to save capability config: {e}"));
                }

                ctx.ui.info(&format!(
                    "\nCapability '{capability_id}' configured successfully!"
                ));

                Ok(())
            }
            None => {
                // Check if it's a configured-only capability
                if let Some(configured) = ctx.config.get_capability(capability_id) {
                    ctx.ui.info(&format!("\nCapability: {capability_id}"));
                    if !configured.config.is_empty() {
                        ctx.ui.info("\nCurrent Settings:");
                        for (k, v) in &configured.config {
                            ctx.ui.info(&format!("  {k} = {v}"));
                        }
                    }
                    ctx.ui.info("\nNote: This capability is configured but not found in the bundled registry.");

                    let overwrite = ctx.ui.confirm("Reconfigure this capability?", false)?;

                    if overwrite {
                        ctx.ui.info("\nPlease remove the existing config first:");
                        ctx.ui
                            .info(&format!("  granite-cli capability remove {capability_id}"));
                        ctx.ui.info("Then run setup again.");
                    }

                    Ok(())
                } else {
                    ctx.ui.error(&format!(
                        "Capability '{capability_id}' not found in registry."
                    ));
                    let available: Vec<_> = CAPABILITY_REGISTRY
                        .entries()
                        .keys()
                        .map(|k| k.to_string())
                        .collect();
                    ctx.ui
                        .info(&format!("Available capabilities: {}", available.join(", ")));
                    anyhow::bail!("Capability not found");
                }
            }
        }
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

    // TODO: Use generic dependency checking
    // fn check_dep_status(ctx: &crate::AppContext, dep: &crate::registry::CapabilityDependency) -> &'static str {
    //     match dep {
    //         crate::registry::CapabilityDependency::Model { id, required: _ } => {
    //             if registry::MODEL_REGISTRY.get(id).is_some()
    //                 || ctx.config.models.contains_key(id.as_str())
    //             {
    //                 " [OK]"
    //             } else {
    //                 " [MISSING]"
    //             }
    //         }
    //         crate::registry::CapabilityDependency::Provider { id, required: _ } => {
    //             if ctx.config.providers.contains_key(id.as_str())
    //                 || Registry::get(&*registry::PROVIDER_REGISTRY, id).is_some()
    //             {
    //                 " [OK]"
    //             } else {
    //                 " [MISSING]"
    //             }
    //         }
    //         crate::registry::CapabilityDependency::ExternalTool { name: _, check_command } => {
    //             let parts: Vec<&str> = check_command.split_whitespace().collect();
    //             let available = if parts.is_empty() {
    //                 false
    //             } else {
    //                 std::process::Command::new(&parts[0])
    //                     .args(&parts[1..])
    //                     .output()
    //                     .map(|o| o.status.success())
    //                     .unwrap_or(false)
    //             };

    //             if available {
    //                 " [OK]"
    //             } else {
    //                 println!("       (command: {})", check_command);
    //                 " [MISSING]"
    //             }
    //         }
    //         crate::registry::CapabilityDependency::Capability { id, required: _ } => {
    //             if registry::CAPABILITY_REGISTRY.get(id).is_some()
    //                 || ctx.config.capabilities.contains_key(id.as_str())
    //             {
    //                 " [OK]"
    //             } else {
    //                 " [MISSING]"
    //             }
    //         }
    //     }
    // }
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

    fn ctx_with_capability(id: &str) -> crate::AppContext {
        let mut ctx = test_ctx();
        ctx.config.capabilities.insert(
            id.to_string(),
            CapabilityConfig {
                capability_id: id.to_string(),
                config: std::collections::HashMap::new(),
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
        let ctx = ctx_with_capability("my-cap");
        CapabilityCommands::list(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0], "my-cap");
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
        let ctx = ctx_with_capability("custom-cap");
        let result = CapabilityCommands::info(&ctx, "custom-cap");
        assert!(result.is_ok());
        assert!(!details!(ctx).is_empty());
    }

    // -- remove -----------------------------------------------------------------

    #[test]
    fn remove_existing_capability_succeeds_and_disappears_from_list() {
        let mut ctx = ctx_with_capability("my-cap");
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
        let mut ctx = ctx_with_capability("my-cap");
        CapabilityCommands::remove(&mut ctx, "my-cap").unwrap();
        CapabilityCommands::list(&ctx).unwrap();
        let tables = tables!(ctx);
        let (_, _, rows) = &tables[0];
        assert!(rows.is_empty());
    }
}
