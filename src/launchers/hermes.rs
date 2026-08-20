use crate::capabilities::{AgentModelBinding, ApiType, Capability};
use crate::launchers::base::{EnvBinding, LaunchContext, Launcher, LauncherMetadata};
use crate::registry::ConfigConstructable;
use crate::utils::resolve_shell_command;
use anyhow::Context;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/*-- public --*/

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct HermesLauncherConfig {
    /// Override path to the `hermes` binary for non-PATH installs.
    /// Leave unset to use PATH lookup.
    #[serde(default)]
    pub command_path: Option<String>,

    /// Extra keys merged (shallow, last-write-wins) into the generated
    /// hermes config file -- e.g. `model.context_length` or provider-specific
    /// overrides. Necessary because the entry is regenerated on every launch.
    #[serde(default)]
    pub model_overrides: Option<serde_json::Value>,
}

pub struct HermesLauncher {
    instance_id: String,
    config: HermesLauncherConfig,
    bound_binding: Option<AgentModelBinding>,
}

impl ConfigConstructable for HermesLauncher {
    type Config = HermesLauncherConfig;

    fn new(
        instance_id: &str,
        cfg: &serde_json::Value,
        _global_config: &crate::config::Config,
    ) -> Self {
        let config: HermesLauncherConfig = serde_json::from_value(cfg.clone()).unwrap_or_default();
        Self {
            instance_id: instance_id.to_string(),
            config,
            bound_binding: None,
        }
    }
}

impl crate::registry::Named for HermesLauncher {
    fn instance_id(&self) -> &str {
        &self.instance_id
    }
}

#[async_trait]
impl Launcher for HermesLauncher {
    fn name(&self) -> &str {
        "Hermes CLI"
    }

    fn command(&self) -> &str {
        self.config.command_path.as_deref().unwrap_or("hermes")
    }

    async fn bind_capability(&mut self, capability: &dyn Capability) -> anyhow::Result<()> {
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
        resolve_shell_command(&self.config.command_path, "hermes")
    }

    async fn env_overlay(&self, ctx: &LaunchContext) -> anyhow::Result<Vec<EnvBinding>> {
        let Some(_binding) = &self.bound_binding else {
            return Ok(vec![]);
        };
        let overlay = vec![EnvBinding {
            key: "HERMES_HOME".to_string(),
            value: hermes_config_path(ctx)?.to_string_lossy().to_string(),
        }];
        Ok(overlay)
    }

    async fn launch(
        &self,
        args: &[String],
        ctx: &LaunchContext,
        ui: &dyn crate::utils::ui::Ui,
    ) -> anyhow::Result<std::process::ExitStatus> {
        let binary = self.validate_command()?;

        if let Some(binding) = &self.bound_binding {
            let config = self.generate_config(binding)?;
            let config_path = hermes_config_path(ctx)?;

            if ctx.dry_run {
                ui.info(&format!(
                    "Would write Hermes config to {}:",
                    config_path.display()
                ));
                ui.info(&serde_json::to_string_pretty(&config)?);
            } else {
                write_hermes_config(&config_path, &config)?;
                ui.info(&format!(
                    "Wrote Hermes config to {}",
                    config_path.display()
                ));
            }
        }

        crate::launchers::base::run_command(binary, &self.env_overlay(ctx).await?, args, ctx, ui).await
    }
}

impl HasHermesLauncherMetadata for HermesLauncher {
    fn metadata() -> LauncherMetadata {
        LauncherMetadata {
            name: "Hermes CLI".to_string(),
            description: "Hermes Agent local CLI launcher".to_string(),
            default_command: "hermes".to_string(),
            supported_capabilities: HashSet::from([crate::capabilities::BindingType::AgentModel]),
            tags: vec!["hermes".to_string(), "agent".to_string()],
        }
    }
}

/*-- private --*/

use crate::launchers::base::HasLauncherMetadata as HasHermesLauncherMetadata;

/// Env var Hermes reads to locate its config directory.
const HERMES_HOME_ENV: &str = "HERMES_HOME";

/// The generated Hermes config file's name, relative to the launcher state dir.
const HERMES_CONFIG_FILE: &str = "config.yaml";

impl HermesLauncher {
    /// Builds the Hermes config describing the bound model.
    fn generate_config(&self, binding: &AgentModelBinding) -> anyhow::Result<serde_json::Value> {
        let mut config = serde_json::json!({
            "model": {
                "provider": binding.provider_name,
                "model": binding.model_name,
            }
        });

        if !binding.base_url.is_empty() {
            config["model"]["base_url"] = serde_json::Value::String(binding.base_url.clone());
        }
        if let Some(context_length) = binding.context_length {
            config["model"]["context_length"] = serde_json::json!(context_length);
        }

        // Merge user-provided overrides on top so they win on conflict.
        // Special case: if the override key is "model", merge the inner object
        // into config["model"] rather than replacing it entirely.
        if let Some(overrides) = self.config
            .model_overrides
            .as_ref()
            .and_then(serde_json::Value::as_object)
        {
            for (key, value) in overrides {
                if key == "model" {
                    if let Some(inner) = value.as_object() {
                        if let Some(target) = config.get_mut("model").and_then(serde_json::Value::as_object_mut) {
                            for (k, v) in inner {
                                target.insert(k.clone(), v.clone());
                            }
                        }
                    }
                } else {
                    if let Some(target) = config.as_object_mut() {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        Ok(config)
    }
}

/// The Hermes config file this launcher instance writes and points `HERMES_HOME` at.
/// Lives under the launcher state dir rather than the user's own Hermes config
/// directory — it is never read by anything else.
fn hermes_config_path(ctx: &LaunchContext) -> anyhow::Result<PathBuf> {
    Ok(crate::config::Config::launcher_state_dir(&ctx.launcher_id)?
        .join(HERMES_CONFIG_FILE))
}

/// Wraps the bound model info in the top-level hermes config shape.
fn write_hermes_config(path: &Path, config: &serde_json::Value) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let mut content = serde_json::to_string_pretty(config)?;
    content.push('\n');
    std::fs::write(path, content)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::BindingType;
    use crate::registry::Named;
    use crate::utils::ui::base::tests::CaptureUi;

    fn launcher(cfg: serde_json::Value) -> HermesLauncher {
        HermesLauncher::new("hermes", &cfg, &crate::config::Config::default())
    }

    fn binding() -> AgentModelBinding {
        AgentModelBinding {
            api_type: ApiType::OpenAI,
            provider_name: "my-ollama".to_string(),
            base_url: "http://localhost:11434".to_string(),
            model_name: "granite4.1:8b".to_string(),
            endpoint_path: "/v1/chat/completions".to_string(),
            api_key: None,
            verify_ssl: true,
            context_length: Some(131072),
            temperature: None,
        }
    }

    fn bound(cfg: serde_json::Value, binding: AgentModelBinding) -> HermesLauncher {
        let mut l = launcher(cfg);
        l.bound_binding = Some(binding);
        l
    }

    fn ctx(dry_run: bool) -> LaunchContext {
        LaunchContext {
            launcher_id: "hermes".to_string(),
            working_dir: PathBuf::from("/tmp"),
            base_env: std::collections::HashMap::new(),
            dry_run,
        }
    }

    // -- command resolution ----------------------------------------------------

    #[test]
    fn command_defaults_to_hermes() {
        assert_eq!(launcher(serde_json::json!({})).command(), "hermes");
    }

    #[test]
    fn command_uses_explicit_path_when_set() {
        let l = launcher(serde_json::json!({ "command_path": "/opt/bin/hermes" }));
        assert_eq!(l.command(), "/opt/bin/hermes");
    }

    #[test]
    fn validate_command_err_for_nonexistent_explicit_path() {
        let l = launcher(serde_json::json!({ "command_path": "/no/such/path/hermes" }));
        assert!(l.validate_command().is_err());
    }

    #[test]
    fn validate_command_falls_back_to_path_for_bare_command_name() {
        let l = launcher(serde_json::json!({ "command_path": "ls" }));
        assert!(l.validate_command().is_ok());
    }

    // -- metadata / schema -----------------------------------------------------

    #[test]
    fn metadata_name_is_hermes_cli() {
        let meta = HermesLauncher::metadata();
        assert_eq!(meta.name, "Hermes CLI");
        assert_eq!(meta.default_command, "hermes");
        assert!(
            meta.supported_capabilities
                .contains(&BindingType::AgentModel)
        );
    }

    #[test]
    fn instance_id_round_trips_from_construction() {
        let l = HermesLauncher::new(
            "hermes-local",
            &serde_json::json!({}),
            &crate::config::Config::default(),
        );
        assert_eq!(l.instance_id(), "hermes-local");
    }

    #[test]
    fn config_schema_exposes_command_path_and_model_overrides() {
        use crate::launchers::base::LauncherFactory;
        let mut factory = LauncherFactory::new();
        factory.register::<HermesLauncher>("hermes");
        let schema = factory.config_schema("hermes").unwrap();
        let props = schema
            .get("properties")
            .and_then(|p| p.as_object())
            .unwrap();
        assert!(props.contains_key("command_path"));
        assert!(props.contains_key("model_overrides"));
    }

    // -- generate_config ------------------------------------------------------

    #[test]
    fn generate_config_sets_provider_and_model() {
        let l = launcher(serde_json::json!({}));
        let config = l.generate_config(&binding()).unwrap();
        assert_eq!(config["model"]["provider"], "my-ollama");
        assert_eq!(config["model"]["model"], "granite4.1:8b");
        assert_eq!(config["model"]["base_url"], "http://localhost:11434");
        assert_eq!(config["model"]["context_length"], serde_json::json!(131072));
    }

    #[test]
    fn generate_config_omits_context_length_when_none() {
        let b = AgentModelBinding {
            context_length: None,
            ..binding()
        };
        let l = launcher(serde_json::json!({}));
        let config = l.generate_config(&b).unwrap();
        assert!(!config["model"].get("context_length").is_some());
    }

    #[test]
    fn generate_config_merges_overrides() {
        let l = launcher(serde_json::json!({
            "model_overrides": {
                "model": {
                    "context_length": 4000
                }
            }
        }));
        let config = l.generate_config(&binding()).unwrap();
        // Override wins
        assert_eq!(config["model"]["context_length"], serde_json::json!(4000));
        // Generated keys survive the merge
        assert_eq!(config["model"]["provider"], "my-ollama");
        assert_eq!(config["model"]["model"], "granite4.1:8b");
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
    async fn env_overlay_sets_hermes_home_when_bound() {
        let overlay = bound(serde_json::json!({}), binding())
            .env_overlay(&ctx(false))
            .await
            .unwrap();
        let home = overlay
            .iter()
            .find(|b| b.key == "HERMES_HOME")
            .expect("HERMES_HOME env");
        assert!(
            home.value.ends_with("launcher-state/hermes/config.yaml"),
            "{}",
            home.value
        );
    }

    // -- launch ---------------------------------------------------------------

    // Deliberately reads whatever `GRANITE_CLI_HOME` is ambient rather than
    // setting it: env mutation would race the other tests in this binary that
    // point that var at their own tempdirs.
    #[tokio::test]
    async fn dry_run_launch_reports_without_writing_anything() {
        let state_dir = crate::config::Config::launcher_state_dir("hermes").unwrap();
        let existed_before = state_dir.exists();

        let l = bound(serde_json::json!({ "command_path": "ls" }), binding());
        let ui = CaptureUi::default();
        let status = l
            .launch(&["--help".to_string()], &ctx(true), &ui)
            .await
            .unwrap();
        assert!(status.success());

        let infos = ui.infos.borrow();
        assert!(
            infos
                .iter()
                .any(|m| m.contains("Would write Hermes config")),
            "expected a dry-run notice, got {infos:?}"
        );
        assert!(
            infos
                .iter()
                .any(|m| m.contains(r#""provider": "my-ollama""#) && m.contains(r#""model": "granite4.1:8b""#)),
            "expected the generated config to select the model, got {infos:?}"
        );
        assert!(
            infos.iter().any(|m| m.contains("args: --help")),
            "expected caller args to pass through unmodified, got {infos:?}"
        );
        assert_eq!(
            state_dir.exists(),
            existed_before,
            "dry run must not create {}",
            state_dir.display()
        );
    }

    #[tokio::test]
    async fn launch_without_binding_passes_args_through_unchanged() {
        let l = launcher(serde_json::json!({ "command_path": "ls" }));
        let ui = CaptureUi::default();
        l.launch(&["--version".to_string()], &ctx(true), &ui)
            .await
            .unwrap();

        let infos = ui.infos.borrow();
        assert!(infos.iter().any(|m| m.contains("--version")));
        assert!(
            !infos.iter().any(|m| m.contains("Would write Hermes config")),
            "expected no config write without binding"
        );
        // Without a binding there is no generated config, so hermes keeps
        // using its own config chain.
        assert!(!infos.iter().any(|m| m.contains(HERMES_HOME_ENV)));
    }
}
