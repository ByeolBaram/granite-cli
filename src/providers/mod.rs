// Standard
use std::sync::LazyLock;

/*-- Public API --------------------------------------------------------------*/

pub static PROVIDER_REGISTRY: LazyLock<base::ProviderFactory> = LazyLock::new(|| {
    let mut factory = base::ProviderFactory::new();
    // Register instances here
    factory
});

// Re-export types from base
mod base;
pub use base::{
    ApiSurface, AuthType, HealthStatus, ModelFormat,
    Provider, ProviderError, ProviderMetadata, ProviderType,
};
