pub mod models;
pub mod capabilities;

use std::sync::LazyLock;

pub use models::{ModelDefinition, ModelRegistry, ModelType};
pub use capabilities::{CapabilityDefinition, CapabilityRegistry};

pub trait Registry<T> {
    fn list(&self) -> Vec<&T>;
    fn get(&self, id: &str) -> Option<&T>;
    fn search(&self, query: &str) -> Vec<&T>;
}

pub static MODEL_REGISTRY: LazyLock<ModelRegistry> = LazyLock::new(ModelRegistry::new);
pub static CAPABILITY_REGISTRY: LazyLock<CapabilityRegistry> = LazyLock::new(CapabilityRegistry::new);
