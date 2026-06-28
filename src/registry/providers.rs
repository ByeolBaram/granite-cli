use crate::providers::{ApiSurface, ModelFormat, Precision};
use serde::{Deserialize, Serialize};

pub trait Registry<T> {
    fn list(&self) -> Vec<&T>;
    fn get(&self, id: &str) -> Option<&T>;
    fn search(&self, query: &str) -> Vec<&T>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub provider_type: ProviderType,
    pub default_endpoint: String,
    pub api_capabilities: Vec<ApiSurface>,
    pub supported_formats: Vec<ModelFormat>,
    pub supported_precisions: Vec<Precision>,
    pub authentication: Vec<AuthType>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    Hosted,
    Local,
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderType::Hosted => write!(f, "Hosted"),
            ProviderType::Local => write!(f, "Local"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthType {
    ApiKey,
    BearerToken,
    None,
}

impl std::fmt::Display for AuthType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthType::ApiKey => write!(f, "API Key"),
            AuthType::BearerToken => write!(f, "Bearer Token"),
            AuthType::None => write!(f, "None"),
        }
    }
}

impl std::fmt::Display for ProviderDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}): {} - {}",
            self.id, self.provider_type, self.name, self.description
        )
    }
}

pub struct ProviderRegistry {
    providers: Vec<ProviderDefinition>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: Self::bundled_providers(),
        }
    }

    fn bundled_providers() -> Vec<ProviderDefinition> {
        vec![
            ProviderDefinition {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                description: "OpenAI API with GPT-4, GPT-3.5, and other models.".to_string(),
                provider_type: ProviderType::Hosted,
                default_endpoint: "https://api.openai.com".to_string(),
                api_capabilities: vec![ApiSurface::OpenAIChat],
                supported_formats: vec![ModelFormat::Safetensors],
                supported_precisions: vec![Precision::BF16, Precision::FP16],
                authentication: vec![AuthType::ApiKey],
                tags: vec!["openai".to_string(), "gpt".to_string(), "hosted".to_string()],
            },
            ProviderDefinition {
                id: "anthropic".to_string(),
                name: "Anthropic".to_string(),
                description: "Anthropic API with Claude models for safe and helpful AI.".to_string(),
                provider_type: ProviderType::Hosted,
                default_endpoint: "https://api.anthropic.com".to_string(),
                api_capabilities: vec![ApiSurface::AnthropicMessages],
                supported_formats: vec![ModelFormat::Safetensors],
                supported_precisions: vec![Precision::BF16, Precision::FP16],
                authentication: vec![AuthType::ApiKey],
                tags: vec!["anthropic".to_string(), "claude".to_string(), "hosted".to_string()],
            },
            ProviderDefinition {
                id: "ollama".to_string(),
                name: "Ollama".to_string(),
                description: "Local model serving via Ollama. Supports many open-source models.".to_string(),
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
                tags: vec!["ollama".to_string(), "local".to_string(), "gguf".to_string()],
            },
            ProviderDefinition {
                id: "watsonx".to_string(),
                name: "IBM watsonx.ai".to_string(),
                description: "IBM's enterprise AI platform with Granite and other models.".to_string(),
                provider_type: ProviderType::Hosted,
                default_endpoint: "https://watsonx.ai".to_string(),
                api_capabilities: vec![ApiSurface::OpenAIChat],
                supported_formats: vec![ModelFormat::Safetensors],
                supported_precisions: vec![Precision::BF16, Precision::FP16],
                authentication: vec![AuthType::ApiKey],
                tags: vec!["ibm".to_string(), "watsonx".to_string(), "granite".to_string(), "hosted".to_string()],
            },
        ]
    }
}

impl Registry<ProviderDefinition> for ProviderRegistry {
    fn list(&self) -> Vec<&ProviderDefinition> {
        self.providers.iter().collect()
    }

    fn get(&self, id: &str) -> Option<&ProviderDefinition> {
        self.providers.iter().find(|p| p.id == id)
    }

    fn search(&self, query: &str) -> Vec<&ProviderDefinition> {
        let query_lower = query.to_lowercase();
        self.providers
            .iter()
            .filter(|p| {
                p.id.to_lowercase().contains(&query_lower)
                    || p.name.to_lowercase().contains(&query_lower)
                    || p.description.to_lowercase().contains(&query_lower)
                    || p.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ApiSurface;

    #[test]
    fn test_provider_registry_has_bundled_providers() {
        let registry = ProviderRegistry::new();
        let providers = registry.list();
        assert!(providers.len() >= 4);
    }

    #[test]
    fn test_provider_registry_get_by_id() {
        let registry = ProviderRegistry::new();
        let provider = registry.get("openai");
        assert!(provider.is_some());
        let provider = provider.unwrap();
        assert_eq!(provider.name, "OpenAI");
        assert_eq!(provider.provider_type, ProviderType::Hosted);
    }

    #[test]
    fn test_provider_registry_get_ollama() {
        let registry = ProviderRegistry::new();
        let provider = registry.get("ollama");
        assert!(provider.is_some());
        let provider = provider.unwrap();
        assert_eq!(provider.provider_type, ProviderType::Local);
        assert_eq!(provider.default_endpoint, "http://localhost:11434");
    }

    #[test]
    fn test_provider_registry_get_anthropic() {
        let registry = ProviderRegistry::new();
        let provider = registry.get("anthropic");
        assert!(provider.is_some());
        let provider = provider.unwrap();
        assert!(provider.api_capabilities.contains(&ApiSurface::AnthropicMessages));
    }

    #[test]
    fn test_provider_registry_get_not_found() {
        let registry = ProviderRegistry::new();
        let provider = registry.get("nonexistent-provider");
        assert!(provider.is_none());
    }

    #[test]
    fn test_provider_registry_search() {
        let registry = ProviderRegistry::new();
        let results = registry.search("local");
        assert!(results.len() > 0);
        assert!(results.iter().all(|p| p.tags.contains(&"local".to_string())));
    }

    #[test]
    fn test_provider_registry_search_name() {
        let registry = ProviderRegistry::new();
        let results = registry.search("claude");
        assert!(results.len() > 0);
    }

    #[test]
    fn test_provider_type_display() {
        assert_eq!(ProviderType::Hosted.to_string(), "Hosted");
        assert_eq!(ProviderType::Local.to_string(), "Local");
    }

    #[test]
    fn test_auth_type_display() {
        assert_eq!(AuthType::ApiKey.to_string(), "API Key");
        assert_eq!(AuthType::None.to_string(), "None");
    }

    #[test]
    fn test_provider_api_capabilities() {
        let registry = ProviderRegistry::new();
        let openai = registry.get("openai").unwrap();
        assert!(openai.api_capabilities.contains(&ApiSurface::OpenAIChat));

        let anthropic = registry.get("anthropic").unwrap();
        assert!(anthropic.api_capabilities.contains(&ApiSurface::AnthropicMessages));

        let ollama = registry.get("ollama").unwrap();
        assert!(ollama.api_capabilities.contains(&ApiSurface::OllamaChat));
    }
}
