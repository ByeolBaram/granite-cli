# Spec 0010: TUI Recommend and Hardware Sections

## Problem

`granite-cli model recommend` and `granite-cli hardware` are CLI-only. The TUI
has no way to surface either. Users who prefer the interactive interface have to
drop back to the shell to see hardware info or recommendations.

## Design

Add two new sections to the TUI nav panel, following the exact same `Section`
enum / `Tab` cycle pattern already established.

```
Nav (Tab cycles):
  Models (59)
  Providers (1)
  Capabilities (0)
  Recommend (N)       ← new
  Hardware            ← new
```

### Recommend section

Behaves identically to the Models section — browseable table, `Enter` opens
detail, `/` filters by ID, `↑↓` navigate rows. The table is the output of
`ModelCommands::recommend_rows()` (new data-query method extracted from
`recommend()`), so it automatically respects the current hardware and stays in
sync with the CLI command.

Columns: `ID | FAMILY | SIZE | VARIANT | TYPE`

Detail pane: delegates to `ModelCommands::info_fields(id)` — same detail view
as the Models section, no duplication.

### Hardware section

No rows to navigate — it is a single detail view, shown immediately on
`Tab` to Hardware (no `Enter` required). Renders the output of
`HardwareCommands::hardware_fields()` (new data-query method) as a
`Paragraph` in the content pane.

Because there are no selectable rows, `↑↓/jk` scroll the content (reuses the
existing `detail_scroll` mechanic from spec 0007). `Tab` moves to the next
section as normal.

Footer hint in Hardware section:
```
[↑↓/jk] Scroll  [Tab] Section  [q] Quit
```

## New data-query methods

### `src/commands/model.rs`

```rust
/// Rows for the recommend table: [id, family, size, variant, type].
/// Shared by the CLI command and the TUI.
pub(crate) fn recommend_rows() -> Vec<Vec<String>>
```

`recommend()` becomes a thin wrapper calling `recommend_rows()`.

### `src/commands/hardware.rs`

```rust
/// Key-value fields for the hardware detail panel.
/// Shared by the CLI command and the TUI.
pub(crate) fn hardware_fields() -> Vec<(&'static str, String)>
```

`show()` becomes a thin wrapper calling `hardware_fields()`.

## Files changed

| File | Change |
|------|--------|
| `src/commands/model.rs` | Extract `recommend_rows()` |
| `src/commands/hardware.rs` | Extract `hardware_fields()` |
| `src/utils/ui/app.rs` | Add `Recommend` + `Hardware` to `Section`; update nav, browse, detail, footer, filtered_ids, row_count |

`main.rs`, `Output` trait, factory macro — **untouched**.

## Blast radius

Two command files get a thin private extract. `app.rs` grows two new `match`
arms. No existing arms change. No existing tests change.

## Tests

| Test | What it verifies |
|------|-----------------|
| `app_tab_cycles_through_all_five_sections` | Tab wraps after Hardware back to Models |
| `recommend_section_has_rows_or_empty` | Recommend section row count ≥ 0 |
| `hardware_section_row_count_is_zero` | Hardware has no selectable rows |
| `hardware_fields_has_all_keys` | `hardware_fields()` contains expected field labels |
| `recommend_rows_all_have_five_columns` | Every recommend row has 5 cells |

## Commit message

```
feat: add Recommend and Hardware sections to TUI

Wires model recommend and hardware profile into the interactive TUI.
Recommend is a browseable table using recommend_rows() from ModelCommands.
Hardware is a scrollable static detail pane using hardware_fields() from
HardwareCommands. Both share the same data-query layer as the CLI commands.
```
