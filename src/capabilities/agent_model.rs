//! The first concrete `Capability`: surfaces a configured model's connection
//! details (base URL, model name, auth, TLS) so a launcher can bind them into
//! an agent's environment.

use crate::capabilities::base::{
    AgentModelBinding, AgentModelBindingRequest, Binding, BindingRequest, BindingType, Capability,
    CapabilityMetadata, Dependency, HasCapabilityMetadata,
};
use crate::capabilities::requirement::ModelRequirement;
use crate::models::{Model, ModelFunction};
use crate::registry::ConfigConstructable;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use std::collections::HashSet;
use std::sync::Arc;

/*-- AgentModelCapabilityConfig ---------------------------------------------------*/

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema, Validate)]
pub struct AgentModelCapabilityConfig {
    #[validate(min_length = 1)]
    pub model_id: String,
}

/*-- AgentModelCapability ---------------------------------------------------------*/

pub struct AgentModelCapability {
    config: AgentModelCapabilityConfig,
    model: Arc<dyn Model>,
}

impl ConfigConstructable for AgentModelCapability {
    type Config = AgentModelCapabilityConfig;

    fn new(cfg: &serde_json::Value) -> Self {
        let config: AgentModelCapabilityConfig =
            serde_json::from_value(cfg.clone()).unwrap_or_default();
        let model = crate::models::MODEL_REGISTRY
            .construct(&config.model_id, &serde_json::json!({}))
            .expect("model must be in registry");
        Self {
            config,
            model: Arc::from(model),
        }
    }
}

impl AgentModelCapability {
    pub fn configured_model_id(&self) -> &str {
        &self.config.model_id
    }
}

#[async_trait]
impl Capability for AgentModelCapability {
    fn name(&self) -> &str {
        "Agent Model Binding"
    }

    fn description(&self) -> &str {
        "Surfaces a configured model's connection details (base URL, model name, auth, TLS) to a launched agent."
    }

    fn dependencies(&self) -> Vec<Dependency> {
        vec![Dependency::Model {
            config_key: "model_id".to_string(),
            requirement: ModelRequirement {
                supported_functions: vec![ModelFunction::Chat, ModelFunction::ToolCalling],
                ..Default::default()
            },
            resolved_id: Some(self.config.model_id.clone()),
            required: true,
        }]
    }

    fn binding_types(&self) -> HashSet<BindingType> {
        HashSet::from([BindingType::AgentModel])
    }

    async fn bind(&self, request: BindingRequest) -> anyhow::Result<Binding> {
        let api_type = match request {
            BindingRequest::AgentModel(AgentModelBindingRequest { api_type }) => api_type,
            #[allow(unreachable_patterns)] // Will remove once more variants are available
            other => anyhow::bail!(
                "AgentModelCapability does not handle {:?} binding requests",
                other.binding_type()
            ),
        };
        let model_id = &self.config.model_id;

        let provider = self
            .model
            .provider()
            .map_err(|e| anyhow::anyhow!("model '{model_id}' has no usable provider: {e}"))?;

        // Primary check -- which ApiType a launcher wants is only known here.
        anyhow::ensure!(
            provider.supported_api_types().contains(&api_type),
            "provider for model '{model_id}' does not support {api_type}"
        );
        // Defensive only: setup-time resolution already guarantees these.
        anyhow::ensure!(
            self.model
                .supported_functions()
                .contains(&ModelFunction::Chat),
            "model '{model_id}' does not support Chat"
        );
        anyhow::ensure!(
            provider.supports_function(&ModelFunction::Chat),
            "provider for model '{model_id}' does not support Chat"
        );

        let endpoint = provider
            .endpoints_for_function(&ModelFunction::Chat)
            .into_iter()
            .find(|e| e.api_type() == api_type)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "provider for model '{model_id}' has no {api_type} endpoint for Chat"
                )
            })?;

        Ok(Binding::AgentModel(AgentModelBinding {
            api_type,
            base_url: provider.base_url().to_string(),
            model_name: model_id.to_string(),
            endpoint_path: endpoint.path().to_string(),
            api_key: provider.api_key().cloned(),
            verify_ssl: provider.verify_ssl(),
        }))
    }
}

impl HasCapabilityMetadata for AgentModelCapability {
    fn metadata() -> CapabilityMetadata {
        CapabilityMetadata {
            name: "Agent Model Binding".to_string(),
            description: "Surfaces a configured model's connection details (base URL, model name, auth, TLS) to a launched agent.".to_string(),
            dependencies: vec![Dependency::Model {
                config_key: "model_id".to_string(),
                requirement: ModelRequirement {
                    supported_functions: vec![ModelFunction::Chat, ModelFunction::ToolCalling],
                    ..Default::default()
                },
                resolved_id: None,
                required: true,
            }],
            tags: vec!["agent".to_string(), "model".to_string()],
            supported_binding_types: HashSet::from([BindingType::AgentModel]),
        }
    }
}

/*-- tests -------------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{
        ApiEndpoint, ApiType, HealthStatus, ModelFormat, Provider, ProviderError,
    };
    use crate::registry::Secret;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[derive(Clone)]
    struct FakeProvider {
        base_url: String,
        api_key: Option<Secret>,
        verify_ssl: bool,
        api_types: Vec<ApiType>,
        endpoints: HashMap<ModelFunction, Vec<ApiEndpoint>>,
    }

    impl ConfigConstructable for FakeProvider {
        type Config = crate::registry::NoConfig;

        fn new(_cfg: &serde_json::Value) -> Self {
            unimplemented!("not used in tests")
        }
    }

    #[async_trait]
    impl Provider for FakeProvider {
        fn name(&self) -> &str {
            "Fake Provider"
        }
        fn function_endpoints(&self) -> HashMap<ModelFunction, Vec<ApiEndpoint>> {
            self.endpoints.clone()
        }
        fn supported_api_types(&self) -> Vec<ApiType> {
            self.api_types.clone()
        }
        fn base_url(&self) -> &str {
            &self.base_url
        }
        fn api_key(&self) -> Option<&Secret> {
            self.api_key.as_ref()
        }
        fn verify_ssl(&self) -> bool {
            self.verify_ssl
        }
        fn supported_formats(&self) -> Vec<ModelFormat> {
            vec![]
        }
        async fn health_check(&self) -> Result<HealthStatus, ProviderError> {
            unimplemented!("not used in tests")
        }
    }

    struct TestModel {
        supported_functions: Vec<ModelFunction>,
        provider: FakeProvider,
    }

    impl ConfigConstructable for TestModel {
        type Config = crate::registry::NoConfig;

        fn new(_cfg: &serde_json::Value) -> Self {
            unimplemented!("not used in tests")
        }
    }

    impl Model for TestModel {
        fn family(&self) -> &str {
            "Test"
        }
        fn version(&self) -> &str {
            "1.0"
        }
        fn size(&self) -> u64 {
            1
        }
        fn context_length(&self) -> u64 {
            4096
        }
        fn model_type(&self) -> &crate::models::ModelType {
            &crate::models::ModelType::Text
        }
        fn huggingface_repo(&self) -> &str {
            "test/test"
        }
        fn native_dtype(&self) -> &str {
            "bfloat16"
        }
        fn architecture(&self) -> &crate::models::ModelArchitecture {
            unimplemented!("not used in tests")
        }
        fn variants(&self) -> &[crate::models::ModelVariant] {
            &[]
        }
        fn description(&self) -> Option<&str> {
            None
        }
        fn tags(&self) -> &[String] {
            &[]
        }
        fn supported_functions(&self) -> &[ModelFunction] {
            &self.supported_functions
        }
        fn provider(&self) -> anyhow::Result<Box<dyn Provider>> {
            Ok(Box::new(self.provider.clone()))
        }
    }

    fn ok_provider(
        api_types: Vec<ApiType>,
        function: ModelFunction,
        endpoint: ApiEndpoint,
    ) -> FakeProvider {
        let mut endpoints = HashMap::new();
        endpoints.insert(function, vec![endpoint]);
        FakeProvider {
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            verify_ssl: true,
            api_types,
            endpoints,
        }
    }

    fn capability_with_model(
        functions: Vec<ModelFunction>,
        provider: FakeProvider,
    ) -> AgentModelCapability {
        let mut cap = AgentModelCapability::new(&serde_json::json!({
            "model_id": "granite-3.1-8b-instruct",
        }));
        cap.model = Arc::new(TestModel {
            supported_functions: functions,
            provider,
        });
        cap
    }

    #[tokio::test]
    async fn bind_succeeds_for_matching_provider_and_model() {
        let cap = capability_with_model(
            vec![ModelFunction::Chat],
            ok_provider(
                vec![ApiType::OpenAI],
                ModelFunction::Chat,
                ApiEndpoint::OpenAIChat,
            ),
        );

        let binding = cap
            .bind(BindingRequest::AgentModel(AgentModelBindingRequest {
                api_type: ApiType::OpenAI,
            }))
            .await
            .unwrap();

        let Binding::AgentModel(binding) = binding;
        assert_eq!(binding.base_url, "http://localhost:11434");
        assert_eq!(binding.model_name, "granite-3.1-8b-instruct");
        assert_eq!(binding.endpoint_path, "/v1/chat/completions");
        assert_eq!(binding.api_type, ApiType::OpenAI);
        assert!(binding.verify_ssl);
    }

    #[tokio::test]
    async fn bind_fails_when_model_has_no_provider() {
        let cap = capability_with_model(
            vec![ModelFunction::Chat],
            FakeProvider {
                base_url: "http://localhost:11434".to_string(),
                api_key: None,
                verify_ssl: true,
                api_types: vec![ApiType::OpenAI],
                endpoints: HashMap::new(),
            },
        );

        let err = cap
            .bind(BindingRequest::AgentModel(AgentModelBindingRequest {
                api_type: ApiType::OpenAI,
            }))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("does not support"));
    }

    #[tokio::test]
    async fn bind_fails_when_provider_lacks_api_type() {
        let cap = capability_with_model(
            vec![ModelFunction::Chat],
            ok_provider(
                vec![ApiType::Ollama],
                ModelFunction::Chat,
                ApiEndpoint::OllamaChat,
            ),
        );

        let err = cap
            .bind(BindingRequest::AgentModel(AgentModelBindingRequest {
                api_type: ApiType::OpenAI,
            }))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("does not support"));
    }

    #[tokio::test]
    async fn bind_fails_when_model_lacks_function() {
        let cap = capability_with_model(
            vec![ModelFunction::Embeddings],
            ok_provider(
                vec![ApiType::OpenAI],
                ModelFunction::Chat,
                ApiEndpoint::OpenAIChat,
            ),
        );

        let err = cap
            .bind(BindingRequest::AgentModel(AgentModelBindingRequest {
                api_type: ApiType::OpenAI,
            }))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("does not support"));
    }

    #[tokio::test]
    async fn bind_fails_when_no_matching_endpoint() {
        let cap = capability_with_model(
            vec![ModelFunction::Chat],
            ok_provider(
                vec![ApiType::OpenAI, ApiType::Ollama],
                ModelFunction::Chat,
                ApiEndpoint::OllamaChat,
            ),
        );

        let err = cap
            .bind(BindingRequest::AgentModel(AgentModelBindingRequest {
                api_type: ApiType::OpenAI,
            }))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no OpenAI endpoint for Chat"));
    }

    #[test]
    fn binding_types_reports_agent_model() {
        let cap = AgentModelCapability::new(&serde_json::json!({
            "model_id": "granite-3.1-8b-instruct",
        }));
        assert_eq!(
            cap.binding_types(),
            HashSet::from([BindingType::AgentModel])
        );
    }

    #[test]
    fn dependencies_carry_resolved_model_id() {
        let cap = AgentModelCapability::new(&serde_json::json!({
            "model_id": "granite-3.1-8b-instruct",
        }));
        let deps = cap.dependencies();
        assert_eq!(deps.len(), 1);
        assert!(deps.iter().any(|d| matches!(
            d,
            Dependency::Model { resolved_id: Some(id), .. } if id == "granite-3.1-8b-instruct"
        )));
    }

    #[test]
    fn metadata_reports_supported_binding_types_and_wildcard_dependency() {
        let meta = AgentModelCapability::metadata();
        assert_eq!(
            meta.supported_binding_types,
            HashSet::from([BindingType::AgentModel])
        );
        assert!(meta.dependencies.iter().any(|d| matches!(
            d,
            Dependency::Model {
                resolved_id: None,
                ..
            }
        )));
    }
}
