// Standard
use std::sync::LazyLock;

/*-- Public API --------------------------------------------------------------*/

pub static MODEL_REGISTRY: LazyLock<base::ModelFactory> = LazyLock::new(|| {
    let mut factory = base::ModelFactory::new();
    // Register instances here
    factory
});

// Re-export types from base
mod base;
pub use base::{ModelMetadata, ModelType, ModelVariant};
