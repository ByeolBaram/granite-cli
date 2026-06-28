use crate::registry::{self, Registry};
use anyhow::Result;
use dialoguer::Confirm;
use std::collections::HashMap;

pub struct CapabilityCommands;

impl CapabilityCommands {
    pub fn list(ctx: &crate::AppContext) -> Result<()> {
        let registry = &*registry::CAPABILITY_REGISTRY;
        let capabilities = registry.list();

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
            if registry.get(id).is_none() {
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
        let registry = &*registry::CAPABILITY_REGISTRY;

        match registry.get(capability_id) {
            Some(cap) => {
                println!();
                println!("Capability: {}", cap.id);
                println!("Name: {}", cap.name);
                println!("Description: {}", cap.description);
                println!("Version: {}", cap.version);

                if !cap.tags.is_empty() {
                    println!("\nTags: {}", cap.tags.join(", "));
                }

                if !cap.dependencies.is_empty() {
                    println!("\nDependencies:");
                    for dep in &cap.dependencies {
                        println!("  - {}", dep);
                    }
                }

                if !cap.hooks.is_empty() {
                    println!("\nExecution Hooks:");
                    for hook in &cap.hooks {
                        let desc = match hook.as_str() {
                            "on_setup" => "One-time initialization",
                            "on_configure" => "Runs during tool configuration",
                            "on_pre_launch" => "Runs before tool launches",
                            "on_post_launch" => "Runs after tool starts",
                            "on_shutdown" => "Cleanup when tool exits",
                            "runtime_bindings" => "Returns environment variables",
                            _ => "Unknown hook",
                        };
                        println!("  - {}: {}", hook, desc);
                    }
                }

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
                    for cap in registry.list() {
                        println!("  - {}", cap.id);
                    }
                    anyhow::bail!("Capability not found");
                }
            }
        }
    }

    pub fn setup(ctx: &mut crate::AppContext, capability_id: &str) -> Result<()> {
        let registry = &*registry::CAPABILITY_REGISTRY;

        match registry.get(capability_id) {
            Some(cap) => {
                println!("\nSetting up capability: {}", cap.id);
                println!("Name: {}", cap.name);
                println!("Description: {}", cap.description);
                println!();
                println!("This capability requires:");

                for dep in &cap.dependencies {
                    println!("  - {}", dep);
                }

                println!();
                println!("Full capability setup will be implemented in Phase 2.");
                println!("For now, this capability can be referenced in tool configurations.");

                let continue_anyway = Confirm::new()
                    .with_prompt("Add capability placeholder to configuration?")
                    .default(true)
                    .interact()?;

                if continue_anyway {
                    let capability_config = crate::config::CapabilityConfig {
                        capability_id: cap.id.clone(),
                        enabled: true,
                        config: HashMap::new(),
                    };

                    ctx.config.insert_capability(capability_id, capability_config);

                    println!("\nCapability '{}' placeholder saved.", cap.id);
                    println!("Note: Full setup (dependency resolution, hooks) will be available in Phase 2.");
                }

                Ok(())
            }
            None => {
                eprintln!("Error: Capability '{}' not found in registry.", capability_id);
                println!("\nAvailable capabilities:");
                for cap in registry.list() {
                    println!("  - {}", cap.id);
                }
                anyhow::bail!("Capability not found");
            }
        }
    }
}
