// Standard
use std::sync::LazyLock;

/*-- Provider Registry -------------------------------------------------------*/

pub static PROVIDER_REGISTRY: LazyLock<base::ProviderFactory> = LazyLock::new(|| {
    let mut factory = base::ProviderFactory::new();
    factory.register::<openai::OpenAIProvider>("openai-compatible");
    factory
});

/*-- Module Declarations -----------------------------------------------------*/

mod base;
pub use base::{
    ApiEndpoint, ApiType, AuthType, HealthStatus, ModelFormat,
    Provider, ProviderError, ProviderMetadata, ProviderType,
};

mod openai;
pub use openai::{OpenAIProvider, OpenAIProviderConfig};
