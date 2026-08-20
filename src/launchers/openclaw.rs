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
pub struct OpenClawLauncherConfig {
    /// Override path to the `openclaw` binary for non-PATH installs.
    #[serde(default)]
    pub command_path: Option<String>,
}

pub struct OpenClawLauncher {
    instance_id: String,
    config: OpenClawLauncherConfig,
    bound_binding: Option<AgentModelBinding>,
}

impl ConfigConstructable for OpenClawLauncher {
    type Config = OpenClawLauncherConfig;

    fn new(
        instance_id: &str,
        cfg: &serde_json::Value,
        _global_config: &crate::config::Config,
    ) -> Self {
        let config: OpenClawLauncherConfig = serde_json::from_value(cfg.clone()).unwrap_or_default();
        Self {
            instance_id: instance_id.to_string(),
            config,
            bound_binding: None,
        }
    }
}

impl crate::registry::Named for OpenClawLauncher {
    fn instance_id(&self) -> &str {
        &self.instance_id
    }
}

#[async_trait]
impl Launcher for OpenClawLauncher {
    fn name(&self) -> &str {
        "OpenClaw CLI"
    }

    fn command(&self) -> &str {
        self.config.command_path.as_deref().unwrap_or("openclaw")
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
        resolve_shell_command(&self.config.command_path, "openclaw")
    }

    async fn env_overlay(&self, ctx: &LaunchContext) -> anyhow::Result<Vec<EnvBinding>> {
        let mut overlay = Vec::new();

        if let Some(binding) = &self.bound_binding {
            // Point OpenClaw at the generated ephemeral config file
            overlay.push(EnvBinding {
                key: CONFIG_ENV.to_string(),
                value: openclaw_config_path(ctx)?.to_string_lossy().to_string(),
            });

            // Set provider and model via env vars (they override config.json)
            overlay.push(EnvBinding {
                key: "OPENCLAW_PROVIDER".to_string(),
                value: binding.provider_name.clone(),
            });
            overlay.push(EnvBinding {
                key: "OPENCLAW_MODEL".to_string(),
                value: binding.model_name.clone(),
            });

            if !binding.base_url.is_empty() {
                // For local models, OPENCLAW_LOCAL_ENDPOINT and OPENCLAW_LOCAL_MODEL
                // are used together. Set both for provider compatibility.
                overlay.push(EnvBinding {
                    key: "OPENCLAW_LOCAL_ENDPOINT".to_string(),
                    value: binding.base_url.clone(),
                });
                // LOCAL_MODEL is typically the model name when using a local server
                overlay.push(EnvBinding {
                    key: "OPENCLAW_LOCAL_MODEL".to_string(),
                    value: binding.model_name.clone(),
                });
            }

            if let Some(context_length) = binding.context_length {
                // OPENCLAW_MAX_TOKENS controls the context window (equivalent to context_length)
                overlay.push(EnvBinding {
                    key: "OPENCLAW_MAX_TOKENS".to_string(),
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

impl HasOpenClawLauncherMetadata for OpenClawLauncher {
    fn metadata() -> LauncherMetadata {
        LauncherMetadata {
            name: "OpenClaw CLI".to_string(),
            description: "OpenClaw autonomous AI agent launcher".to_string(),
            default_command: "openclaw".to_string(),
            supported_capabilities: HashSet::from([crate::capabilities::BindingType::AgentModel]),
            tags: vec!["openclaw".to_string(), "agent".to_string()],
        }
    }
}

/*-- private --*/

use crate::launchers::base::HasLauncherMetadata as HasOpenClawLauncherMetadata;

/// Env var OpenClaw merges an extra config file from, in addition to its own
/// global/project config.
const CONFIG_ENV: &str = "OPENCLAW_CONFIG_PATH";

/// The generated config file's name, relative to the launcher state dir.
const CONFIG_FILE: &str = "openclaw.json";

/// The OpenClaw config file this launcher instance writes and points `OPENCLAW_CONFIG_PATH` at.
/// Lives under the launcher state dir rather than the user's own OpenClaw config directory.
fn openclaw_config_path(ctx: &LaunchContext) -> anyhow::Result<PathBuf> {
    Ok(crate::config::Config::launcher_state_dir(&ctx.launcher_id)?
        .join(CONFIG_FILE))
}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Named;
    use crate::utils::ui::base::tests::CaptureUi;

    fn launcher(cfg: serde_json::Value) -> OpenClawLauncher {
        OpenClawLauncher::new("openclaw", &cfg, &crate::config::Config::default())
    }

    fn binding() -> AgentModelBinding {
        AgentModelBinding {
            api_type: ApiType::OpenAI,
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

    fn bound(cfg: serde_json::Value, binding: AgentModelBinding) -> OpenClawLauncher {
        let mut l = launcher(cfg);
        l.bound_binding = Some(binding);
        l
    }

    fn ctx(dry_run: bool) -> crate::launchers::base::LaunchContext {
        crate::launchers::base::LaunchContext {
            launcher_id: "openclaw".to_string(),
            working_dir: std::path::PathBuf::from("/tmp"),
            base_env: std::collections::HashMap::new(),
            dry_run,
        }
    }

    // -- command resolution ----------------------------------------------------

    #[test]
    fn command_defaults_to_openclaw() {
        assert_eq!(launcher(serde_json::json!({})).command(), "openclaw");
    }

    #[test]
    fn command_uses_explicit_path_when_set() {
        let l = launcher(serde_json::json!({ "command_path": "/opt/bin/openclaw" }));
        assert_eq!(l.command(), "/opt/bin/openclaw");
    }

    #[test]
    fn validate_command_falls_back_to_path_for_bare_command_name() {
        let l = launcher(serde_json::json!({ "command_path": "ls" }));
        assert!(l.validate_command().is_ok());
    }

    // -- metadata / schema -----------------------------------------------------

    #[test]
    fn metadata_name_is_openclaw_cli() {
        let meta = OpenClawLauncher::metadata();
        assert_eq!(meta.name, "OpenClaw CLI");
        assert_eq!(meta.default_command, "openclaw");
        assert!(
            meta.supported_capabilities
                .contains(&crate::capabilities::BindingType::AgentModel)
        );
    }

    #[test]
    fn instance_id_round_trips_from_construction() {
        let l = OpenClawLauncher::new(
            "openclaw-local",
            &serde_json::json!({}),
            &crate::config::Config::default(),
        );
        assert_eq!(l.instance_id(), "openclaw-local");
    }

    #[test]
    fn config_schema_exposes_command_path() {
        use crate::launchers::base::LauncherFactory;
        let mut factory = LauncherFactory::new();
        factory.register::<OpenClawLauncher>("openclaw");
        let schema = factory.config_schema("openclaw").unwrap();
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
    async fn env_overlay_sets_openclaw_config_path_when_bound() {
        let overlay = bound(serde_json::json!({}), binding())
            .env_overlay(&ctx(false))
            .await
            .unwrap();
        let config_path = overlay
            .iter()
            .find(|b| b.key == "OPENCLAW_CONFIG_PATH")
            .expect("OPENCLAW_CONFIG_PATH env");
        assert!(config_path.value.ends_with("launcher-state/openclaw/openclaw.json"));
    }

    #[tokio::test]
    async fn env_overlay_sets_openclaw_provider_and_model() {
        let overlay = bound(serde_json::json!({}), binding())
            .env_overlay(&ctx(false))
            .await
            .unwrap();
        let provider = overlay
            .iter()
            .find(|b| b.key == "OPENCLAW_PROVIDER")
            .expect("OPENCLAW_PROVIDER env");
        assert_eq!(provider.value, "ollama");
        let model = overlay
            .iter()
            .find(|b| b.key == "OPENCLAW_MODEL")
            .expect("OPENCLAW_MODEL env");
        assert_eq!(model.value, "mistral/mistral-large-v0.2");
    }

    #[tokio::test]
    async fn env_overlay_sets_openclaw_local_endpoint() {
        let overlay = bound(serde_json::json!({}), binding())
            .env_overlay(&ctx(false))
            .await
            .unwrap();
        let endpoint = overlay
            .iter()
            .find(|b| b.key == "OPENCLAW_LOCAL_ENDPOINT")
            .expect("OPENCLAW_LOCAL_ENDPOINT env");
        assert_eq!(endpoint.value, "http://localhost:11434");
    }

    #[tokio::test]
    async fn env_overlay_sets_openclaw_max_tokens() {
        let overlay = bound(serde_json::json!({}), binding())
            .env_overlay(&ctx(false))
            .await
            .unwrap();
        let max_tokens = overlay
            .iter()
            .find(|b| b.key == "OPENCLAW_MAX_TOKENS")
            .expect("OPENCLAW_MAX_TOKENS env");
        assert_eq!(max_tokens.value, "131072");
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
        // Without a binding there is no OPENCLAW_* env override.
        assert!(!infos.iter().any(|m| m.contains("OPENCLAW_PROVIDER")));
    }
}
