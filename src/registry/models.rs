use crate::registry::Registry;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModelType {
    Text,
    Vision,
    Speech,
    Embedding,
}

impl std::fmt::Display for ModelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelType::Text => write!(f, "Text"),
            ModelType::Vision => write!(f, "Vision"),
            ModelType::Speech => write!(f, "Speech"),
            ModelType::Embedding => write!(f, "Embedding"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVariant {
    pub format: String,
    pub precision: String,
    pub size_gb: f64,
    pub huggingface_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDefinition {
    pub id: String,
    pub family: String,
    pub version: String,
    pub size: u64,
    pub context_length: u64,
    pub model_type: ModelType,
    pub huggingface_repo: String,
    pub required_provider_capabilities: Vec<String>,
    pub variants: Vec<ModelVariant>,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

impl std::fmt::Display for ModelDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}) - {}B params, {} context, Type: {}",
            self.id, self.family, self.size / 1_000_000_000, self.context_length, self.model_type
        )
    }
}

pub struct ModelRegistry {
    models: Vec<ModelDefinition>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        let models = Self::bundled_models();
        Self { models }
    }

    fn bundled_models() -> Vec<ModelDefinition> {
        vec![
            // Granite 3.1 family
            ModelDefinition {
                id: "granite-3.1-3b-instruct".to_string(),
                family: "Granite".to_string(),
                version: "3.1".to_string(),
                size: 3_460_000_000,
                context_length: 8_192,
                model_type: ModelType::Text,
                huggingface_repo: "ibm-granite/granite-3.1-3b-instruct".to_string(),
                required_provider_capabilities: vec!["OpenAIChat".to_string(), "OllamaChat".to_string()],
                variants: vec![
                    ModelVariant {
                        format: "GGUF".to_string(),
                        precision: "Q4_K_M".to_string(),
                        size_gb: 1.9,
                        huggingface_path: "ibm-granite/granite-3.1-3b-instruct-GGUF/granite-3.1-3b-instruct-Q4_K_M.gguf".to_string(),
                    },
                    ModelVariant {
                        format: "GGUF".to_string(),
                        precision: "Q8_0".to_string(),
                        size_gb: 3.4,
                        huggingface_path: "ibm-granite/granite-3.1-3b-instruct-GGUF/granite-3.1-3b-instruct-Q8_0.gguf".to_string(),
                    },
                    ModelVariant {
                        format: "safetensors".to_string(),
                        precision: "BF16".to_string(),
                        size_gb: 6.6,
                        huggingface_path: "ibm-granite/granite-3.1-3b-instruct".to_string(),
                    },
                ],
                description: Some("Granite 3.1 3B instruct-tuned model for text generation.".to_string()),
                tags: vec!["instruct".to_string(), "text".to_string(), "efficient".to_string()],
            },
            ModelDefinition {
                id: "granite-3.1-8b-instruct".to_string(),
                family: "Granite".to_string(),
                version: "3.1".to_string(),
                size: 8_290_000_000,
                context_length: 8_192,
                model_type: ModelType::Text,
                huggingface_repo: "ibm-granite/granite-3.1-8b-instruct".to_string(),
                required_provider_capabilities: vec!["OpenAIChat".to_string(), "OllamaChat".to_string()],
                variants: vec![
                    ModelVariant {
                        format: "GGUF".to_string(),
                        precision: "Q4_K_M".to_string(),
                        size_gb: 4.9,
                        huggingface_path: "ibm-granite/granite-3.1-8b-instruct-GGUF/granite-3.1-8b-instruct-Q4_K_M.gguf".to_string(),
                    },
                    ModelVariant {
                        format: "GGUF".to_string(),
                        precision: "Q8_0".to_string(),
                        size_gb: 8.3,
                        huggingface_path: "ibm-granite/granite-3.1-8b-instruct-GGUF/granite-3.1-8b-instruct-Q8_0.gguf".to_string(),
                    },
                    ModelVariant {
                        format: "safetensors".to_string(),
                        precision: "BF16".to_string(),
                        size_gb: 16.0,
                        huggingface_path: "ibm-granite/granite-3.1-8b-instruct".to_string(),
                    },
                ],
                description: Some("Granite 3.1 8B instruct-tuned model for general-purpose text generation.".to_string()),
                tags: vec!["instruct".to_string(), "text".to_string(), "general-purpose".to_string()],
            },
            ModelDefinition {
                id: "granite-3.1-20b-instruct".to_string(),
                family: "Granite".to_string(),
                version: "3.1".to_string(),
                size: 20_700_000_000,
                context_length: 8_192,
                model_type: ModelType::Text,
                huggingface_repo: "ibm-granite/granite-3.1-20b-instruct".to_string(),
                required_provider_capabilities: vec!["OpenAIChat".to_string(), "OllamaChat".to_string()],
                variants: vec![
                    ModelVariant {
                        format: "GGUF".to_string(),
                        precision: "Q4_K_M".to_string(),
                        size_gb: 12.8,
                        huggingface_path: "ibm-granite/granite-3.1-20b-instruct-GGUF/granite-3.1-20b-instruct-Q4_K_M.gguf".to_string(),
                    },
                    ModelVariant {
                        format: "GGUF".to_string(),
                        precision: "Q5_K_M".to_string(),
                        size_gb: 14.7,
                        huggingface_path: "ibm-granite/granite-3.1-20b-instruct-GGUF/granite-3.1-20b-instruct-Q5_K_M.gguf".to_string(),
                    },
                    ModelVariant {
                        format: "safetensors".to_string(),
                        precision: "BF16".to_string(),
                        size_gb: 40.0,
                        huggingface_path: "ibm-granite/granite-3.1-20b-instruct".to_string(),
                    },
                ],
                description: Some("Granite 3.1 20B instruct-tuned model for complex reasoning tasks.".to_string()),
                tags: vec!["instruct".to_string(), "text".to_string(), "reasoning".to_string()],
            },
            // Granite Vision
            ModelDefinition {
                id: "granite-vision-3.1-8b".to_string(),
                family: "Granite Vision".to_string(),
                version: "3.1".to_string(),
                size: 8_290_000_000,
                context_length: 4_096,
                model_type: ModelType::Vision,
                huggingface_repo: "ibm-granite/granite-vision-3.1-8b".to_string(),
                required_provider_capabilities: vec!["OpenAIChat".to_string()],
                variants: vec![
                    ModelVariant {
                        format: "safetensors".to_string(),
                        precision: "BF16".to_string(),
                        size_gb: 16.0,
                        huggingface_path: "ibm-granite/granite-vision-3.1-8b".to_string(),
                    },
                ],
                description: Some("Granite Vision 3.1 8B for visual analysis and image understanding.".to_string()),
                tags: vec!["vision".to_string(), "image".to_string(), "multimodal".to_string()],
            },
            // Granite Speech
            ModelDefinition {
                id: "granite-speech-1.0".to_string(),
                family: "Granite Speech".to_string(),
                version: "1.0".to_string(),
                size: 3_000_000_000,
                context_length: 3_000,
                model_type: ModelType::Speech,
                huggingface_repo: "ibm-granite/granite-speech-1.0".to_string(),
                required_provider_capabilities: vec!["OllamaChat".to_string()],
                variants: vec![
                    ModelVariant {
                        format: "GGUF".to_string(),
                        precision: "Q4_K_M".to_string(),
                        size_gb: 1.6,
                        huggingface_path: "ibm-granite/granite-speech-1.0-GGUF/granite-speech-1.0-Q4_K_M.gguf".to_string(),
                    },
                    ModelVariant {
                        format: "GGUF".to_string(),
                        precision: "Q8_0".to_string(),
                        size_gb: 2.9,
                        huggingface_path: "ibm-granite/granite-speech-1.0-GGUF/granite-speech-1.0-Q8_0.gguf".to_string(),
                    },
                ],
                description: Some("Granite Speech 1.0 for audio transcription and translation.".to_string()),
                tags: vec!["speech".to_string(), "audio".to_string(), "transcription".to_string()],
            },
            // Granite Guardian
            ModelDefinition {
                id: "granite-guardian-3.1-8b".to_string(),
                family: "Granite Guardian".to_string(),
                version: "3.1".to_string(),
                size: 8_290_000_000,
                context_length: 128_000,
                model_type: ModelType::Text,
                huggingface_repo: "ibm-granite/granite-guardian-3.1-8b".to_string(),
                required_provider_capabilities: vec!["OpenAIChat".to_string(), "OllamaChat".to_string()],
                variants: vec![
                    ModelVariant {
                        format: "GGUF".to_string(),
                        precision: "Q4_K_M".to_string(),
                        size_gb: 4.9,
                        huggingface_path: "ibm-granite/granite-guardian-3.1-8b-GGUF/granite-guardian-3.1-8b-Q4_K_M.gguf".to_string(),
                    },
                    ModelVariant {
                        format: "safetensors".to_string(),
                        precision: "BF16".to_string(),
                        size_gb: 16.0,
                        huggingface_path: "ibm-granite/granite-guardian-3.1-8b".to_string(),
                    },
                ],
                description: Some("Granite Guardian 3.1 8B for safety classification and content moderation.".to_string()),
                tags: vec!["guardian".to_string(), "safety".to_string(), "moderation".to_string()],
            },
        ]
    }
}

impl Registry<ModelDefinition> for ModelRegistry {
    fn list(&self) -> Vec<&ModelDefinition> {
        self.models.iter().collect()
    }

    fn get(&self, id: &str) -> Option<&ModelDefinition> {
        self.models.iter().find(|m| m.id == id)
    }

    fn search(&self, query: &str) -> Vec<&ModelDefinition> {
        let query_lower = query.to_lowercase();
        self.models
            .iter()
            .filter(|m| {
                m.id.to_lowercase().contains(&query_lower)
                    || m.family.to_lowercase().contains(&query_lower)
                    || m.description.as_ref().map(|d| d.to_lowercase().contains(&query_lower)).unwrap_or(false)
                    || m.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;

    #[test]
    fn test_model_registry_has_bundled_models() {
        let registry = ModelRegistry::new();
        let models = registry.list();
        assert!(models.len() >= 6);
    }

    #[test]
    fn test_model_registry_get_by_id() {
        let registry = ModelRegistry::new();
        let model = registry.get("granite-3.1-8b-instruct");
        assert!(model.is_some());
        let model = model.unwrap();
        assert_eq!(model.family, "Granite");
        assert_eq!(model.version, "3.1");
        assert_eq!(model.model_type, ModelType::Text);
    }

    #[test]
    fn test_model_registry_get_vision() {
        let registry = ModelRegistry::new();
        let model = registry.get("granite-vision-3.1-8b");
        assert!(model.is_some());
        assert_eq!(model.unwrap().model_type, ModelType::Vision);
    }

    #[test]
    fn test_model_registry_get_speech() {
        let registry = ModelRegistry::new();
        let model = registry.get("granite-speech-1.0");
        assert!(model.is_some());
        assert_eq!(model.unwrap().model_type, ModelType::Speech);
    }

    #[test]
    fn test_model_registry_get_guardian() {
        let registry = ModelRegistry::new();
        let model = registry.get("granite-guardian-3.1-8b");
        assert!(model.is_some());
        assert_eq!(model.unwrap().family, "Granite Guardian");
    }

    #[test]
    fn test_model_registry_get_not_found() {
        let registry = ModelRegistry::new();
        let model = registry.get("nonexistent-model");
        assert!(model.is_none());
    }

    #[test]
    fn test_model_registry_search() {
        let registry = ModelRegistry::new();
        let results = registry.search("vision");
        assert!(results.len() > 0);
        assert!(results.iter().all(|m| m.id.contains("vision") || m.family.to_lowercase().contains("vision")));
    }

    #[test]
    fn test_model_registry_search_family() {
        let registry = ModelRegistry::new();
        let results = registry.search("guardian");
        assert!(results.len() > 0);
    }

    #[test]
    fn test_model_registry_search_tags() {
        let registry = ModelRegistry::new();
        let results = registry.search("safety");
        assert!(results.len() > 0);
    }

    #[test]
    fn test_model_display() {
        let registry = ModelRegistry::new();
        let model = registry.get("granite-3.1-8b-instruct").unwrap();
        let display = model.to_string();
        assert!(display.contains("granite-3.1-8b-instruct"));
        assert!(display.contains("8B"));
    }

    #[test]
    fn test_model_type_display() {
        assert_eq!(ModelType::Text.to_string(), "Text");
        assert_eq!(ModelType::Vision.to_string(), "Vision");
        assert_eq!(ModelType::Speech.to_string(), "Speech");
        assert_eq!(ModelType::Embedding.to_string(), "Embedding");
    }

    #[test]
    fn test_model_variants() {
        let registry = ModelRegistry::new();
        let model = registry.get("granite-3.1-8b-instruct").unwrap();
        assert!(model.variants.len() >= 2);
        let gguf_variant = model.variants.iter().find(|v| v.format == "GGUF").unwrap();
        assert!(gguf_variant.size_gb > 0.0);
    }
}
