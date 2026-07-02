use crate::registry::{self, Registry};
use crate::capabilities::base::Dependency;
use crate::config::Config;
use std::sync::Arc;
use std::sync::RwLock;

use super::DependencyResolution;

/// Validates capability dependencies against the current configuration.
pub struct DependencyResolver {
    config: Arc<RwLock<Config>>,
}

impl DependencyResolver {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self { config }
    }

    /// Validate all dependencies in topological order.
    pub async fn validate_dependencies(
        &self,
        order: &[String],
    ) -> Result<DependencyResolution, anyhow::Error> {
        let mut resolved = Vec::new();
        let mut unresolved = Vec::new();

        for item_id in order {
            let cap_def = registry::CAPABILITY_REGISTRY.get(item_id);
            if cap_def.is_none() {
                continue;
            }
            let cap_def = cap_def.unwrap();

            let mut missing = Vec::new();

            for dep in &cap_def.dependencies {
                match self.check_dependency(dep).await {
                    DependencyStatus::Satisfied => {}
                    DependencyStatus::Missing(msg) => {
                        missing.push(msg);
                    }
                }
            }

            if missing.is_empty() {
                resolved.push(item_id.clone());
            } else {
                let any_required_missing = cap_def.dependencies.iter()
                    .any(|d| Self::is_required(d) && missing.iter().any(|m| m.contains(&format!("{:?}", d))));

                unresolved.push(super::UnresolvedDependency {
                    capability_id: item_id.clone(),
                    missing,
                    required: any_required_missing,
                });
            }
        }

        Ok(DependencyResolution {
            resolved,
            unresolved,
            order: order.to_vec(),
        })
    }

    async fn check_dependency(&self, dep: &Dependency) -> DependencyStatus {
        match dep {
            Dependency::Model { id, required: _ } => {
                let has_config = self.config.read().unwrap()
                    .models
                    .contains_key(id.as_str());
                let in_registry = registry::MODEL_REGISTRY.get(id).is_some();

                if !has_config && !in_registry {
                    DependencyStatus::Missing(format!("Model '{}' not available", id))
                } else {
                    DependencyStatus::Satisfied
                }
            }
            Dependency::Provider { id, required: _ } => {
                let has_config = self.config.read().unwrap()
                    .providers
                    .contains_key(id.as_str());

                if !has_config {
                    DependencyStatus::Missing(format!("Provider '{}' not configured", id))
                } else {
                    DependencyStatus::Satisfied
                }
            }
            Dependency::ExternalTool { name, check_command } => {
                let available = Self::check_external_tool(check_command);
                if !available {
                    DependencyStatus::Missing(format!("External tool '{}' not found ({})", name, check_command))
                } else {
                    DependencyStatus::Satisfied
                }
            }
            Dependency::Capability { id, required: _ } => {
                let is_configured = self.config.read().unwrap()
                    .capabilities
                    .contains_key(id.as_str());
                let in_registry = registry::CAPABILITY_REGISTRY.get(id).is_some();

                if !is_configured && !in_registry {
                    DependencyStatus::Missing(format!("Capability '{}' not available", id))
                } else {
                    DependencyStatus::Satisfied
                }
            }
        }
    }

    fn check_external_tool(command: &str) -> bool {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return false;
        }

        let result = std::process::Command::new(&parts[0])
            .args(&parts[1..])
            .output();

        match result {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    fn is_required(dep: &Dependency) -> bool {
        match dep {
            Dependency::Model { required, .. } => *required,
            Dependency::Provider { required, .. } => *required,
            Dependency::ExternalTool { .. } => false,
            Dependency::Capability { required, .. } => *required,
        }
    }
}

enum DependencyStatus {
    Satisfied,
    Missing(String),
}
