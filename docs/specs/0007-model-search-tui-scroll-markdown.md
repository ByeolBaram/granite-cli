# Spec 0007: Model Search, TUI Detail Scroll, and Markdown Output Backend

## Overview

Three additive improvements to land in the existing `feature/ratatui` PR before
it merges. Each is independently self-contained — they share no code paths,
can be implemented and committed in any order, and none touches the `Output`
trait, the factory macro, or any command that has already been stabilised.

---

## Feature 1: `model search` Command

### Problem

`model catalog` shows all 59 models. Finding a specific one requires visual
scanning or running `--output plain | grep`. There is no first-class search
from the CLI.

### Design

A new `granite-cli model search <query>` subcommand. It performs a
case-insensitive substring match and renders results through the existing
`Output` trait — all backends work automatically.

```
granite-cli model search 3.1
granite-cli model search vision --output json
granite-cli model search granite-4 --output plain
```

#### The `Searchable` trait — source of truth for searchable fields

Rather than hardcoding `id.contains() || family.contains()` in the command,
`ModelMetadata` implements a `Searchable` trait that declares which fields
participate in search. The command stays thin; the metadata owns the contract.

```rust
// src/models/base.rs
pub trait Searchable {
    /// All string values that should be matched against a search query.
    fn search_fields(&self) -> Vec<&str>;
}

impl Searchable for ModelMetadata {
    fn search_fields(&self) -> Vec<&str> {
        let mut fields = vec![self.family.as_str()];
        if let Some(desc) = &self.description {
            fields.push(desc.as_str());
        }
        fields.extend(self.tags.iter().map(String::as_str));
        fields
    }
}
```

Note: the ID is matched separately in the command (it is the map key, not on
`ModelMetadata` directly), so `search_fields` covers the remaining metadata
fields. Adding a new field to search (e.g. `model_type`, `tags`) is a
one-line change to `search_fields` — the command never changes.

The same `Searchable` trait can be added to `ProviderMetadata` and
`CapabilityMetadata` later, making `provider search` and
`capability search` follow the same pattern.

#### Command implementation

```rust
// src/commands/model.rs
pub fn search(_ctx: &crate::AppContext, query: &str, out: &dyn Output) -> Result<()> {
    let q = query.to_lowercase();
    let models = MODEL_REGISTRY.entries();

    let mut rows: Vec<Vec<String>> = models
        .iter()
        .filter(|(id, m)| {
            id.to_lowercase().contains(&q)
                || m.search_fields().iter().any(|f| f.to_lowercase().contains(&q))
        })
        .map(…)
        .collect();
    …
}
```

### Files changed

| File | Change |
|------|--------|
| `src/models/base.rs` | Add `Searchable` trait + `impl Searchable for ModelMetadata` |
| `src/main.rs` | Add `Search { query: String }` arm to `ModelSubcommands`; add match arm |
| `src/commands/model.rs` | Add `pub fn search()` using `Searchable` trait |

Everything else — `Output` trait, factory, other commands — **untouched**.

### Tests

New tests in `commands/model.rs`:

| Test | What it verifies |
|------|-----------------|
| `search_returns_matching_models_by_id` | Query "3.1" returns rows whose ID contains "3.1" |
| `search_is_case_insensitive` | Query "GRANITE" matches lowercase IDs |
| `search_no_match_emits_info_not_table` | Unknown query → info message, no table |
| `search_family_match_returns_rows` | Query on family substring returns correct rows |

New tests in `models/base.rs`:

| Test | What it verifies |
|------|-----------------|
| `searchable_fields_includes_family` | `search_fields()` contains the family string |
| `searchable_fields_includes_tags` | Tags appear in `search_fields()` when present |

### Commit message

```
feat: add model search subcommand with cross-field substring matching
```

---

## Feature 2: TUI Detail Pane Scrolling

### Problem

`render_detail` in `app.rs` renders a static `Paragraph`. For models with many
fields (variants, functions, description), content overflows the pane with no
way to scroll to see it.

### Design

Add `detail_scroll: usize` to `App`. In `AppMode::Detail`, `↑`/`k` and `↓`/`j`
adjust the scroll offset instead of navigating rows. `render_detail` passes the
offset to `Paragraph::scroll((offset, 0))`. `Backspace`/`Esc` resets the offset
to 0 when returning to Browse.

Key bindings in Detail mode:

| Key | Action |
|-----|--------|
| `↓` / `j` | Scroll down one line |
| `↑` / `k` | Scroll up one line |
| `Backspace` / `Esc` / `q` | Return to Browse, reset scroll to 0 |

### Files changed

| File | Change |
|------|--------|
| `src/utils/ui/app.rs` | Add `detail_scroll: usize` to `App`; update `handle_key` Detail arm; update `render_detail` to use `Paragraph::scroll`; update `render_footer` hints |

No other files change. Factory, backends, commands — **untouched**.

### Implementation detail

```rust
pub struct App {
    pub ctx:          crate::AppContext,
    pub section:      Section,
    pub row:          usize,
    pub mode:         AppMode,
    table_state:      TableState,
    pub detail_scroll: usize,   // ← new
}
```

```rust
// In handle_key, AppMode::Detail arm:
AppMode::Detail(_) => match key.code {
    KeyCode::Char('q') | KeyCode::Esc | KeyCode::Backspace => {
        self.mode = AppMode::Browse;
        self.detail_scroll = 0;       // reset on exit
    }
    KeyCode::Down | KeyCode::Char('j') => {
        self.detail_scroll += 1;
    }
    KeyCode::Up | KeyCode::Char('k') => {
        self.detail_scroll = self.detail_scroll.saturating_sub(1);
    }
    _ => {}
},
```

```rust
// In render_detail:
let para = Paragraph::new(content)
    .block(…)
    .wrap(ratatui::widgets::Wrap { trim: false })
    .scroll((self.detail_scroll as u16, 0));  // ← new
```

```rust
// Footer hint update:
AppMode::Detail(_) => "[↑↓/jk] Scroll  [Backspace/Esc/q] Back",
```

### Tests

New tests in `app.rs`:

| Test | What it verifies |
|------|-----------------|
| `detail_scroll_default_is_zero` | `app.detail_scroll == 0` on construction |
| `detail_down_increments_scroll` | `↓` in Detail mode increments `detail_scroll` |
| `detail_up_at_zero_stays_zero` | `↑` at offset 0 does not underflow |
| `detail_esc_resets_scroll_to_zero` | Exiting Detail resets `detail_scroll` |
| `detail_scroll_does_not_affect_browse_row` | `row` is unchanged when scrolling in Detail mode |

### Commit message

```
feat: add scrollable detail pane in TUI (↑↓ in Detail mode)
```

---

## Feature 3: `--output markdown` Backend

### Problem

There is no way to get GFM markdown table output from the CLI. This is useful
for pasting model catalogs into documentation, GitHub issues, or README files.

### Design

A fourth `Output` backend registered in `OUTPUT_REGISTRY` under the name
`"markdown"`. Adding it requires:

1. One new file: `src/utils/ui/backends/markdown.rs`
2. One new line in `src/utils/ui/backends/mod.rs`
3. One new `register` line in `OUTPUT_REGISTRY` in `src/utils/ui/output.rs`

Nothing else changes. This is the canonical demonstration that the factory
pattern works — adding a backend is three lines outside the new file.

```
granite-cli model catalog --output markdown
granite-cli model info granite-3.1-8b-instruct --output markdown
```

### Output format

```markdown
## Model Catalog (59 models)

| ID | FAMILY | SIZE | CONTEXT | TYPE |
|----|--------|------|---------|------|
| granite-3.0-1b-a400m-instruct | Granite 3.0 | 1B | 4096 | Text |
…

## granite-3.1-8b-instruct

| Field | Value |
|-------|-------|
| Family | Granite 3.1 |
…
```

- `table()` → GFM table with `##` title heading
- `detail()` → two-column GFM table (Field / Value) with `##` title heading
- `status()` → `✓` or `✗` prefix, plain line
- `info()` / `warn()` / `error()` → plain lines (warn/error prefixed)

### Files changed

| File | Change |
|------|--------|
| `src/utils/ui/backends/markdown.rs` | New file — full `Output` impl |
| `src/utils/ui/backends/mod.rs` | Add `pub mod markdown;` |
| `src/utils/ui/output.rs` | Add `f.register::<MarkdownOutput>("markdown");` in `OUTPUT_REGISTRY` |

`main.rs` `--output` flag help text should be updated to mention `markdown`
as a valid value.

### Tests

`markdown.rs` invokes `output_contract_tests!(MarkdownOutput::new(...))` for
the 8 panic-safety tests, plus content-assertion tests:

| Test | What it verifies |
|------|-----------------|
| `markdown_table_contains_pipe_chars` | Output contains `|` characters |
| `markdown_table_has_header_separator` | Output contains `|---|` separator row |
| `markdown_detail_is_two_column_table` | `detail()` output contains `Field` and `Value` headers |
| `markdown_status_ok_contains_checkmark` | `status(ok=true)` output contains `✓` |

### Commit message

```
feat: add --output markdown backend for GFM table output
```

---

## Blast Radius Summary

| Feature | Files added | Files changed | Files untouched |
|---------|-------------|---------------|-----------------|
| `model search` | 0 | `main.rs`, `commands/model.rs` | Everything else |
| TUI detail scroll | 0 | `app.rs` | Everything else |
| `--output markdown` | `backends/markdown.rs` | `backends/mod.rs`, `output.rs`, `main.rs` (help text) | Everything else |

The `Output` trait gains no new methods. The factory macro is not touched.
No existing tests change.

---

## Commit Sequence

| # | Message | Net test delta |
|---|---------|----------------|
| 1 | `feat: add model search subcommand with cross-field substring matching` | +4 |
| 2 | `feat: add scrollable detail pane in TUI (↑↓ in Detail mode)` | +5 |
| 3 | `feat: add --output markdown backend for GFM table output` | +12 |

Each commit leaves `cargo test` green. Target after all three: **~166 tests**
(145 current + 21 new).

---

## What This PR Will Contain After These Land

- Output abstraction + factory pattern (commits 1–10 from spec 0005)
- Scrollable tables + section-scoped TUI search (spec 0006)
- `TerminalOutput` ANSI fix (issue #7)
- `model search` subcommand ← new
- TUI detail pane scrolling ← new
- `--output markdown` backend ← new
