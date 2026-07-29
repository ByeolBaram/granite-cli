use crate::models::ModelFunction;
use crate::providers::base::{
    http_health_check, ApiEndpoint, ApiType, AuthType, HealthStatus,
    ModelFormat, Provider, ProviderError, ProviderMetadata, ProviderType,
    HasProviderMetadata,
};
use crate::registry::{ConfigConstructable, Secret};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/*-- Ollama Provider Configuration -------------------------------------------*/

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OllamaProviderConfig {
    /// Base URL for the Ollama API
    #[serde(default = "default_ollama_url")]
    pub base_url: String,

    /// API key for authentication (optional)
    pub api_key: Option<Secret>,

    /// Timeout for health checks in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Whether to verify SSL certificates
    #[serde(default = "default_verify_ssl")]
    pub verify_ssl: bool,

    /// Endpoint to use for health checks
    #[serde(default = "default_ollama_health_endpoint")]
    pub health_check_endpoint: String,
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_timeout() -> u64 {
    10
}

fn default_verify_ssl() -> bool {
    true
}

fn default_ollama_health_endpoint() -> String {
    "/api/tags".to_string()
}

impl Default for OllamaProviderConfig {
    fn default() -> Self {
        Self {
            base_url: default_ollama_url(),
            api_key: None,
            timeout_secs: default_timeout(),
            verify_ssl: default_verify_ssl(),
            health_check_endpoint: default_ollama_health_endpoint(),
        }
    }
}

/*-- Ollama Provider Implementation ------------------------------------------*/

pub struct OllamaProvider {
    config: OllamaProviderConfig,
    client: reqwest::Client,
}

impl OllamaProvider {
    fn default_function_endpoints() -> HashMap<ModelFunction, Vec<ApiEndpoint>> {
        let mut map = HashMap::new();

        map.insert(ModelFunction::Chat, vec![
            ApiEndpoint::OpenAIChat,
            ApiEndpoint::OllamaChat,
            ApiEndpoint::AnthropicMessages,
        ]);

        map.insert(ModelFunction::Embeddings, vec![
            ApiEndpoint::OpenAIEmbeddings,
            ApiEndpoint::OllamaEmbeddings,
        ]);

        map
    }
}

impl ConfigConstructable for OllamaProvider {
    fn new(cfg: &serde_json::Value) -> Self {
        let config: OllamaProviderConfig = serde_json::from_value(cfg.clone())
            .unwrap_or_default();

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .danger_accept_invalid_certs(!config.verify_ssl)
            .build()
            .expect("Failed to create HTTP client");

        Self { config, client }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &str {
        "Ollama"
    }

    fn function_endpoints(&self) -> HashMap<ModelFunction, Vec<ApiEndpoint>> {
        Self::default_function_endpoints()
    }

    fn supported_api_types(&self) -> Vec<ApiType> {
        vec![ApiType::OpenAI, ApiType::Ollama, ApiType::Anthropic]
    }

    fn supported_formats(&self) -> Vec<ModelFormat> {
        vec![ModelFormat::GGUF]
    }

    fn can_run_model(&self, variant_format: &str, _variant_precision: &str) -> bool {
        variant_format.eq_ignore_ascii_case("gguf") || variant_format.eq_ignore_ascii_case("ollama")
    }

    async fn health_check(&self) -> Result<HealthStatus, ProviderError> {
        http_health_check(
            &self.client,
            &self.config.base_url,
            &self.config.health_check_endpoint,
            self.config.api_key.as_ref(),
        ).await
    }
}

impl HasProviderMetadata for OllamaProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            name: "Ollama".to_string(),
            description: "Local inference server supporting multiple API protocols and GGUF models".to_string(),
            provider_type: ProviderType::Local,
            default_endpoint: "http://localhost:11434".to_string(),
            supported_api_types: vec![ApiType::OpenAI, ApiType::Ollama, ApiType::Anthropic],
            default_function_endpoints: Self::default_function_endpoints(),
            supported_formats: vec![ModelFormat::GGUF, ModelFormat::Ollama],
            authentication: vec![AuthType::None, AuthType::BearerToken],
            tags: vec![
                "ollama".to_string(),
                "local".to_string(),
                "gguf".to_string(),
                "multi-api".to_string(),
            ],
        }
    }

    fn config_schema() -> schemars::Schema {
        schemars::schema_for!(OllamaProviderConfig)
    }

    fn default_config() -> serde_json::Value {
        serde_json::to_value(OllamaProviderConfig::default()).unwrap_or_default()
    }
}

/*-- tests -------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = OllamaProviderConfig::default();
        assert_eq!(config.base_url, "http://localhost:11434");
        assert!(config.api_key.is_none());
        assert_eq!(config.timeout_secs, 10);
        assert!(config.verify_ssl);
        assert_eq!(config.health_check_endpoint, "/api/tags");
    }

    #[test]
    fn test_provider_metadata() {
        let meta = OllamaProvider::metadata();
        assert_eq!(meta.name, "Ollama");
        assert!(meta.supported_api_types.contains(&ApiType::OpenAI));
        assert!(meta.supported_api_types.contains(&ApiType::Ollama));
        assert!(meta.supported_api_types.contains(&ApiType::Anthropic));
        assert!(meta.default_function_endpoints.contains_key(&ModelFunction::Chat));
    }

    #[test]
    fn test_provider_constructs_from_json() {
        let cfg = serde_json::json!({
            "base_url": "http://example.com:8080",
            "timeout_secs": 30
        });
        let provider = OllamaProvider::new(&cfg);
        assert_eq!(provider.config.base_url, "http://example.com:8080");
        assert_eq!(provider.config.timeout_secs, 30);
    }

    #[test]
    fn test_can_run_model_accepts_gguf() {
        let provider = OllamaProvider::new(&serde_json::json!({}));
        assert!(provider.can_run_model("gguf", "Q4_K_M"));
        assert!(provider.can_run_model("GGUF", "fp16"));
    }

    #[test]
    fn test_can_run_model_rejects_non_gguf() {
        let provider = OllamaProvider::new(&serde_json::json!({}));
        assert!(!provider.can_run_model("safetensors", "fp16"));
        assert!(!provider.can_run_model("onnx", "fp32"));
    }
}
