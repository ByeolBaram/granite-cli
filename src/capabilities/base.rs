use crate::capabilities::requirement::{
    CapabilityRequirement, ModelRequirement, ProviderRequirement, ShellToolRequirement,
};
use crate::dependency::Configured;
use crate::models::Model;
use crate::providers::Provider;
use crate::registry::ConfigConstructable;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// Canonical launch-time types live in `launchers::base` -- re-exported here so
// capabilities and launchers share one `LaunchContext`/`EnvBinding` pair.
pub use crate::launchers::{EnvBinding, LaunchContext};

/*-- BindingType / BindingRequest / Binding -----------------------------------*/

/// Which binding surface a `Capability` can fill. Payload-free and hashable
/// so a `Launcher` can declare `HashSet<BindingType>` for the surfaces it
/// knows how to consume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BindingType {
    AgentModel,
}

impl std::fmt::Display for BindingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindingType::AgentModel => write!(f, "Agent Model"),
        }
    }
}

/// A request for a capability to produce a `Binding` for a specific binding
/// surface, parameterized by whatever detail that surface needs (e.g. which
/// `ApiType` the launcher's environment expects).
#[derive(Debug, Clone)]
pub enum BindingRequest {
    AgentModel { api_type: crate::providers::ApiType },
}

impl BindingRequest {
    pub fn binding_type(&self) -> BindingType {
        match self {
            BindingRequest::AgentModel { .. } => BindingType::AgentModel,
        }
    }
}

/// The result of a successful `Capability::bind` call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Binding {
    AgentModel(crate::capabilities::agent_model::AgentModelBinding),
}

/*-- Capability Trait ----------------------------------------------------------*/

/// Core trait for capability implementations.
/// All capabilities must implement this trait along with ConfigConstructable.
#[async_trait]
pub trait Capability: ConfigConstructable + Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn dependencies(&self) -> Vec<Dependency>;

    /// Which binding surfaces this capability instance can fill.
    fn binding_types(&self) -> HashSet<BindingType> {
        HashSet::new()
    }

    /// Resolve a `BindingRequest` into a concrete `Binding`, looking up this
    /// capability's model/provider dependencies from the given sources.
    async fn bind(
        &self,
        request: BindingRequest,
        _providers: &(dyn Configured<dyn Provider> + Sync),
        _models: &(dyn Configured<dyn Model> + Sync),
    ) -> anyhow::Result<Binding> {
        anyhow::bail!(
            "capability '{}' does not support binding type {}",
            self.name(),
            request.binding_type()
        )
    }

    // Execution hooks (all optional with NoOp defaults)
    async fn on_setup(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn on_pre_launch(&self, _context: &LaunchContext) -> anyhow::Result<()> {
        Ok(())
    }
    async fn on_post_launch(&self, _context: &LaunchContext) -> anyhow::Result<()> {
        Ok(())
    }
    async fn on_shutdown(&self, _context: &LaunchContext) -> anyhow::Result<()> {
        Ok(())
    }
    fn runtime_bindings(&self) -> Vec<EnvBinding> {
        vec![]
    }
}

/*-- Metadata Types ----------------------------------------------------------*/

/// Metadata describing a capability implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityMetadata {
    pub name: String,
    pub description: String,
    pub dependencies: Vec<Dependency>,
    pub tags: Vec<String>,
    pub binding_types: HashSet<BindingType>,
}

impl std::fmt::Display for CapabilityMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description)
    }
}

/*-- Supporting Types --------------------------------------------------------*/

/// A capability's declared dependency on a model, provider, external shell
/// tool, or another capability. `resolved_id` is `None` at the type level
/// (catalog display, before any instance is configured) and `Some(id)` once
/// a concrete instance has picked a specific dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Dependency {
    Model {
        requirement: ModelRequirement,
        resolved_id: Option<String>,
        required: bool,
    },
    Provider {
        requirement: ProviderRequirement,
        resolved_id: Option<String>,
        required: bool,
    },
    ExternalTool {
        requirement: ShellToolRequirement,
        required: bool,
    },
    Capability {
        requirement: CapabilityRequirement,
        resolved_id: Option<String>,
        required: bool,
    },
}

impl std::fmt::Display for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Dependency::Model {
                resolved_id,
                required,
                ..
            } => {
                write!(
                    f,
                    "Model: {}{}",
                    resolved_id.as_deref().unwrap_or("<unresolved>"),
                    if *required { " (required)" } else { "" }
                )
            }
            Dependency::Provider {
                resolved_id,
                required,
                ..
            } => {
                write!(
                    f,
                    "Provider: {}{}",
                    resolved_id.as_deref().unwrap_or("<unresolved>"),
                    if *required { " (required)" } else { "" }
                )
            }
            Dependency::ExternalTool {
                requirement,
                required,
            } => {
                write!(
                    f,
                    "ExternalTool: {}{}",
                    requirement.command,
                    if *required { " (required)" } else { "" }
                )
            }
            Dependency::Capability {
                resolved_id,
                required,
                ..
            } => {
                write!(
                    f,
                    "Capability: {}{}",
                    resolved_id.as_deref().unwrap_or("<unresolved>"),
                    if *required { " (required)" } else { "" }
                )
            }
        }
    }
}

/*-- Factory Definition ------------------------------------------------------*/

use crate::define_factory;

define_factory!(Capability, CapabilityMetadata, CapabilityFactory);
