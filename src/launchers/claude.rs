// Standard
use std::collections::HashSet;
use std::path::PathBuf;

// Third Party
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// Local
use crate::capabilities::{Binding, BindingType, Capability, McpBinding};
use crate::launchers::base::HasLauncherMetadata as HasClaudeLauncherMetadata;
use crate::launchers::base::{EnvBinding, LaunchContext, Launcher, LauncherMetadata, run_command};
use crate::launchers::shared::mcp_cli::{
    mcp_binding_request, register_mcp_server, remove_mcp_server,
};
use crate::registry::ConfigConstructable;
use crate::utils::resolve_shell_command;
use crate::utils::ui::Ui;

/*-- public --*/

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct ClaudeLauncherConfig {
    /// Override path to the `claude` binary for non-PATH installs.
    /// Leave unset to use PATH lookup.
    #[serde(default)]
    pub command_path: Option<String>,
}

pub struct ClaudeLauncher {
    instance_id: String,
    config: ClaudeLauncherConfig,
    bound_agent_model: Option<crate::capabilities::AgentModelBinding>,
    /// `(server_name, binding)` for every MCP-capable capability bound to
    /// this launcher, registered/removed around `run_command` in `launch()`.
    bound_mcp_bindings: Vec<(String, McpBinding)>,
}

impl ConfigConstructable for ClaudeLauncher {
    type Config = ClaudeLauncherConfig;

    fn new(
        instance_id: &str,
        cfg: &serde_json::Value,
        _global_config: &crate::config::Config,
    ) -> Self {
        let config: ClaudeLauncherConfig = serde_json::from_value(cfg.clone()).unwrap_or_default();
        Self {
            instance_id: instance_id.to_string(),
            config,
            bound_agent_model: None,
            bound_mcp_bindings: vec![],
        }
    }
}

impl crate::registry::Named for ClaudeLauncher {
    fn instance_id(&self) -> &str {
        &self.instance_id
    }
}

#[async_trait]
impl Launcher for ClaudeLauncher {
    fn name(&self) -> &str {
        "Claude CLI"
    }

    fn command(&self) -> &str {
        self.config.command_path.as_deref().unwrap_or("claude")
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

        // Claude knows it expects Anthropic API type
        let request = crate::capabilities::BindingRequest::AgentModel(
            crate::capabilities::AgentModelBindingRequest {
                api_type: crate::providers::ApiType::Anthropic,
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
        resolve_shell_command(&self.config.command_path, "claude")
    }

    async fn env_overlay(&self, _ctx: &LaunchContext) -> anyhow::Result<Vec<EnvBinding>> {
        if let Some(binding) = &self.bound_agent_model {
            let mut api_key_val = match &binding.api_key {
                Some(api_key) => api_key.clone().0,
                _ => "".to_string(),
            };
            if api_key_val.is_empty() {
                api_key_val = "unset".to_string(); // Claude treats empty strings like unset
            }
            let bindings = vec![
                EnvBinding {
                    key: "ANTHROPIC_BASE_URL".to_string(),
                    value: binding.base_url.clone(),
                },
                EnvBinding {
                    key: "ANTHROPIC_MODEL".to_string(),
                    value: binding.model_name.clone(),
                },
                EnvBinding {
                    key: "CLAUDE_CODE_MAX_CONTEXT_TOKENS".to_string(),
                    value: binding.context_length.map_or(String::new(), |v| v.to_string()),
                },
                EnvBinding {
                    key: "ANTHROPIC_AUTH_TOKEN".to_string(),
                    value: api_key_val,
                },
            ];
            // verify_ssl is dropped per user's note
            Ok(bindings)
        } else {
            Ok(vec![])
        }
    }

    /// Registers each bound MCP server with `claude mcp add-json` (scoped
    /// `local` so it only applies to this invocation) before exec'ing, and
    /// best-effort removes them again afterwards -- failure to clean up is
    /// logged, not propagated, since the launch itself already succeeded or
    /// failed on its own terms by that point.
    async fn launch(
        &self,
        args: &[String],
        ctx: &LaunchContext,
        ui: &dyn Ui,
    ) -> anyhow::Result<std::process::ExitStatus> {
        let binary = self.validate_command()?;
        let overlay = self.env_overlay(ctx).await?;

        const SCOPE: &[&str] = &["--scope", "local"];
        for (name, binding) in &self.bound_mcp_bindings {
            register_mcp_server(&binary, name, binding, SCOPE, ctx, ui)?;
        }

        let result = run_command(binary.clone(), &overlay, args, ctx, ui).await;

        for (name, _) in &self.bound_mcp_bindings {
            remove_mcp_server(&binary, name, SCOPE, ctx, ui);
        }

        result
    }
}

impl HasClaudeLauncherMetadata for ClaudeLauncher {
    fn metadata() -> LauncherMetadata {
        LauncherMetadata {
            name: "Claude CLI".to_string(),
            description: "Anthropic's Claude CLI tool".to_string(),
            default_command: "claude".to_string(),
            supported_capabilities: HashSet::from([BindingType::AgentModel, BindingType::Mcp]),
            tags: vec!["claude".to_string(), "anthropic".to_string()],
        }
    }
}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_defaults_to_claude() {
        let l = ClaudeLauncher::new(
            "my-claude",
            &serde_json::json!({}),
            &crate::config::Config::default(),
        );
        assert_eq!(l.command(), "claude");
    }

    #[test]
    fn command_uses_explicit_path_when_set() {
        let l = ClaudeLauncher::new(
            "my-claude",
            &serde_json::json!({
                "command_path": "/opt/bin/claude"
            }),
            &crate::config::Config::default(),
        );
        assert_eq!(l.command(), "/opt/bin/claude");
    }

    #[test]
    fn validate_command_err_for_nonexistent_explicit_path() {
        let l = ClaudeLauncher::new(
            "my-claude",
            &serde_json::json!({
                "command_path": "/no/such/path/claude"
            }),
            &crate::config::Config::default(),
        );
        assert!(l.validate_command().is_err());
    }

    #[test]
    fn validate_command_falls_back_to_path_for_bare_command_name() {
        let l = ClaudeLauncher::new(
            "my-claude",
            &serde_json::json!({
                "command_path": "ls"
            }),
            &crate::config::Config::default(),
        );
        assert!(l.validate_command().is_ok());
    }

    #[test]
    fn metadata_name_is_claude_cli() {
        let meta = ClaudeLauncher::metadata();
        assert_eq!(meta.name, "Claude CLI");
        assert_eq!(meta.default_command, "claude");
    }

    #[test]
    fn config_schema_is_present() {
        use crate::launchers::base::LauncherFactory;
        let mut factory = LauncherFactory::new();
        factory.register::<ClaudeLauncher>("claude");
        let schema = factory.config_schema("claude").unwrap();
        // Schema should reference ClaudeLauncherConfig properties
        let props = schema.get("properties").and_then(|p| p.as_object());
        assert!(props.is_some());
        assert!(props.unwrap().contains_key("command_path"));
    }
}
