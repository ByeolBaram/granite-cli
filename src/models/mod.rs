pub mod base;
pub mod generic;

use base::ModelConfig;
use crate::registry::ConfigConstructable;
use generic::{config_to_metadata, GenericModel};
use std::collections::HashMap;
use std::sync::LazyLock;

/*-- Public API --------------------------------------------------------------*/

// Re-export types from base
pub use base::ModelMetadata;
pub use base::{ModelType, ModelVariant, ModelMetadata as ModelDefinition};

/*-- Model Configuration Storage ---------------------------------------------*/

/// Global storage for model configurations.
static MODEL_CONFIGS: LazyLock<HashMap<String, ModelConfig>> = LazyLock::new(|| {
    load_bundled_models()
        .into_iter()
        .map(|cfg| (cfg.id.clone(), cfg))
        .collect()
});

/*-- Custom Metadata Provider for Models ------------------------------------*/

/// Custom metadata provider that looks up config by ID
struct ModelMetadataProvider {
    id: String,
}

impl ModelMetadataProvider {
    fn new(id: String) -> Self {
        Self { id }
    }
}

// We need to manually implement the internal trait since we can't use the macro's version
trait ModelMetadataProviderTrait: Send + Sync {
    fn describe(&self) -> ModelMetadata;
    fn construct(&self, cfg: &ModelConfig) -> Box<dyn base::Model<Config = ModelConfig>>;
}

impl ModelMetadataProviderTrait for ModelMetadataProvider {
    fn describe(&self) -> ModelMetadata {
        MODEL_CONFIGS
            .get(&self.id)
            .map(config_to_metadata)
            .unwrap_or_else(|| ModelMetadata {
                id: self.id.clone(),
                family: String::new(),
                version: String::new(),
                size: 0,
                context_length: 0,
                model_type: base::ModelType::Text,
                huggingface_repo: String::new(),
                required_provider_capabilities: vec![],
                variants: vec![],
                description: None,
                tags: vec![],
            })
    }

    fn construct(&self, _cfg: &ModelConfig) -> Box<dyn base::Model<Config = ModelConfig>> {
        let config = MODEL_CONFIGS.get(&self.id).expect("Model config not found");
        Box::new(GenericModel::new(config))
    }
}

/*-- Custom Model Factory ----------------------------------------------------*/

/// Custom model factory that uses our metadata provider
pub struct CustomModelFactory {
    registry: HashMap<String, Box<dyn ModelMetadataProviderTrait>>,
}

impl CustomModelFactory {
    fn new() -> Self {
        Self {
            registry: HashMap::new(),
        }
    }

    fn register(&mut self, id: String) {
        self.registry
            .insert(id.clone(), Box::new(ModelMetadataProvider::new(id)));
    }

    pub(crate) fn construct(
        &self,
        name: &str,
        cfg: &ModelConfig,
    ) -> Result<Box<dyn base::Model<Config = ModelConfig>>, String> {
        self.registry
            .get(name)
            .map(|x| x.construct(cfg))
            .ok_or_else(|| format!("Unknown model: {}", name))
    }

    pub(crate) fn get(&self, name: &str) -> Option<ModelMetadata> {
        self.registry.get(name).map(|x| x.describe())
    }

    pub(crate) fn list(&self) -> HashMap<&str, ModelMetadata> {
        self.registry
            .iter()
            .map(|(k, v)| (k.as_str(), v.describe()))
            .collect()
    }
}

/*-- Factory Registration ----------------------------------------------------*/

/// Global model factory with all models registered.
pub static MODEL_FACTORY: LazyLock<CustomModelFactory> = LazyLock::new(|| {
    let mut factory = CustomModelFactory::new();

    // Register each model by ID
    for id in MODEL_CONFIGS.keys() {
        factory.register(id.clone());
    }

    factory
});

/*-- Model Loading -----------------------------------------------------------*/

fn load_bundled_models() -> Vec<ModelConfig> {
    use base::{ModelType, ModelVariant};

    vec![
        ModelConfig {
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
        ModelConfig {
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
        ModelConfig {
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
        ModelConfig {
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
        ModelConfig {
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
        ModelConfig {
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

/*-- tests -------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_factory_loads_models() {
        let list = MODEL_FACTORY.list();
        assert!(list.len() >= 6, "Should have at least 6 models");
    }

    #[test]
    fn test_model_factory_get_metadata() {
        let metadata = MODEL_FACTORY.get("granite-3.1-8b-instruct");
        assert!(metadata.is_some());

        let meta = metadata.unwrap();
        assert_eq!(meta.family, "Granite");
        assert_eq!(meta.version, "3.1");
        assert_eq!(meta.model_type, base::ModelType::Text);
    }

    #[test]
    fn test_model_factory_list_all() {
        let list = MODEL_FACTORY.list();

        assert!(list.contains_key("granite-3.1-3b-instruct"));
        assert!(list.contains_key("granite-3.1-8b-instruct"));
        assert!(list.contains_key("granite-vision-3.1-8b"));
        assert!(list.contains_key("granite-speech-1.0"));
        assert!(list.contains_key("granite-guardian-3.1-8b"));
    }

    #[test]
    fn test_model_factory_construct() {
        let config = MODEL_CONFIGS.get("granite-3.1-8b-instruct").unwrap();
        let model = MODEL_FACTORY.construct("granite-3.1-8b-instruct", config).unwrap();

        assert_eq!(model.id(), "granite-3.1-8b-instruct");
        assert_eq!(model.family(), "Granite");
    }
}
