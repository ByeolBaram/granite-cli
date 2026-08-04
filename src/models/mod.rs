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
pub use base::{
    LayerKind, LayerTypeCount, MambaShape, Model, ModelArchitecture, ModelFunction, ModelMetadata,
    ModelType, ModelVariant,
};

pub(crate) mod context_fit;
pub use context_fit::ContextFit;

pub mod huggingface;

/*-- tests -------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_models_registered() {
        let models = MODEL_REGISTRY.entries();
        assert!(!models.is_empty(), "Expected models to be registered");
    }

    #[test]
    fn test_get_specific_model() {
        let model = MODEL_REGISTRY.get("granite-3.1-8b-instruct");
        assert!(
            model.is_some(),
            "granite-3.1-8b-instruct should be registered"
        );

        let metadata = model.unwrap();
        assert_eq!(metadata.family, "Granite 3.1");
        assert_eq!(metadata.version, "3.1");
        assert_eq!(metadata.context_length, 131072);
        assert_eq!(metadata.model_type, ModelType::Text);
    }

    #[test]
    fn test_model_variants() {
        let model = MODEL_REGISTRY.get("granite-3.1-8b-instruct").unwrap();
        assert!(
            !model.variants.is_empty(),
            "granite-3.1-8b-instruct should have variants"
        );

        // Check first variant
        let variant = &model.variants[0];
        assert!(!variant.format.is_empty());
        assert!(!variant.precision.is_empty());
        assert!(variant.size_gb > 0.0);
    }

    #[test]
    fn test_all_model_ids() {
        let models = MODEL_REGISTRY.entries();
        let ids: Vec<&str> = models.keys().copied().collect();

        assert!(ids.contains(&"granite-3.1-8b-instruct"));
        assert!(ids.contains(&"granite-guardian-3.1-8b"));
    }

    #[test]
    fn test_model_types() {
        let text_model = MODEL_REGISTRY.get("granite-3.1-8b-instruct").unwrap();
        assert_eq!(text_model.model_type, ModelType::Text);

        let vision_model = MODEL_REGISTRY.get("granite-vision-3.3-2b").unwrap();
        assert_eq!(vision_model.model_type, ModelType::Vision);

        let speech_model = MODEL_REGISTRY.get("granite-speech-4.1-2b").unwrap();
        assert_eq!(speech_model.model_type, ModelType::Speech);
    }

    #[test]
    fn test_model_supported_functions() {
        let text_model = MODEL_REGISTRY.get("granite-3.1-8b-instruct").unwrap();
        assert!(
            text_model
                .supported_functions
                .contains(&ModelFunction::Chat)
        );

        let vision_model = MODEL_REGISTRY.get("granite-vision-3.3-2b").unwrap();
        assert!(
            vision_model
                .supported_functions
                .contains(&ModelFunction::Chat)
        );
        assert!(
            vision_model
                .supported_functions
                .contains(&ModelFunction::ImageUnderstanding)
        );

        let speech_model = MODEL_REGISTRY.get("granite-speech-4.1-2b").unwrap();
        assert!(
            speech_model
                .supported_functions
                .contains(&ModelFunction::Chat)
        );
        assert!(
            speech_model
                .supported_functions
                .contains(&ModelFunction::Transcription)
        );
    }
}
