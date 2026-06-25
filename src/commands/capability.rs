use crate::registry::{self, Registry};
use anyhow::Result;
use dialoguer::Confirm;
use std::collections::HashMap;

pub struct CapabilityCommands;

impl CapabilityCommands {
    pub fn list() -> Result<()> {
        let registry = &*registry::CAPABILITY_REGISTRY;
        let capabilities = registry.list();

        if capabilities.is_empty() {
            println!("No capabilities found.");
            return Ok(());
        }

        println!();
        println!("{:<20} {:<30} {}", "ID", "NAME", "DEPENDENCIES");
        println!("{:<20} {:<30} {}", "--", "----", "------------");

        for cap in &capabilities {
            let deps: Vec<_> = cap.dependencies.iter().map(|d| format!("{}", d)).collect();
            let deps_str = if deps.is_empty() {
                "None".to_string()
            } else {
                deps.join(", ")
            };
            println!("{:<20} {:<30} {}", cap.id, cap.name, deps_str);
        }

        println!();
        println!("Total: {} capabilities", capabilities.len());
        Ok(())
    }

    pub fn info(capability_id: &str) -> Result<()> {
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

    pub fn setup(capability_id: &str) -> Result<()> {
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

                    let config = crate::config::Config::load()?;
                    config.save_capability(capability_id, &capability_config)?;

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
