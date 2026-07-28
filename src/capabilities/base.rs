use crate::models::ModelFunction;
use crate::providers::Provider;
use crate::registry::ConfigConstructable;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
                write!(f, "ExternalTool: {} ({})", name, check_command)
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

define_factory!( Capability, CapabilityMetadata, CapabilityFactory);

/*-- Capability Matching Logic ------------------------------------------------*/

/// Check if a provider instance can serve a model for a specific function
pub fn can_provider_serve_model_function(
    provider: &dyn Provider,
    model: &dyn crate::models::Model,
    function: &ModelFunction,
) -> bool {
    if !model.supported_functions().contains(function) {
        return false;
    }
    provider.supports_function(function)
}

/// Check if a provider instance can serve a model for any function
pub fn can_provider_serve_model(
    provider: &dyn Provider,
    model: &dyn crate::models::Model,
) -> bool {
    let provider_eps = provider.function_endpoints();
    let provider_functions: HashSet<_> = provider_eps.keys().collect();
    let model_functions: HashSet<_> = model.supported_functions().iter().collect();
    !provider_functions.is_disjoint(&model_functions)
}

/// Get the functions a provider can serve for a model
pub fn get_servable_functions(
    provider: &dyn Provider,
    model: &dyn crate::models::Model,
) -> Vec<ModelFunction> {
    let provider_functions: HashSet<_> = provider.function_endpoints().keys().cloned().collect();
    let model_functions: HashSet<_> = model.supported_functions().iter().cloned().collect();
    provider_functions.intersection(&model_functions).cloned().collect()
}

/*-- tests -------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ApiEndpoint, ApiType, HealthStatus, ModelFormat, ProviderError};
    use crate::models::{ModelType, ModelVariant};
    use std::collections::HashMap;

    struct TestProvider {
        functions: HashMap<ModelFunction, Vec<ApiEndpoint>>,
    }

    impl crate::registry::ConfigConstructable for TestProvider {
        fn new(_cfg: &serde_json::Value) -> Self {
            Self {
                functions: HashMap::new(),
            }
        }
    }

    #[async_trait]
    impl Provider for TestProvider {
        fn name(&self) -> &str { "Test" }
        fn function_endpoints(&self) -> HashMap<ModelFunction, Vec<ApiEndpoint>> {
            self.functions.clone()
        }
        fn supported_api_types(&self) -> Vec<ApiType> { vec![] }
        fn supported_formats(&self) -> Vec<ModelFormat> { vec![] }
        fn supported_precisions(&self) -> Vec<String> { vec![] }
        async fn health_check(&self) -> Result<HealthStatus, ProviderError> {
            Ok(HealthStatus {
                healthy: true,
                latency: std::time::Duration::from_millis(0),
                error: None,
            })
        }
    }

    struct TestModel {
        functions: Vec<ModelFunction>,
    }

    impl crate::registry::ConfigConstructable for TestModel {
        fn new(_cfg: &serde_json::Value) -> Self {
            Self { functions: vec![] }
        }
    }

    impl crate::models::Model for TestModel {
        fn family(&self) -> &str { "test" }
        fn version(&self) -> &str { "1.0" }
        fn size(&self) -> u64 { 1000 }
        fn context_length(&self) -> u64 { 4096 }
        fn model_type(&self) -> &ModelType { &ModelType::Text }
        fn huggingface_repo(&self) -> &str { "test/repo" }
        fn variants(&self) -> &[ModelVariant] { &[] }
        fn description(&self) -> Option<&str> { None }
        fn tags(&self) -> &[String] { &[] }
        fn supported_functions(&self) -> &[ModelFunction] { &self.functions }
    }

    #[test]
    fn test_provider_serves_function() {
        let provider = TestProvider {
            functions: HashMap::from([
                (ModelFunction::Chat, vec![ApiEndpoint::OpenAIChat]),
                (ModelFunction::Embeddings, vec![ApiEndpoint::OpenAIEmbeddings]),
            ]),
        };

        let model = TestModel {
            functions: vec![ModelFunction::Chat, ModelFunction::Thinking],
        };

        assert!(can_provider_serve_model_function(&provider, &model, &ModelFunction::Chat));
        assert!(!can_provider_serve_model_function(&provider, &model, &ModelFunction::Embeddings));
        assert!(!can_provider_serve_model_function(&provider, &model, &ModelFunction::Transcription));
    }

    #[test]
    fn test_can_serve_any_function() {
        let provider = TestProvider {
            functions: HashMap::from([
                (ModelFunction::Chat, vec![ApiEndpoint::OpenAIChat]),
            ]),
        };

        let model1 = TestModel {
            functions: vec![ModelFunction::Chat],
        };
        let model2 = TestModel {
            functions: vec![ModelFunction::Transcription],
        };

        assert!(can_provider_serve_model(&provider, &model1));
        assert!(!can_provider_serve_model(&provider, &model2));
    }

    #[test]
    fn test_get_servable_functions() {
        let provider = TestProvider {
            functions: HashMap::from([
                (ModelFunction::Chat, vec![ApiEndpoint::OpenAIChat]),
                (ModelFunction::Embeddings, vec![ApiEndpoint::OpenAIEmbeddings]),
            ]),
        };

        let model = TestModel {
            functions: vec![
                ModelFunction::Chat,
                ModelFunction::Thinking,
                ModelFunction::Embeddings,
            ],
        };

        let servable = get_servable_functions(&provider, &model);
        assert_eq!(servable.len(), 2);
        assert!(servable.contains(&ModelFunction::Chat));
        assert!(servable.contains(&ModelFunction::Embeddings));
    }
}

