use crate::capabilities::{AgentModelBinding, ApiType, Capability};
use crate::launchers::base::{EnvBinding, LaunchContext, Launcher, LauncherMetadata};
use crate::registry::ConfigConstructable;
use crate::utils::resolve_shell_command;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/*-- public --*/

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct GooseLauncherConfig {
    /// Override path to the `goose` binary for non-PATH installs.
    #[serde(default)]
    pub command_path: Option<String>,
}

pub struct GooseLauncher {
    instance_id: String,
    config: GooseLauncherConfig,
    bound_binding: Option<AgentModelBinding>,
}

impl ConfigConstructable for GooseLauncher {
    type Config = GooseLauncherConfig;

    fn new(
        instance_id: &str,
        cfg: &serde_json::Value,
        _global_config: &crate::config::Config,
    ) -> Self {
        let config: GooseLauncherConfig = serde_json::from_value(cfg.clone()).unwrap_or_default();
        Self {
            instance_id: instance_id.to_string(),
            config,
            bound_binding: None,
        }
    }
}

impl crate::registry::Named for GooseLauncher {
    fn instance_id(&self) -> &str {
        &self.instance_id
    }
}

#[async_trait]
impl Launcher for GooseLauncher {
    fn name(&self) -> &str {
        "Goose CLI"
    }

    fn command(&self) -> &str {
        self.config.command_path.as_deref().unwrap_or("goose")
    }

    async fn bind_capability(
        &mut self,
        capability: &dyn Capability,
    ) -> anyhow::Result<()> {
        let supported = Self::metadata().supported_capabilities;
        let capability_types = capability.binding_types();
        if !capability_types.is_subset(&supported) {
            anyhow::bail!(
                "capability supports {:?} which this launcher does not support",
                capability_types.difference(&supported).collect::<Vec<_>>()
            );
        }

        let binding = capability.bind(crate::capabilities::BindingRequest::AgentModel(
            crate::capabilities::AgentModelBindingRequest {
                api_type: ApiType::OpenAI,
            },
        )).await?;
        match binding {
            crate::capabilities::Binding::AgentModel(binding) => {
                self.bound_binding = Some(binding);
            }
        }
        Ok(())
    }

    fn validate_command(&self) -> anyhow::Result<PathBuf> {
        resolve_shell_command(&self.config.command_path, "goose")
    }

    async fn env_overlay(&self, _ctx: &LaunchContext) -> anyhow::Result<Vec<EnvBinding>> {
        let mut overlay = Vec::new();

        if let Some(binding) = &self.bound_binding {
            // GOOSE_PROVIDER and GOOSE_MODEL override config.yaml
            overlay.push(EnvBinding {
                key: "GOOSE_PROVIDER".to_string(),
                value: binding.provider_name.clone(),
            });
            overlay.push(EnvBinding {
                key: "GOOSE_MODEL".to_string(),
                value: binding.model_name.clone(),
            });

            if !binding.base_url.is_empty() {
                // For OpenAI-compatible providers, set OPENAI_HOST to override base URL
                // Goose's env var precedence: env vars > config.yaml > defaults
                overlay.push(EnvBinding {
                    key: "OPENAI_HOST".to_string(),
                    value: binding.base_url.clone(),
                });
            }

            if let Some(context_length) = binding.context_length {
                // Goose respects GOOSE_CONTEXT_LIMIT env var (see issue #7839)
                overlay.push(EnvBinding {
                    key: "GOOSE_CONTEXT_LIMIT".to_string(),
                    value: context_length.to_string(),
                });
            }
        }

        Ok(overlay)
    }

    async fn launch(
        &self,
        args: &[String],
        ctx: &LaunchContext,
        ui: &dyn crate::utils::ui::Ui,
    ) -> anyhow::Result<std::process::ExitStatus> {
        let binary = self.validate_command()?;
        crate::launchers::base::run_command(binary, &self.env_overlay(ctx).await?, args, ctx, ui).await
    }
}

impl HasGooseLauncherMetadata for GooseLauncher {
    fn metadata() -> LauncherMetadata {
        LauncherMetadata {
            name: "Goose CLI".to_string(),
            description: "Goose Agent CLI launcher".to_string(),
            default_command: "goose".to_string(),
            supported_capabilities: HashSet::from([crate::capabilities::BindingType::AgentModel]),
            tags: vec!["goose".to_string(), "agent".to_string()],
        }
    }
}

/*-- private --*/

use crate::launchers::base::HasLauncherMetadata as HasGooseLauncherMetadata;

impl GooseLauncher {
}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Named;
    use crate::utils::ui::base::tests::CaptureUi;

    fn launcher(cfg: serde_json::Value) -> GooseLauncher {
        GooseLauncher::new("goose", &cfg, &crate::config::Config::default())
    }

    fn binding() -> crate::capabilities::AgentModelBinding {
        crate::capabilities::AgentModelBinding {
            api_type: crate::capabilities::ApiType::OpenAI,
            provider_name: "ollama".to_string(),
            base_url: "http://localhost:11434".to_string(),
            model_name: "mistral/mistral-large-v0.2".to_string(),
            endpoint_path: "/v1/chat/completions".to_string(),
            api_key: None,
            verify_ssl: true,
            context_length: Some(131072),
            temperature: None,
        }
    }

    fn bound(cfg: serde_json::Value, binding: crate::capabilities::AgentModelBinding) -> GooseLauncher {
        let mut l = launcher(cfg);
        l.bound_binding = Some(binding);
        l
    }

    fn ctx(dry_run: bool) -> crate::launchers::base::LaunchContext {
        crate::launchers::base::LaunchContext {
            launcher_id: "goose".to_string(),
            working_dir: std::path::PathBuf::from("/tmp"),
            base_env: std::collections::HashMap::new(),
            dry_run,
        }
    }

    // -- command resolution ----------------------------------------------------

    #[test]
    fn command_defaults_to_goose() {
        assert_eq!(launcher(serde_json::json!({})).command(), "goose");
    }

    #[test]
    fn command_uses_explicit_path_when_set() {
        let l = launcher(serde_json::json!({ "command_path": "/opt/bin/goose" }));
        assert_eq!(l.command(), "/opt/bin/goose");
    }

    #[test]
    fn validate_command_falls_back_to_path_for_bare_command_name() {
        let l = launcher(serde_json::json!({ "command_path": "ls" }));
        assert!(l.validate_command().is_ok());
    }

    // -- metadata / schema -----------------------------------------------------

    #[test]
    fn metadata_name_is_goose_cli() {
        let meta = GooseLauncher::metadata();
        assert_eq!(meta.name, "Goose CLI");
        assert_eq!(meta.default_command, "goose");
        assert!(
            meta.supported_capabilities
                .contains(&crate::capabilities::BindingType::AgentModel)
        );
    }

    #[test]
    fn instance_id_round_trips_from_construction() {
        let l = GooseLauncher::new(
            "goose-local",
            &serde_json::json!({}),
            &crate::config::Config::default(),
        );
        assert_eq!(l.instance_id(), "goose-local");
    }

    #[test]
    fn config_schema_exposes_command_path() {
        use crate::launchers::base::LauncherFactory;
        let mut factory = LauncherFactory::new();
        factory.register::<GooseLauncher>("goose");
        let schema = factory.config_schema("goose").unwrap();
        let props = schema
            .get("properties")
            .and_then(|p| p.as_object())
            .unwrap();
        assert!(props.contains_key("command_path"));
    }

    // -- env overlay -----------------------------------------------------------

    #[tokio::test]
    async fn env_overlay_is_empty_without_a_binding() {
        let overlay = launcher(serde_json::json!({}))
            .env_overlay(&ctx(false))
            .await
            .unwrap();
        assert!(overlay.is_empty());
    }

    #[tokio::test]
    async fn env_overlay_sets_goose_provider_and_model() {
        let overlay = bound(serde_json::json!({}), binding())
            .env_overlay(&ctx(false))
            .await
            .unwrap();
        let provider = overlay
            .iter()
            .find(|b| b.key == "GOOSE_PROVIDER")
            .expect("GOOSE_PROVIDER env");
        assert_eq!(provider.value, "ollama");
        let model = overlay
            .iter()
            .find(|b| b.key == "GOOSE_MODEL")
            .expect("GOOSE_MODEL env");
        assert_eq!(model.value, "mistral/mistral-large-v0.2");
    }

    #[tokio::test]
    async fn env_overlay_sets_openai_host_for_base_url() {
        let overlay = bound(serde_json::json!({}), binding())
            .env_overlay(&ctx(false))
            .await
            .unwrap();
        let host = overlay
            .iter()
            .find(|b| b.key == "OPENAI_HOST")
            .expect("OPENAI_HOST env");
        assert_eq!(host.value, "http://localhost:11434");
    }

    #[tokio::test]
    async fn env_overlay_sets_goose_context_limit() {
        let overlay = bound(serde_json::json!({}), binding())
            .env_overlay(&ctx(false))
            .await
            .unwrap();
        let limit = overlay
            .iter()
            .find(|b| b.key == "GOOSE_CONTEXT_LIMIT")
            .expect("GOOSE_CONTEXT_LIMIT env");
        assert_eq!(limit.value, "131072");
    }

    // -- launch ---------------------------------------------------------------

    #[tokio::test]
    async fn launch_without_binding_passes_args_through_unchanged() {
        let l = launcher(serde_json::json!({ "command_path": "ls" }));
        let ui = CaptureUi::default();
        l.launch(&["--version".to_string()], &ctx(true), &ui)
            .await
            .unwrap();

        let infos = ui.infos.borrow();
        assert!(infos.iter().any(|m| m.contains("--version")));
        // Without a binding there is no GOOSE_* env override.
        assert!(!infos.iter().any(|m| m.contains("GOOSE_PROVIDER")));
    }
}
