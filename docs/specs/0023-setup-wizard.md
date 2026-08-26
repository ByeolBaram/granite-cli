# Spec 0023: `granite-cli setup` — Unified Auto Setup Wizard

## Overview

This spec introduces a new top-level `granite-cli setup` command that orchestrates the automatic discovery and configuration of providers, models, launchers, and capabilities in a single guided flow. It combines the existing per-component `setup` wizards with an extended recommendation engine that surfaces the full dependency graph — not just models, but every layer needed to make a capable setup work.

The wizard works **backwards from capabilities**: the user first selects which capabilities they want, then which launchers support those capabilities, then which providers serve the launchers, and finally which models satisfy the capability requirements. Each section's selections inform the next section's recommendations.

Two execution modes are supported:

- **Interactive** (default): Step through each section with prompts.
- **`--auto`**: Non-interactive — detect, recommend, and configure everything that can be auto-configured with default settings. Consent is implied.

## Goals

1. Add a `setup` top-level subcommand (`granite-cli setup`) with `--auto` and `--skip-pull` flags
2. Implement a `Discover` engine that scans all four layers (providers, models, launchers, capabilities) and produces structured recommendations
3. Build an interactive wizard that presents sections **in dependency order**: Capabilities → Launchers → Providers → Models
4. Auto-configure detected items in `--auto` mode using registry defaults
5. Never auto-pull model weights — pull is always explicit
6. Use each component's default health check for availability detection (no impl-specific logic)
7. Filter model recommendations to only those models that would be used by at least one selected capability

## Non-Goals

- Provider binary installation (e.g., "ollama not installed, run `brew install ollama`") — outside granite-cli's scope
- Model weight auto-pulling in `--auto` mode (always requires explicit `model pull`)
- Provider failover or retry logic — deferred to a future spec
- TUI implementation — wizard uses the existing `dialoguer`-based UI layer
- Shell export integration — out of scope for the wizard; users export separately

---

## 1. Command Interface

### Top-level command

```
granite-cli setup              # Interactive full wizard
granite-cli setup --auto       # Non-interactive: detect + configure automatically
granite-cli setup --skip-pull  # Interactive, but never prompt to pull weights
granite-cli setup --auto --skip-pull  # Both flags combined
```

### CLI definition (Rust)

Add to `src/main.rs`:

```rust
#[derive(Subcommand, Debug)]
enum SetupSubcommands {
    /// Run the full setup wizard to configure providers, models, launchers,
    /// and capabilities. Works backwards from capabilities — selecting a
    /// capability determines which launchers, providers, and models are
    /// recommended.
    Setup {
        /// Auto-detect and configure everything that can be auto-configured.
        /// Consent is implied. Uses registry defaults for all config fields.
        #[arg(long)]
        auto: bool,

        /// Skip the model weight pull prompt at the end of the wizard.
        /// Model weights are never auto-pulled in --auto mode regardless.
        #[arg(long)]
        skip_pull: bool,
    },
}
```

Wire `Setup(SetupSubcommands)` into the `Commands` enum and route it in `main()`.

---

## 2. Discovery Engine

All discovery happens in a new `src/commands/setup.rs` module, in a `Discover` struct that produces a `DiscoveryResult`. The `Discover::run()` method is async and runs all four discovery phases sequentially.

### 2.1 Result Types

```rust
/// One recommendation produced during the discovery phase.
enum Recommendation {
    Provider {
        provider_type: &'static str,
        provider_name: String,
        health_healthy: bool,
        health_error: Option<String>,
        suggested_instance_id: String,
    },
    Model {
        model_id: String,
        family: String,
        version: String,
        size: String,
        model_type: ModelType,
        best_variant: ModelVariant,   // largest size fitting hardware, latest in family
        context_fit: ContextFit,
        can_run_by: Vec<String>,      // provider ids that can run it
    },
    Launcher {
        launcher_type: String,
        launcher_name: String,
        binary_path: Option<PathBuf>,
        suggested_instance_id: String,
    },
    Capability {
        capability_type: String,
        capability_name: String,
        // Resolved at recommendation time; may be empty if user hasn't
        // selected launchers/models yet.
        satisfied_by_launchers: Vec<String>,
        satisfied_by_models: Vec<String>,
    },
}

/// The complete output of the discovery engine.
struct DiscoveryResult {
    recommendations: Vec<Recommendation>,
    /// Provider ids that are already configured (not recommended, just existing).
    configured_providers: Vec<String>,
    /// Model ids that are already configured.
    configured_models: Vec<String>,
    /// Launcher ids that are already configured.
    configured_launchers: Vec<String>,
    /// Capability ids that are already configured.
    configured_capabilities: Vec<String>,
}
```

### 2.2 Provider Discovery — `Discover::discover_providers()`

For each provider type in `PROVIDER_REGISTRY.entries()`:

1. Check `ctx.config.providers` — skip if already configured, record in `configured_providers`
2. For unconfigured providers:
   a. Construct a transient instance using registry default config (`LAUNCHER_REGISTRY.default_config()` / `PROVIDER_REGISTRY.default_config()`)
   b. Run the provider's default `health_check()` method
   c. If the health check succeeds, emit a `Recommendation::Provider` with `health_healthy: true`
   d. If the health check fails, still emit a recommendation with `health_healthy: false` and the error message — the user may want to configure it anyway
3. Use the provider type as the `suggested_instance_id`

**Key rule: No impl-specific detection logic.** Every provider is discovered the same way — construct with defaults, run health check. The health check itself may be provider-specific (ollama checks `/tags`, llama-cpp checks `/models`, etc.) but the discovery engine does not contain any such logic.

### 2.3 Model Discovery — `Discover::discover_models()`

1. Group `MODEL_REGISTRY.entries()` by family (e.g., "Granite Language", "Granite Vision")
2. For each family:
   a. Find the latest version using `compare_versions_desc()` (existing utility)
   b. For the latest version, find the strongest/largest size that fits current hardware:
      - Call `detect_hardware()` to get the `HardwareProfile`
      - For each variant, call `context_fit::estimate()` to compute `ContextFit`
      - Prefer variants with `ContextFit::Full` over `ContextFit::Partial`
      - Among equally-fit variants, prefer the largest by `size_gb`
   c. For the winning variant, check which configured providers (from `ctx.config` or from discovered healthy providers) can run it via `can_run_model(format, precision)`
   d. Skip if the model is already in `ctx.config.models`
   e. Emit `Recommendation::Model`

**Key rule: At discovery time, ALL families produce recommendations.** No filtering by capability occurs yet. Filtering by capability happens in the wizard selection phase.

### 2.4 Launcher Discovery — `Discover::discover_launchers()`

For each launcher type in `LAUNCHER_REGISTRY.entries()`:

1. Check `ctx.config.launchers` — skip if already configured, record in `configured_launchers`
2. For unconfigured launchers:
   a. Construct a transient instance using registry default config
   b. Call `launcher.validate_command()` to check if the binary is resolvable
   c. Record the resolved path (or `None` if not found)
   d. Emit `Recommendation::Launcher` with the binary path

### 2.5 Capability Discovery — `Discover::discover_capabilities()`

For each capability type in `CAPABILITY_REGISTRY.entries()`:

1. Check `ctx.config.capabilities` — skip if already configured, record in `configured_capabilities`
2. For unconfigured capabilities:
   a. Check which configured/recommended launchers support this capability's binding types:
      - Iterate `LAUNCHER_REGISTRY.entries()` and check `LauncherMetadata.supported_capabilities` against `CapabilityMetadata.supported_binding_types`
   b. For capabilities with model requirements (e.g., `agent-model`), check which recommended models satisfy the `ModelRequirement`:
      - Use `ModelRequirement` to filter `MODEL_REGISTRY.entries()`
      - A model satisfies the requirement if all non-empty fields match
   c. Emit `Recommendation::Capability` with the lists of satisfying launchers and models

**These lists are populated from the full discovery result.** When the wizard filters launchers/models, the capability satisfaction lists are re-evaluated.

---

## 3. Wizard Flow

The wizard is implemented in `SetupCommands::run(ctx, auto, skip_pull)`.

### 3.1 Phase 0 — Discovery

Run `Discover::run(ctx)` to produce `DiscoveryResult`. This is the single source of truth for all recommendations.

### 3.2 Phase 1 — Capabilities Selection

Present all capability recommendations (and already-configured capabilities) as a multi-select list:

```
== Capabilities ==
  [x] agent-model        — Provides agent model bindings for AI tools
  [x] vision-mcp         — Provides vision analysis via MCP
  [ ] sub-agent          — Provides sub-agent delegation
  [x] sub-agent-explore  — Provides explore sub-agent capability
```

In `--auto` mode, all capabilities with at least one satisfying launcher and model are auto-selected.

The user's selections are recorded as `selected_capabilities: HashSet<String>`.

### 3.3 Phase 2 — Launchers Selection

Re-evaluate launcher recommendations based on selected capabilities. A launcher is recommended if it supports at least one selected capability's binding types:

```
== Launchers ==
  [x] claude     — Claude Code, binary found: /opt/claude/claude
  [ ] bob        — Bob, binary not found on PATH
  [x] opencode   — OpenCode, binary found: /usr/local/bin/opencode
```

In `--auto` mode, all auto-selected capabilities require certain launchers — those launchers are auto-selected. If a launcher's binary is not found, it is still recommended but flagged.

The user's selections are recorded as `selected_launchers: HashSet<String>`.

### 3.4 Phase 3 — Providers Selection

Re-evaluate provider recommendations based on selected launchers. A provider is recommended if at least one selected launcher needs it (i.e., the launcher's configured capabilities need models that the provider can serve). Show all detected+healthy providers, plus any configured providers:

```
== Providers ==
  [x] ollama       — Local, healthy ✓
  [ ] openrouter   — Hosted, health check failed: timeout
  [x] llama-cpp    — Local, healthy ✓
```

In `--auto` mode, all healthy detected providers are auto-selected.

The user's selections are recorded as `selected_providers: HashSet<String>`.

### 3.5 Phase 4 — Models Selection

Re-evaluate model recommendations based on selected capabilities. A model is recommended **only if it would be used by at least one selected capability**. This is the key filtering step that answers requirement 3 from the spec:

```
== Models ==
  [x] granite-3.3-8b-instruct    — Granite Language 3.3, 8B, gguf/fp16 (4.7 GB), Full fit
  [x] granite-vision-3.3-2b      — Granite Vision 3.3, 2B, gguf/fp16 (1.3 GB), Full fit
```

In `--auto` mode, models that satisfy the requirements of all auto-selected capabilities are auto-selected.

The user's selections are recorded as `selected_models: HashSet<String>`.

### 3.6 Phase 5 — Configuration

Iterate through all four sections in forward order and configure selected items:

1. **Providers**: For each selected provider, call `ProviderCommands::setup()` with the registry default config
2. **Launchers**: For each selected launcher, call `LauncherCommands::setup()` with the registry default config
3. **Models**: For each selected model, call `ModelCommands::setup()` — this resolves a provider for the model
4. **Capabilities**: For each selected capability, call `CapabilityCommands::setup()` — this resolves models/providers as needed

In `--auto` mode, all steps use defaults without prompting.

### 3.7 Phase 6 — Pull (if not skipped)

For each configured model backed by a local provider, ask whether to pull model weights:

```
== Pull Weights ==
  [x] ollama → granite-3.3-8b-instruct (gguf/fp16)
  [x] ollama → granite-vision-3.3-2b (gguf/fp16)
  → Pull now? (y/N)
```

In `--auto` mode, this phase is always skipped (never auto-pull).

### 3.8 Phase 7 — Summary

Display a final summary of everything that was configured:

```
== Setup Complete ==
  Providers:  ollama, llama-cpp
  Models:     granite-3.3-8b-instruct, granite-vision-3.3-2b
  Launchers:  claude, opencode
  Capabilities: agent-model, vision-mcp, sub-agent-explore

Run `granite-cli launcher list` to see configured launchers.
Run `granite-cli launch claude` to launch Claude Code with Granite overlay.
```

---

## 4. Re-evaluation Logic

After each selection phase, the wizard re-evaluates subsequent sections. This is implemented by a `Revaluator` that takes the current selections and the full `DiscoveryResult`, and returns filtered recommendations for the next section.

```rust
struct Revaluator;

impl Revaluator {
    /// Filter launcher recommendations to only those that support
    /// at least one of the selected capability types.
    fn for_launchers(
        discovery: &DiscoveryResult,
        selected_caps: &HashSet<String>,
    ) -> Vec<Recommendation>;

    /// Filter provider recommendations to only those that can serve
    /// at least one of the selected model variants.
    fn for_providers(
        discovery: &DiscoveryResult,
        selected_models: &HashSet<String>,
        selected_launchers: &HashSet<String>,
    ) -> Vec<Recommendation>;

    /// Filter model recommendations to only those that satisfy at least
    /// one of the selected capability's model requirements.
    fn for_models(
        discovery: &DiscoveryResult,
        selected_caps: &HashSet<String>,
    ) -> Vec<Recommendation>;
}
```

**Provider filtering logic:** A provider is recommended if it can run at least one variant of at least one selected model. Additionally, if a selected launcher has capabilities that require specific provider types (e.g., MCP capabilities need OpenAI-compatible API), those providers are prioritized.

---

## 5. File Changes

| File | Action | Description |
|------|--------|-------------|
| `src/main.rs` | Extend | Add `Setup` subcommand, `--auto`/`--skip-pull` flags, routing |
| `src/commands/mod.rs` | Extend | Add `pub mod setup;` |
| `src/commands/setup.rs` | **Create** | Full implementation: `Discover`, `Revaluator`, `SetupCommands::run()` |

---

## 6. Commit Plan

### Commit 1 — `src/commands/setup.rs`: Discovery engine + types

Add the `Discover` struct, `Recommendation` enum, and `DiscoveryResult` struct. Implement all four discovery methods:

- `discover_providers()` — construct with defaults, run health check
- `discover_models()` — group by family, find latest + best variant, check provider compatibility
- `discover_launchers()` — construct with defaults, validate command
- `discover_capabilities()` — check launcher compatibility + model requirement satisfaction

**Tests:**
- `discover_providers_detects_healthy_ollama` — configure a mock ollama with a responding health check; assert it appears as a healthy recommendation
- `discover_providers_skips_configured` — pre-configure ollama; assert it appears in `configured_providers`, not `recommendations`
- `discover_models_groups_by_family` — assert that the result contains one recommendation per family, not per variant
- `discover_models_picks_largest_fitting_variant` — assert that for a family with multiple sizes, the largest variant fitting the hardware is selected
- `discover_launchers_validates_binary` — assert that a launcher with an existing binary has `binary_path` set
- `discover_launchers_skips_configured` — pre-configure a launcher; assert it appears in `configured_launchers`
- `discover_capabilities_checks_launcher_compatibility` — assert that a capability appears with the correct list of supporting launchers

### Commit 2 — `src/commands/setup.rs`: Revaluator + Wizard

Add the `Revaluator` struct and implement the wizard flow:

- `run_wizard()` — interactive step-through of all 7 phases
- `run_auto()` — non-interactive mode with `--auto`

**Tests:**
- `wizard_filters_launchers_by_capabilities` — select a capability that only supports claude; assert bob is not offered in launcher selection
- `wizard_filters_models_by_capabilities` — select only agent-model (which needs text models); assert vision-only models are not offered
- `auto_mode_selects_all_detectable` — in auto mode, all detected+healthy providers and all binary-found launchers are auto-selected

### Commit 3 — `src/main.rs`: CLI wiring

Add the `Setup` subcommand to `Commands`, the `--auto`/`--skip-pull` flags, and routing in `main()`.

**Tests:**
- `setup_subcommand_exists` — parse `granite-cli setup`; assert subcommand is `Setup(SetupSubcommands::Setup { auto: false, skip_pull: false })`
- `setup_subcommand_with_auto` — parse `granite-cli setup --auto`; assert `auto: true`
- `setup_subcommand_with_skip_pull` — parse `granite-cli setup --skip-pull`; assert `skip_pull: true`

---

## 7. Success Criteria

- [ ] `granite-cli setup` runs the interactive wizard with all four sections in order (Capabilities → Launchers → Providers → Models)
- [ ] Deselecting a launcher in the Launchers section removes capabilities that the launcher was the only support for
- [ ] Deselecting a capability in the Capabilities section removes models that the capability was the only consumer for
- [ ] `granite-cli setup --auto` configures all detectable providers, all binary-found launchers, and all capability-recommended models without any prompts
- [ ] Model weights are never auto-pulled in any mode
- [ ] `granite-cli setup --skip-pull` suppresses the pull prompt at the end of the wizard
- [ ] Provider detection uses only the default health check — no impl-specific detection logic in the discovery engine
- [ ] All new unit tests pass under `cargo test`
- [ ] `cargo clippy -- -D warnings` passes with no new warnings

---

## 8. Appendix: Section Dependency Flow Diagram

```
                    ┌─────────────┐
                    │  Discovery  │  ← runs once, collects everything
                    └──────┬──────┘
                           │
              ┌────────────▼────────────┐
              │  Phase 1: Capabilities  │  ← user multi-selects
              └────────────┬────────────┘
                           │ selected capabilities
              ┌────────────▼────────────┐
              │   Phase 2: Launchers    │  ← filtered by selected caps
              └────────────┬────────────┘
                           │ selected launchers
              ┌────────────▼────────────┐
              │    Phase 3: Providers   │  ← filtered by selected launchers
              └────────────┬────────────┘
                           │ selected providers
              ┌────────────▼────────────┐
              │    Phase 4: Models      │  ← filtered by selected caps
              └────────────┬────────────┘
                           │ selected models
              ┌────────────▼────────────┐
              │  Phase 5: Configure     │  ← write configs
              └────────────┬────────────┘
                           │
              ┌────────────▼────────────┐
              │  Phase 6: Pull (opt)    │  ← optional, never auto
              └────────────┬────────────┘
                           │
              ┌────────────▼────────────┐
              │  Phase 7: Summary       │  ← display result
              └─────────────────────────┘
```

Each arrow represents a re-evaluation: the selections from the upstream phase filter the recommendations in the downstream phase. This ensures the user never sees an option that would be useless given their upstream choices.
