# Spec 0014: Launcher Architecture

## Overview

This spec implements the core `Launcher` architecture described in issue #20. A **Launcher** is the outward-facing counterpart to a **Provider**: where a Provider answers "which inference server do I talk to?", a Launcher answers "which external AI tool do I exec, and how do I configure it?". Granite-cli bridges the two by injecting an env-var overlay into the tool's subprocess environment so the tool talks to the right provider with the right capabilities active — without modifying the tool's own config files.

The Launcher layer follows the identical three-file pattern established by Providers and Capabilities (`base.rs` / `mod.rs` / concrete impls) and reuses the `define_factory!` macro unchanged.

## Goals

1. Define `trait Launcher` and `LauncherMetadata` mirroring the Provider pattern
2. Add `LauncherConfig` to the config system with per-directory persistence
3. Implement two concrete launchers: `ClaudeLauncher` and `BobLauncher`
4. Add command validation with user-overridable binary path
5. Wire named-instance setup wizard (default ID = type name; detect same-type clash)
6. Wire the existing `Commands::Launch` stub to actually exec the tool with an env overlay
7. Add `launcher` subcommand (`catalog`, `list`, `setup`, `validate`)

## Non-Goals

- The `configure <tool>` wizard (capability binding during setup) — separate issue
- Capability `on_pre_launch` / `on_shutdown` hooks invocation — deferred until capabilities are concrete
- Shell export (`--export`) for launcher env vars — covered by the existing export infrastructure
- Any provider failover logic at launch time — deferred

## Pattern Divergences

Two intentional divergences from the Provider pattern are called out explicitly:

1. **Type-aware instance clash detection.** Provider setup only checks whether the exact instance ID already exists. Launcher setup additionally scans all configured launchers for any entry with the same `launcher_type` and, if one is found, offers "update existing" vs. "create new with a different name". This is required by the issue's UX spec.

2. **Binary validation at construction time.** Providers are always reachable over HTTP so construction is always cheap. `Launcher::validate_command()` does a filesystem/PATH lookup and must be called explicitly — it is not called inside `ConfigConstructable::new` — so that the factory can construct launchers for catalog/list display without requiring the binary to be installed.

---

## Commit 1 — `src/launchers/base.rs`: trait, metadata, supporting types

**Files changed:** `src/launchers/base.rs` *(new)*

Define the core trait and all supporting types. No concrete impls, no registration.

### `LauncherMetadata`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherMetadata {
    pub name: String,
    pub description: String,
    /// The default binary name looked up on PATH (e.g. `"claude"`, `"bob"`).
    pub default_command: String,
    /// Capability IDs this launcher type can make use of.
    pub supported_capabilities: Vec<String>,
    pub tags: Vec<String>,
}
```

### `trait Launcher`

```rust
#[async_trait]
pub trait Launcher: ConfigConstructable + Send + Sync {
    fn name(&self) -> &str;

    /// The binary/command this instance will exec (may be overridden by config).
    fn command_name(&self) -> &str;

    /// Capability IDs this launcher instance supports.
    fn supported_capabilities(&self) -> Vec<String>;

    /// Resolve the binary to an absolute path.
    ///
    /// Checks `LauncherConfig.command_path` override first, then PATH.
    /// Returns `Err` with an actionable message if the binary cannot be found.
    /// **Not called during construction** — see Pattern Divergences above.
    fn validate_command(&self) -> anyhow::Result<std::path::PathBuf>;

    /// Build the environment variable overlay for this launcher.
    ///
    /// Collects `runtime_bindings()` from each enabled capability and merges
    /// them with any launcher-specific env vars. Default impl returns empty vec.
    async fn env_overlay(
        &self,
        _ctx: &LaunchContext,
    ) -> anyhow::Result<Vec<EnvBinding>> {
        Ok(vec![])
    }

    /// Exec the tool as a subprocess with the env overlay applied.
    ///
    /// Spawns the binary, waits for it to exit, and returns the exit status.
    /// The default implementation covers the common case: resolve binary,
    /// build overlay, merge with current env, spawn, wait.
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
```

### Supporting types

```rust
pub struct LaunchContext {
    pub launcher_id: String,
    pub working_dir: std::path::PathBuf,
    /// Env vars already resolved (e.g. provider URL, model ID) before
    /// capability bindings are merged on top.
    pub base_env: std::collections::HashMap<String, String>,
}

pub struct EnvBinding {
    pub key: String,
    pub value: String,
}
```

### Factory definition

```rust
// at the bottom of base.rs, exactly mirroring providers/base.rs
use crate::define_factory;
define_factory!(Launcher, LauncherMetadata, LauncherFactory);
```

**Tests in this commit:**
- `validate_command_returns_err_for_unknown_binary` — construct a minimal `Launcher` impl with a nonsense command name; assert `validate_command()` returns `Err`.
- `env_overlay_default_is_empty` — default impl returns empty vec.

---

## Commit 2 — `src/config/mod.rs`: `LauncherConfig` + persistence

**Files changed:** `src/config/mod.rs`

### New struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherConfig {
    pub launcher_id: String,
    #[serde(rename = "type")]
    pub launcher_type: String,
    /// Optional absolute path to the binary, for non-PATH installs.
    pub command_path: Option<std::path::PathBuf>,
    /// Capability IDs enabled for this launcher instance.
    pub enabled_capabilities: Vec<String>,
    /// Launcher-type-specific config (passed to `ConfigConstructable::new`).
    pub config: serde_json::Value,
    pub enabled: bool,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            launcher_id: String::new(),
            launcher_type: String::new(),
            command_path: None,
            enabled_capabilities: vec![],
            config: serde_json::Value::Object(serde_json::Map::new()),
            enabled: true,
        }
    }
}
```

### Changes to `Config`

- Add `pub launchers: HashMap<String, LauncherConfig>` field.
- Add `fn launchers_dir() -> Result<PathBuf>` returning `config_dir/launchers`.
- Wire `launchers_dir` into `ensure_directories`, `load_dir` call in `new()`, and `save()`.
- Add CRUD helpers `get_launcher`, `insert_launcher`, `remove_launcher`, `update_launcher` following the identical pattern as `get_provider` / `insert_provider` / etc.

**Tests in this commit:**
- `launcher_config_default_round_trips` — serialize/deserialize `LauncherConfig::default()` and assert equality.
- `config_launchers_dir_created_on_new` — call `Config::new()` with a temp `GRANITE_CLI_HOME`; assert the `launchers/` subdirectory exists.
- `insert_and_remove_launcher` — insert a launcher, verify `get_launcher` returns it, remove it, verify it returns `None`.

---

## Commit 3 — `src/launchers/claude.rs` + `src/launchers/bob.rs`: concrete launchers

**Files changed:** `src/launchers/claude.rs` *(new)*, `src/launchers/bob.rs` *(new)*

Each file follows the same structure as `src/providers/ollama.rs`: a config struct with `schemars::JsonSchema`, a provider struct, `ConfigConstructable`, the trait impl, and `HasLauncherMetadata`.

### `ClaudeLauncherConfig`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClaudeLauncherConfig {
    /// Override path to the `claude` binary (leave empty to use PATH).
    #[serde(default)]
    pub command_path: Option<String>,
}

impl Default for ClaudeLauncherConfig { ... }
```

### `ClaudeLauncher` impl

- `command_name()` → `"claude"` (or `command_path` if set)
- `supported_capabilities()` → empty `vec![]` for now (populated when concrete capabilities exist)
- `validate_command()` — calls `which::which(self.command_name())` (see note below on `which` crate)
- `env_overlay()` — default (no-op); overridden in a future commit when capability hooks are wired

### `HasLauncherMetadata` for `ClaudeLauncher`

```rust
impl HasLauncherMetadata for ClaudeLauncher {
    fn metadata() -> LauncherMetadata {
        LauncherMetadata {
            name: "Claude CLI".to_string(),
            description: "Anthropic's Claude CLI tool".to_string(),
            default_command: "claude".to_string(),
            supported_capabilities: vec![],
            tags: vec!["claude".to_string(), "anthropic".to_string()],
        }
    }
    fn config_schema() -> schemars::Schema {
        schemars::schema_for!(ClaudeLauncherConfig)
    }
    fn default_config() -> serde_json::Value {
        serde_json::to_value(ClaudeLauncherConfig::default()).unwrap_or_default()
    }
}
```

`BobLauncher` is identical in structure with `command_name = "bob"`.

### Dependency note — `which` crate

`validate_command` needs PATH lookup. The `which` crate is the idiomatic choice and is already used by many Rust CLIs; add it to `Cargo.toml`. If the team prefers not to add the dependency, `validate_command` can be implemented with `std::process::Command::new("which").arg(name).output()` as a fallback — call this out in the PR for a decision.

**Tests in this commit:**
- `claude_launcher_command_name` — asserts `command_name()` returns `"claude"`.
- `bob_launcher_metadata_name` — asserts metadata name is `"Bob CLI"`.
- `validate_command_with_explicit_path_nonexistent` — pass an explicit path that doesn't exist; assert `Err`.

---

## Commit 4 — `src/launchers/mod.rs`: registry + `LauncherSource`

**Files changed:** `src/launchers/mod.rs` *(new)*

Mirrors `src/providers/mod.rs` exactly.

```rust
pub static LAUNCHER_REGISTRY: LazyLock<base::LauncherFactory> = LazyLock::new(|| {
    let mut factory = base::LauncherFactory::new();
    factory.register::<claude::ClaudeLauncher>("claude");
    factory.register::<bob::BobLauncher>("bob");
    factory
});
```

### `LauncherSource`

Provides the same `Configured<dyn Launcher>` interface used by the dependency resolver.

```rust
pub struct LauncherSource {
    constructed: Vec<(String, Box<dyn Launcher>)>,
}

impl LauncherSource {
    pub fn from_config(config: &crate::config::Config) -> Self { ... }
}

impl crate::dependency::Configured<dyn Launcher> for LauncherSource { ... }
```

Re-export all public types from `base`, `claude`, and `bob`.

**Tests in this commit:**
- `launcher_registry_contains_claude_and_bob` — assert `LAUNCHER_REGISTRY.get("claude").is_some()` and `"bob"`.
- `launcher_source_constructs_enabled_launchers_only` — build a `Config` with one enabled and one disabled launcher; assert `LauncherSource` only holds one instance.

---

## Commit 5 — `src/commands/launcher.rs`: catalog, list, setup, validate

**Files changed:** `src/commands/launcher.rs` *(new)*, `src/commands/mod.rs`

Mirrors `src/commands/provider.rs` with the two pattern divergences called out explicitly in comments.

### `catalog`

Reads from `LAUNCHER_REGISTRY.entries()`. Displays `ID` and `DEFAULT COMMAND` columns.

### `list`

Reads from `ctx.config.launchers`. Displays `ID`, `TYPE`, `ENABLED`, `COMMAND` columns (where `COMMAND` is `command_path` if set, otherwise `"(PATH)"`).

### `setup`

```
launcher setup <type> [--id <instance-id>]
```

1. Look up `type` in `LAUNCHER_REGISTRY`; bail with available types if not found.
2. Default `instance_id` to `type`.
3. **Type-aware clash detection** *(diverges from Provider)*: scan `ctx.config.launchers` for any entry where `launcher_type == type`. If one exists and its `launcher_id != instance_id`, inform the user and offer to either update the existing one or proceed with the new name. If `launcher_id == instance_id`, ask overwrite/skip as normal.
4. Prompt for config using `prompt_from_schema` (same as Provider setup).
5. Validate command binary exists before saving; warn (not fail) if not found.
6. Save `LauncherConfig` via `ctx.config.insert_launcher`.

### `validate`

```
launcher validate <instance-id>
```

Constructs the launcher instance from config, calls `validate_command()`, and prints success (with the resolved absolute path) or failure. Useful for diagnosing non-PATH installs.

**Tests in this commit:**
- `catalog_has_id_and_default_command_columns`
- `list_empty_config_has_zero_rows`
- `list_configured_launcher_shows_path_or_path_sentinel`
- `setup_warns_on_same_type_existing_instance` *(the new divergent behavior)*

---

## Commit 6 — `src/main.rs`: wire `Launcher` subcommand + exec `Commands::Launch`

**Files changed:** `src/main.rs`

### New subcommand enum

```rust
#[derive(Subcommand, Debug)]
enum LauncherSubcommands {
    /// Show the catalog of all available launcher types
    Catalog,
    /// List all configured launcher instances
    List,
    /// Interactive launcher setup wizard
    Setup {
        launcher_type: String,
        #[arg(long = "id")]
        instance_id: Option<String>,
    },
    /// Validate that the launcher's binary is reachable
    Validate {
        launcher_id: String,
    },
}
```

Wire into `Commands` and `run_launcher_command` dispatch, identical to `run_provider_command`.

### Wire `Commands::Launch`

Replace the existing stub:

```rust
Some(Commands::Launch(wrapper)) => {
    let ui = construct_ui(&wrapper.output);
    run_launch(&*ui, &wrapper.tool_id, &wrapper.args, wrapper.dry_run)
        .await
        .map_err(|e| ui.error(&e.to_string()))
}
```

```rust
async fn run_launch(
    ui: &dyn Ui,
    launcher_id: &str,
    args: &[String],
    dry_run: bool,
) -> anyhow::Result<()> {
    use crate::launchers::LAUNCHER_REGISTRY;
    use crate::launchers::base::LaunchContext;

    // Load config fresh so we always get the latest saved state.
    let config = crate::config::Config::new()?;

    let launcher_cfg = config.get_launcher(launcher_id).ok_or_else(|| {
        anyhow::anyhow!(
            "No launcher configured with id '{}'. Run `granite-cli launcher setup` first.",
            launcher_id
        )
    })?;

    if !launcher_cfg.enabled {
        anyhow::bail!("Launcher '{}' is disabled.", launcher_id);
    }

    let launcher = LAUNCHER_REGISTRY
        .construct(&launcher_cfg.launcher_type, &launcher_cfg.config)
        .map_err(|e| anyhow::anyhow!("Failed to construct launcher: {}", e))?;

    let binary = launcher.validate_command().map_err(|e| {
        anyhow::anyhow!(
            "Cannot launch '{}': {}. \
             Use `launcher setup --id {}` to set a custom path.",
            launcher_id, e, launcher_id
        )
    })?;

    let ctx = LaunchContext {
        launcher_id: launcher_id.to_string(),
        working_dir: std::env::current_dir()?,
        base_env: std::collections::HashMap::new(),
    };

    let overlay = launcher.env_overlay(&ctx).await?;

    if dry_run {
        ui.info(&format!("Would exec: {}", binary.display()));
        ui.info(&format!("  args: {}", args.join(" ")));
        if overlay.is_empty() {
            ui.info("  env overlay: (none)");
        } else {
            for binding in &overlay {
                ui.info(&format!("  env: {}={}", binding.key, binding.value));
            }
        }
        return Ok(());
    }

    let status = launcher.launch(args, &ctx).await?;
    if !status.success() {
        anyhow::bail!(
            "'{}' exited with status {}",
            launcher_id,
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}
```

**Tests in this commit:**
- `run_launch_unknown_launcher_id_returns_err` — `run_launch` with an id not in config returns `Err` with the setup hint.
- `run_launch_disabled_launcher_returns_err`
- `run_launch_dry_run_prints_binary_and_overlay` — use a `ClaudeLauncher` with a known path override; assert `ui.infos` contain the binary path.

---

## File Map

| File | Action | Commit |
|---|---|---|
| `src/launchers/base.rs` | Create | 1 |
| `src/config/mod.rs` | Extend | 2 |
| `src/launchers/claude.rs` | Create | 3 |
| `src/launchers/bob.rs` | Create | 3 |
| `src/launchers/mod.rs` | Create | 4 |
| `src/commands/launcher.rs` | Create | 5 |
| `src/commands/mod.rs` | Extend | 5 |
| `src/main.rs` | Extend | 6 |
| `Cargo.toml` | Extend (`which`) | 3 |

---

## Success Criteria

- [ ] `granite-cli launcher catalog` lists `claude` and `bob`
- [ ] `granite-cli launcher setup claude` saves a `LauncherConfig` under `launchers/claude.yaml`
- [ ] Setup wizard detects a same-type existing instance and offers update vs. new-name
- [ ] `granite-cli launcher validate claude` reports the resolved binary path or a clear error
- [ ] `granite-cli launch claude --dry-run` prints binary path and env overlay without execing
- [ ] `granite-cli launch claude` execs `claude` and propagates its exit code
- [ ] `granite-cli launch unknown-id` exits with error and setup hint
- [ ] All new unit tests pass under `cargo test`
- [ ] `cargo clippy -- -D warnings` passes with no new warnings
