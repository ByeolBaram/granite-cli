pub mod base;
// TODO: mod <implementation mod>;

// Re-export capability implementations
// TODO: pub use <implementation mod>::<Name>Capability;

use base::{CapabilityConfig, CapabilityMetadata};
use crate::registry::ConfigConstructable;
use std::collections::HashMap;
use std::sync::LazyLock;

/*-- Public API --------------------------------------------------------------*/

// Re-export types from base
pub use base::{
    Capability, ConfigureResult, Dependency, EnvBinding, Factory, LaunchContext, ToolConfig,
};

/*-- Capability Configuration Storage ----------------------------------------*/

/// Global storage for capability configurations.
static CAPABILITY_CONFIGS: LazyLock<HashMap<String, CapabilityConfig>> = LazyLock::new(|| {
    load_bundled_capabilities()
        .into_iter()
        .map(|cfg| (cfg.id.clone(), cfg))
        .collect()
});

/*-- Custom Metadata Provider for Capabilities ------------------------------*/

/// Custom metadata provider that looks up config by ID
struct CapabilityMetadataProvider {
    id: String,
}

impl CapabilityMetadataProvider {
    fn new(id: String) -> Self {
        Self { id }
    }
}

// We need to manually implement the internal trait
trait CapabilityMetadataProviderTrait: Send + Sync {
    fn describe(&self) -> CapabilityMetadata;
    fn construct(
        &self,
        cfg: &CapabilityConfig,
    ) -> Box<dyn base::Capability<Config = CapabilityConfig>>;
}

impl CapabilityMetadataProviderTrait for CapabilityMetadataProvider {
    fn describe(&self) -> CapabilityMetadata {
        CAPABILITY_CONFIGS
            .get(&self.id)
            .map(config_to_metadata)
            .unwrap_or_else(|| CapabilityMetadata {
                id: self.id.clone(),
                name: String::new(),
                description: String::new(),
                dependencies: vec![],
                tags: vec![],
            })
    }

    fn construct(
        &self,
        _cfg: &CapabilityConfig,
    ) -> Box<dyn base::Capability<Config = CapabilityConfig>> {
        let config = CAPABILITY_CONFIGS
            .get(&self.id)
            .expect("Capability config not found");

        // Route to the correct capability implementation based on ID
        match self.id.as_str() {
            // "docling" => Box::new(DoclingCapability::new(config)),
            // "vision" => Box::new(VisionCapability::new(config)),
            // "speech" => Box::new(SpeechCapability::new(config)),
            // "compiler" => Box::new(CompilerCapability::new(config)),
            _ => panic!("Unknown capability: {}", self.id),
        }
    }
}

/// Convert a CapabilityConfig to CapabilityMetadata
fn config_to_metadata(config: &CapabilityConfig) -> CapabilityMetadata {
    CapabilityMetadata {
        id: config.id.clone(),
        name: config.name.clone(),
        description: config.description.clone(),
        dependencies: config.dependencies.clone(),
        tags: config.tags.clone(),
    }
}

/*-- Custom Capability Factory -----------------------------------------------*/

/// Custom capability factory that uses our metadata provider
pub struct CustomCapabilityFactory {
    registry: HashMap<String, Box<dyn CapabilityMetadataProviderTrait>>,
}

impl CustomCapabilityFactory {
    fn new() -> Self {
        Self {
            registry: HashMap::new(),
        }
    }

    fn register(&mut self, id: String) {
        self.registry
            .insert(id.clone(), Box::new(CapabilityMetadataProvider::new(id)));
    }

    pub(crate) fn construct(
        &self,
        name: &str,
        cfg: &CapabilityConfig,
    ) -> Result<Box<dyn base::Capability<Config = CapabilityConfig>>, String> {
        self.registry
            .get(name)
            .map(|x| x.construct(cfg))
            .ok_or_else(|| format!("Unknown capability: {}", name))
    }

    pub(crate) fn get(&self, name: &str) -> Option<CapabilityMetadata> {
        self.registry.get(name).map(|x| x.describe())
    }

    pub(crate) fn list(&self) -> HashMap<&str, CapabilityMetadata> {
        self.registry
            .iter()
            .map(|(k, v)| (k.as_str(), v.describe()))
            .collect()
    }
}

/*-- Factory Registration ----------------------------------------------------*/

/// Global capability factory with all capabilities registered.
pub static CAPABILITY_FACTORY: LazyLock<CustomCapabilityFactory> = LazyLock::new(|| {
    let mut factory = CustomCapabilityFactory::new();

    // Register each capability by ID
    for id in CAPABILITY_CONFIGS.keys() {
        factory.register(id.clone());
    }

    factory
});

/*-- Capability Loading ------------------------------------------------------*/

fn load_bundled_capabilities() -> Vec<CapabilityConfig> {
    vec![]
}

/*-- Backward Compatibility Helper ------------------------------------------*/

/// Resolve a capability instance from the factory (replaces old registry lookup).
pub fn resolve_capability_from_registry(
    id: &str,
) -> anyhow::Result<Box<dyn Capability<Config = CapabilityConfig>>> {
    let config = CAPABILITY_CONFIGS
        .get(id)
        .ok_or_else(|| anyhow::anyhow!("Capability '{}' not found in registry", id))?;

    CAPABILITY_FACTORY
        .construct(id, config)
        .map_err(|e| anyhow::anyhow!("Failed to construct capability: {}", e))
}

/*-- tests -------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_factory_loads_capabilities() {
        let list = CAPABILITY_FACTORY.list();
        assert!(list.len() >= 4, "Should have at least 4 capabilities");
    }

    #[test]
    fn test_capability_factory_get_metadata() {
        let metadata = CAPABILITY_FACTORY.get("docling");
        assert!(metadata.is_some());

        let meta = metadata.unwrap();
        assert_eq!(meta.name, "Document Conversion");
    }

    #[test]
    fn test_capability_factory_list_all() {
        let list = CAPABILITY_FACTORY.list();

        assert!(list.contains_key("docling"));
        assert!(list.contains_key("vision"));
        assert!(list.contains_key("speech"));
        assert!(list.contains_key("compiler"));
    }

    #[test]
    fn test_capability_factory_construct() {
        let config = CAPABILITY_CONFIGS.get("docling").unwrap();
        let capability = CAPABILITY_FACTORY.construct("docling", config).unwrap();

        assert_eq!(capability.id(), "docling");
        assert_eq!(capability.name(), "Document Conversion");
    }

    #[test]
    fn test_resolve_capability_from_registry() {
        let capability = resolve_capability_from_registry("vision").unwrap();
        assert_eq!(capability.id(), "vision");
    }
}