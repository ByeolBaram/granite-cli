# Spec 0008: `hardware` Command and `model recommend`

## Overview

Two additive commands that wire existing dead code to user-facing value.
Neither touches the `Output` trait, the factory macro, or any previously
stabilised command. Both follow the same patterns established in specs 0005–0007.

---

## Feature 1: `hardware` Command

### Problem

`HardwareProfile::detect()` and `HardwareProfile::recommend_precision()` exist
in `src/utils/hardware.rs` but are never called from any command. Users have
no way to inspect what the CLI sees about their machine.

### Design

A new top-level subcommand `granite-cli hardware` that detects the current
machine's profile and renders it through the `Output` trait.

```
granite-cli hardware
granite-cli hardware --output json
granite-cli hardware --output markdown
```

#### Output

`out.detail()` with title `"Hardware Profile"` and the following fields:

| Field | Value example |
|-------|---------------|
| CPU Cores | `10` |
| CPU Architecture | `aarch64` |
| RAM | `16.00 GB` |
| GPU Vendor | `None` (or actual vendor) |
| VRAM | `None` (or `8.00 GB`) |
| Recommended Precision | `Q4_K_M` |

#### Files changed

| File | Change |
|------|--------|
| `src/main.rs` | Add `Hardware` arm to top-level `Commands`; dispatch to `HardwareCommands::show` |
| `src/commands/hardware.rs` | New file — `pub struct HardwareCommands; impl { pub fn show(...) }` |
| `src/commands/mod.rs` | Add `pub mod hardware;` + `pub use hardware::HardwareCommands;` |

`src/utils/hardware.rs` is **untouched** — it is already correct.

---

## Feature 2: `model recommend`

### Problem

Users don't know which models from the 59-model catalog will actually run
on their machine. They have to manually compare `model info` sizes against
their RAM/VRAM.

### Design

`granite-cli model recommend` detects hardware, filters the registry to models
whose *best fitting variant* can run given available memory, and renders a
table sorted by descending model size (largest runnable model first). An
optional `--type` filter (same as `model catalog`) narrows results.

```
granite-cli model recommend
granite-cli model recommend --type text
granite-cli model recommend --output json
```

#### Fit logic

For each model, find the **best variant**: the largest `size_gb` variant
whose `precision` matches `HardwareProfile::recommend_precision()`, or if none
match, the smallest variant overall. Then apply `HardwareProfile::can_run_model`
against that variant's `size_gb`.

```
Recommendation algorithm (per model):
  1. recommended_precision = profile.recommend_precision()
  2. preferred = variants where precision == recommended_precision (case-insensitive)
  3. best_variant = if preferred non-empty → largest size_gb in preferred
                    else                   → smallest size_gb across all variants
  4. fits = profile.can_run_model(best_variant.size_gb)
  5. Include model in results only if fits == true
```

#### Output columns

| Column | Source |
|--------|--------|
| ID | registry key |
| FAMILY | `metadata.family` |
| SIZE | `metadata.size` formatted as `NB` |
| VARIANT | `best_variant.format / best_variant.precision (N.N GB)` |
| TYPE | `metadata.model_type` |

Sorted: descending by `best_variant.size_gb` (largest runnable model first).

Title: `"Recommended Models for this hardware (N models)"`.

No models found → `out.info("No models fit the current hardware profile.")`.

#### Files changed

| File | Change |
|------|--------|
| `src/commands/model.rs` | Add `pub fn recommend()` to `ModelCommands` |
| `src/main.rs` | Add `Recommend { type }` arm to `ModelSubcommands`; dispatch |

Nothing else changes.

---

## Blast Radius

| Feature | Files added | Files changed | Untouched |
|---------|-------------|---------------|-----------|
| `hardware` | `commands/hardware.rs` | `commands/mod.rs`, `main.rs` | Everything else |
| `model recommend` | 0 | `commands/model.rs`, `main.rs` | Everything else |

`Output` trait: **no new methods**.
Factory macro: **untouched**.
`src/utils/hardware.rs`: **untouched**.
No existing tests change.

---

## Tests

### `commands/hardware.rs`

| Test | What it verifies |
|------|-----------------|
| `hardware_show_renders_detail` | `show()` calls `out.detail()` exactly once |
| `hardware_detail_has_cpu_and_ram_fields` | Detail fields include `"CPU Cores"` and `"RAM"` |
| `hardware_detail_has_recommended_precision` | Detail fields include `"Recommended Precision"` |

### `commands/model.rs`

| Test | What it verifies |
|------|-----------------|
| `recommend_returns_table_or_info` | Either a table or an info message is emitted (never both) |
| `recommend_all_rows_have_five_columns` | Every returned row has exactly 5 cells |
| `recommend_type_filter_limits_results` | `--type text` rows all have `"Text"` in TYPE column |
| `recommend_rows_sorted_descending_by_variant_size` | `VARIANT` size values are non-increasing |

---

## Branching strategy

Branching `feature/hardware-recommend` from `feature/ratatui` before that PR
merges is standard "stacked branch" / "branch from branch" practice. It is
fine as long as you keep in mind:

- When `feature/ratatui` is updated you need to `git rebase feature/ratatui`
  (or merge it in) to pick up the changes.
- When `feature/ratatui` merges to `main`, rebase `feature/hardware-recommend`
  onto `main` before opening its own PR.
- Keep commits on `feature/hardware-recommend` small so rebasing is cheap.

This is the same pattern used by projects like git itself and most large
open-source repos — it's perfectly fine practice.

---

## Commit sequence

| # | Message |
|---|---------|
| 1 | `feat: add hardware command wiring HardwareProfile to Output` |
| 2 | `feat: add model recommend command using hardware-aware variant filtering` |
