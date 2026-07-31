# Spec 0009: Decouple TUI Data Layer from Registry

## Problem

`src/utils/ui/app.rs` currently queries `MODEL_REGISTRY` and `PROVIDER_REGISTRY`
directly and duplicates all data-preparation logic that already lives in
`src/commands/model.rs`. Every new field or formatting change must be made in
two places — the command and the TUI — or it silently diverges (as seen with
`format_size()`, which had to be fixed in `app.rs` separately after being added
to `ModelCommands`).

```
Before:

  MODEL_REGISTRY ──► ModelCommands::catalog()  ──► out.table()
        │
        └──────────► app.rs render_browse()    ──► ratatui Rows   ← duplicate logic
```

## Solution

Extract thin **data-query methods** from `ModelCommands` that return plain
`Vec<Vec<String>>` rows and `Vec<(&str, String)>` field pairs — no `Output`
involved. Both the CLI command path and the TUI call the same methods. The TUI
retains full control over how it renders (ratatui widgets, styles, column
widths) — it just no longer owns the data preparation.

```
After:

  MODEL_REGISTRY ──► ModelCommands::catalog_rows()  ◄── ModelCommands::catalog() ──► out.table()
                              │
                              └──────────────────────◄── app.rs render_browse()   ──► ratatui Rows
```

## New methods

All new methods are `pub(crate)` — visible to `app.rs` but not part of the
public CLI API.

### `src/commands/model.rs`

```rust
/// Rows for the model catalog table: [id, family, size, context, type]
pub(crate) fn catalog_rows(filter_type: Option<&ModelType>) -> Vec<Vec<String>>

/// Rows for the model search table: [id, family, size, context, type]
pub(crate) fn search_rows(query: &str) -> Vec<Vec<String>>

/// Key-value fields for model detail: [(label, value), …]
/// Returns None if the model ID is not in the registry.
pub(crate) fn info_fields(id: &str) -> Option<Vec<(&'static str, String)>>
```

The existing `pub fn catalog()`, `pub fn search()`, and `pub fn info()` become
thin wrappers that call the new methods and pass results to `out`.

### `src/commands/model.rs` — existing public signatures unchanged

```rust
pub fn catalog(ctx, filter_type, out) {
    let rows = Self::catalog_rows(filter_type.as_ref());
    out.table(…, &rows);
}

pub fn search(ctx, query, out) {
    let rows = Self::search_rows(query);
    if rows.is_empty() { out.info(…); } else { out.table(…, &rows); }
}

pub fn info(ctx, id, out) {
    match Self::info_fields(id) {
        Some(fields) => out.detail(id, &fields),
        None => { out.error(…); bail!(…) }
    }
}
```

## TUI changes

`app.rs` replaces its direct registry calls with the new data-query methods:

| Current (app.rs) | After |
|-----------------|-------|
| `MODEL_REGISTRY.entries()` + manual map in `render_browse` | `ModelCommands::catalog_rows(None)` |
| `MODEL_REGISTRY.get(id)` + manual format in `render_detail` | `ModelCommands::info_fields(id)` |
| `MODEL_REGISTRY.entries().keys()` in `filtered_ids` | unchanged — filtering by ID only for TUI nav is correct |

`render_detail` builds its `Paragraph` content from `info_fields()` instead of
formatting strings manually.

## What this does NOT change

- The `Output` trait — no new methods
- The factory macro — untouched
- Provider and capability rendering in the TUI — out of scope for this spec;
  same pattern can be applied later as `ProviderCommands` data methods
- `filtered_ids()` — this is TUI-specific navigation state (ID-only substring
  filter for the nav list), not a data query, so it stays in `app.rs`
- All public command signatures — `catalog`, `search`, `info`, `recommend` are
  unchanged; callers (including `main.rs`) need zero changes

## Blast radius

| File | Change |
|------|--------|
| `src/commands/model.rs` | Extract `catalog_rows`, `search_rows`, `info_fields`; existing methods delegate to them |
| `src/utils/ui/app.rs` | `render_browse` (Models arm) and `render_detail` (Models arm) use new methods |
| Everything else | **Untouched** |

No existing tests change. The refactor is purely internal — all observable
behaviour is identical.

## Tests

No new tests are required: the existing command tests already cover
`catalog_rows` / `search_rows` / `info_fields` indirectly through the public
`catalog` / `search` / `info` wrappers. The TUI tests in `app.rs` already
verify `render_browse` and `render_detail` behaviour at the mode/key level.

## Commit message

```
refactor: extract ModelCommands data-query methods, decouple TUI from registry

catalog_rows(), search_rows(), info_fields() are now the single source of
truth for model data preparation. app.rs render_browse and render_detail
delegate to these instead of querying MODEL_REGISTRY directly. Adding a
new field or changing formatting now requires a change in one place only.
```
