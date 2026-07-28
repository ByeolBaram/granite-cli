# UI Overhaul: Output Abstraction and TUI Integration

## Overview

This document specifies the full UI overhaul for granite-cli. The goal is to replace all
scattered `println!` calls with a decoupled, factory-backed output system, integrate ratatui
for rich terminal rendering, and introduce a full interactive TUI when the CLI is invoked
with no arguments. The design extends the existing `define_factory!` pattern already used for
providers, models, and capabilities — no new infrastructure concepts are introduced.

## Problem Statement

The current output layer has the following issues:

1. **Output is hardcoded at every call site.** Every command method calls `println!` directly.
   There is no way to change the output format (e.g. JSON for scripting, plain text for CI)
   without modifying command code.

2. **Commands are untestable in isolation.** Because output goes straight to stdout, command
   logic can only be verified by spawning a subprocess and parsing its output, or by visual
   inspection. There are zero tests for command output correctness today.

3. **No interactive entry point.** Running `granite-cli` with no arguments prints a static
   help block. There is no browsable TUI for users who want to explore models, providers, and
   capabilities interactively.

4. **Dead code.** `src/utils/web_fetch.rs` is exported but never called by any production
   code. `reqwest` and `html2md` are compile dependencies for code that has no callers.

5. **Output logic is scattered.** Column widths, header labels, and formatting strings are
   duplicated across `model.rs`, `provider.rs`, and `capability.rs`. Changing a column
   requires touching multiple files.

## Goals

1. **Single output abstraction.** All command output routes through one `Output` trait. Command
   code never calls `println!` directly.
2. **Factory-backed backends.** Output implementations are registered in `OUTPUT_REGISTRY` using
   the existing `define_factory!` macro. Adding a new format requires one new file and one
   `register` call — nothing else changes.
3. **Testable command layer.** A `CaptureOutput` test double captures all calls into inspectable
   `Vec`s. Command tests assert on structured data, not parsed strings.
4. **Rich terminal output.** The default `TerminalOutput` backend uses ratatui to render bordered
   tables, coloured status indicators, and aligned detail blocks.
5. **Interactive TUI home screen.** `granite-cli` with no arguments launches a navigable
   ratatui application with a left navigation panel and a right content pane.
6. **Selectable output format.** A global `--output` flag lets callers choose `terminal`,
   `plain`, or `json` without changing any command code.

## Non-Goals

- Rewriting the setup wizards (`model setup`, `provider setup`). These interleave `println!`
  with `dialoguer` prompts in a way that requires raw-mode input handling. They are excluded
  from this overhaul and continue using `dialoguer` as-is. A future spec will address them.
- Changing the CLI command structure, argument names, or config file format.
- Adding new commands or new model/provider data.

## Architecture

### Two Rendering Modes

The split is determined in `main()` at the `match cli.command` branch:

```
granite-cli                    →  None arm   →  run_interactive_tui(ctx)
granite-cli model catalog      →  Some arm   →  dispatch with OUTPUT_REGISTRY backend
granite-cli provider setup …   →  Some arm   →  dispatch with OUTPUT_REGISTRY backend
                                               (setup wizard uses dialoguer internally,
                                                Output only used for non-prompt messages)
```

The interactive TUI is not a backend registered in `OUTPUT_REGISTRY` because it owns a live
event loop rather than rendering once and returning. It is constructed directly.

### The Output Trait

Defined in `src/utils/ui/output.rs`. All command methods receive `out: &dyn Output` as their
final parameter. The trait is intentionally narrow — it describes the *kinds* of content
commands produce, not how they look.

```rust
pub trait Output: Send + Sync {
    /// Render a tabular result (catalog, list, health).
    fn table(&self, title: &str, headers: &[&str], rows: &[Vec<String>]);

    /// Render a key-value detail block (info commands).
    fn detail(&self, title: &str, fields: &[(&str, String)]);

    /// Render a single status row with pass/fail colouring (health checks).
    fn status(&self, label: &str, ok: bool, detail: &str);

    /// Plain informational message.
    fn info(&self, msg: &str);

    /// Warning message.
    fn warn(&self, msg: &str);

    /// Error message (goes to stderr in terminal/plain backends).
    fn error(&self, msg: &str);
}
```

### The OutputFactory

`Output` participates in the same factory pattern as `Provider`, `Model`, and `Capability`.
The macro call and static registry follow the identical structure used in those domains:

```rust
// src/utils/ui/output.rs
define_factory!(Output, OutputMetadata, OutputFactory);

// src/utils/ui/mod.rs
pub static OUTPUT_REGISTRY: LazyLock<OutputFactory> = LazyLock::new(|| {
    let mut f = OutputFactory::new();
    f.register::<TerminalOutput>("terminal");
    f.register::<PlainOutput>("plain");
    f.register::<JsonOutput>("json");
    f
});
```

Adding a new backend (e.g. `"markdown"`) requires:
1. Create `src/utils/ui/backends/markdown.rs` implementing `Output` and `HasOutputMetadata`.
2. Add `f.register::<MarkdownOutput>("markdown");` to the registry initialiser.
3. Nothing else changes — commands, `main.rs`, and the trait are untouched.

### The --output Global Flag

A `global = true` clap argument is added to `Cli`:

```rust
#[arg(long, global = true, default_value = "terminal")]
output: String,
```

In the `Some(cmd)` arm of `main()`:

```rust
let out = OUTPUT_REGISTRY
    .construct(&cli.output, &serde_json::json!({}))
    .unwrap_or_else(|_| {
        eprintln!("Unknown output format '{}'. Valid: terminal, plain, json", cli.output);
        std::process::exit(1);
    });
dispatch_command(&mut ctx, cmd, out.as_ref()).await
```

### Output Backends

#### TerminalOutput (default)

Renders using **direct crossterm ANSI codes** alongside `println!` — no ratatui
widgets, no raw mode, no `Terminal`, no draw loop. Each `Output` method uses
`crossterm::style::{SetForegroundColor, SetAttribute, ResetColor}` inline to
produce coloured, column-aligned output that prints directly to the terminal
and stays there, exactly like any other CLI tool.

If stdout is not a tty (pipe or file redirection, detected via
`crossterm::terminal::size()` failing), `TerminalOutput` delegates every call
to `PlainOutput` automatically. This means `granite-cli model catalog > out.txt`
produces readable plain text with no ANSI codes.

Ratatui is **not used** by `TerminalOutput`. Ratatui belongs only in `app.rs`
for the interactive full-screen TUI (`granite-cli` with no arguments). The
two rendering targets — one-shot command output and interactive TUI — are
distinct and use distinct mechanisms.

#### PlainOutput

Plain text with no ANSI escape codes. Columns are space-padded to consistent widths computed from content. Suitable for CI environments, piped output, and terminals that do not support ANSI.

#### JsonOutput

Each `Output` method emits one JSON object to stdout, newline-delimited. This makes the output pipeable to `jq` and other tools.

```
table call  →  {"type":"table","title":"…","headers":[…],"rows":[[…],…]}
detail call →  {"type":"detail","title":"…","fields":{"key":"value",…}}
status call →  {"type":"status","label":"…","ok":true,"detail":"…"}
info call   →  {"type":"info","message":"…"}
```

`JsonOutput` accepts an optional writer via `JsonOutput::with_writer(w)` so its output can be
captured in tests without spawning a subprocess.

### CaptureOutput (test double)

Defined alongside the `Output` trait in `src/utils/ui/output.rs`. Available to all test
modules via `use crate::utils::ui::output::tests::CaptureOutput`. Uses `RefCell<Vec<…>>` for interior
mutability (required because `Output` methods take `&self`).

```rust
pub struct CaptureOutput {
    pub tables:   RefCell<Vec<(String, Vec<String>, Vec<Vec<String>>)>>,
    pub details:  RefCell<Vec<(String, Vec<(String, String)>)>>,
    pub statuses: RefCell<Vec<(String, bool, String)>>,
    pub infos:    RefCell<Vec<String>>,
    pub warns:    RefCell<Vec<String>>,
    pub errors:   RefCell<Vec<String>>,
}
```

Command tests use it like this:

```rust
let out = CaptureOutput::default();
ModelCommands::catalog(&ctx, None, &out).unwrap();
let (title, headers, rows) = &out.tables.borrow()[0];
assert!(headers.contains(&"FAMILY".to_string()));
assert!(!rows.is_empty());
```

### The Interactive TUI

Implemented in `src/utils/ui/app.rs`. Owns the full ratatui event loop.

Layout:

```
┌─ granite-cli ─────────────────────────────────────────────────────────────┐
│                                                                           │
│  ┌── Navigation ──────┐  ┌── Content ────────────────────────────────┐  │
│  │                    │  │                                            │  │
│  │  ▶ Models    (24)  │  │  ID                    FAMILY  SIZE  TYPE  │  │
│  │    Providers  (1)  │  │  ──────────────────────────────────────── │  │
│  │    Capabilities(0) │  │  granite-3.1-8b-inst…  Granite  8B   Text │  │
│  │                    │  │  granite-3.3-8b-inst…  Granite  8B   Text │  │
│  │                    │  │  granite-vision-3.3…   Granite  2B  Vision│  │
│  │                    │  │  …                                         │  │
│  └────────────────────┘  └────────────────────────────────────────────┘  │
│                                                                           │
│  [↑↓ / jk] Navigate  [Tab] Switch section  [Enter] Detail  [q] Quit     │
└───────────────────────────────────────────────────────────────────────────┘
```

State:

```rust
pub enum Section { Models, Providers, Capabilities }
pub enum AppMode  { Browse, Detail(String) }

pub struct App {
    ctx:     AppContext,
    section: Section,
    row:     usize,
    mode:    AppMode,
}
```

Key bindings:

| Key | Action |
|-----|--------|
| `q` / `Esc` | Quit |
| `Tab` | Cycle section (Models → Providers → Capabilities → Models) |
| `↑` / `k` | Move row up (clamps at 0) |
| `↓` / `j` | Move row down (clamps at last row) |
| `Enter` | Switch to `AppMode::Detail(selected_id)` |
| `Backspace` | Return to `AppMode::Browse` |

The Detail view renders the same data as the `info` command for the selected item, reusing
widget code from `TerminalOutput`.

## File Structure

```
src/utils/
├── hardware.rs                     (unchanged)
├── mod.rs                          (updated: pub mod ui; remove schema_prompt)
└── ui/
    ├── mod.rs                      (OUTPUT_REGISTRY, run_interactive_tui re-export)
    ├── output.rs                   (Output trait, OutputMetadata, OutputFactory,
    │                                CaptureOutput test double)
    ├── prompt.rs                   (moved from schema_prompt.rs — no logic changes)
    ├── tui.rs                      (setup_terminal, restore_terminal, render_once)
    ├── app.rs                      (App struct, AppMode, run_interactive_tui)
    └── backends/
        ├── mod.rs
        ├── terminal.rs             (TerminalOutput — ratatui one-shot)
        ├── plain.rs                (PlainOutput — no ANSI)
        └── json.rs                 (JsonOutput — newline-delimited JSON)
```

Files removed:
- `src/utils/schema_prompt.rs` (contents moved to `src/utils/ui/prompt.rs`)
- `src/utils/web_fetch.rs` (dead code, no callers)

Dependencies removed from `Cargo.toml`:
- `reqwest` (only used by web_fetch)
- `html2md` (only used by web_fetch)
- `regex` (only used by web_fetch)

## Migration of Command Methods

Each read-only command method (`catalog`, `list`, `info`, `health`) gains one parameter:
`out: &dyn Output`. All `println!` calls inside that method are replaced with `out.*` calls.

Setup wizard methods (`model setup`, `provider setup`, `capability setup`) are **not changed**
in this overhaul. They continue using `println!` and `dialoguer` directly.

Before:

```rust
pub fn catalog(_ctx: &AppContext) -> Result<()> {
    let providers = PROVIDER_REGISTRY.entries();
    println!("{:<20} {:<10} {:<35}", "ID", "TYPE", "ENDPOINT");
    println!("{:<20} {:<10} {:<35}", "----", "----", "--------");
    for (id, p) in &providers {
        println!("{:<20} {:<10} {:<35}", id, p.provider_type, p.default_endpoint);
    }
    println!("Total: {} providers", providers.len());
    Ok(())
}
```

After:

```rust
pub fn catalog(_ctx: &AppContext, out: &dyn Output) -> Result<()> {
    let providers = PROVIDER_REGISTRY.entries();
    let rows: Vec<Vec<String>> = providers.iter()
        .map(|(id, p)| vec![id.to_string(), p.provider_type.to_string(), p.default_endpoint.clone()])
        .collect();
    out.table("Provider Catalog", &["ID", "TYPE", "ENDPOINT"], &rows);
    Ok(())
}
```

The public type signatures of `ModelCommands`, `ProviderCommands`, and `CapabilityCommands`
change. Any caller that previously called these methods must pass an `out` argument. The only
callers are the `run_*_command` functions in `main.rs`, which are updated in the same commit.

## Testing Strategy

### Layers

| Layer | File | What is tested | Terminal required |
|-------|------|----------------|-------------------|
| Output trait helpers | `output.rs` | `CaptureOutput` records correctly, factory registration, `OutputMetadata` completeness | No |
| Command logic | `commands/*.rs` | Correct columns, row counts, filter behaviour, error paths | No |
| Backend contracts | `backends/*.rs` | All backends handle empty inputs, long strings, unicode without panicking | No (plain/json); Yes (terminal) |
| App state machine | `app.rs` | Navigation, section cycling, detail/browse transitions | No |
| Registry integration | `models/mod.rs` etc. | Existing tests unchanged | No |

### Test Double Contract

`CaptureOutput` itself has a dedicated test suite (6 tests) in `output.rs`. These run before
any command test relies on the double, ensuring spy bugs are caught early rather than
silently invalidating downstream assertions.

### Backend Contract Macro

A `output_contract_tests!(expr)` macro defined in `output.rs` generates 8 panic-safety tests
for any `Output` implementation. Each backend's test module invokes it with one line:

```rust
// plain.rs
output_contract_tests!(PlainOutput::new());

// json.rs
output_contract_tests!(JsonOutput::with_writer(Vec::new()));
```

### What Is Not Tested Automatically

- `TerminalOutput` rendering fidelity (requires a real tty — verified manually)
- The ratatui draw loop in `run_interactive_tui` (verified manually)
- `App::render` widget layout (verified manually)

The App state machine (key handling, section cycling, row clamping, mode transitions) is
fully unit-tested without a terminal because it is pure logic on the `App` struct.

## Commit Sequence

Each commit leaves `cargo test` green. No commit mixes structural changes with behaviour
changes.

| # | Message | Key changes | Net test delta |
|---|---------|-------------|----------------|
| 1 | `refactor: remove web_fetch and unused deps` | Delete `web_fetch.rs`, remove `reqwest`/`html2md`/`regex` from `Cargo.toml` | −3 |
| 2 | `refactor: move schema_prompt into utils/ui/prompt module` | Create `ui/` module, move file verbatim, update re-export | 0 |
| 3 | `feat: add Output trait and CaptureOutput test double` | `output.rs` with trait + spy | 0 |
| 4 | `test: add CaptureOutput self-tests` | 6 tests in `output.rs` | +6 |
| 5 | `refactor: thread Output through model commands` | Add `out` param to model methods, 10 new tests | +10 |
| 6 | `refactor: thread Output through provider and capability commands` | Add `out` param to provider/capability methods, 13 new tests | +13 |
| 7 | `feat: add OutputFactory registry and --output global flag` | `define_factory!` for Output, `OUTPUT_REGISTRY`, `--output` flag in `Cli`, 4 new tests | +4 |
| 8 | `feat: add PlainOutput and JsonOutput backends with contract tests` | Two backend files, contract macro, JSON assertions, ~20 new tests | +20 |
| 9 | `feat: add TerminalOutput ratatui one-shot backend` | Replace `println!` stub with ratatui widgets, tty-fallback to plain | 0 |
| 10 | `feat: add interactive TUI home screen (no-args entry point)` | `app.rs`, `run_interactive_tui`, `None` arm in `main.rs`, 8 state machine tests | +8 |

Starting baseline: 71 tests. Target after all commits: ~129 tests.

## Success Criteria

- `cargo test` passes on every commit with no new warnings.
- `granite-cli model catalog` produces a bordered, aligned ratatui table.
- `granite-cli model catalog --output json` produces newline-delimited JSON parseable by `jq`.
- `granite-cli model catalog --output plain` produces readable plain text when piped to a file.
- `granite-cli` (no args) launches the interactive TUI and responds to `q`, `Tab`, `↑↓`, `Enter`.
- All existing setup wizard flows (`provider setup`, `model setup`) continue to work
  identically to before.
- No `println!` calls remain in `catalog`, `list`, `info`, or `health` command methods.
- `src/utils/web_fetch.rs` is deleted and `reqwest`/`html2md`/`regex` are removed from
  `Cargo.toml`.

## Future Work (Out of Scope for This Spec)

- **Setup wizard TUI forms.** Replacing `dialoguer` prompts in `model setup` / `provider setup`
  with ratatui form widgets. Blocked on raw-mode input handling design.
- **Additional output backends.** `--output markdown`, `--output csv`. Both can be added as
  new files with one `register` line — no spec update required.
- **Scrollable detail pane in the interactive TUI.** The detail view currently renders a static
  block; long descriptions may overflow. A scroll offset can be added to `App` state.
- **Search/filter in the interactive TUI.** A `/` key to enter a filter mode on the content
  table. Requires an additional `AppMode::Filter(String)` variant and a text input widget.


---

## Implementation Record

This section documents every file that was added, changed, or deleted during the implementation
of this spec, together with the reason for each decision. The changes were applied across 10
atomic commits on the `feature/ratatui` branch.

### Files Deleted

| File | Reason |
|------|--------|
| `src/utils/web_fetch.rs` | Dead code — exported `fetch_markdown` and `extract_urls` but no production code called either function. Deleting it removed `html2md` and `regex` from the compile graph. `reqwest` was *not* removed here (see `Cargo.toml` below). |
| `src/utils/schema_prompt.rs` | Contents moved verbatim to `src/utils/ui/prompt.rs` as part of gathering all UI concerns under a single `ui/` module. No logic was changed. |

### Files Added

#### `src/utils/ui/mod.rs`
Created as the root of the new `ui` submodule. Declares all child modules (`app`, `backends`,
`output`, `prompt`, `tui`) and re-exports the three most-used symbols
(`run_interactive_tui`, `CaptureOutput`, `Output`, `OUTPUT_REGISTRY`) so call sites only need
`use crate::utils::ui::…`.

Also contains the `OUTPUT_REGISTRY` static (a `LazyLock<OutputFactory>`) that registers the
three built-in backends by name.

#### `src/utils/ui/output.rs`
The heart of the abstraction layer. Contains:

- **`Output` trait** — the six-method contract (`table`, `detail`, `status`, `info`, `warn`,
  `error`) that every backend and command now uses.
- **`OutputMetadata`** — the metadata struct required by `define_factory!`.
- **`OutputFactory`** — generated by `define_factory!`; maps string names to boxed
  `Output` implementations.
- **`OUTPUT_REGISTRY`** — the global singleton that registers `terminal`, `plain`, and `json`.
- **`CaptureOutput`** — a test double with `RefCell<Vec<…>>` fields for each call type,
  allowing command tests to assert on structured data instead of parsing stdout.
- **`output_contract_tests!` macro** — generates 8 panic-safety tests for any `Output`
  implementation; defined with `#[macro_export]` so backend modules can invoke it with one line.

The choice to place `CaptureOutput` in the same file as the trait (rather than a separate
`test_utils.rs`) was deliberate: the test double is the canonical reference implementation
of the trait, and co-locating it with the trait makes the contract explicit.

#### `src/utils/ui/tui.rs`
Low-level ratatui plumbing for the interactive TUI only:

- **`setup_terminal()`** — enables raw mode, enters the alternate screen, returns a `Term`.
- **`restore_terminal()`** — disables raw mode, leaves the alternate screen, shows the cursor.
- **`Term`** type alias — `Terminal<CrosstermBackend<Stdout>>`.

`render_once()` was removed in the fix for issue #7 (see `docs/issues/0007-terminal-output-inline-render.md`). It was an attempt to use ratatui for one-shot command output, which is the wrong tool for that job. `tui.rs` now contains only what `app.rs` needs for the interactive TUI.

#### `src/utils/ui/app.rs`
The full interactive TUI application. Contains:

- **`Section`** enum (`Models`, `Providers`, `Capabilities`) with `next()` for Tab cycling and
  `label()` for display.
- **`AppMode`** enum (`Browse`, `Detail(String)`) for the two display states.
- **`App`** struct holding `ctx`, `section`, `row`, and `mode`, with:
  - `handle_key()` — pure state-machine logic, returns `true` to signal quit. Tested without
    a terminal because it is pure logic.
  - `render()` — drives the ratatui layout and dispatches to private render helpers.
  - Private helpers: `render_nav`, `render_browse`, `render_detail`, `render_footer`,
    `row_count`, `selected_id`.
- **`run_interactive_tui()`** — the `async fn` called from `main.rs`'s `None` arm. Owns the
  event loop: `terminal.draw → event::read → handle_key → repeat until quit`.
- **10 unit tests** for `handle_key` covering every key binding. These run without a real
  terminal because they call `handle_key` directly and assert on `app.section`, `app.row`,
  and `app.mode`.

#### `src/utils/ui/prompt.rs`
Moved from `src/utils/schema_prompt.rs` without any logic changes. The file contains the
`prompt_from_schema` function used by the setup wizards. Moving it here co-locates all UI
code and allowed the deletion of the old path. All existing callers were updated to the new
path in the same commit.

#### `src/utils/ui/backends/mod.rs`
One-line file declaring the three backend submodules (`json`, `plain`, `terminal`). Required
by the Rust module system; kept minimal by design.

#### `src/utils/ui/backends/plain.rs`
`PlainOutput` — plain text, no ANSI escape codes. Columns are space-padded to the widest
value in each column. Chosen as the fallback for `TerminalOutput` when stdout is not a tty
(pipe, file redirect, CI environment). Also the correct choice for `--output plain`. Uses
`eprintln!` for `warn` and `error` to match the convention that error output goes to stderr.
Contains `output_contract_tests!(PlainOutput::new())` for 8 panic-safety tests.

#### `src/utils/ui/backends/json.rs`
`JsonOutput` — newline-delimited JSON. Each `Output` method emits one JSON object. Accepts an
optional writer via `JsonOutput::with_writer(w)` using a `SharedWriter(Arc<Mutex<Vec<u8>>>)`
pattern so tests can capture output without spawning a subprocess. `serde_json::json!` is used
for serialisation — no new dependencies required since `serde_json` was already in the graph.
Contains `output_contract_tests!` plus 4 content-assertion tests that parse the emitted JSON
and check field names and values.

#### `src/utils/ui/backends/terminal.rs`
`TerminalOutput` — the default backend. Uses **direct crossterm ANSI codes** alongside
`println!` for coloured, column-aligned output. No ratatui widgets, no raw mode, no `Terminal`.

Non-tty detection is done once at the call site via `crossterm::terminal::size()` — if it
returns an error stdout is not a tty and every method delegates to `PlainOutput` instead.
This makes `granite-cli model catalog > out.txt` produce readable plain text automatically.

The original implementation used ratatui one-shot rendering via `render_once()`, which caused
compiler warnings from `cargo run` to bleed inside widget borders (issue #7). Replacing it
with direct ANSI output resolves the issue and simplifies the code — `TerminalOutput` no
longer depends on `tui.rs` at all.

### Files Changed

#### `src/main.rs`
Three targeted edits across the 10 commits:

1. **Added `--output` global flag** (`#[arg(long, global = true, default_value = "terminal")]`)
   to `Cli`. This single field drives the entire backend selection.
2. **Added `OUTPUT_REGISTRY` construction** in the `main()` preamble, with a clean error
   message and `process::exit(1)` for unknown format names.
3. **Replaced the `None` arm** (a block of 9 `println!` calls) with a call to
   `run_interactive_tui(ctx).await`. This is the change that turns no-argument invocation
   from a static help dump into a live TUI.
4. **Added `run_interactive_tui` to the import** from `utils::ui`.

The `run_*_command` dispatch functions gained `out: &dyn Output` as a final parameter,
threading the selected backend all the way down to the command methods.

#### `src/utils/mod.rs`
Two changes:
1. Removed `pub mod schema_prompt` and its re-export of `prompt_from_schema` (the module was
   moved to `utils/ui/prompt.rs`).
2. Added `pub mod ui` so the new submodule tree is reachable.

#### `src/commands/model.rs`
Every read-only method (`catalog`, `list`, `info`) gained `out: &dyn Output` as its final
parameter, and all `println!` calls inside those methods were replaced with the corresponding
`out.*` calls. The setup wizard (`setup`) was **not changed** — it continues using `println!`
and `dialoguer` directly per the non-goals in this spec. 10 new tests were added using
`CaptureOutput` to assert on column headers, row counts, and filter correctness.

#### `src/commands/provider.rs`
Same pattern as `model.rs`: `catalog`, `list`, and `health` gained `out: &dyn Output`.
`setup` was not changed. 6 new tests added.

#### `src/commands/capability.rs`
Same pattern: `catalog`, `list`, and `info` gained `out: &dyn Output`. `setup` was not
changed. 7 new tests added.

#### `Cargo.toml` / `Cargo.lock`
- **Removed** `html2md` — only used by the deleted `web_fetch.rs`.
- **Removed** `regex` — only used by the deleted `web_fetch.rs`.
- **Kept** `reqwest` — despite being pulled in by `web_fetch.rs`, it is also used directly by
  `src/providers/openai.rs` for HTTP health checks. Removing it would break provider health.
- **Added** `ratatui` — the TUI rendering library. Provides `Terminal`, `Frame`, widget types
  (`Table`, `List`, `Paragraph`, `Block`), layout primitives (`Layout`, `Constraint`), and
  style/colour types.
- **Added** `crossterm` — the cross-platform terminal backend for ratatui. Although ratatui
  pulls it in transitively, it must be declared directly in `Cargo.toml` to use its API
  (`enable_raw_mode`, `execute!`, `EnterAlternateScreen`, `event::read`) directly in our code.

### Test Count Progression

| After commit | Tests | Delta | New coverage |
|---|---|---|---|
| Baseline | 71 | — | — |
| 1 — remove web_fetch | 68 | −3 | (3 web_fetch tests deleted) |
| 2 — move schema_prompt | 68 | 0 | — |
| 3 — Output trait + CaptureOutput | 68 | 0 | — |
| 4 — CaptureOutput self-tests | 74 | +6 | CaptureOutput spy correctness |
| 5 — Output through model commands | 84 | +10 | Model command correctness |
| 6 — Output through provider/capability | 97 | +13 | Provider/capability command correctness |
| 7 — OutputFactory + --output flag | 101 | +4 | Registry construction, unknown format |
| 8 — PlainOutput + JsonOutput | 121 | +20 | Contract macro × 2 backends + JSON content |
| 9 — TerminalOutput | 121 | 0 | (tty-dependent; no auto tests) |
| 10 — interactive TUI | 132 | +11 | App state machine (10 tests) + 1 app module |
| **Final** | **132** | **+61** | |
