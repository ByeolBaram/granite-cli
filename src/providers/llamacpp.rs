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

/*-- llama.cpp Provider Configuration ----------------------------------------*/

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LlamaCppProviderConfig {
    /// Base URL for the llama.cpp server
    #[serde(default = "default_llamacpp_url")]
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
    #[serde(default = "default_llamacpp_health_endpoint")]
    pub health_check_endpoint: String,
}

fn default_llamacpp_url() -> String {
    "http://localhost:8080".to_string()
}

fn default_timeout() -> u64 {
    10
}

fn default_verify_ssl() -> bool {
    true
}

fn default_llamacpp_health_endpoint() -> String {
    "/health".to_string()
}

impl Default for LlamaCppProviderConfig {
    fn default() -> Self {
        Self {
            base_url: default_llamacpp_url(),
            api_key: None,
            timeout_secs: default_timeout(),
            verify_ssl: default_verify_ssl(),
            health_check_endpoint: default_llamacpp_health_endpoint(),
        }
    }
}

/*-- llama.cpp Provider Implementation ---------------------------------------*/

pub struct LlamaCppProvider {
    config: LlamaCppProviderConfig,
    client: reqwest::Client,
}

impl LlamaCppProvider {
    fn default_function_endpoints() -> HashMap<ModelFunction, Vec<ApiEndpoint>> {
        let mut map = HashMap::new();

        map.insert(ModelFunction::Chat, vec![
            ApiEndpoint::OpenAIChat,
            ApiEndpoint::AnthropicMessages,
        ]);

        map.insert(ModelFunction::Embeddings, vec![
            ApiEndpoint::OpenAIEmbeddings,
        ]);

        map
    }
}

impl ConfigConstructable for LlamaCppProvider {
    fn new(cfg: &serde_json::Value) -> Self {
        let config: LlamaCppProviderConfig = serde_json::from_value(cfg.clone())
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
impl Provider for LlamaCppProvider {
    fn name(&self) -> &str {
        "llama.cpp"
    }

    fn function_endpoints(&self) -> HashMap<ModelFunction, Vec<ApiEndpoint>> {
        Self::default_function_endpoints()
    }

    fn supported_api_types(&self) -> Vec<ApiType> {
        vec![ApiType::OpenAI, ApiType::Anthropic]
    }

    fn supported_formats(&self) -> Vec<ModelFormat> {
        vec![ModelFormat::GGUF]
    }

    fn can_run_model(&self, variant_format: &str, _variant_precision: &str) -> bool {
        variant_format.eq_ignore_ascii_case("gguf")
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

impl HasProviderMetadata for LlamaCppProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            name: "llama.cpp".to_string(),
            description: "High-performance local inference server for GGUF models with OpenAI and Anthropic API compatibility".to_string(),
            provider_type: ProviderType::Local,
            default_endpoint: "http://localhost:8080".to_string(),
            supported_api_types: vec![ApiType::OpenAI, ApiType::Anthropic],
            default_function_endpoints: Self::default_function_endpoints(),
            supported_formats: vec![ModelFormat::GGUF],
            authentication: vec![AuthType::None, AuthType::BearerToken],
            tags: vec![
                "llama.cpp".to_string(),
                "local".to_string(),
                "gguf".to_string(),
                "high-performance".to_string(),
            ],
        }
    }

    fn config_schema() -> schemars::Schema {
        schemars::schema_for!(LlamaCppProviderConfig)
    }

    fn default_config() -> serde_json::Value {
        serde_json::to_value(LlamaCppProviderConfig::default()).unwrap_or_default()
    }
}

/*-- tests -------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = LlamaCppProviderConfig::default();
        assert_eq!(config.base_url, "http://localhost:8080");
        assert!(config.api_key.is_none());
        assert_eq!(config.timeout_secs, 10);
        assert!(config.verify_ssl);
        assert_eq!(config.health_check_endpoint, "/health");
    }

    #[test]
    fn test_provider_metadata() {
        let meta = LlamaCppProvider::metadata();
        assert_eq!(meta.name, "llama.cpp");
        assert!(meta.supported_api_types.contains(&ApiType::OpenAI));
        assert!(meta.supported_api_types.contains(&ApiType::Anthropic));
        assert!(meta.default_function_endpoints.contains_key(&ModelFunction::Chat));
    }

    #[test]
    fn test_provider_constructs_from_json() {
        let cfg = serde_json::json!({
            "base_url": "http://example.com:9000",
            "timeout_secs": 30
        });
        let provider = LlamaCppProvider::new(&cfg);
        assert_eq!(provider.config.base_url, "http://example.com:9000");
        assert_eq!(provider.config.timeout_secs, 30);
    }

    #[test]
    fn test_can_run_model_accepts_gguf() {
        let provider = LlamaCppProvider::new(&serde_json::json!({}));
        assert!(provider.can_run_model("gguf", "Q4_K_M"));
        assert!(provider.can_run_model("GGUF", "fp16"));
    }

    #[test]
    fn test_can_run_model_rejects_non_gguf() {
        let provider = LlamaCppProvider::new(&serde_json::json!({}));
        assert!(!provider.can_run_model("safetensors", "fp16"));
        assert!(!provider.can_run_model("onnx", "fp32"));
    }
}
