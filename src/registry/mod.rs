// Standard
use std::collections::HashMap;

/*-- Generic Factory Infrastructure ------------------------------------------*/

/// Core trait that all factory-managed types must implement.
/// Provides construction from a configuration object.
pub trait ConfigConstructable {
    type Config;

    /// Construct with a config instance
    fn new(cfg: &Self::Config) -> Self
    where
        Self: Sized;
}

/// Macro to define a complete factory infrastructure for a trait hierarchy.
///
/// This macro generates:
/// - An internal metadata trait for type erasure
/// - A HasMetadata trait that implementations must provide
/// - A MetaOf wrapper for connecting implementations to metadata
/// - A Factory struct with registration and construction capabilities
///
/// # Arguments
///
/// * `$trait` - The trait being factored (e.g., Provider, Capability)
/// * `$config` - The config type used for construction
/// * `$metadata` - The metadata type returned by describe()
/// * `$factory` - Name for the Factory struct
///
/// # Example
///
/// ```ignore
/// trait MyTrait: ConfigConstructable {
///     fn do_something(&self);
/// }
/// struct MyConfig { value: i32 }
///
/// define_factory!(
///     MyTrait,
///     MyConfig,
///     String,
///     MyTraitFactory
/// );
///
/// struct MySomething { value: i32 }
/// impl MyTrait for MySomething {
///     fn do_something(&self) { println!("My value: {}", self.value); }
/// }
/// impl HasMyTraitMetadata for MySomething {
///     fn metadata() -> String { "I belong to you".to_string() }
/// }
/// ```
#[macro_export]
macro_rules! define_factory {
    ($trait:ident, $config:ty, $metadata:ty, $factory:ident) => {

        /// Wrapper type that connects an implementation to its metadata.
        /// Uses PhantomData to maintain type information without storing instances.
        struct MetaOf<T>(std::marker::PhantomData<T>);
        impl<T> MetaOf<T> {
            const fn new() -> Self { Self(std::marker::PhantomData) }
        }

        $crate::paste::paste! {
            /// Internal trait for metadata provision and construction.
            /// This trait enables type erasure while maintaining type safety.
            pub(crate) trait [<$trait Metadata_>]: Send + Sync {
                /// Get metadata describing this implementation
                fn describe(&self) -> $metadata;

                /// Construct an instance with the given config
                fn construct(&self, cfg: &$config) -> Box<dyn $trait<Config = $config>>;
            }

            /// Trait that implementations must provide to supply metadata.
            /// This is the public interface for implementations to describe themselves.
            pub trait [<Has $trait Metadata>] {
                /// Return metadata describing this implementation
                fn metadata() -> $metadata;
            }

            /// Implementation of the internal metadata trait for any type T
            /// that implements the required traits.
            impl<T> [<$trait Metadata_>] for MetaOf<T>
            where
                T: $trait<Config = $config> + [<Has $trait Metadata>] + Send + Sync + 'static,
            {
                fn describe(&self) -> $metadata {
                    T::metadata()
                }

                fn construct(&self, cfg: &$config) -> Box<dyn $trait<Config = $config>> {
                    Box::new(T::new(cfg))
                }
            }

            /// Factory for creating and managing instances of the trait.
            ///
            /// The factory maintains a registry of implementations and provides
            /// methods to:
            /// - Register new implementations
            /// - Construct instances by name
            /// - Query metadata
            /// - List all registered implementations
            pub struct $factory {
                registry: std::collections::HashMap<&'static str, Box<dyn [<$trait Metadata_>]>>,
            }

            impl $factory {
                /// Create a new empty factory
                pub(crate) fn new() -> Self {
                    Self {
                        registry: std::collections::HashMap::new(),
                    }
                }

                /// Register an implementation with the given name.
                ///
                /// # Type Parameters
                ///
                /// * `T` - The implementation type to register
                ///
                /// # Arguments
                ///
                /// * `name` - Static string identifier for this implementation
                pub(crate) fn register<T>(&mut self, name: &'static str)
                where
                    T: $trait<Config = $config> + [<Has $trait Metadata>] + Send + Sync + 'static,
                {
                    self.registry.insert(name, Box::new(MetaOf::<T>::new()));
                }

                /// Construct an instance by name with the given configuration.
                ///
                /// # Arguments
                ///
                /// * `name` - The name of the implementation to construct
                /// * `cfg` - Configuration to pass to the constructor
                ///
                /// # Returns
                ///
                /// * `Ok(Box<dyn Trait>)` - Successfully constructed instance
                /// * `Err(String)` - Error message if name not found
                pub(crate) fn construct(
                    &self,
                    name: &str,
                    cfg: &$config,
                ) -> Result<Box<dyn $trait<Config = $config>>, String> {
                    self.registry
                        .get(name)
                        .map(|x| x.construct(cfg))
                        .ok_or_else(|| format!("Unknown instance type: {}", name))
                }

                /// Get metadata for a specific implementation by name.
                ///
                /// # Arguments
                ///
                /// * `name` - The name of the implementation
                ///
                /// # Returns
                ///
                /// * `Some(metadata)` - Metadata if found
                /// * `None` - If name not registered
                pub(crate) fn get(&self, name: &str) -> Option<$metadata> {
                    self.registry.get(name).map(|x| x.describe())
                }

                /// List all registered implementations with their metadata.
                ///
                /// # Returns
                ///
                /// HashMap mapping names to metadata for all registered implementations
                pub(crate) fn list(&self) -> std::collections::HashMap<&str, $metadata> {
                    self.registry
                        .iter()
                        .map(|(k, v)| (*k, v.describe()))
                        .collect()
                }
            }
        }

        impl Default for $factory {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

/*-- Temporary re-exports for backward compatibility -------------------------*/
// These will be removed as we migrate each module to the new factory pattern

// Re-export types from the new factory-based modules
pub use crate::models::{ModelDefinition, ModelType, ModelVariant};
pub use crate::providers::{ProviderDefinition, ProviderType, AuthType};
pub use crate::capabilities::base::Dependency as CapabilityDependency;

// Re-export CapabilityMetadata as CapabilityDefinition for backward compatibility
pub use crate::capabilities::base::CapabilityMetadata as CapabilityDefinition;

use std::sync::LazyLock;

/*-- Backward Compatibility Trait -------------------------------------------*/

pub trait Registry<T> {
    fn list(&self) -> Vec<&T>;
    fn get(&self, id: &str) -> Option<&T>;
    fn search(&self, query: &str) -> Vec<&T>;
}

/*-- Backward Compatibility Wrappers ----------------------------------------*/

// Backward compatibility wrapper for ModelRegistry
pub struct ModelRegistry {
    cache: HashMap<String, ModelDefinition>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        let cache = crate::models::MODEL_FACTORY
            .list()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        Self { cache }
    }
}

impl Registry<ModelDefinition> for ModelRegistry {
    fn list(&self) -> Vec<&ModelDefinition> {
        self.cache.values().collect()
    }

    fn get(&self, id: &str) -> Option<&ModelDefinition> {
        self.cache.get(id)
    }

    fn search(&self, query: &str) -> Vec<&ModelDefinition> {
        let query_lower = query.to_lowercase();
        self.cache
            .values()
            .filter(|m| {
                m.id.to_lowercase().contains(&query_lower)
                    || m.family.to_lowercase().contains(&query_lower)
                    || m.description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
                    || m.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}

// Backward compatibility wrapper for ProviderRegistry
pub struct ProviderRegistry {
    cache: HashMap<String, ProviderDefinition>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        let cache = crate::providers::PROVIDER_FACTORY
            .list()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        Self { cache }
    }
}

impl Registry<ProviderDefinition> for ProviderRegistry {
    fn list(&self) -> Vec<&ProviderDefinition> {
        self.cache.values().collect()
    }

    fn get(&self, id: &str) -> Option<&ProviderDefinition> {
        self.cache.get(id)
    }

    fn search(&self, query: &str) -> Vec<&ProviderDefinition> {
        let query_lower = query.to_lowercase();
        self.cache
            .values()
            .filter(|p| {
                p.id.to_lowercase().contains(&query_lower)
                    || p.name.to_lowercase().contains(&query_lower)
                    || p.description.to_lowercase().contains(&query_lower)
                    || p.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}

// Backward compatibility wrapper for CapabilityRegistry
pub struct CapabilityRegistry {
    cache: HashMap<String, CapabilityDefinition>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        let cache = crate::capabilities::CAPABILITY_FACTORY
            .list()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        Self { cache }
    }
}

impl Registry<CapabilityDefinition> for CapabilityRegistry {
    fn list(&self) -> Vec<&CapabilityDefinition> {
        self.cache.values().collect()
    }

    fn get(&self, id: &str) -> Option<&CapabilityDefinition> {
        self.cache.get(id)
    }

    fn search(&self, query: &str) -> Vec<&CapabilityDefinition> {
        let query_lower = query.to_lowercase();
        self.cache
            .values()
            .filter(|c| {
                c.id.to_lowercase().contains(&query_lower)
                    || c.name.to_lowercase().contains(&query_lower)
                    || c.description.to_lowercase().contains(&query_lower)
                    || c.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}

/*-- Global Registry Instances -----------------------------------------------*/

pub static MODEL_REGISTRY: LazyLock<ModelRegistry> = LazyLock::new(ModelRegistry::new);
pub static CAPABILITY_REGISTRY: LazyLock<CapabilityRegistry> =
    LazyLock::new(CapabilityRegistry::new);
pub static PROVIDER_REGISTRY: LazyLock<ProviderRegistry> = LazyLock::new(ProviderRegistry::new);

/*-- tests -------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;

    // Test trait and types
    trait TestTrait: ConfigConstructable {
        fn get_value(&self) -> i32;
    }

    struct TestConfig {
        value: i32,
    }

    // Define factory for test trait
    define_factory!(
        TestTrait,
        TestConfig,
        TestTraitMetadata,
        String,
        HasTestTraitMetadata,
        TestTraitFactory
    );

    // Test implementation 1
    struct TestImpl1 {
        value: i32,
    }

    impl ConfigConstructable for TestImpl1 {
        type Config = TestConfig;

        fn new(cfg: &Self::Config) -> Self {
            Self { value: cfg.value }
        }
    }

    impl TestTrait for TestImpl1 {
        fn get_value(&self) -> i32 {
            self.value
        }
    }

    impl HasTestTraitMetadata for TestImpl1 {
        fn metadata() -> String {
            "TestImpl1: A test implementation".to_string()
        }
    }

    // Test implementation 2
    struct TestImpl2 {
        value: i32,
    }

    impl ConfigConstructable for TestImpl2 {
        type Config = TestConfig;

        fn new(cfg: &Self::Config) -> Self {
            Self {
                value: cfg.value * 2,
            }
        }
    }

    impl TestTrait for TestImpl2 {
        fn get_value(&self) -> i32 {
            self.value
        }
    }

    impl HasTestTraitMetadata for TestImpl2 {
        fn metadata() -> String {
            "TestImpl2: Another test implementation".to_string()
        }
    }

    #[test]
    fn test_factory_registration() {
        let mut factory = TestTraitFactory::new();
        factory.register::<TestImpl1>("impl1");
        factory.register::<TestImpl2>("impl2");

        assert!(factory.get("impl1").is_some());
        assert!(factory.get("impl2").is_some());
        assert!(factory.get("impl3").is_none());
    }

    #[test]
    fn test_factory_metadata() {
        let mut factory = TestTraitFactory::new();
        factory.register::<TestImpl1>("impl1");
        factory.register::<TestImpl2>("impl2");

        let meta1 = factory.get("impl1").unwrap();
        assert!(meta1.contains("TestImpl1"));

        let meta2 = factory.get("impl2").unwrap();
        assert!(meta2.contains("TestImpl2"));
    }

    #[test]
    fn test_factory_construction() {
        let mut factory = TestTraitFactory::new();
        factory.register::<TestImpl1>("impl1");
        factory.register::<TestImpl2>("impl2");

        let cfg = TestConfig { value: 42 };

        let inst1 = factory.construct("impl1", &cfg).unwrap();
        assert_eq!(inst1.get_value(), 42);

        let inst2 = factory.construct("impl2", &cfg).unwrap();
        assert_eq!(inst2.get_value(), 84); // TestImpl2 doubles the value
    }

    #[test]
    fn test_factory_construct_unknown() {
        let factory = TestTraitFactory::new();
        let cfg = TestConfig { value: 42 };

        let result = factory.construct("unknown", &cfg);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("Unknown instance type"));
    }

    #[test]
    fn test_factory_list() {
        let mut factory = TestTraitFactory::new();
        factory.register::<TestImpl1>("impl1");
        factory.register::<TestImpl2>("impl2");

        let list = factory.list();
        assert_eq!(list.len(), 2);
        assert!(list.contains_key("impl1"));
        assert!(list.contains_key("impl2"));
    }

    #[test]
    fn test_factory_default() {
        let factory = TestTraitFactory::default();
        assert_eq!(factory.list().len(), 0);
    }
}
