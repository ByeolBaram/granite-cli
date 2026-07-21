# Issue #7: TerminalOutput One-Shot Rendering

**Upstream issue:** https://github.ibm.com/ghart/granite-cli/issues/7  
**Affects:** `src/utils/ui/backends/terminal.rs`, `src/utils/ui/tui.rs`  
**Introduced in:** commit `d23f3bc` (feat: add TerminalOutput ratatui one-shot backend)

---

## Problem

When `granite-cli` is invoked via `cargo run`, compiler warnings printed to
stderr by cargo appear *inside* the ratatui widget border:

```
┌ Model Catalog ─────────────────────────────────────────────────────────────┐
│ID   FAMILY   SIZE   TYPE                                                    │
│warning: variable does not need to be mutable                                │
│  --> src/commands/capability.rs:100:21                                      │
└─────────────────────────────────────────────────────────────────────────────┘
```

Even after fixing the viewport approach (using `Viewport::Inline` +
`insert_before`), the output either shows nothing or leaves excessive blank
space below the table — because ratatui is a *full-screen TUI framework*, not
a line-printer.

## Root Cause: Wrong Tool

Ratatui is designed for two use cases:

1. **Interactive full-screen TUI** — owns the terminal for a session, redraws
   on every event. This is what `run_interactive_tui` (`app.rs`) uses.
   **This works perfectly and is not changing.**

2. **Inline viewport** (`Viewport::Inline` + `insert_before`) — intended for a
   *live* TUI that also emits log lines above itself as it runs. Still expects
   to own the terminal across multiple draw cycles.

Neither use case matches one-shot command output: print a table, return to
the shell prompt, done. Every ratatui approach for this pattern requires
fighting the framework's assumptions about terminal ownership.

## Solution: Direct ANSI Output in TerminalOutput

`TerminalOutput` for one-shot command output should use **crossterm ANSI codes
directly** alongside `println!` — exactly like `PlainOutput` does, but with
colour. No `Terminal`. No `draw()`. No raw mode. No viewport.

This is the correct factory-pattern answer:

- The `Output` trait says *what* to render (table, detail, status…)
- `PlainOutput` implements *how* for plain text
- `TerminalOutput` implements *how* for coloured ANSI text
- `app.rs` + `tui.rs` implement *how* for the interactive full-screen TUI

These are three distinct rendering targets. Conflating the last two was the
design error.

## Design

```
cargo run -- model catalog
    │
    └── TerminalOutput::table()
            │
            └── crossterm::style::Print / SetForegroundColor / ResetColor
                    │
                    └── println! coloured rows  →  stays in terminal  ✓
```

```
cargo run   (no args)
    │
    └── run_interactive_tui()
            │
            └── setup_terminal() → ratatui draw loop → restore_terminal()
                    │
                    └── full-screen TUI  ✓
```

### TerminalOutput rendering

Each `Output` method uses crossterm's `style` module inline with `print!` /
`println!` — no raw mode, no terminal setup:

```rust
use crossterm::style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor};

fn table(&self, title: &str, headers: &[&str], rows: &[Vec<String>]) {
    // Title line in bold
    println!("\n{}{}{}", SetAttribute(Attribute::Bold), title, SetAttribute(Attribute::Reset));
    // Cyan header row
    print!("{}", SetForegroundColor(Color::Cyan));
    // ... column-aligned headers ...
    println!("{}", ResetColor);
    // Alternating row colours, etc.
}
```

Non-tty detection: `crossterm::style` works on any writer. For piped output
(`> file`, CI), ANSI codes are undesirable. `TerminalOutput` checks
`crossterm::terminal::size()` at construction time — if it fails (not a tty),
it delegates every call to `PlainOutput` instead.

### tui.rs

`render_once()` is **deleted** — nothing calls it anymore. `tui.rs` retains
only `setup_terminal`, `restore_terminal`, and the `Term` type alias — the
three things `app.rs` needs for the interactive TUI.

## Blast Radius

**Only two files change:**

| File | Change |
|------|--------|
| `src/utils/ui/backends/terminal.rs` | Rewritten: drop ratatui widgets + `render_once`, add direct crossterm ANSI rendering with non-tty detection |
| `src/utils/ui/tui.rs` | Remove `render_once()`, `TerminalOptions`, `Viewport` imports; keep `setup_terminal`, `restore_terminal`, `Term` |

| File | Status |
|------|--------|
| `src/utils/ui/backends/plain.rs` | Untouched |
| `src/utils/ui/backends/json.rs` | Untouched |
| `src/utils/ui/output.rs` | Untouched |
| `src/utils/ui/app.rs` | Untouched |
| `src/utils/ui/mod.rs` | Untouched |
| `src/commands/*.rs` | Untouched |
| `src/main.rs` | Untouched |
| `Cargo.toml` | Untouched (`crossterm` already declared directly) |

## Success Criteria

- `cargo run -- model catalog` prints a clean coloured table directly in the
  terminal with no warning bleed and no extra blank space
- `cargo run -- model catalog > out.txt` produces readable plain text (non-tty
  fallback to `PlainOutput`)
- `cargo run -- model catalog --output plain` works unchanged
- `cargo run -- model catalog --output json` works unchanged
- `cargo run` (no args) launches the interactive TUI correctly (untouched path)
- `cargo test` stays green at 145+

## Commit Plan

| # | Message | Files |
|---|---------|-------|
| 1 | `fix: replace ratatui one-shot rendering with direct ANSI in TerminalOutput` | `terminal.rs`, `tui.rs` |

Single commit. Resolves issue #7.
