pub mod docling;
pub mod vision;
pub mod speech;
pub mod compiler;

pub use docling::DoclingCapability;
pub use vision::VisionCapability;
pub use speech::SpeechCapability;
pub use compiler::CompilerCapability;

use async_trait::async_trait;
use std::path::PathBuf;
use std::collections::HashMap;
use anyhow::Result;

use crate::registry;
use crate::registry::Registry;

#[async_trait]
pub trait Capability: Send + Sync {
    // Metadata
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn dependencies(&self) -> Vec<Dependency>;

    // Execution hooks (all optional with NoOp defaults)
    async fn on_setup(&self, _factory: &dyn Factory) -> Result<()> { Ok(()) }
    async fn on_configure(&self, _tool: &ToolConfig) -> Result<ConfigureResult> {
        Ok(ConfigureResult::default())
    }
    async fn on_pre_launch(&self, _context: &LaunchContext) -> Result<()> { Ok(()) }
    async fn on_post_launch(&self, _context: &LaunchContext) -> Result<()> { Ok(()) }
    async fn on_shutdown(&self, _context: &LaunchContext) -> Result<()> { Ok(()) }
    fn runtime_bindings(&self) -> Vec<EnvBinding> { vec![] }
}

/// Resolve a capability instance from the static registry.
pub fn resolve_capability_from_registry(id: &str) -> Result<Box<dyn Capability>> {
    registry::CAPABILITY_REGISTRY
        .get(id)
        .ok_or_else(|| anyhow::anyhow!("Capability '{}' not found in registry", id))?;

    let capability: Box<dyn Capability> = match id {
        "docling" => Box::new(DoclingCapability::new()),
        "vision" => Box::new(VisionCapability::new()),
        "speech" => Box::new(SpeechCapability::new()),
        "compiler" => Box::new(CompilerCapability::new()),
        _ => anyhow::bail!("No implementation registered for capability '{}'", id),
    };

    Ok(capability)
}

#[derive(Clone)]
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
                write!(f, "Model: {}{}", id, if *required { " (required)" } else { "" })
            }
            Dependency::Provider { id, required } => {
                write!(f, "Provider: {}{}", id, if *required { " (required)" } else { "" })
            }
            Dependency::ExternalTool { name, check_command } => {
                write!(f, "ExternalTool: {} ({})", name, check_command)
            }
            Dependency::Capability { id, required } => {
                write!(f, "Capability: {}{}", id, if *required { " (required)" } else { "" })
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
    async fn resolve_model(&self, id: &str) -> Result<String>;
    async fn resolve_provider(&self, id: &str) -> Result<String>;
    async fn resolve_capability(&self, id: &str) -> Result<String>;
}

pub struct ToolConfig {
    pub tool_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub env_vars: HashMap<String, String>,
}
