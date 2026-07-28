# TUI: Scrollable Tables and Section-Scoped Search

## Overview

This document specifies two incremental improvements to the interactive TUI introduced in
spec 0005. Both changes are self-contained to `src/utils/ui/app.rs`. No factories, registries,
backends, command methods, or `main.rs` are touched.

## Problem Statement

### 1 — Tables Do Not Scroll

The browse pane renders a ratatui `Table` widget via `render_widget` (the stateless path).
Ratatui's `Table` is a *stateful* widget — scroll position is tracked through a `TableState`
struct that must be passed to `render_stateful_widget`. Because `TableState` is never wired
up, the cursor highlight moves correctly in `App.row` but the viewport never follows it.
Once the list of items exceeds the visible height of the pane, rows below the fold are
unreachable in practice — the user can navigate down but never sees the selected row.

### 2 — No Search

There is no way to filter the content pane to a substring. With 24+ models, finding a
specific model by pressing `↓` dozens of times is tedious. Search should be scoped to the
active section so that pressing `/` in the Models pane searches only model IDs, and pressing
`Tab` then `/` searches only providers.

## Goals

1. The selected row is always visible — the table viewport scrolls automatically to keep it
   on screen.
2. Pressing `/` in Browse mode enters a per-section search input.
3. Typing filters the visible rows in real time; the match is case-insensitive substring on
   the item's ID.
4. `Esc` cancels search and restores the full list at the previously selected row.
5. `Enter` confirms the search, returns to Browse mode, and positions the cursor at the first
   matching row.
6. All new state transitions are covered by unit tests that run without a terminal.

## Non-Goals

- Regex or fuzzy search. Plain case-insensitive substring is sufficient.
- Searching across sections simultaneously.
- Filtering on columns other than ID (family, type, endpoint). Those can be added later.
- Changing any factory, registry, backend, command, or `main.rs` code.

## Architecture

### The Only File That Changes: `src/utils/ui/app.rs`

#### New `AppMode` variant

```rust
pub enum AppMode {
    Browse,
    Search(String),   // ← new: the current query string
    Detail(String),
}
```

`Search` holds the query being typed. It is separate from `Browse` so the render path can
draw a search input bar and the key handler can route characters to the query instead of to
navigation.

#### `TableState` stored in `App`

```rust
pub struct App {
    pub ctx:      crate::AppContext,
    pub section:  Section,
    pub row:      usize,
    pub mode:     AppMode,
    table_state:  TableState,   // ← new: ratatui scroll tracker
}
```

`TableState` is `ratatui::widgets::TableState`. It is private because nothing outside `App`
needs to read or set it — only `render_browse` uses it.

Every time `self.row` changes (in `handle_key`), `self.table_state.select(Some(self.row))`
is called immediately after. This keeps the two fields in sync at every transition point
rather than trying to synchronise them inside the render path.

A helper `sync_table_state` encapsulates the single call so every branch in `handle_key`
only calls one function:

```rust
fn sync_table_state(&mut self) {
    self.table_state.select(Some(self.row));
}
```

When `Tab` switches section, both `self.row` and `self.table_state` are reset to 0 /
`select(Some(0))`.

#### Scroll in `render_browse`

The stateful call replaces the current stateless call:

```rust
// before
frame.render_widget(table, area);

// after
frame.render_stateful_widget(table, area, &mut self.table_state);
```

No other changes to the widget construction code. Ratatui handles viewport calculation
automatically once `TableState` has a selected index.

#### Search flow

```
AppMode::Browse
    │
    │  user presses /
    ▼
AppMode::Search("")           ← query starts empty
    │
    │  user types characters   → query grows: "gran", "granite", "granite-3"
    │  matching rows re-render in real time (filtered in render_browse)
    │
    ├─ user presses Esc        → AppMode::Browse, row unchanged (cursor stays where it was)
    │
    └─ user presses Enter      → AppMode::Browse, row = index of first matching item (or 0)
                                  table_state synced to new row
```

Key routing in `handle_key`:

| Mode | Key | Action |
|------|-----|--------|
| Browse | `/` | `self.mode = AppMode::Search(String::new())` |
| Search | `Char(c)` | append `c` to query |
| Search | `Backspace` | pop last char from query |
| Search | `Esc` | `self.mode = AppMode::Browse` (no row change) |
| Search | `Enter` | set `self.row` to first match index, sync table state, `self.mode = AppMode::Browse` |

`Ctrl-C` and `q` do **not** quit while in Search mode — `q` is a valid search character.
Only `Esc` exits search without committing.

#### Filtering logic

A private helper returns the filtered, sorted ID list for the current section:

```rust
fn filtered_ids(&self, query: &str) -> Vec<String> {
    let q = query.to_lowercase();
    let mut ids: Vec<String> = match self.section {
        Section::Models       => MODEL_REGISTRY.entries().keys()…,
        Section::Providers    => PROVIDER_REGISTRY.entries().keys()…,
        Section::Capabilities => CAPABILITY_REGISTRY.entries().keys()…,
    };
    ids.sort();
    if q.is_empty() {
        ids
    } else {
        ids.into_iter().filter(|id| id.to_lowercase().contains(&q)).collect()
    }
}
```

`row_count` and `selected_id` are updated to delegate to `filtered_ids` with the active
query (empty string when in Browse mode):

```rust
fn active_query(&self) -> &str {
    match &self.mode {
        AppMode::Search(q) => q.as_str(),
        _ => "",
    }
}

fn row_count(&self) -> usize {
    self.filtered_ids(self.active_query()).len()
}

fn selected_id(&self) -> Option<String> {
    self.filtered_ids(self.active_query()).into_iter().nth(self.row)
}
```

This means that while typing, navigation (`↓`/`↑`) continues to work against the filtered
list, and `row_count` naturally clamps navigation to the smaller result set.

#### Search bar in `render_browse`

The outer layout for the content pane gains a one-line input bar at the bottom when in
Search mode:

```
┌── Models ─────────────────────────────────────────────────────────┐
│  ID                    FAMILY   SIZE  TYPE                         │
│  granite-3.1-8b-inst…  Granite   8B   Text    ← highlighted row   │
│  …                                                                 │
├───────────────────────────────────────────────────────────────────┤
│  / granite-3                                   ← search bar       │
└───────────────────────────────────────────────────────────────────┘
```

The bar is only rendered in `AppMode::Search`. It uses a `Paragraph` with a yellow border
so it is visually distinct from the table.

#### Footer hints update

| Mode | Footer text |
|------|-------------|
| Browse | `[↑↓/jk] Navigate  [Tab] Section  [Enter] Detail  [/] Search  [q] Quit` |
| Search | `[typing] Filter  [Enter] Confirm  [Esc] Cancel` |
| Detail | `[Backspace/Esc] Back  [q] Quit` |

## File Structure

Only one file changes:

```
src/utils/ui/app.rs    ← TableState field, AppMode::Search, handle_key routing,
                          filtered_ids, render_browse stateful + search bar,
                          render_footer updated hints
```

All other files are untouched.

## Testing Strategy

All new tests are pure state-machine tests — no terminal required.

| Test | What it verifies |
|------|-----------------|
| `search_slash_enters_search_mode` | `/` key sets `AppMode::Search("")` |
| `search_typing_appends_to_query` | Char keys grow the query string |
| `search_backspace_removes_last_char` | Backspace shrinks the query |
| `search_esc_returns_to_browse_without_changing_row` | Esc exits search, row unchanged |
| `search_enter_sets_row_to_first_match` | Enter positions cursor at first matching row |
| `search_enter_with_no_match_sets_row_to_zero` | Empty result set clamps to row 0 |
| `search_q_does_not_quit` | `q` is appended to query, returns `false` |
| `filtered_ids_empty_query_returns_all` | `filtered_ids("")` length == full registry size |
| `filtered_ids_substring_filters_correctly` | `filtered_ids("3.1")` returns only matching IDs |
| `tab_in_search_mode_does_nothing` | Tab is ignored while in Search mode |

## Commit Plan

| # | Message | Changes | Test delta |
|---|---------|---------|------------|
| 1 | `feat: wire TableState for scrollable table in TUI browse pane` | Add `table_state` field, `sync_table_state`, switch to `render_stateful_widget` | 0 (render change, no new logic tests) |
| 2 | `feat: add section-scoped search to TUI (/ key)` | `AppMode::Search`, `filtered_ids`, `active_query`, key routing, search bar, footer hints | +10 |

Each commit leaves `cargo test` green.
