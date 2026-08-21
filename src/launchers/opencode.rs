//! Launcher for the `opencode` coding agent (<https://opencode.ai>).
//!
//! Unlike `pi`, OpenCode's config sources are additive: `OPENCODE_CONFIG`
//! points at one extra file that gets merged into the chain (loaded after the
//! global config, before the project config) rather than a whole directory to
//! redirect wholesale. So this launcher never touches the user's own
//! `opencode.json`, credentials, or session store -- it only ever writes its
//! own small generated file under `GRANITE_CLI_HOME` and points
//! `OPENCODE_CONFIG` at it.

// Standard
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// Third Party
use alog::{MessageLevel, alog_channel, use_channel};
use anyhow::Context;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// Local
use crate::capabilities::{AgentModelBinding, Binding, BindingType, Capability, McpBinding};
use crate::launchers::base::{EnvBinding, LaunchContext, Launcher, LauncherMetadata, run_command};
use crate::launchers::shared::mcp_cli::mcp_binding_request;
use crate::providers::ApiType;
use crate::registry::ConfigConstructable;
use crate::utils::resolve_shell_command;
use crate::utils::ui::Ui;

use_channel!("OPNCD");

/*-- public --*/

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct OpenCodeLauncherConfig {
    /// Override path to the `opencode` binary for non-PATH installs.
    /// Leave unset to use PATH lookup.
    #[serde(default)]
    pub command_path: Option<String>,

    /// Extra keys merged (shallow, last-write-wins) into the generated
    /// provider entry -- e.g. `headers` a particular server needs. Necessary
    /// because the entry is regenerated on every launch.
    #[serde(default)]
    pub provider_overrides: Option<serde_json::Value>,
}

pub struct OpenCodeLauncher {
    instance_id: String,
    config: OpenCodeLauncherConfig,
    bound_agent_model: Option<AgentModelBinding>,
    /// `(server_name, binding)` for every MCP-capable capability bound to
    /// this launcher, written into the generated config's `mcp` block.
    bound_mcp_bindings: Vec<(String, McpBinding)>,
}

impl ConfigConstructable for OpenCodeLauncher {
    type Config = OpenCodeLauncherConfig;

    fn new(
        instance_id: &str,
        cfg: &serde_json::Value,
        _global_config: &crate::config::Config,
    ) -> Self {
        let config: OpenCodeLauncherConfig =
            serde_json::from_value(cfg.clone()).unwrap_or_default();
        Self {
            instance_id: instance_id.to_string(),
            config,
            bound_agent_model: None,
            bound_mcp_bindings: vec![],
        }
    }
}

impl crate::registry::Named for OpenCodeLauncher {
    fn instance_id(&self) -> &str {
        &self.instance_id
    }
}

#[async_trait]
impl Launcher for OpenCodeLauncher {
    fn name(&self) -> &str {
        "OpenCode CLI"
    }

    fn command(&self) -> &str {
        self.config.command_path.as_deref().unwrap_or("opencode")
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

        if capability_types.contains(&BindingType::Mcp) {
            let binding = capability.bind(mcp_binding_request()).await?;
            match binding {
                Binding::Mcp(binding) => {
                    self.bound_mcp_bindings
                        .push((capability.instance_id().to_string(), binding));
                }
                other => anyhow::bail!("expected an Mcp binding, got {:?}", other.binding_type()),
            }
            return Ok(());
        }

        // OpenCode's custom-provider config speaks whatever dialect its `npm`
        // SDK package implements. `@ai-sdk/openai-compatible` is the one every
        // granite-cli provider can serve, so that is what we ask for.
        let request = crate::capabilities::BindingRequest::AgentModel(
            crate::capabilities::AgentModelBindingRequest {
                api_type: ApiType::OpenAI,
            },
        );

        let binding = capability.bind(request).await?;
        match binding {
            Binding::AgentModel(binding) => {
                self.bound_agent_model = Some(binding);
            }
            other => anyhow::bail!(
                "expected an AgentModel binding, got {:?}",
                other.binding_type()
            ),
        }
        Ok(())
    }

    fn validate_command(&self) -> anyhow::Result<PathBuf> {
        resolve_shell_command(&self.config.command_path, "opencode")
    }

    /// Points OpenCode at the granite-cli-generated config file and supplies
    /// the credential the file interpolates.
    ///
    /// The `apiKey` is written as an environment reference
    /// (`${GRANITE_CLI_OPENCODE_API_KEY}`) rather than a literal, so the
    /// secret stays out of the generated file and off OpenCode's command
    /// line. Providers with no key omit `apiKey` entirely -- OpenCode's
    /// custom-provider config does not require one.
    async fn env_overlay(&self, ctx: &LaunchContext) -> anyhow::Result<Vec<EnvBinding>> {
        let mut overlay = vec![];
        if self.bound_agent_model.is_some() || !self.bound_mcp_bindings.is_empty() {
            overlay.push(EnvBinding {
                key: CONFIG_ENV.to_string(),
                value: opencode_config_path(ctx)?.to_string_lossy().to_string(),
            });

            if let Some(binding) = &self.bound_agent_model
                && let Some(api_key) = binding
                    .api_key
                    .as_ref()
                    .map(|api_key| api_key.0.clone())
                    .filter(|key| !key.is_empty())
            {
                overlay.push(EnvBinding {
                    key: API_KEY_ENV.to_string(),
                    value: api_key,
                });
            }
        }
        Ok(overlay)
    }

    /// Writes the granite-cli OpenCode config file, then execs `opencode`
    /// with the caller's arguments untouched.
    ///
    /// Model selection goes through the config's top-level `model` key
    /// rather than a `--model` CLI flag: that flag only exists on some of
    /// OpenCode's subcommands (`run`, `attach`, the default TUI), so
    /// injecting it ahead of an arbitrary subcommand (e.g. `models`,
    /// `agent`) would either be rejected or silently misparsed. The config
    /// key is documented to apply uniformly across all of those surfaces.
    async fn launch(
        &self,
        args: &[String],
        ctx: &LaunchContext,
        ui: &dyn Ui,
    ) -> anyhow::Result<std::process::ExitStatus> {
        if self.bound_agent_model.is_some() || !self.bound_mcp_bindings.is_empty() {
            let provider_entry = self
                .bound_agent_model
                .as_ref()
                .map(|binding| self.provider_entry(binding))
                .transpose()?;
            let config = generate_config(
                self.bound_agent_model.as_ref(),
                provider_entry,
                &self.bound_mcp_bindings,
            );
            let config_path = opencode_config_path(ctx)?;

            if ctx.dry_run {
                ui.info(&format!(
                    "Would write OpenCode config to {}:",
                    config_path.display()
                ));
                ui.info(&serde_json::to_string_pretty(&config)?);
            } else {
                write_opencode_config(&config_path, &config)?;
                ui.info(&format!(
                    "Wrote OpenCode config to {}",
                    config_path.display()
                ));
            }
        }

        let binary = self.validate_command()?;
        let overlay = self.env_overlay(ctx).await?;
        alog_channel!(MessageLevel::Debug2, "Env Overlay: {:#?}", overlay);

        run_command(binary, &overlay, args, ctx, ui).await
    }
}

impl HasOpenCodeLauncherMetadata for OpenCodeLauncher {
    fn metadata() -> LauncherMetadata {
        LauncherMetadata {
            name: "OpenCode CLI".to_string(),
            description: "OpenCode terminal coding agent".to_string(),
            default_command: "opencode".to_string(),
            supported_capabilities: HashSet::from([BindingType::AgentModel, BindingType::Mcp]),
            tags: vec!["opencode".to_string(), "coding-agent".to_string()],
        }
    }
}

/*-- private --*/

// HasOpenCodeLauncherMetadata is the macro-generated trait; re-exported via mod.rs.
use crate::launchers::base::HasLauncherMetadata as HasOpenCodeLauncherMetadata;

/// Env var OpenCode merges an extra config file from, in addition to its own
/// global/project config.
const CONFIG_ENV: &str = "OPENCODE_CONFIG";

/// Env var the generated provider entry interpolates its `apiKey` from.
const API_KEY_ENV: &str = "GRANITE_CLI_OPENCODE_API_KEY";

/// The generated config file's name, relative to the launcher state dir.
const CONFIG_FILE: &str = "opencode.json";

impl OpenCodeLauncher {
    /// Builds the `provider.<name>` entry describing the bound model.
    fn provider_entry(&self, binding: &AgentModelBinding) -> anyhow::Result<serde_json::Value> {
        let mut options = serde_json::json!({ "baseURL": opencode_base_url(binding) });
        if binding
            .api_key
            .as_ref()
            .is_some_and(|key| !key.0.is_empty())
        {
            options["apiKey"] = serde_json::Value::String(format!("{{env:{API_KEY_ENV}}}"));
        }

        // `limit` is all-or-nothing in OpenCode's schema: if present, both
        // `context` and `output` are required. granite-cli only tracks a
        // context length, so `limit` is left out entirely rather than
        // guessing an output cap.
        let mut models = serde_json::Map::new();
        models.insert(
            binding.model_name.clone(),
            serde_json::json!({
                "name": binding.model_name,
            }),
        );

        let mut entry = serde_json::json!({
            "npm": "@ai-sdk/openai-compatible",
            "name": binding.provider_name,
            "options": options,
            "models": serde_json::Value::Object(models),
        });

        // Shallow merge so a user override of e.g. `headers` doesn't clobber
        // the generated `options`/`models`, and vice versa.
        if let (Some(overrides), Some(target)) = (
            self.config
                .provider_overrides
                .as_ref()
                .and_then(serde_json::Value::as_object),
            entry.as_object_mut(),
        ) {
            for (key, value) in overrides {
                target.insert(key.clone(), value.clone());
            }
        }
        Ok(entry)
    }
}

/// OpenCode's `baseURL` is the API root the SDK appends operation paths to
/// (e.g. `/chat/completions`), so drop that trailing operation from the
/// binding's full endpoint path and keep the version prefix.
fn opencode_base_url(binding: &AgentModelBinding) -> String {
    let root = binding.base_url.trim_end_matches('/');
    let prefix = binding
        .endpoint_path
        .strip_suffix("/chat/completions")
        .unwrap_or("");
    format!("{root}{prefix}")
}

/// The granite-cli-owned config file this launcher instance writes and points
/// `OPENCODE_CONFIG` at. Lives under the launcher state dir rather than the
/// user's own OpenCode config directory -- it is never read by anything else.
fn opencode_config_path(ctx: &LaunchContext) -> anyhow::Result<PathBuf> {
    Ok(crate::config::Config::launcher_state_dir(&ctx.launcher_id)?.join(CONFIG_FILE))
}

/// Builds the top-level `opencode.json` shape: a provider entry (if bound)
/// selected via the top-level `model` key (`provider/model`) so it applies
/// uniformly across the TUI, `run`, `attach`, and GitHub Action; plus an
/// `mcp` block (if any MCP servers are bound), using opencode's
/// `McpLocalConfig`/`McpRemoteConfig` shape (see
/// <https://opencode.ai/config.json>).
fn generate_config(
    binding: Option<&AgentModelBinding>,
    provider_entry: Option<serde_json::Value>,
    mcp_bindings: &[(String, McpBinding)],
) -> serde_json::Value {
    let mut config = serde_json::json!({ "$schema": "https://opencode.ai/config.json" });
    if let (Some(binding), Some(entry)) = (binding, provider_entry) {
        let mut providers = serde_json::Map::new();
        providers.insert(binding.provider_name.clone(), entry);
        config["model"] =
            serde_json::Value::String(format!("{}/{}", binding.provider_name, binding.model_name));
        config["provider"] = serde_json::Value::Object(providers);
    }
    if !mcp_bindings.is_empty() {
        let mut mcp = serde_json::Map::new();
        for (name, binding) in mcp_bindings {
            mcp.insert(name.clone(), {
                match binding {
                    McpBinding::Stdio { command, args, env } => {
                        let mut full_command = vec![command.clone()];
                        full_command.extend(args.iter().cloned());
                        serde_json::json!({
                            "type": "local",
                            "command": full_command,
                            "environment": env,
                        })
                    }
                    McpBinding::Http { url, headers } | McpBinding::Sse { url, headers } => {
                        serde_json::json!({
                            "type": "remote",
                            "url": url,
                            "headers": headers,
                        })
                    }
                }
            });
        }
        config["mcp"] = serde_json::Value::Object(mcp);
    }
    config
}

fn write_opencode_config(path: &Path, config: &serde_json::Value) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let mut content = serde_json::to_string_pretty(config)?;
    content.push('\n');
    std::fs::write(path, content).with_context(|| format!("Failed to write {}", path.display()))
}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{Named, Secret};
    use crate::utils::ui::base::tests::CaptureUi;

    fn launcher(cfg: serde_json::Value) -> OpenCodeLauncher {
        OpenCodeLauncher::new("opencode", &cfg, &crate::config::Config::default())
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

    fn bound(cfg: serde_json::Value, binding: AgentModelBinding) -> OpenCodeLauncher {
        let mut l = launcher(cfg);
        l.bound_agent_model = Some(binding);
        l
    }

    fn ctx(dry_run: bool) -> LaunchContext {
        LaunchContext {
            launcher_id: "opencode".to_string(),
            working_dir: PathBuf::from("/tmp"),
            base_env: std::collections::HashMap::new(),
            dry_run,
        }
    }

    // -- command resolution ----------------------------------------------------

    #[test]
    fn command_defaults_to_opencode() {
        assert_eq!(launcher(serde_json::json!({})).command(), "opencode");
    }

    #[test]
    fn command_uses_explicit_path_when_set() {
        let l = launcher(serde_json::json!({ "command_path": "/opt/bin/opencode" }));
        assert_eq!(l.command(), "/opt/bin/opencode");
    }

    #[test]
    fn validate_command_err_for_nonexistent_explicit_path() {
        let l = launcher(serde_json::json!({ "command_path": "/no/such/path/opencode" }));
        assert!(l.validate_command().is_err());
    }

    #[test]
    fn validate_command_falls_back_to_path_for_bare_command_name() {
        let l = launcher(serde_json::json!({ "command_path": "ls" }));
        assert!(l.validate_command().is_ok());
    }

    // -- metadata / schema -----------------------------------------------------

    #[test]
    fn metadata_name_is_opencode_cli() {
        let meta = OpenCodeLauncher::metadata();
        assert_eq!(meta.name, "OpenCode CLI");
        assert_eq!(meta.default_command, "opencode");
        assert!(
            meta.supported_capabilities
                .contains(&BindingType::AgentModel)
        );
    }

    #[test]
    fn instance_id_round_trips_from_construction() {
        let l = OpenCodeLauncher::new(
            "opencode-local",
            &serde_json::json!({}),
            &crate::config::Config::default(),
        );
        assert_eq!(l.instance_id(), "opencode-local");
    }

    #[test]
    fn config_schema_exposes_only_command_path_and_overrides() {
        use crate::launchers::base::LauncherFactory;
        let mut factory = LauncherFactory::new();
        factory.register::<OpenCodeLauncher>("opencode");
        let schema = factory.config_schema("opencode").unwrap();
        let props = schema
            .get("properties")
            .and_then(|p| p.as_object())
            .unwrap();
        assert!(props.contains_key("command_path"));
        assert!(props.contains_key("provider_overrides"));
        // The OpenCode provider name comes from the binding, never from
        // launcher config.
        assert!(!props.contains_key("provider_name"));
    }

    // -- provider entry --------------------------------------------------------

    #[test]
    fn provider_entry_describes_bound_model() {
        let entry = launcher(serde_json::json!({}))
            .provider_entry(&binding())
            .unwrap();
        assert_eq!(entry["npm"], "@ai-sdk/openai-compatible");
        assert_eq!(entry["options"]["baseURL"], "http://localhost:11434/v1");
        assert_eq!(entry["models"]["granite4.1:8b"]["name"], "granite4.1:8b");
        // No output-token data means no `limit` at all: OpenCode requires
        // both `context` and `output` together when `limit` is present.
        assert!(entry["models"]["granite4.1:8b"].get("limit").is_none());
        // No key means no apiKey field at all.
        assert!(entry["options"].get("apiKey").is_none());
    }

    #[test]
    fn provider_entry_interpolates_env_when_key_present() {
        let b = AgentModelBinding {
            api_key: Some(Secret::from("sk-test")),
            ..binding()
        };
        let entry = launcher(serde_json::json!({})).provider_entry(&b).unwrap();
        assert_eq!(
            entry["options"]["apiKey"],
            "{env:GRANITE_CLI_OPENCODE_API_KEY}"
        );
    }

    #[test]
    fn provider_entry_omits_api_key_for_empty_secret() {
        let b = AgentModelBinding {
            api_key: Some(Secret::from("")),
            ..binding()
        };
        let entry = launcher(serde_json::json!({})).provider_entry(&b).unwrap();
        assert!(entry["options"].get("apiKey").is_none());
    }

    #[test]
    fn provider_entry_merges_overrides() {
        let l = launcher(serde_json::json!({
            "provider_overrides": { "headers": { "X-Custom": "1" } }
        }));
        let entry = l.provider_entry(&binding()).unwrap();
        assert_eq!(entry["headers"]["X-Custom"], "1");
        // Generated keys survive the merge.
        assert_eq!(entry["options"]["baseURL"], "http://localhost:11434/v1");
    }

    #[test]
    fn provider_entry_overrides_win_on_conflict() {
        let l = launcher(serde_json::json!({
            "provider_overrides": { "npm": "@ai-sdk/openai" }
        }));
        let entry = l.provider_entry(&binding()).unwrap();
        assert_eq!(entry["npm"], "@ai-sdk/openai");
    }

    // -- base url ----------------------------------------------------------

    #[test]
    fn base_url_keeps_version_prefix_and_drops_operation() {
        assert_eq!(opencode_base_url(&binding()), "http://localhost:11434/v1");
    }

    #[test]
    fn base_url_trims_trailing_slash_from_provider_url() {
        let b = AgentModelBinding {
            base_url: "http://localhost:1234/".to_string(),
            ..binding()
        };
        assert_eq!(opencode_base_url(&b), "http://localhost:1234/v1");
    }

    // -- generate_config -----------------------------------------------------

    #[test]
    fn generate_config_nests_entry_under_provider_name_and_sets_default_model() {
        let entry = serde_json::json!({ "npm": "@ai-sdk/openai-compatible" });
        let config = generate_config(Some(&binding()), Some(entry), &[]);
        assert_eq!(config["$schema"], "https://opencode.ai/config.json");
        assert_eq!(config["model"], "my-ollama/granite4.1:8b");
        assert_eq!(
            config["provider"]["my-ollama"]["npm"],
            "@ai-sdk/openai-compatible"
        );
    }

    #[test]
    fn generate_config_writes_mcp_block_without_a_model_binding() {
        let mcp_binding = McpBinding::Http {
            url: "http://127.0.0.1:9999".to_string(),
            headers: Default::default(),
        };
        let config = generate_config(None, None, &[("vision".to_string(), mcp_binding)]);
        assert!(config.get("model").is_none());
        assert_eq!(config["mcp"]["vision"]["type"], "remote");
        assert_eq!(config["mcp"]["vision"]["url"], "http://127.0.0.1:9999");
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
    async fn env_overlay_redirects_config_and_exports_api_key() {
        let b = AgentModelBinding {
            api_key: Some(Secret::from("sk-test")),
            ..binding()
        };
        let overlay = bound(serde_json::json!({}), b)
            .env_overlay(&ctx(false))
            .await
            .unwrap();

        let config = overlay
            .iter()
            .find(|b| b.key == "OPENCODE_CONFIG")
            .expect("config redirect");
        assert!(
            config
                .value
                .ends_with("launcher-state/opencode/opencode.json"),
            "{}",
            config.value
        );

        let key = overlay
            .iter()
            .find(|b| b.key == "GRANITE_CLI_OPENCODE_API_KEY")
            .expect("api key");
        assert_eq!(key.value, "sk-test");
    }

    #[tokio::test]
    async fn env_overlay_omits_api_key_when_provider_has_none() {
        let overlay = bound(serde_json::json!({}), binding())
            .env_overlay(&ctx(false))
            .await
            .unwrap();
        assert!(
            !overlay
                .iter()
                .any(|b| b.key == "GRANITE_CLI_OPENCODE_API_KEY")
        );
    }

    // -- launch ----------------------------------------------------------------

    // Deliberately reads whatever `GRANITE_CLI_HOME` is ambient rather than
    // setting it: env mutation would race the other tests in this binary that
    // point that var at their own tempdirs.
    #[tokio::test]
    async fn dry_run_launch_reports_without_writing_anything() {
        let state_dir = crate::config::Config::launcher_state_dir("opencode").unwrap();
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
                .any(|m| m.contains("Would write OpenCode config")),
            "expected a dry-run notice, got {infos:?}"
        );
        assert!(
            infos
                .iter()
                .any(|m| m.contains(r#""model": "my-ollama/granite4.1:8b""#)),
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
        assert!(infos.iter().any(|m| m.contains("args: --version")));
        assert!(
            !infos
                .iter()
                .any(|m| m.contains("Would write OpenCode config"))
        );
        // Without a binding there is no generated config, so OpenCode keeps
        // using its own config chain.
        assert!(!infos.iter().any(|m| m.contains(CONFIG_ENV)));
    }
}
