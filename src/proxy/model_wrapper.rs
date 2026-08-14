// Standard
use std::sync::{Arc, Mutex};

// Third Party
use async_trait::async_trait;

// Local
use crate::models::{
    Model, ModelArchitecture, ModelFunction, ModelMetadata, ModelType, ModelVariant,
};
use crate::providers::{
    ApiEndpoint, HealthStatus, ModelFormat, Provider, ProviderError, PullResult,
};
use crate::registry::Secret;
use crate::utils::ui::Ui;

use super::{ProxyServer, UsageTracker};

/*-- public --*/

/// Per-launch usage-tracking session, threaded through
/// `ModelSource::from_config` via `Config::usage_tracking`. Cheap to clone --
/// every field is an `Arc`.
#[derive(Clone)]
pub struct UsageTrackingContext {
    pub tracker: Arc<UsageTracker>,
    pub servers: Arc<Mutex<Vec<ProxyServer>>>,
}

impl std::fmt::Debug for UsageTrackingContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsageTrackingContext")
            .finish_non_exhaustive()
    }
}

/// A `Model` decorator whose `provider()` points at a local reverse proxy
/// instead of the real upstream. Every other method delegates straight to
/// `inner`, so any caller resolving connection details via
/// `model.provider()` gets tracked transparently.
pub struct UsageTrackingModel {
    inner: Arc<dyn Model>,
    local_base_url: String,
}

impl UsageTrackingModel {
    /// Starts a `ProxyServer` for `inner`'s real provider and registers it
    /// into `ctx.servers`. Synchronous: `ProxyServer::start` binds a
    /// non-blocking `std::net::TcpListener` and hands it to `tokio::spawn`,
    /// which only needs an ambient Tokio runtime, not an `.await`.
    pub fn wrap(
        inner: Arc<dyn Model>,
        label: String,
        ctx: UsageTrackingContext,
    ) -> anyhow::Result<Self> {
        let real_provider = inner.provider()?;
        let server = ProxyServer::start(
            real_provider.base_url().to_string(),
            real_provider.api_key().cloned(),
            real_provider.verify_ssl(),
            Arc::clone(&ctx.tracker),
            label,
        )?;
        let local_base_url = server.local_base_url.clone();
        ctx.servers.lock().unwrap().push(server);
        Ok(Self {
            inner,
            local_base_url,
        })
    }
}

impl Model for UsageTrackingModel {
    fn family(&self) -> &str {
        self.inner.family()
    }
    fn version(&self) -> &str {
        self.inner.version()
    }
    fn size(&self) -> u64 {
        self.inner.size()
    }
    fn context_length(&self) -> u64 {
        self.inner.context_length()
    }
    fn model_type(&self) -> &ModelType {
        self.inner.model_type()
    }
    fn huggingface_repo(&self) -> &str {
        self.inner.huggingface_repo()
    }
    fn native_dtype(&self) -> &str {
        self.inner.native_dtype()
    }
    fn architecture(&self) -> &ModelArchitecture {
        self.inner.architecture()
    }
    fn variants(&self) -> &[ModelVariant] {
        self.inner.variants()
    }
    fn description(&self) -> Option<&str> {
        self.inner.description()
    }
    fn tags(&self) -> &[String] {
        self.inner.tags()
    }
    fn supported_functions(&self) -> &[ModelFunction] {
        self.inner.supported_functions()
    }

    fn provider(&self) -> anyhow::Result<Box<dyn Provider>> {
        Ok(Box::new(UsageTrackingProvider {
            inner: self.inner.provider()?,
            local_base_url: self.local_base_url.clone(),
        }))
    }
}

/*-- private --*/

/// A `Provider` decorator that redirects connection details at the local
/// proxy while delegating everything else -- including the real upstream
/// call made by `health_check`/`pull_model` -- to `inner`.
struct UsageTrackingProvider {
    inner: Box<dyn Provider>,
    local_base_url: String,
}

#[async_trait]
impl Provider for UsageTrackingProvider {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn function_endpoints(&self) -> std::collections::HashMap<ModelFunction, Vec<ApiEndpoint>> {
        self.inner.function_endpoints()
    }
    fn supported_api_types(&self) -> Vec<crate::providers::ApiType> {
        self.inner.supported_api_types()
    }
    fn base_url(&self) -> &str {
        &self.local_base_url
    }
    fn api_key(&self) -> Option<&Secret> {
        // The proxy holds the real credential and injects it upstream; the
        // launched process talking to the local proxy never needs to see it.
        None
    }
    fn verify_ssl(&self) -> bool {
        // The local proxy speaks plain HTTP.
        true
    }
    fn supported_formats(&self) -> Vec<ModelFormat> {
        self.inner.supported_formats()
    }
    fn can_run_model(&self, variant_format: &str, variant_precision: &str) -> bool {
        self.inner.can_run_model(variant_format, variant_precision)
    }
    fn model_alias(&self, variant: Option<&ModelVariant>) -> Option<String> {
        self.inner.model_alias(variant)
    }
    async fn health_check(&self) -> Result<HealthStatus, ProviderError> {
        self.inner.health_check().await
    }
    async fn pull_model(
        &self,
        model: &ModelMetadata,
        variant: &ModelVariant,
        ui: &dyn Ui,
    ) -> Result<PullResult, ProviderError> {
        self.inner.pull_model(model, variant, ui).await
    }
}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct FakeProvider {
        base_url: String,
        api_key: Option<Secret>,
    }

    #[async_trait]
    impl Provider for FakeProvider {
        fn name(&self) -> &str {
            "fake"
        }
        fn function_endpoints(&self) -> std::collections::HashMap<ModelFunction, Vec<ApiEndpoint>> {
            std::collections::HashMap::new()
        }
        fn supported_api_types(&self) -> Vec<crate::providers::ApiType> {
            vec![crate::providers::ApiType::OpenAI]
        }
        fn base_url(&self) -> &str {
            &self.base_url
        }
        fn api_key(&self) -> Option<&Secret> {
            self.api_key.as_ref()
        }
        fn verify_ssl(&self) -> bool {
            true
        }
        fn supported_formats(&self) -> Vec<ModelFormat> {
            vec![]
        }
        async fn health_check(&self) -> Result<HealthStatus, ProviderError> {
            unimplemented!("not exercised by these tests")
        }
    }

    struct FakeModel {
        provider: FakeProvider,
    }

    impl Model for FakeModel {
        fn family(&self) -> &str {
            "Test"
        }
        fn version(&self) -> &str {
            "1.0"
        }
        fn size(&self) -> u64 {
            1
        }
        fn context_length(&self) -> u64 {
            4096
        }
        fn model_type(&self) -> &ModelType {
            &ModelType::Text
        }
        fn huggingface_repo(&self) -> &str {
            "test/test"
        }
        fn native_dtype(&self) -> &str {
            "bfloat16"
        }
        fn architecture(&self) -> &ModelArchitecture {
            unimplemented!("not exercised by these tests")
        }
        fn variants(&self) -> &[ModelVariant] {
            &[]
        }
        fn description(&self) -> Option<&str> {
            None
        }
        fn tags(&self) -> &[String] {
            &[]
        }
        fn supported_functions(&self) -> &[ModelFunction] {
            &[]
        }
        fn provider(&self) -> anyhow::Result<Box<dyn Provider>> {
            Ok(Box::new(self.provider.clone()))
        }
    }

    #[tokio::test]
    async fn wrap_points_provider_at_local_proxy_and_clears_api_key() {
        let ctx = UsageTrackingContext {
            tracker: Arc::new(UsageTracker::new()),
            servers: Arc::new(Mutex::new(Vec::new())),
        };
        let model: Arc<dyn Model> = Arc::new(FakeModel {
            provider: FakeProvider {
                base_url: "https://api.example.com".to_string(),
                api_key: Some(Secret("real-secret".to_string())),
            },
        });

        let wrapped =
            UsageTrackingModel::wrap(Arc::clone(&model), "chat".to_string(), ctx.clone()).unwrap();

        let provider = wrapped.provider().unwrap();
        assert!(provider.base_url().starts_with("http://127.0.0.1:"));
        assert_ne!(provider.base_url(), "https://api.example.com");
        assert!(provider.api_key().is_none());
        assert!(provider.verify_ssl());

        assert_eq!(ctx.servers.lock().unwrap().len(), 1);

        let servers: Vec<_> = ctx.servers.lock().unwrap().drain(..).collect();
        for server in servers {
            server.shutdown().await;
        }
    }

    #[tokio::test]
    async fn metadata_methods_delegate_to_inner() {
        let ctx = UsageTrackingContext {
            tracker: Arc::new(UsageTracker::new()),
            servers: Arc::new(Mutex::new(Vec::new())),
        };
        let model: Arc<dyn Model> = Arc::new(FakeModel {
            provider: FakeProvider {
                base_url: "https://api.example.com".to_string(),
                api_key: None,
            },
        });
        let wrapped = UsageTrackingModel::wrap(model, "chat".to_string(), ctx.clone()).unwrap();

        assert_eq!(wrapped.family(), "Test");
        assert_eq!(wrapped.version(), "1.0");
        assert_eq!(wrapped.context_length(), 4096);
        assert_eq!(wrapped.huggingface_repo(), "test/test");

        let servers: Vec<_> = ctx.servers.lock().unwrap().drain(..).collect();
        for server in servers {
            server.shutdown().await;
        }
    }
}
