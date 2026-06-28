pub mod graph;
pub mod recommender;
pub mod resolver;

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;

use crate::capabilities::Capability;
use crate::config::{Config, ProviderConfig};
use crate::providers::Provider;
use crate::registry;
use crate::registry::Registry;
use graph::DependencyGraph;
use resolver::DependencyResolver;

/// Content hash of a config — used as the cache key for resolved instances.
fn content_hash<T: serde::Serialize>(item: &T) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let serialized = serde_json::to_string(item).unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    serialized.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Cached resolved instance.
struct CachedInstance {
    _content_hash: String,
    instance: Arc<dyn Provider>,
}

/// Factory — the central DI container that lazily resolves providers,
/// models, and capabilities.
pub struct Factory {
    config: Arc<RwLock<Config>>,
    provider_cache: Arc<RwLock<HashMap<String, CachedInstance>>>,
    _capability_cache: Arc<RwLock<HashMap<String, CachedInstance>>>,
}

impl Factory {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            provider_cache: Arc::new(RwLock::new(HashMap::new())),
            _capability_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Resolve a configured provider from config and cache it.
    pub async fn resolve_configured_provider(&self, id: &str) -> Result<Arc<dyn Provider>> {
        // Check cache first
        {
            let cache = self.provider_cache.read().unwrap();
            if let Some(cached) = cache.get(id) {
                return Ok(cached.instance.clone());
            }
        }

        let provider_config = {
            let config_guard = self.config.read().unwrap();
            config_guard
                .get_provider(id)
                .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found in configuration", id))?
                .clone()
        };

        let provider = self.create_provider_from_config(&provider_config)?;
        let provider_arc: Arc<dyn Provider> = Arc::from(provider);

        // Cache the result
        {
            let mut cache = self.provider_cache.write().unwrap();
            cache.insert(
                id.to_string(),
                CachedInstance {
                    _content_hash: content_hash(&provider_config),
                    instance: provider_arc.clone(),
                },
            );
        }

        Ok(provider_arc)
    }

    /// List all configured provider IDs.
    pub fn list_configured_providers(&self) -> Vec<String> {
        self.config.read().unwrap().providers.keys().cloned().collect()
    }

    /// Resolve a registered model ID and return the provider that can serve it.
    /// Falls back to configured providers if the model has a provider assigned.
    pub async fn resolve_model_provider(&self, model_id: &str) -> Result<Option<Arc<dyn Provider>>> {
        let model_def = registry::MODEL_REGISTRY.get(model_id);
        if model_def.is_none() {
            return Ok(None);
        }

        let provider_id = {
            let config_guard = self.config.read().unwrap();
            if let Some(model_config) = config_guard.get_model(model_id) {
                model_config.provider_id.clone()
            } else {
                None
            }
        };

        if let Some(provider_id) = provider_id {
            return self.resolve_configured_provider(&provider_id).await.map(Some);
        }

        Ok(None)
    }

    /// Resolve a capability instance from the static registry.
    /// Returns a boxed capability if found.
    pub fn resolve_capability_registry(&self, id: &str) -> Result<Box<dyn Capability>> {
        crate::capabilities::resolve_capability_from_registry(id)
    }

    /// Resolve all dependencies for a capability and check if they're satisfied.
    pub async fn resolve_capability_deps(&self, capability_id: &str) -> Result<DependencyResolution> {
        let cap_def = registry::CAPABILITY_REGISTRY
            .get(capability_id)
            .ok_or_else(|| anyhow::anyhow!("Capability '{}' not found in registry", capability_id))?;

        let graph = DependencyGraph::new(capability_id.to_string(), cap_def.clone());
        let cycles = graph.detect_cycles()?;

        if !cycles.is_empty() {
            anyhow::bail!("Circular dependency detected: {:?}", cycles);
        }

        let order = graph.topological_sort()?;
        let resolver = DependencyResolver::new(self.config.clone());

        resolver.validate_dependencies(&order).await
    }

    /// Validate that all dependencies for a capability are satisfied.
    pub async fn validate_capability_setup(&self, capability_id: &str) -> Result<bool> {
        let resolution = self.resolve_capability_deps(capability_id).await?;
        Ok(resolution.unresolved.is_empty())
    }

    /// Create a provider instance from its configuration.
    fn create_provider_from_config(&self, config: &ProviderConfig) -> Result<Box<dyn Provider>> {
        let endpoint = &config.endpoint;
        let api_key = config.api_key.as_deref().unwrap_or("");

        let provider: Box<dyn Provider> = match config.name.to_lowercase().as_str() {
            "openai" => Box::new(crate::providers::OpenAiCompatProvider::new(
                &config.provider_id, &config.name, endpoint.clone(), api_key.to_string(),
            )),
            "anthropic" => Box::new(crate::providers::AnthropicProvider::new(
                &config.provider_id, &config.name, endpoint.clone(), api_key.to_string(),
            )),
            "ollama" => Box::new(crate::providers::OllamaProvider::new(
                &config.provider_id, &config.name, endpoint.clone(),
            )),
            "watsonx" => Box::new(crate::providers::OpenAiCompatProvider::new(
                &config.provider_id, &config.name, endpoint.clone(), api_key.to_string(),
            )),
            _ => {
                let is_local = config.provider_type.to_lowercase() == "local";
                if is_local {
                    Box::new(crate::providers::OllamaProvider::new(
                        &config.provider_id, &config.name, endpoint.clone(),
                    ))
                } else {
                    Box::new(crate::providers::OpenAiCompatProvider::new(
                        &config.provider_id, &config.name, endpoint.clone(), api_key.to_string(),
                    ))
                }
            }
        };

        Ok(provider)
    }
}

/// Result of resolving capability dependencies.
pub struct DependencyResolution {
    /// Dependencies that are already configured/available.
    pub resolved: Vec<String>,
    /// Dependencies that still need to be set up.
    pub unresolved: Vec<UnresolvedDependency>,
    /// Topological resolution order.
    pub order: Vec<String>,
}

/// A dependency that still needs to be configured.
pub struct UnresolvedDependency {
    pub capability_id: String,
    pub missing: Vec<String>,
    pub required: bool,
}

// Implement capabilities::Factory for di::Factory
#[async_trait::async_trait]
impl crate::capabilities::Factory for Factory {
    async fn resolve_model(&self, id: &str) -> Result<String> {
        let model_def = registry::MODEL_REGISTRY.get(id)
            .ok_or_else(|| anyhow::anyhow!("Model '{}' not found in registry", id))?;
        Ok(model_def.huggingface_repo.clone())
    }

    async fn resolve_provider(&self, id: &str) -> Result<String> {
        let provider = self.resolve_configured_provider(id).await?;
        Ok(provider.id().to_string())
    }

    async fn resolve_capability(&self, id: &str) -> Result<String> {
        let _cap = self.resolve_capability_registry(id)?;
        Ok(id.to_string())
    }
}
