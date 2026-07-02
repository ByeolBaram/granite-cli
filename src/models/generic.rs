use super::base::{
    HasModelMetadata, Model, ModelConfig, ModelMetadata, ModelType, ModelVariant,
};
use crate::registry::ConfigConstructable;

/*-- Generic Model Implementation --------------------------------------------*/

/// Generic model implementation that works for all models.
/// Stores the configuration data and provides access through the Model trait.
pub struct GenericModel {
    config: ModelConfig,
}

impl ConfigConstructable for GenericModel {
    type Config = ModelConfig;

    fn new(cfg: &Self::Config) -> Self {
        Self {
            config: cfg.clone(),
        }
    }
}

impl Model for GenericModel {
    fn id(&self) -> &str {
        &self.config.id
    }

    fn family(&self) -> &str {
        &self.config.family
    }

    fn version(&self) -> &str {
        &self.config.version
    }

    fn size(&self) -> u64 {
        self.config.size
    }

    fn context_length(&self) -> u64 {
        self.config.context_length
    }

    fn model_type(&self) -> &ModelType {
        &self.config.model_type
    }

    fn huggingface_repo(&self) -> &str {
        &self.config.huggingface_repo
    }

    fn required_provider_capabilities(&self) -> &[String] {
        &self.config.required_provider_capabilities
    }

    fn variants(&self) -> &[ModelVariant] {
        &self.config.variants
    }

    fn description(&self) -> Option<&str> {
        self.config.description.as_deref()
    }

    fn tags(&self) -> &[String] {
        &self.config.tags
    }
}

impl HasModelMetadata for GenericModel {
    fn metadata() -> ModelMetadata {
        // This is never actually called - the factory overrides this
        // by using a custom metadata provider that looks up configs
        ModelMetadata {
            id: String::new(),
            family: String::new(),
            version: String::new(),
            size: 0,
            context_length: 0,
            model_type: ModelType::Text,
            huggingface_repo: String::new(),
            required_provider_capabilities: vec![],
            variants: vec![],
            description: None,
            tags: vec![],
        }
    }
}

/// Convert a ModelConfig to ModelMetadata
pub(crate) fn config_to_metadata(config: &ModelConfig) -> ModelMetadata {
    ModelMetadata {
        id: config.id.clone(),
        family: config.family.clone(),
        version: config.version.clone(),
        size: config.size,
        context_length: config.context_length,
        model_type: config.model_type.clone(),
        huggingface_repo: config.huggingface_repo.clone(),
        required_provider_capabilities: config.required_provider_capabilities.clone(),
        variants: config.variants.clone(),
        description: config.description.clone(),
        tags: config.tags.clone(),
    }
}

/*-- tests -------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> ModelConfig {
        ModelConfig {
            id: "test-model".to_string(),
            family: "Test".to_string(),
            version: "1.0".to_string(),
            size: 3_000_000_000,
            context_length: 8192,
            model_type: ModelType::Text,
            huggingface_repo: "test/model".to_string(),
            required_provider_capabilities: vec!["OpenAIChat".to_string()],
            variants: vec![ModelVariant {
                format: "GGUF".to_string(),
                precision: "Q4_K_M".to_string(),
                size_gb: 1.9,
                huggingface_path: "test/model/file.gguf".to_string(),
            }],
            description: Some("A test model".to_string()),
            tags: vec!["test".to_string()],
        }
    }

    #[test]
    fn test_generic_model_construction() {
        let config = create_test_config();
        let model = GenericModel::new(&config);

        assert_eq!(model.id(), "test-model");
        assert_eq!(model.family(), "Test");
        assert_eq!(model.version(), "1.0");
        assert_eq!(model.size(), 3_000_000_000);
        assert_eq!(model.context_length(), 8192);
        assert_eq!(model.model_type(), &ModelType::Text);
        assert_eq!(model.huggingface_repo(), "test/model");
        assert_eq!(model.required_provider_capabilities().len(), 1);
        assert_eq!(model.variants().len(), 1);
        assert_eq!(model.description(), Some("A test model"));
        assert_eq!(model.tags().len(), 1);
    }

    #[test]
    fn test_generic_model_trait_bounds() {
        let config = create_test_config();
        let model: Box<dyn Model<Config = ModelConfig>> = Box::new(GenericModel::new(&config));

        assert_eq!(model.id(), "test-model");
        assert_eq!(model.family(), "Test");
    }

    #[test]
    fn test_config_to_metadata() {
        let config = create_test_config();
        let metadata = config_to_metadata(&config);

        assert_eq!(metadata.id, "test-model");
        assert_eq!(metadata.family, "Test");
        assert_eq!(metadata.version, "1.0");
        assert_eq!(metadata.size, 3_000_000_000);
    }
}