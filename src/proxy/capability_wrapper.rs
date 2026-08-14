// Standard
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

// Third Party
use async_trait::async_trait;

// Local
use crate::capabilities::{
    AgentModelBinding, Binding, BindingRequest, BindingType, Capability, Dependency, EnvBinding,
    LaunchContext,
};
use crate::proxy::server::ProxyServer;
use crate::proxy::usage::UsageTracker;

/*-- public --*/

/// Decorates a `Capability`, delegating every method to `inner` except
/// `bind`. When the inner capability resolves an `AgentModel` binding, a
/// local reverse proxy is started for its upstream and the returned
/// binding is rewritten to point at the proxy instead -- the launcher and
/// its `env_overlay` never need to know tracking is happening.
pub struct UsageTrackingCapability {
    inner: Box<dyn Capability>,
    label: String,
    tracker: Arc<UsageTracker>,
    servers: Arc<Mutex<Vec<ProxyServer>>>,
}

impl UsageTrackingCapability {
    /// `label` identifies this capability's usage in the final summary
    /// (typically the configured capability id). Every started `ProxyServer`
    /// is pushed into `servers` so the caller can shut them all down (and
    /// read `tracker`'s snapshot) once the launched process exits.
    pub fn new(
        inner: Box<dyn Capability>,
        label: String,
        tracker: Arc<UsageTracker>,
        servers: Arc<Mutex<Vec<ProxyServer>>>,
    ) -> Self {
        Self {
            inner,
            label,
            tracker,
            servers,
        }
    }
}

#[async_trait]
impl Capability for UsageTrackingCapability {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn dependencies(&self) -> Vec<Dependency> {
        self.inner.dependencies()
    }

    fn binding_types(&self) -> HashSet<BindingType> {
        self.inner.binding_types()
    }

    async fn bind(&self, request: BindingRequest) -> anyhow::Result<Binding> {
        let binding = self.inner.bind(request).await?;
        match binding {
            Binding::AgentModel(agent_model) => {
                let server =
                    ProxyServer::start(&agent_model, Arc::clone(&self.tracker), self.label.clone())
                        .await?;
                let proxied = AgentModelBinding {
                    base_url: server.local_base_url.clone(),
                    // The proxy injects the real key upstream; the launched
                    // process never needs to see it.
                    api_key: None,
                    ..agent_model
                };
                self.servers.lock().unwrap().push(server);
                Ok(Binding::AgentModel(proxied))
            }
        }
    }

    async fn on_setup(&self) -> anyhow::Result<()> {
        self.inner.on_setup().await
    }

    async fn on_pre_launch(&self, context: &LaunchContext) -> anyhow::Result<()> {
        self.inner.on_pre_launch(context).await
    }

    async fn on_post_launch(&self, context: &LaunchContext) -> anyhow::Result<()> {
        self.inner.on_post_launch(context).await
    }

    async fn on_shutdown(&self, context: &LaunchContext) -> anyhow::Result<()> {
        self.inner.on_shutdown(context).await
    }

    fn runtime_bindings(&self) -> Vec<EnvBinding> {
        self.inner.runtime_bindings()
    }
}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ApiType;

    struct FakeCapability {
        base_url: String,
    }

    #[async_trait]
    impl Capability for FakeCapability {
        fn name(&self) -> &str {
            "fake"
        }
        fn description(&self) -> &str {
            "fake capability for tests"
        }
        fn dependencies(&self) -> Vec<Dependency> {
            vec![]
        }
        fn binding_types(&self) -> HashSet<BindingType> {
            HashSet::from([BindingType::AgentModel])
        }
        async fn bind(&self, _request: BindingRequest) -> anyhow::Result<Binding> {
            Ok(Binding::AgentModel(AgentModelBinding {
                api_type: ApiType::Anthropic,
                base_url: self.base_url.clone(),
                model_name: "fake-model".to_string(),
                endpoint_path: "/v1/messages".to_string(),
                api_key: Some(crate::registry::Secret("real-secret".to_string())),
                verify_ssl: true,
                context_length: 8192,
            }))
        }
    }

    #[tokio::test]
    async fn bind_rewrites_base_url_and_drops_api_key() {
        let tracker = Arc::new(UsageTracker::new());
        let servers = Arc::new(Mutex::new(Vec::new()));
        let wrapper = UsageTrackingCapability::new(
            Box::new(FakeCapability {
                base_url: "https://api.anthropic.com".to_string(),
            }),
            "chat".to_string(),
            Arc::clone(&tracker),
            Arc::clone(&servers),
        );

        let request = BindingRequest::AgentModel(crate::capabilities::AgentModelBindingRequest {
            api_type: ApiType::Anthropic,
        });
        let binding = wrapper.bind(request).await.unwrap();
        let Binding::AgentModel(agent_model) = binding;
        assert!(agent_model.base_url.starts_with("http://127.0.0.1:"));
        assert_ne!(agent_model.base_url, "https://api.anthropic.com");
        assert!(agent_model.api_key.is_none());
        assert_eq!(agent_model.model_name, "fake-model");

        let started = servers.lock().unwrap().drain(..).collect::<Vec<_>>();
        assert_eq!(started.len(), 1);
        for server in started {
            server.shutdown().await;
        }
    }

    #[tokio::test]
    async fn delegates_metadata_methods_to_inner() {
        let tracker = Arc::new(UsageTracker::new());
        let servers = Arc::new(Mutex::new(Vec::new()));
        let wrapper = UsageTrackingCapability::new(
            Box::new(FakeCapability {
                base_url: "https://api.anthropic.com".to_string(),
            }),
            "chat".to_string(),
            tracker,
            servers,
        );
        assert_eq!(wrapper.name(), "fake");
        assert_eq!(wrapper.description(), "fake capability for tests");
        assert!(wrapper.binding_types().contains(&BindingType::AgentModel));
    }
}
