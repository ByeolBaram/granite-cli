use crate::registry::ConfigConstructable;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/*-- Capability Trait --------------------------------------------------------*/

/// Core trait for capability implementations.
/// All capabilities must implement this trait along with ConfigConstructable.
#[async_trait]
pub trait Capability: ConfigConstructable + Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn dependencies(&self) -> Vec<Dependency>;

    // Execution hooks (all optional with NoOp defaults)
    async fn on_setup(&self, _factory: &dyn Factory) -> anyhow::Result<()> {
        Ok(())
    }
    async fn on_configure(&self, _tool: &ToolConfig) -> anyhow::Result<ConfigureResult> {
        Ok(ConfigureResult::default())
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
}

impl std::fmt::Display for CapabilityMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description)
    }
}

/*-- Supporting Types --------------------------------------------------------*/

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Dependency {
    Model { id: String, required: bool },
    Provider { id: String, required: bool },
    ExternalTool { name: String, check_command: String },
    Capability { id: String, required: bool },
}

impl std::fmt::Display for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Dependency::Model { id, required } => {
                write!(
                    f,
                    "Model: {}{}",
                    id,
                    if *required { " (required)" } else { "" }
                )
            }
            Dependency::Provider { id, required } => {
                write!(
                    f,
                    "Provider: {}{}",
                    id,
                    if *required { " (required)" } else { "" }
                )
            }
            Dependency::ExternalTool {
                name,
                check_command,
            } => {
                write!(f, "ExternalTool: {name} ({check_command})")
            }
            Dependency::Capability { id, required } => {
                write!(
                    f,
                    "Capability: {}{}",
                    id,
                    if *required { " (required)" } else { "" }
                )
            }
        }
    }
}

pub struct ConfigureResult {
    pub success: bool,
    pub artifacts: Vec<PathBuf>,
    pub messages: Vec<String>,
}

impl Default for ConfigureResult {
    fn default() -> Self {
        Self {
            success: true,
            artifacts: vec![],
            messages: vec![],
        }
    }
}

pub struct LaunchContext {
    pub tool_id: String,
    pub tool_version: String,
    pub working_dir: PathBuf,
    pub env_vars: HashMap<String, String>,
}

pub struct EnvBinding {
    pub key: String,
    pub value: String,
}

#[async_trait]
pub trait Factory: Send + Sync {
    async fn resolve_model(&self, id: &str) -> anyhow::Result<String>;
    async fn resolve_provider(&self, id: &str) -> anyhow::Result<String>;
    async fn resolve_capability(&self, id: &str) -> anyhow::Result<String>;
}

pub struct ToolConfig {
    pub tool_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub env_vars: HashMap<String, String>,
}

/*-- Factory Definition ------------------------------------------------------*/

use crate::define_factory;

define_factory!(Capability, CapabilityMetadata, CapabilityFactory);
