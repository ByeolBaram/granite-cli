# Spec 0012: Table Sorting Refactor

**Status**: Draft  
**Created**: 2026-07-30  
**Author**: System

## Problem Statement

Currently, all table outputs (models, providers, capabilities) use simple alphanumeric sorting on the ID column. This produces unintuitive ordering:

- **Models**: `granite-3.0-2b-instruct` appears before `granite-3.1-8b-instruct` (lexicographic on ID)
- **Providers**: No logical grouping by provider type
- **Result**: Users must scan the entire table to find related items

## Requirements (from interview)

### Model Sorting
Sort models by:
1. **Family** (exact family field from metadata, e.g., "Granite 3.1")
2. **Version** (from metadata version field, descending - newer first)
3. **Size** (from metadata size field, descending - larger first)
4. **ID** (case-sensitive ascending as tiebreaker)

**Example ordering**:
```
granite-3.1-8b-instruct    (Granite 3.1, v3.1, 8B)
granite-3.1-2b-instruct    (Granite 3.1, v3.1, 2B)
granite-3.0-8b-instruct    (Granite 3.0, v3.0, 8B)
granite-3.0-2b             (Granite 3.0, v3.0, 2B)
```

### Provider Sorting

**Catalog view**:
- Remove TYPE column (Local/Remote distinction is confusing)
- Sort by ID (factory key) only, case-sensitive ascending
- Columns: [ID, ENDPOINT]

**List view**:
- Keep TYPE column (shows factory key like "ollama", "openai-compatible")
- Sort by: TYPE (factory key) → ID (instance name)
- Both case-sensitive ascending
- Columns: [ID, TYPE, ENABLED, BASE_URL]

### Capability Sorting
Keep current alphanumeric sorting (already reasonable).

## Design Goals

1. **Domain-specific sorting**: Each command type implements its own sorting logic
2. **Metadata-driven**: Use registry metadata for accurate sorting (not string parsing)
3. **Code reuse**: Shared table rendering helpers within each command module
4. **Testability**: Sorting logic should be unit-testable
5. **Maintainability**: Clear separation between data preparation and rendering

## Architecture

### Model Commands Pattern

```rust
impl ModelCommands {
    // Existing row generation (returns Vec<Vec<String>>)
    pub(crate) fn catalog_rows(...) -> Vec<Vec<String>> { ... }
    
    // NEW: Enrich rows with metadata for sorting
    fn enrich_rows(rows: Vec<Vec<String>>) -> Vec<(Vec<String>, ModelMetadata)> {
        rows.into_iter()
            .filter_map(|row| {
                let id = &row[0];
                MODEL_REGISTRY.get(id).map(|meta| (row, meta.clone()))
            })
            .collect()
    }
    
    // NEW: Sort enriched rows
    fn sort_model_rows(enriched: &mut Vec<(Vec<String>, ModelMetadata)>) {
        enriched.sort_by(|(_, a), (_, b)| {
            // 1. Family (ascending)
            let family_cmp = a.family.cmp(&b.family);
            if family_cmp != Ordering::Equal {
                return family_cmp;
            }
            
            // 2. Version (descending - newer first)
            let version_cmp = compare_versions_desc(&a.version, &b.version);
            if version_cmp != Ordering::Equal {
                return version_cmp;
            }
            
            // 3. Size (descending - larger first)
            let size_cmp = b.size.cmp(&a.size);
            if size_cmp != Ordering::Equal {
                return size_cmp;
            }
            
            // 4. ID (case-sensitive ascending)
            a.family.cmp(&b.family)
        });
    }
    
    // NEW: Shared table rendering helper
    fn render_model_table(
        ctx: &crate::AppContext,
        title: &str,
        headers: &[&str],
        rows: Vec<Vec<String>>,
    ) {
        if rows.is_empty() {
            return;
        }
        
        let mut enriched = Self::enrich_rows(rows);
        Self::sort_model_rows(&mut enriched);
        let sorted_rows: Vec<Vec<String>> = enriched.into_iter()
            .map(|(row, _)| row)
            .collect();
        
        ctx.ui.table(title, headers, &sorted_rows);
    }
    
    // Updated commands use the helper
    pub fn catalog(ctx: &crate::AppContext, filter_type: Option<ModelType>) -> Result<()> {
        let rows = Self::catalog_rows(filter_type.as_ref());
        if rows.is_empty() {
            ctx.ui.info("No models found.");
            return Ok(());
        }
        Self::render_model_table(
            ctx,
            &format!("Model Catalog ({} models)", rows.len()),
            &["ID", "FAMILY", "SIZE", "CONTEXT", "TYPE"],
            rows,
        );
        Ok(())
    }
}
```

### Provider Commands Pattern

```rust
impl ProviderCommands {
    // NEW: Sort provider rows
    fn sort_provider_catalog_rows(rows: &mut Vec<Vec<String>>) {
        // Catalog: just sort by ID (factory key)
        rows.sort_by(|a, b| a[0].cmp(&b[0]));
    }
    
    fn sort_provider_list_rows(rows: &mut Vec<Vec<String>>) {
        // List: sort by TYPE (factory key) then ID (instance name)
        rows.sort_by(|a, b| {
            // a[1] = TYPE (factory key), a[0] = ID (instance name)
            let type_cmp = a[1].cmp(&b[1]);
            if type_cmp != Ordering::Equal {
                return type_cmp;
            }
            a[0].cmp(&b[0])
        });
    }
    
    // NEW: Shared helpers
    fn render_catalog_table(ctx: &crate::AppContext, mut rows: Vec<Vec<String>>) {
        Self::sort_provider_catalog_rows(&mut rows);
        ctx.ui.table(
            &format!("Provider Catalog ({} providers)", rows.len()),
            &["ID", "ENDPOINT"],
            &rows,
        );
    }
    
    fn render_list_table(ctx: &crate::AppContext, mut rows: Vec<Vec<String>>) {
        Self::sort_provider_list_rows(&mut rows);
        ctx.ui.table(
            &format!("Configured Providers ({} providers)", rows.len()),
            &["ID", "TYPE", "ENABLED", "BASE_URL"],
            &rows,
        );
    }
    
    // Updated commands
    pub fn catalog(ctx: &crate::AppContext) -> Result<()> {
        let rows: Vec<Vec<String>> = PROVIDER_REGISTRY.entries()
            .iter()
            .map(|(id, p)| vec![id.to_string(), p.default_endpoint.clone()])
            .collect();
        Self::render_catalog_table(ctx, rows);
        Ok(())
    }
    
    pub fn list(ctx: &crate::AppContext) -> Result<()> {
        let rows: Vec<Vec<String>> = ctx.config.providers
            .iter()
            .map(|(id, cfg)| {
                let base_url = cfg.config.get("base_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-")
                    .to_string();
                vec![id.clone(), cfg.provider_type.clone(), cfg.enabled.to_string(), base_url]
            })
            .collect();
        Self::render_list_table(ctx, rows);
        Ok(())
    }
}
```

## Implementation Details

### Version Comparison

```rust
/// Compare semantic versions in descending order (higher versions first).
/// Handles versions like "3.1", "4.0", "3.0.1".
fn compare_versions_desc(a: &str, b: &str) -> Ordering {
    let parse_version = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect()
    };
    
    let va = parse_version(a);
    let vb = parse_version(b);
    
    // Compare component by component, descending
    for (a_part, b_part) in va.iter().zip(vb.iter()) {
        match b_part.cmp(a_part) {  // Note: b.cmp(a) for descending
            Ordering::Equal => continue,
            other => return other,
        }
    }
    
    // If all compared parts are equal, longer version is "greater"
    // e.g., "3.0.1" > "3.0"
    vb.len().cmp(&va.len())
}
```

### Row Enrichment Pattern

The "enrich rows with metadata" approach:
1. Takes the generated `Vec<Vec<String>>` rows
2. Looks up metadata from registry for each row's ID
3. Creates tuples of `(row, metadata)`
4. Sorts using metadata fields
5. Extracts just the rows for display

**Benefits**:
- Explicit about what data is used for sorting
- Metadata lookup happens once, not on every comparison
- Easy to test (can mock metadata)
- Clear separation of concerns

**Trade-offs**:
- Slightly more memory (temporary metadata copies)
- Rows without metadata are filtered out (acceptable - they shouldn't exist)

## Changes Required

### Files to Modify

1. **`src/commands/model.rs`**
   - Add `enrich_rows()` function
   - Add `sort_model_rows()` function
   - Add `render_model_table()` helper
   - Update `catalog()`, `search()`, `list()` to use helper
   - Keep `recommend()` as-is (already has custom sorting)

2. **`src/commands/provider.rs`**
   - Add `sort_provider_catalog_rows()` function
   - Add `sort_provider_list_rows()` function
   - Add `render_catalog_table()` helper
   - Add `render_list_table()` helper
   - Update `catalog()` to remove TYPE column and use helper
   - Update `list()` to use helper

3. **Tests**
   - Add unit tests for `compare_versions_desc()`
   - Add integration tests for `sort_model_rows()`
   - Add integration tests for provider sorting
   - Update existing tests that check row order

### No Changes Required

- **`src/utils/ui/base.rs`**: Ui trait stays the same
- **`src/utils/ui/backends/*.rs`**: No backend changes
- **`src/commands/capability.rs`**: Keep current sorting
- **`src/commands/hardware.rs`**: No tables to sort

## Testing Strategy

### Unit Tests

```rust
#[test]
fn compare_versions_desc_simple() {
    assert_eq!(compare_versions_desc("3.1", "3.0"), Ordering::Less);  // 3.1 > 3.0
    assert_eq!(compare_versions_desc("3.0", "3.1"), Ordering::Greater);
    assert_eq!(compare_versions_desc("3.1", "3.1"), Ordering::Equal);
}

#[test]
fn compare_versions_desc_multi_part() {
    assert_eq!(compare_versions_desc("3.1.1", "3.1.0"), Ordering::Less);
    assert_eq!(compare_versions_desc("3.1", "3.1.0"), Ordering::Greater);  // shorter < longer
}

#[test]
fn compare_versions_desc_major_difference() {
    assert_eq!(compare_versions_desc("4.0", "3.1"), Ordering::Less);  // 4.0 > 3.1
}
```

### Integration Tests

```rust
#[test]
fn sort_model_rows_by_family_version_size() {
    let rows = vec![
        vec!["granite-3.0-8b".to_string(), "Granite 3.0".to_string(), "8B".to_string()],
        vec!["granite-3.1-2b".to_string(), "Granite 3.1".to_string(), "2B".to_string()],
        vec!["granite-3.1-8b".to_string(), "Granite 3.1".to_string(), "8B".to_string()],
        vec!["granite-3.0-2b".to_string(), "Granite 3.0".to_string(), "2B".to_string()],
    ];
    
    let mut enriched = ModelCommands::enrich_rows(rows);
    ModelCommands::sort_model_rows(&mut enriched);
    let sorted: Vec<String> = enriched.into_iter()
        .map(|(row, _)| row[0].clone())
        .collect();
    
    // Expected: Granite 3.0 family, then 3.1 family (newer first)
    // Within each: larger size first
    assert_eq!(sorted, vec![
        "granite-3.0-8b",
        "granite-3.0-2b",
        "granite-3.1-8b",
        "granite-3.1-2b",
    ]);
}

#[test]
fn sort_provider_list_by_type_then_id() {
    let mut rows = vec![
        vec!["prod-openai".to_string(), "openai-compatible".to_string()],
        vec!["local-ollama".to_string(), "ollama".to_string()],
        vec!["dev-openai".to_string(), "openai-compatible".to_string()],
    ];
    
    ProviderCommands::sort_provider_list_rows(&mut rows);
    
    assert_eq!(rows[0][0], "local-ollama");  // ollama < openai-compatible
    assert_eq!(rows[1][0], "dev-openai");    // openai-compatible, dev < prod
    assert_eq!(rows[2][0], "prod-openai");
}
```

### Test Updates

Existing tests that verify row order will need updates:
- `model.rs::catalog_table_has_correct_column_headers` - no change needed
- `model.rs::catalog_no_filter_returns_all_models` - update expected order
- `provider.rs::catalog_contains_openai_compatible_entry` - no change needed

## Implementation Plan

### Phase 1: Model Sorting (Priority)
1. ✅ Document requirements and design
2. Add `compare_versions_desc()` utility function with tests
3. Add `enrich_rows()` function
4. Add `sort_model_rows()` function with tests
5. Add `render_model_table()` helper
6. Update `catalog()` to use helper
7. Update `search()` to use helper
8. Update `list()` to use helper (note: has extra PROVIDER column)
9. Update existing tests that check row order
10. Manual testing with real data

### Phase 2: Provider Sorting
1. Remove TYPE column from catalog view
2. Add `sort_provider_catalog_rows()` function
3. Add `sort_provider_list_rows()` function with tests
4. Add `render_catalog_table()` helper
5. Add `render_list_table()` helper
6. Update `catalog()` to use helper
7. Update `list()` to use helper
8. Update existing tests
9. Manual testing

### Phase 3: Documentation & Cleanup
1. Update any user-facing documentation
2. Add inline code comments
3. Final review and testing

## Success Criteria

- [x] Requirements documented and confirmed
- [ ] Models sort by family → version (desc) → size (desc) → ID
- [ ] Providers catalog sorts by ID only (TYPE column removed)
- [ ] Providers list sorts by TYPE → ID
- [ ] All existing tests pass (with updated expectations)
- [ ] New tests cover version comparison and sorting logic
- [ ] Code duplication reduced (shared table helpers)
- [ ] No changes to Ui trait or backend implementations
- [ ] Manual testing confirms intuitive ordering

## Future Enhancements

- Add sorting options to CLI flags (e.g., `--sort-by=size`)
- Support reverse sorting
- Add sorting to TUI table views
- Generalize sorting framework for other table types
- Consider adding version field to table display for clarity

## Notes

- The `recommend()` command already has custom sorting by variant size (descending) and should be preserved as-is
- The Local/Remote provider type distinction may be fully refactored out in a future change
- Case-sensitive sorting is used throughout for consistency
- Metadata-driven sorting is more reliable than string parsing but requires registry lookups
