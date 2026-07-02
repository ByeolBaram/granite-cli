pub mod base;
mod anthropic;
mod ollama;
mod openai_compat;

// Re-export provider implementations
pub use anthropic::AnthropicProvider;
pub use ollama::OllamaProvider;
pub use openai_compat::OpenAiCompatProvider;

use base::ProviderConfig;
use crate::registry::ConfigConstructable;
use std::collections::HashMap;
use std::sync::LazyLock;

/*-- Public API --------------------------------------------------------------*/

// Re-export types from base
pub use base::{
    ApiSurface, AuthType, ChatChunk, ChatMessage, ChatRequest, ChatResponse, HealthStatus,
    MessageRole, ModelFormat, Precision, Provider, ProviderError, ProviderType, UsageInfo,
};
pub use base::ProviderMetadata;
pub use base::ProviderMetadata as ProviderDefinition; // Backward compatibility alias

/*-- Provider Configuration Storage ------------------------------------------*/

/// Global storage for provider configurations.
static PROVIDER_CONFIGS: LazyLock<HashMap<String, ProviderConfig>> = LazyLock::new(|| {
    load_bundled_providers()
        .into_iter()
        .map(|cfg| (cfg.id.clone(), cfg))
        .collect()
});

/*-- Custom Metadata Provider for Providers ---------------------------------*/

/// Custom metadata provider that looks up config by ID
struct ProviderMetadataProvider {
    id: String,
}

impl ProviderMetadataProvider {
    fn new(id: String) -> Self {
        Self { id }
    }
}

// We need to manually implement the internal trait
trait ProviderMetadataProviderTrait: Send + Sync {
    fn describe(&self) -> ProviderMetadata;
    fn construct(&self, cfg: &ProviderConfig) -> Box<dyn base::Provider<Config = ProviderConfig>>;
}

impl ProviderMetadataProviderTrait for ProviderMetadataProvider {
    fn describe(&self) -> ProviderMetadata {
        PROVIDER_CONFIGS
            .get(&self.id)
            .map(config_to_metadata)
            .unwrap_or_else(|| ProviderMetadata {
                id: self.id.clone(),
                name: String::new(),
                description: String::new(),
                provider_type: ProviderType::Hosted,
                default_endpoint: String::new(),
                api_capabilities: vec![],
                supported_formats: vec![],
                supported_precisions: vec![],
                authentication: vec![],
                tags: vec![],
            })
    }

    fn construct(&self, _cfg: &ProviderConfig) -> Box<dyn base::Provider<Config = ProviderConfig>> {
        let config = PROVIDER_CONFIGS.get(&self.id).expect("Provider config not found");

        // Route to the correct provider implementation based on ID
        match self.id.as_str() {
            "openai" | "watsonx" => Box::new(OpenAiCompatProvider::new(config)),
            "ollama" => Box::new(OllamaProvider::new(config)),
            "anthropic" => Box::new(AnthropicProvider::new(config)),
            _ => panic!("Unknown provider: {}", self.id),
        }
    }
}

/// Convert a ProviderConfig to ProviderMetadata
fn config_to_metadata(config: &ProviderConfig) -> ProviderMetadata {
    ProviderMetadata {
        id: config.id.clone(),
        name: config.name.clone(),
        description: config.description.clone(),
        provider_type: config.provider_type.clone(),
        default_endpoint: config.default_endpoint.clone(),
        api_capabilities: config.api_capabilities.clone(),
        supported_formats: config.supported_formats.clone(),
        supported_precisions: config.supported_precisions.clone(),
        authentication: config.authentication.clone(),
        tags: config.tags.clone(),
    }
}

/*-- Custom Provider Factory -------------------------------------------------*/

/// Custom provider factory that uses our metadata provider
pub struct CustomProviderFactory {
    registry: HashMap<String, Box<dyn ProviderMetadataProviderTrait>>,
}

impl CustomProviderFactory {
    fn new() -> Self {
        Self {
            registry: HashMap::new(),
        }
    }

    fn register(&mut self, id: String) {
        self.registry
            .insert(id.clone(), Box::new(ProviderMetadataProvider::new(id)));
    }

    pub(crate) fn construct(
        &self,
        name: &str,
        cfg: &ProviderConfig,
    ) -> Result<Box<dyn base::Provider<Config = ProviderConfig>>, String> {
        self.registry
            .get(name)
            .map(|x| x.construct(cfg))
            .ok_or_else(|| format!("Unknown provider: {}", name))
    }

    pub(crate) fn get(&self, name: &str) -> Option<ProviderMetadata> {
        self.registry.get(name).map(|x| x.describe())
    }

    pub(crate) fn list(&self) -> HashMap<&str, ProviderMetadata> {
        self.registry
            .iter()
            .map(|(k, v)| (k.as_str(), v.describe()))
            .collect()
    }
}

/*-- Factory Registration ----------------------------------------------------*/

/// Global provider factory with all providers registered.
pub static PROVIDER_FACTORY: LazyLock<CustomProviderFactory> = LazyLock::new(|| {
    let mut factory = CustomProviderFactory::new();

    // Register each provider by ID
    for id in PROVIDER_CONFIGS.keys() {
        factory.register(id.clone());
    }

    factory
});

/*-- Provider Loading --------------------------------------------------------*/

fn load_bundled_providers() -> Vec<ProviderConfig> {
    vec![
        ProviderConfig {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            description: "OpenAI API with GPT-4, GPT-3.5, and other models.".to_string(),
            provider_type: ProviderType::Hosted,
            default_endpoint: "https://api.openai.com".to_string(),
            api_capabilities: vec![ApiSurface::OpenAIChat],
            supported_formats: vec![ModelFormat::Safetensors],
            supported_precisions: vec![Precision::BF16, Precision::FP16],
            authentication: vec![AuthType::ApiKey],
            tags: vec![
                "openai".to_string(),
                "gpt".to_string(),
                "hosted".to_string(),
            ],
        },
        ProviderConfig {
            id: "anthropic".to_string(),
            name: "Anthropic".to_string(),
            description: "Anthropic API with Claude models for safe and helpful AI.".to_string(),
            provider_type: ProviderType::Hosted,
            default_endpoint: "https://api.anthropic.com".to_string(),
            api_capabilities: vec![ApiSurface::AnthropicMessages],
            supported_formats: vec![ModelFormat::Safetensors],
            supported_precisions: vec![Precision::BF16, Precision::FP16],
            authentication: vec![AuthType::ApiKey],
            tags: vec![
                "anthropic".to_string(),
                "claude".to_string(),
                "hosted".to_string(),
            ],
        },
        ProviderConfig {
            id: "ollama".to_string(),
            name: "Ollama".to_string(),
            description: "Local model serving via Ollama. Supports many open-source models."
                .to_string(),
            provider_type: ProviderType::Local,
            default_endpoint: "http://localhost:11434".to_string(),
            api_capabilities: vec![ApiSurface::OllamaChat],
            supported_formats: vec![ModelFormat::GGUF],
            supported_precisions: vec![
                Precision::Q8_0,
                Precision::Q4_K_M,
                Precision::Q5_K_M,
                Precision::Q3_K_M,
            ],
            authentication: vec![AuthType::None],
            tags: vec![
                "ollama".to_string(),
                "local".to_string(),
                "gguf".to_string(),
            ],
        },
        ProviderConfig {
            id: "watsonx".to_string(),
            name: "IBM watsonx.ai".to_string(),
            description: "IBM's enterprise AI platform with Granite and other models.".to_string(),
            provider_type: ProviderType::Hosted,
            default_endpoint: "https://watsonx.ai".to_string(),
            api_capabilities: vec![ApiSurface::OpenAIChat],
            supported_formats: vec![ModelFormat::Safetensors],
            supported_precisions: vec![Precision::BF16, Precision::FP16],
            authentication: vec![AuthType::ApiKey],
            tags: vec![
                "ibm".to_string(),
                "watsonx".to_string(),
                "granite".to_string(),
                "hosted".to_string(),
            ],
        },
    ]
}

/*-- tests -------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_factory_loads_providers() {
        let list = PROVIDER_FACTORY.list();
        assert!(list.len() >= 4, "Should have at least 4 providers");
    }

    #[test]
    fn test_provider_factory_get_metadata() {
        let metadata = PROVIDER_FACTORY.get("openai");
        assert!(metadata.is_some());

        let meta = metadata.unwrap();
        assert_eq!(meta.name, "OpenAI");
        assert_eq!(meta.provider_type, ProviderType::Hosted);
    }

    #[test]
    fn test_provider_factory_list_all() {
        let list = PROVIDER_FACTORY.list();

        assert!(list.contains_key("openai"));
        assert!(list.contains_key("anthropic"));
        assert!(list.contains_key("ollama"));
        assert!(list.contains_key("watsonx"));
    }

    #[test]
    fn test_provider_factory_construct() {
        let config = PROVIDER_CONFIGS.get("ollama").unwrap();
        let provider = PROVIDER_FACTORY.construct("ollama", config).unwrap();

        assert_eq!(provider.id(), "ollama");
        assert_eq!(provider.name(), "Ollama");
    }
}