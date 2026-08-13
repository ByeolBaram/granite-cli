# Plan: Wire `Capability.bind_capabilities` to `launcher setup` and `run_launch`

## Overview

`LauncherConfig.enabled_capabilities` exists as a `Vec<String>` but is never
populated and never consulted. This plan wires it end-to-end:

1. **During `launcher setup`**: after the binary is validated, the user is
   shown all configured capability instances whose `binding_types()` overlap the
   launcher's `supported_capabilities`, plus a "Configure a new capability…"
   option. They can select zero or more; the IDs of chosen ones are saved into
   `enabled_capabilities`.

2. **During `run_launch`**: after the launcher is constructed, each
   `enabled_capabilities` ID is looked up in config, the capability is
   constructed, and `bind_capability` is called on the launcher (mutably) before
   `launch()` is invoked.

Neither change touches the `Launcher` or `Capability` trait interfaces — all
work is in the command layer and the launch runner.

## Out of Scope (Future Work)

**Mutual exclusivity of capabilities by binding type**: A launcher may support
at most one `AgentModel` capability at a time (since binding overwrites the
single stored `AgentModelBinding`), but could in future support multiple `Skill`
capabilities. The current plan does not enforce this — a user could select two
`AgentModel`-compatible capabilities during setup and both `bind_capability`
calls would be made at launch time (second wins). Enforcement should be added
in a follow-up once the `Skill` binding type exists and the per-type cardinality
policy is clearer. For now, behavior on over-selection is implementation-defined.

---

## Sub-Tasks

---

### Sub-Task 1 — Add `multi_select` to the `Ui` trait

**Intent**
Capability selection at setup time requires the user to choose zero-or-more
items from a list. The `Ui` trait today only has single-selection (`select`).
Adding `multi_select` keeps the pattern consistent and testable via `CaptureUi`.

**Expected Outcomes**
- `Ui` trait in `src/utils/ui/base.rs` gains a `multi_select` method with the
  same signature shape as `select`, but returning `Vec<usize>`.
- Default implementation delegates to `dialoguer::MultiSelect`. This covers
  the `plain` and `terminal` backends, which do not override interactive methods.
- `CaptureUi` records calls and consumes canned answers
  (`multi_select_answers: RefCell<VecDeque<Vec<usize>>>`).
- `JsonOutput` and `MarkdownOutput` explicitly override `multi_select` to return
  `non_interactive()`, matching how they handle `select`, `confirm`, `text`,
  and `password` today.
- `PlainOutput` and `TerminalOutput` use the default trait implementation (no
  override needed).

**Todo List**
1. Add `multi_select(&self, prompt: &str, items: &[String], defaults: &[bool]) -> anyhow::Result<Vec<usize>>` to the `Ui` trait in `src/utils/ui/base.rs`, with a default body that calls `dialoguer::MultiSelect`.
2. Add `multi_select_prompts: RefCell<Vec<(String, Vec<String>, Vec<bool>)>>` and `multi_select_answers: RefCell<VecDeque<Vec<usize>>>` fields to `CaptureUi`; implement the method to record the call and pop a canned answer (falling back to an empty vec when the queue is empty).
3. Override `multi_select` in `JsonOutput` (`src/utils/ui/backends/json.rs`) and `MarkdownOutput` (`src/utils/ui/backends/markdown.rs`) to return `non_interactive()`.
4. Add a `multi_select` contract test to the `output_contract_tests!` macro in `src/utils/ui/base.rs`.

**Relevant Context**
- `src/utils/ui/base.rs`: `Ui` trait definition, `CaptureUi`, `non_interactive()`
- `src/utils/ui/backends/json.rs` lines 122–136: overrides `select`, `confirm`, `text`, `password` with `non_interactive()`
- `src/utils/ui/backends/markdown.rs` lines 65–79: same pattern
- `src/utils/ui/backends/plain.rs` and `terminal.rs`: no interactive overrides needed
- `dialoguer` is already a dependency; `MultiSelect` is part of its API

**Status** — `[x] done`

---

### Sub-Task 2 — Capability selection during `launcher setup`

**Intent**  
After the launcher binary is validated, present the user with the subset of
configured capability instances (and configurable capability types) that are
compatible with this launcher. Let the user pick zero or more; save the chosen
IDs into `enabled_capabilities` in the saved `LauncherConfig`.

**Expected Outcomes**
- `LauncherCommands::setup` in `src/commands/launcher.rs` calls a new private
  helper `select_capabilities` after binary validation and before saving config.
- `select_capabilities` filters the `CapabilitySource` to instances whose
  `binding_types()` intersect the launcher metadata's `supported_capabilities`.
  For configurable types the catalog's `supported_binding_types` is used for
  the same intersection test.
- Items presented: sorted list of matching configured instance IDs + "Configure
  a new capability…" sentinel if any compatible type exists in the registry.
- The multi-select defaults all checkboxes to the previously saved
  `enabled_capabilities` IDs (pre-tick on re-run).
- If the user picks "Configure a new capability…", drive `CapabilityCommands::setup`
  the same way `select_provider` drives `ProviderCommands::setup` (single type
  → auto-select type; multiple → let user pick type; prompt for nickname).
  After setup, add the new capability ID to the final selection.
- The resulting list of IDs is saved as `enabled_capabilities` in `LauncherConfig`.
- If `launcher_def.supported_capabilities` is empty (e.g. the `bob` launcher),
  skip the capability selection step and emit a `ui.warn` noting that this
  launcher does not support any capabilities (since the intent of the project
  is for launchers to have capabilities, an empty set is worth surfacing).

**Todo List**
1. Import `CapabilityCommands`, `CapabilitySource`, and `CAPABILITY_REGISTRY`
   into `src/commands/launcher.rs`.
2. Add `async fn select_capabilities(ctx, launcher_def, existing_enabled) -> Result<Vec<String>>` as a private function.
3. In the function body:
   a. Build a `CapabilitySource` from `ctx.config`.
   b. Filter instances to those whose `binding_types()` intersect `launcher_def.supported_capabilities`.
   c. Filter catalog types to those whose `supported_binding_types` intersect `launcher_def.supported_capabilities`.
   d. If both are empty, print info and return empty vec.
   e. Build display items list: `[instance_ids..., "Configure a new capability..."]` (sentinel only if compatible types exist).
   f. Build `defaults: Vec<bool>` by checking each item against `existing_enabled`.
   g. Call `ctx.ui.multi_select(...)`, get back selected indices.
   h. For each selected index:
         - If it's a regular instance, add its ID to the result.
         - If it's the "Configure a new" sentinel, drive `CapabilityCommands::setup`
           (same flow as `select_provider` in model.rs: single compatible type →
           auto-select; multiple → let user pick type; prompt for nickname). Add
           the new ID to the accumulated result. All previously-selected IDs are
           preserved — the "Configure new" path appends rather than replaces.
4. Call `select_capabilities` in `LauncherCommands::setup` after binary validation, passing any previously saved `enabled_capabilities` as defaults.
5. Use the returned vec as `enabled_capabilities` in the `LauncherConfig` that is saved.

**Relevant Context**
- `src/commands/launcher.rs`: `LauncherCommands::setup` (lines 74–226), especially lines 204–209 where `LauncherConfig` is built with `enabled_capabilities: vec![]`
- `src/commands/model.rs`: `select_provider` (lines 703–751) — the pattern to follow
- `src/capabilities/mod.rs`: `CapabilitySource`, `CAPABILITY_REGISTRY`
- `src/commands/capability.rs`: `CapabilityCommands::setup` — to be called for "Configure new"
- `src/launchers/base.rs`: `LauncherMetadata.supported_capabilities: HashSet<BindingType>`
- `src/capabilities/base.rs`: `CapabilityMetadata.supported_binding_types`

**Status** — `[x] done`

---

### Sub-Task 3 — Bind enabled capabilities in `run_launch`

**Intent**  
At launch time, read `lc.enabled_capabilities` from the loaded config, construct
each capability, and call `bind_capability` on the launcher before `launch()` is
called. The launcher (e.g. `ClaudeLauncher`) already implements `bind_capability`
fully; this sub-task just plumbs the missing call site.

**Expected Outcomes**
- `run_launch` in `src/main.rs` constructs a `CapabilitySource` from the fresh
  config and, for each ID in `lc.enabled_capabilities`, looks up the capability
  and calls `launcher.bind_capability(capability)`.
- The launcher must be `mut` for this (it stores the bound result); change
  `let launcher =` to `let mut launcher =`.
- If a capability ID listed in `enabled_capabilities` is not found in config,
  bail with a clear error message.
- If `bind_capability` returns an error, propagate it (no silent swallowing).
- No change to the `Launcher` or `Capability` trait signatures.

**Todo List**
1. In `run_launch` in `src/main.rs`, change `let launcher =` to `let mut launcher =`.
2. Add import for `crate::capabilities::CapabilitySource` and `crate::capabilities::CAPABILITY_REGISTRY`.
3. After constructing `launcher`, build a `CapabilitySource` from `&config`.
4. For each capability ID in `lc.enabled_capabilities`:
   a. Look it up in `config.capabilities`; bail if not found.
   b. Construct the capability via `CAPABILITY_REGISTRY.construct(...)`.
   c. Call `launcher.bind_capability(capability.as_ref()).await?`.
5. Continue to `launcher.launch(args, &launch_ctx, ui).await?` as before.

**Relevant Context**
- `src/main.rs`: `run_launch` (lines 635–674)
- `src/capabilities/mod.rs`: `CapabilitySource::from_config`, `CAPABILITY_REGISTRY`
- `src/launchers/claude.rs`: `bind_capability` implementation — what happens when it's called
- `src/config/mod.rs`: `config.capabilities: HashMap<String, CapabilityConfig>`

**Status** — `[x] done`

---

### Sub-Task 4 — Tests

**Intent**  
Cover the two new behaviours: capability selection during setup and capability
binding during launch. Follow existing test patterns in `launcher.rs` and
`main.rs` (unit tests using `CaptureUi`).

**Expected Outcomes**
- `src/commands/launcher.rs` tests cover:
  - When `supported_capabilities` is empty (bob), `select_capabilities` is not
    called and `enabled_capabilities` is empty in saved config.
  - When compatible capabilities exist, `multi_select` is called with the right
    items and defaults.
  - Previously-enabled capabilities are pre-checked in the multi-select prompt.
  - Selecting "Configure a new capability…" drives `CapabilityCommands::setup`.
- `src/main.rs` (or a dedicated integration-style test):
  - `run_launch` with no `enabled_capabilities` calls `launch()` directly.
  - `run_launch` with an unknown capability ID returns an error.

**Todo List**
1. Add tests to `src/commands/launcher.rs` for `select_capabilities` behaviour using `CaptureUi`'s new `multi_select_answers`.
2. Add a test for `run_launch` with a missing capability ID (can be done without a real binary by testing the early-bail path).
3. Confirm all existing tests still pass (`cargo test`).

**Relevant Context**
- `src/commands/launcher.rs` tests section (lines 247–443)
- `src/utils/ui/base.rs` `CaptureUi` — will have `multi_select_answers` after Sub-Task 1
- `src/launchers/bob.rs` — zero `supported_capabilities`, good for the skip path

**Status** — `[x] done`
