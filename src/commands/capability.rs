// Standard
use std::collections::HashMap;

// Third Party
use anyhow::Result;
use dialoguer::Confirm;

// Local
use crate::capabilities::CAPABILITY_REGISTRY;

pub struct CapabilityCommands;

impl CapabilityCommands {
    pub fn catalog(ctx: &crate::AppContext) -> Result<()> {
        let capabilities = CAPABILITY_REGISTRY.entries();

        let mut rows: Vec<Vec<String>> = capabilities.iter().map(|(cap_id, cap)| {
            let deps: Vec<_> = cap.dependencies.iter().map(|d| d.to_string()).collect();
            let deps_str = if deps.is_empty() { "None".to_string() } else { deps.join(", ") };
            vec![cap_id.to_string(), cap.name.clone(), deps_str]
        }).collect();
        rows.sort_by(|a, b| a[0].cmp(&b[0]));

        ctx.out.table(
            &format!("Capability Catalog ({} capabilities)", capabilities.len()),
            &["ID", "NAME", "DEPENDENCIES"],
            &rows,
        );
        Ok(())
    }

    pub fn list(ctx: &crate::AppContext) -> Result<()> {
        let mut rows: Vec<Vec<String>> = ctx.config.capabilities.iter().map(|(id, cfg)| {
            let name = CAPABILITY_REGISTRY.get(id)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| id.clone());
            vec![id.clone(), name, cfg.enabled.to_string()]
        }).collect();
        rows.sort_by(|a, b| a[0].cmp(&b[0]));

        ctx.out.table(
            &format!("Configured Capabilities ({} capabilities)", rows.len()),
            &["ID", "NAME", "ENABLED"],
            &rows,
        );
        Ok(())
    }

    pub fn info(ctx: &crate::AppContext, capability_id: &str) -> Result<()> {
        match CAPABILITY_REGISTRY.get(capability_id) {
            Some(cap) => {
                let mut fields: Vec<(&str, String)> = vec![
                    ("Name",        cap.name.clone()),
                    ("Description", cap.description.clone()),
                ];

                if !cap.tags.is_empty() {
                    fields.push(("Tags", cap.tags.join(", ")));
                }

                fields.push(("Execution Hooks", "on_setup, on_configure, on_pre_launch, on_post_launch, on_shutdown, runtime_bindings".to_string()));

                if let Some(configured) = ctx.config.get_capability(capability_id) {
                    fields.push(("Config: Enabled", configured.enabled.to_string()));
                    for (k, v) in &configured.config {
                        fields.push(("Config", format!("{} = {}", k, v)));
                    }
                }

                ctx.out.detail(capability_id, &fields);
                Ok(())
            }
            None => {
                if let Some(configured) = ctx.config.get_capability(capability_id) {
                    let fields: Vec<(&str, String)> = vec![
                        ("Enabled", configured.enabled.to_string()),
                        ("Note", "Configured but not found in bundled registry.".to_string()),
                    ];
                    ctx.out.detail(capability_id, &fields);
                    Ok(())
                } else {
                    ctx.out.error(&format!("Capability '{}' not found in registry.", capability_id));
                    anyhow::bail!("Capability not found");
                }
            }
        }
    }

    pub async fn setup(ctx: &mut crate::AppContext, capability_id: &str) -> Result<()> {
        match CAPABILITY_REGISTRY.get(capability_id) {
            Some(cap) => {
                println!("\nSetting up capability: {}", capability_id);
                println!("Name: {}", cap.name);
                println!("Description: {}", cap.description);
                println!();

                // Check dependencies
                println!("Checking dependencies:");
                let mut all_satisfied = true;

                // for dep in &cap.dependencies {
                //     let status = Self::check_dep_status(ctx, dep);
                //     println!("  - {} {}", dep, status);
                //     if status == " [MISSING]" {
                //         all_satisfied = false;
                //     }
                // }

                // Use DI factory to validate dependencies
                // TODO: Recursively resolve dependencies

                if !all_satisfied {
                    println!("\nSome dependencies are missing. You may want to:");
                    println!("  - Configure required models: granite-cli model setup <model-id>");
                    println!("  - Configure required providers: granite-cli provider setup <provider-id>");
                    println!("  - Install required external tools");
                    println!();

                    let continue_anyway = Confirm::new()
                        .with_prompt("Continue with setup anyway?")
                        .default(false)
                        .interact()?;

                    if !continue_anyway {
                        println!("Capability setup cancelled.");
                        return Ok(());
                    }
                }

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
                let mut config_map = HashMap::new();

                let enabled = Confirm::new()
                    .with_prompt(&format!("Enable '{}' capability?", cap.name))
                    .default(true)
                    .interact()?;

                if enabled {
                    println!("\nCapability {} will be available at tool launch time.", cap.name);
                    config_map.insert("enabled".to_string(), "true".to_string());

                    // Get runtime bindings to show what will be injected
                    // TODO: Resolve runtime bindings?
                    // if let Ok(capability) = crate::capabilities::resolve_capability_from_registry(capability_id) {
                    //     let bindings = capability.runtime_bindings();
                    //     if !bindings.is_empty() {
                    //         println!("\nRuntime bindings (environment variables at launch):");
                    //         for binding in &bindings {
                    //             println!("  {}={}", binding.key, binding.value);
                    //         }
                    //     }
                    // }
                } else {
                    println!("\nCapability {} is disabled.", cap.name);
                    config_map.insert("enabled".to_string(), "false".to_string());
                }

                let capability_config = crate::config::CapabilityConfig {
                    capability_id: capability_id.to_string(),
                    enabled: config_map.get("enabled").map(|v| v == "true").unwrap_or(true),
                    config: config_map,
                };

                ctx.config.insert_capability(capability_id, capability_config);

                println!("\nCapability '{}' configured successfully!", capability_id);

                Ok(())
            }
            None => {
                // Check if it's a configured-only capability
                if let Some(configured) = ctx.config.get_capability(capability_id) {
                    println!("\nCapability: {}", capability_id);
                    println!("Enabled: {}", configured.enabled);
                    if !configured.config.is_empty() {
                        println!("\nCurrent Settings:");
                        for (k, v) in &configured.config {
                            println!("  {} = {}", k, v);
                        }
                    }
                    println!("\nNote: This capability is configured but not found in the bundled registry.");

                    let overwrite = Confirm::new()
                        .with_prompt("Reconfigure this capability?")
                        .default(false)
                        .interact()?;

                    if overwrite {
                        println!("\nPlease remove the existing config first:");
                        println!("  granite-cli capability remove {}", capability_id);
                        println!("Then run setup again.");
                    }

                    Ok(())
                } else {
                    eprintln!("Error: Capability '{}' not found in registry.", capability_id);
                    println!("\nAvailable capabilities:");
                    for (reg_cap_id, _) in CAPABILITY_REGISTRY.entries() {
                        println!("  - {}", reg_cap_id);
                    }
                    anyhow::bail!("Capability not found");
                }
            }
        }
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
    use crate::utils::ui::CaptureOutput;

    fn empty_ctx() -> crate::AppContext {
        crate::AppContext {
            config: Config::default(),
            out: Box::new(CaptureOutput::default()),
        }
    }

    fn ctx_with_capability(id: &str, enabled: bool) -> crate::AppContext {
        let mut ctx = empty_ctx();
        ctx.config.capabilities.insert(id.to_string(), CapabilityConfig {
            capability_id: id.to_string(),
            enabled,
            config: std::collections::HashMap::new(),
        });
        ctx
    }

    // -- catalog --------------------------------------------------------------

    #[test]
    fn catalog_table_has_id_name_dependencies_columns() {
        let ctx = empty_ctx();
        CapabilityCommands::catalog(&ctx, &out).unwrap();
        let tables = ctx.out.tables.borrow();
        assert_eq!(tables.len(), 1);
        let (_, headers, _) = &tables[0];
        assert!(headers.contains(&"ID".to_string()));
        assert!(headers.contains(&"NAME".to_string()));
        assert!(headers.contains(&"DEPENDENCIES".to_string()));
    }

    // -- list -----------------------------------------------------------------

    #[test]
    fn list_empty_config_has_zero_rows() {
        let ctx = empty_ctx();
        CapabilityCommands::list(&ctx, &out).unwrap();
        let tables = ctx.out.tables.borrow();
        let (_, _, rows) = &tables[0];
        assert_eq!(rows.len(), 0);
    }

    #[test]
    fn list_configured_capability_shows_enabled_state() {
        let ctx = ctx_with_capability("my-cap", true);
        CapabilityCommands::list(&ctx, &out).unwrap();
        let tables = ctx.out.tables.borrow();
        let (_, _, rows) = &tables[0];
        assert_eq!(rows.len(), 1);
        assert!(rows[0].iter().any(|c| c == "true"));
    }

    // -- info -----------------------------------------------------------------

    #[test]
    fn info_unknown_capability_returns_err() {
        let ctx = empty_ctx();
        let result = CapabilityCommands::info(&ctx, "does-not-exist", &out);
        assert!(result.is_err());
    }

    #[test]
    fn info_configured_only_capability_renders_detail_not_err() {
        let ctx = ctx_with_capability("custom-cap", false);
        let result = CapabilityCommands::info(&ctx, "custom-cap", &out);
        assert!(result.is_ok());
        assert!(!ctx.out.details.borrow().is_empty());
    }
}
