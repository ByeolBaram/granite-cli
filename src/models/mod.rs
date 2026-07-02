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

// pub mod base;
// pub mod generic;

// use base::ModelConfig;
// use crate::registry::ConfigConstructable;
// use generic::{config_to_metadata, GenericModel};
// use std::collections::HashMap;
// use std::sync::LazyLock;

// /*-- Public API --------------------------------------------------------------*/


// /*-- Model Configuration Storage ---------------------------------------------*/

// /// Global storage for model configurations.
// static MODEL_CONFIGS: LazyLock<HashMap<String, ModelConfig>> = LazyLock::new(|| {
//     load_bundled_models()
//         .into_iter()
//         .map(|cfg| (cfg.id.clone(), cfg))
//         .collect()
// });

// /*-- Custom Metadata Provider for Models ------------------------------------*/

// /// Custom metadata provider that looks up config by ID
// struct ModelMetadataProvider {
//     id: String,
// }

// impl ModelMetadataProvider {
//     fn new(id: String) -> Self {
//         Self { id }
//     }
// }

// // We need to manually implement the internal trait since we can't use the macro's version
// trait ModelMetadataProviderTrait: Send + Sync {
//     fn describe(&self) -> ModelMetadata;
//     fn construct(&self, cfg: &ModelConfig) -> Box<dyn base::Model<Config = ModelConfig>>;
// }

// impl ModelMetadataProviderTrait for ModelMetadataProvider {
//     fn describe(&self) -> ModelMetadata {
//         MODEL_CONFIGS
//             .get(&self.id)
//             .map(config_to_metadata)
//             .unwrap_or_else(|| ModelMetadata {
//                 id: self.id.clone(),
//                 family: String::new(),
//                 version: String::new(),
//                 size: 0,
//                 context_length: 0,
//                 model_type: base::ModelType::Text,
//                 huggingface_repo: String::new(),
//                 required_provider_capabilities: vec![],
//                 variants: vec![],
//                 description: None,
//                 tags: vec![],
//             })
//     }

//     fn construct(&self, _cfg: &ModelConfig) -> Box<dyn base::Model<Config = ModelConfig>> {
//         let config = MODEL_CONFIGS.get(&self.id).expect("Model config not found");
//         Box::new(GenericModel::new(config))
//     }
// }

// /*-- Custom Model Factory ----------------------------------------------------*/

// /// Custom model factory that uses our metadata provider
// pub struct CustomModelFactory {
//     registry: HashMap<String, Box<dyn ModelMetadataProviderTrait>>,
// }

// impl CustomModelFactory {
//     fn new() -> Self {
//         Self {
//             registry: HashMap::new(),
//         }
//     }

//     fn register(&mut self, id: String) {
//         self.registry
//             .insert(id.clone(), Box::new(ModelMetadataProvider::new(id)));
//     }

//     pub(crate) fn construct(
//         &self,
//         name: &str,
//         cfg: &ModelConfig,
//     ) -> Result<Box<dyn base::Model<Config = ModelConfig>>, String> {
//         self.registry
//             .get(name)
//             .map(|x| x.construct(cfg))
//             .ok_or_else(|| format!("Unknown model: {}", name))
//     }

//     pub(crate) fn get(&self, name: &str) -> Option<ModelMetadata> {
//         self.registry.get(name).map(|x| x.describe())
//     }

//     pub(crate) fn list(&self) -> HashMap<&str, ModelMetadata> {
//         self.registry
//             .iter()
//             .map(|(k, v)| (k.as_str(), v.describe()))
//             .collect()
//     }
// }

// /*-- Factory Registration ----------------------------------------------------*/

// /// Global model factory with all models registered.
// pub static MODEL_FACTORY: LazyLock<CustomModelFactory> = LazyLock::new(|| {
//     let mut factory = CustomModelFactory::new();

//     // Register each model by ID
//     for id in MODEL_CONFIGS.keys() {
//         factory.register(id.clone());
//     }

//     factory
// });

// /*-- tests -------------------------------------------------------------------*/

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_model_factory_loads_models() {
//         let list = MODEL_FACTORY.list();
//         assert!(list.len() >= 6, "Should have at least 6 models");
//     }

//     #[test]
//     fn test_model_factory_get_metadata() {
//         let metadata = MODEL_FACTORY.get("granite-3.1-8b-instruct");
//         assert!(metadata.is_some());

//         let meta = metadata.unwrap();
//         assert_eq!(meta.family, "Granite");
//         assert_eq!(meta.version, "3.1");
//         assert_eq!(meta.model_type, base::ModelType::Text);
//     }

//     #[test]
//     fn test_model_factory_list_all() {
//         let list = MODEL_FACTORY.list();

//         assert!(list.contains_key("granite-3.1-3b-instruct"));
//         assert!(list.contains_key("granite-3.1-8b-instruct"));
//         assert!(list.contains_key("granite-vision-3.1-8b"));
//         assert!(list.contains_key("granite-speech-1.0"));
//         assert!(list.contains_key("granite-guardian-3.1-8b"));
//     }

//     #[test]
//     fn test_model_factory_construct() {
//         let config = MODEL_CONFIGS.get("granite-3.1-8b-instruct").unwrap();
//         let model = MODEL_FACTORY.construct("granite-3.1-8b-instruct", config).unwrap();

//         assert_eq!(model.id(), "granite-3.1-8b-instruct");
//         assert_eq!(model.family(), "Granite");
//     }
// }
