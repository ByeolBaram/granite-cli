use crate::registry::ConfigConstructable;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/*-- public --*/

/// Core trait for launcher implementations.
/// All launchers must implement this trait along with ConfigConstructable.
#[async_trait]
pub trait Launcher: ConfigConstructable + Send + Sync {
    fn name(&self) -> &str;

    /// The binary/command this instance will exec.
    /// Returns the full command string — either a bare binary name for PATH
    /// lookup (e.g. `"claude"`) or an absolute path set by the user in config.
    fn command(&self) -> &str;

    /// Capability IDs this launcher type supports.
    /// Default implementation delegates to `metadata()` so there is no need
    /// to duplicate the list on the instance.
    fn supported_capabilities(&self) -> Vec<String>;

    /// Resolve the binary to an absolute path.
    ///
    /// Checks the optional `command_path` override baked into the instance at
    /// construction time first, then falls back to a PATH lookup.
    ///
    /// **Not called during construction** — callers invoke this explicitly so
    /// that `catalog` and `list` work even when the tool is not installed.
    fn validate_command(&self) -> anyhow::Result<PathBuf>;

    /// Build the environment variable overlay for this launch.
    ///
    /// The default implementation returns an empty vec; concrete launchers
    /// override this once capability hooks are wired up.
    async fn env_overlay(&self, _ctx: &LaunchContext) -> anyhow::Result<Vec<EnvBinding>> {
        Ok(vec![])
    }

    /// Exec the tool as a subprocess with the env overlay applied.
    ///
    /// The default implementation covers the common case: resolve binary,
    /// build overlay, merge with current process env, spawn, wait.
    /// Concrete launchers override only when they need non-standard behaviour.
    async fn launch(
        &self,
        args: &[String],
        ctx: &LaunchContext,
    ) -> anyhow::Result<std::process::ExitStatus> {
        let binary = self.validate_command()?;
        let overlay = self.env_overlay(ctx).await?;

        let mut cmd = std::process::Command::new(&binary);
        cmd.args(args);
        for binding in &overlay {
            cmd.env(&binding.key, &binding.value);
        }

        Ok(cmd.spawn()?.wait()?)
    }
}

/// Metadata describing a launcher implementation (type-level, catalog entry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherMetadata {
    pub name: String,
    pub description: String,
    /// The default binary name used for PATH lookup (e.g. `"claude"`, `"bob"`).
    pub default_command: String,
    /// Capability IDs this launcher type can make use of.
    pub supported_capabilities: Vec<String>,
    pub tags: Vec<String>,
}

impl std::fmt::Display for LauncherMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description)
    }
}

/// Runtime context passed through the launch lifecycle.
pub struct LaunchContext {
    pub launcher_id: String,
    /// The working directory for the spawned subprocess (i.e. the CWD the
    /// tool process will see). Typically set to the user's current directory.
    pub working_dir: PathBuf,
    /// Env vars already resolved (e.g. provider URL, model ID) before any
    /// capability bindings are merged on top.
    pub base_env: HashMap<String, String>,
}

/// A single environment variable binding contributed to the subprocess overlay.
pub struct EnvBinding {
    pub key: String,
    pub value: String,
}

/*-- private --*/

use crate::define_factory;

define_factory!(Launcher, LauncherMetadata, LauncherFactory);

/*-- tests --*/

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::registry::ConfigConstructable;

    /// Minimal Launcher implementation used only in tests.
    pub(crate) struct FakeLauncher {
        /// When `Some`, `validate_command` resolves to this path directly.
        command_path: Option<PathBuf>,
        command_name: String,
    }

    impl ConfigConstructable for FakeLauncher {
        fn new(cfg: &serde_json::Value) -> Self {
            let command_name = cfg
                .get("command_name")
                .and_then(|v| v.as_str())
                .unwrap_or("fake-binary-that-does-not-exist")
                .to_string();
            let command_path = cfg
                .get("command_path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from);
            Self {
                command_name,
                command_path,
            }
        }
    }

    #[async_trait]
    impl Launcher for FakeLauncher {
        fn name(&self) -> &str {
            "Fake Launcher"
        }

        fn command(&self) -> &str {
            &self.command_name
        }

        fn supported_capabilities(&self) -> Vec<String> {
            Self::metadata().supported_capabilities
        }

        fn validate_command(&self) -> anyhow::Result<PathBuf> {
            crate::utils::resolve_shell_command(
                &self.command_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                &self.command_name,
            )
        }
    }

    impl HasLauncherMetadata for FakeLauncher {
        fn metadata() -> LauncherMetadata {
            LauncherMetadata {
                name: "Fake Launcher".to_string(),
                description: "Test double".to_string(),
                default_command: "fake-binary-that-does-not-exist".to_string(),
                supported_capabilities: vec![],
                tags: vec![],
            }
        }
    }

    #[test]
    fn validate_command_returns_err_for_unknown_binary() {
        let launcher = FakeLauncher::new(&serde_json::json!({
            "command_name": "this-binary-absolutely-does-not-exist-9x7z"
        }));
        assert!(launcher.validate_command().is_err());
    }

    #[test]
    fn validate_command_returns_err_for_nonexistent_explicit_path() {
        let launcher = FakeLauncher::new(&serde_json::json!({
            "command_name": "fake",
            "command_path": "/this/path/does/not/exist/fake"
        }));
        assert!(launcher.validate_command().is_err());
    }

    #[test]
    fn validate_command_falls_back_to_path_for_bare_command_name() {
        let launcher = FakeLauncher::new(&serde_json::json!({
            "command_path": "ls"
        }));
        assert!(launcher.validate_command().is_ok());
    }

    #[tokio::test]
    async fn env_overlay_default_is_empty() {
        let launcher = FakeLauncher::new(&serde_json::json!({}));
        let ctx = LaunchContext {
            launcher_id: "test".to_string(),
            working_dir: PathBuf::from("/tmp"),
            base_env: HashMap::new(),
        };
        let overlay = launcher.env_overlay(&ctx).await.unwrap();
        assert!(overlay.is_empty());
    }

    #[test]
    fn launcher_factory_register_and_get() {
        let mut factory = LauncherFactory::new();
        factory.register::<FakeLauncher>("fake");
        assert!(factory.get("fake").is_some());
        assert!(factory.get("nonexistent").is_none());
    }

    #[test]
    fn launcher_factory_construct() {
        let mut factory = LauncherFactory::new();
        factory.register::<FakeLauncher>("fake");
        let result = factory.construct("fake", &serde_json::json!({}));
        assert!(result.is_ok());
    }

    #[test]
    fn launcher_metadata_display() {
        let meta = LauncherMetadata {
            name: "Test".to_string(),
            description: "A test launcher".to_string(),
            default_command: "test".to_string(),
            supported_capabilities: vec![],
            tags: vec![],
        };
        assert_eq!(meta.to_string(), "A test launcher");
    }
}
