// Standard
use std::collections::HashMap;

// Third Party
use anyhow::Result;
use dialoguer::Confirm;

// Local
use crate::capabilities::CAPABILITY_REGISTRY;

pub struct CapabilityCommands;

impl CapabilityCommands {
    pub fn list(ctx: &crate::AppContext) -> Result<()> {
        let capabilities = CAPABILITY_REGISTRY.list();

        println!();
        println!("{:<20} {:<30} {} {}", "ID", "NAME", "DEPENDENCIES", "STATUS");
        println!("{:<20} {:<30} {} {}", "----", "----", "------------", "------");

        for cap in &capabilities {
            let deps: Vec<_> = cap.dependencies.iter().map(|d| format!("{}", d)).collect();
            let deps_str = if deps.is_empty() {
                "None".to_string()
            } else {
                deps.join(", ")
            };
            let status = if ctx.config.capabilities.contains_key(&cap.id) {
                "CONFIGURED"
            } else {
                "BUNDLED"
            };
            println!("{:<20} {:<30} {} {}", cap.id, cap.name, deps_str, status);
        }

        // Show configured capabilities not in the registry
        let mut extra_configured = Vec::new();
        for id in ctx.config.capabilities.keys() {
            if CAPABILITY_REGISTRY.get(id).is_none() {
                extra_configured.push(id.clone());
            }
        }
        extra_configured.sort();

        if !extra_configured.is_empty() {
            println!();
            println!("Additional configured capabilities:");
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
        println!("Total: {} capabilities{}", capabilities.len(), extra_suffix);
        Ok(())
    }

    pub fn info(ctx: &crate::AppContext, capability_id: &str) -> Result<()> {
        match CAPABILITY_REGISTRY.get(capability_id) {
            Some(cap) => {
                println!();
                println!("Capability: {}", cap.id);
                println!("Name: {}", cap.name);
                println!("Description: {}", cap.description);

                if !cap.tags.is_empty() {
                    println!("\nTags: {}", cap.tags.join(", "));
                }

                // TODO: Properly check dependency status
                // if !cap.dependencies.is_empty() {
                //     println!("\nDependencies:");
                //     for dep in &cap.dependencies {
                //         let status = Self::check_dep_status(ctx, dep);
                //         println!("  - {} {}", dep, status);
                //     }
                // }

                // Note: Hooks are now implemented as trait methods, not metadata fields
                println!("\nExecution Hooks:");
                println!("  - on_setup: One-time initialization");
                println!("  - on_configure: Runs during tool configuration");
                println!("  - on_pre_launch: Runs before tool launches");
                println!("  - on_post_launch: Runs after tool starts");
                println!("  - on_shutdown: Cleanup when tool exits");
                println!("  - runtime_bindings: Returns environment variables");

                // Show configuration state
                if let Some(configured) = ctx.config.get_capability(capability_id) {
                    println!("\nConfiguration:");
                    println!("  Enabled: {}", configured.enabled);
                    if !configured.config.is_empty() {
                        println!("  Settings:");
                        for (k, v) in &configured.config {
                            println!("    {} = {}", k, v);
                        }
                    }
                }

                Ok(())
            }
            None => {
                // Check if it's a configured-only capability
                if let Some(configured) = ctx.config.get_capability(capability_id) {
                    println!();
                    println!("Capability: {}", capability_id);
                    println!("Enabled: {}", configured.enabled);
                    if !configured.config.is_empty() {
                        println!("\nSettings:");
                        for (k, v) in &configured.config {
                            println!("  {} = {}", k, v);
                        }
                    }
                    println!("\nNote: This capability is configured but not found in the bundled registry.");
                    Ok(())
                } else {
                    eprintln!("Error: Capability '{}' not found in registry.", capability_id);
                    println!("\nAvailable capabilities:");
                    for cap in CAPABILITY_REGISTRY.list() {
                        println!("  - {}", cap.id);
                    }
                    anyhow::bail!("Capability not found");
                }
            }
        }
    }

    pub async fn setup(ctx: &mut crate::AppContext, capability_id: &str) -> Result<()> {
        match CAPABILITY_REGISTRY.get(capability_id) {
            Some(cap) => {
                println!("\nSetting up capability: {}", cap.id);
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
                    capability_id: cap.id.clone(),
                    enabled: config_map.get("enabled").map(|v| v == "true").unwrap_or(true),
                    config: config_map,
                };

                ctx.config.insert_capability(capability_id, capability_config);

                println!("\nCapability '{}' configured successfully!", cap.id);

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
                    for cap in CAPABILITY_REGISTRY.list() {
                        println!("  - {}", cap.id);
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
