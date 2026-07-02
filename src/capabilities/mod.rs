// Standard
use std::sync::LazyLock;

/*-- Public API --------------------------------------------------------------*/

pub static CAPABILITY_REGISTRY: LazyLock<base::CapabilityFactory> = LazyLock::new(|| {
    let mut factory = base::CapabilityFactory::new();
    // Register instances here
    factory
});

// Re-export types from base
mod base;
pub use base::{
    Capability, ConfigureResult, Dependency, EnvBinding, Factory, LaunchContext, ToolConfig,
};
