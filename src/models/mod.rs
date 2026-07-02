// Standard
use std::sync::LazyLock;

// Include generated code from build.rs
include!(concat!(env!("OUT_DIR"), "/generated_models.rs"));

/*-- Public API --------------------------------------------------------------*/

pub static MODEL_REGISTRY: LazyLock<base::ModelFactory> = LazyLock::new(|| {
    let mut factory = base::ModelFactory::new();
    register_all_models(&mut factory);
    factory
});

// Re-export types from base
mod base;
pub use base::{Model, ModelMetadata, ModelType, ModelVariant};

/*-- tests -------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_models_registered() {
        let models = MODEL_REGISTRY.list();
        assert_eq!(models.len(), 6, "Expected 6 models to be registered");
    }

    #[test]
    fn test_get_specific_model() {
        let model = MODEL_REGISTRY.get("granite-3.1-8b-instruct");
        assert!(model.is_some(), "granite-3.1-8b-instruct should be registered");

        let metadata = model.unwrap();
        assert_eq!(metadata.id, "granite-3.1-8b-instruct");
        assert_eq!(metadata.family, "Granite");
        assert_eq!(metadata.version, "3.1");
        assert_eq!(metadata.size, 8290000000);
        assert_eq!(metadata.context_length, 8192);
        assert_eq!(metadata.model_type, ModelType::Text);
    }

    #[test]
    fn test_model_variants() {
        let model = MODEL_REGISTRY.get("granite-3.1-8b-instruct").unwrap();
        assert_eq!(model.variants.len(), 3, "granite-3.1-8b-instruct should have 3 variants");

        // Check first variant
        let variant = &model.variants[0];
        assert_eq!(variant.format, "GGUF");
        assert_eq!(variant.precision, "Q4_K_M");
        assert_eq!(variant.size_gb, 4.9);
    }

    #[test]
    fn test_all_model_ids() {
        let models = MODEL_REGISTRY.list();
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();

        assert!(ids.contains(&"granite-3.1-3b-instruct"));
        assert!(ids.contains(&"granite-3.1-8b-instruct"));
        assert!(ids.contains(&"granite-3.1-20b-instruct"));
        assert!(ids.contains(&"granite-vision-3.1-8b"));
        assert!(ids.contains(&"granite-speech-1.0"));
        assert!(ids.contains(&"granite-guardian-3.1-8b"));
    }

    #[test]
    fn test_model_types() {
        let text_model = MODEL_REGISTRY.get("granite-3.1-8b-instruct").unwrap();
        assert_eq!(text_model.model_type, ModelType::Text);

        let vision_model = MODEL_REGISTRY.get("granite-vision-3.1-8b").unwrap();
        assert_eq!(vision_model.model_type, ModelType::Vision);

        let speech_model = MODEL_REGISTRY.get("granite-speech-1.0").unwrap();
        assert_eq!(speech_model.model_type, ModelType::Speech);
    }
}
