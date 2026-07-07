---
name: update-models
description: Use this skill any time you need to update the set of models surfaced to the CLI in resources/models.yaml
---

# Update Models Skill

This skill automates the process of updating `resources/models.yaml` with the latest Granite models from HuggingFace and Ollama. It combines automated data collection with manual review to ensure accuracy and completeness.

## When to Use This Skill

- New Granite model versions are released
- New model families are added to the IBM Granite organization
- Quantized variants are published
- Ollama adds new Granite model support
- Model metadata needs to be updated or corrected

## Prerequisites

- `curl` - for HTTP requests
- `jq` - for JSON parsing
- Standard Unix tools: `grep`, `sed`, `awk`
- Internet connection to access HuggingFace and Ollama APIs

## Quick Start

```bash
# Run the complete workflow
.agents/skills/update-models/run-update.sh

# Review and merge
git diff -- resources/models.yaml
```

## Workflow Overview

### Phase 1: Perform Automated Update

```bash
# Run the complete workflow (this can take 5-10 minutes)
.agents/skills/update-models/run-update.sh
```

### Phase 2: Review

```bash
git diff -- resources/models.yaml
```

**Review Checklist:**
- [ ] All model IDs are unique
- [ ] Descriptions are accurate and informative prose
- [ ] Tags are appropriate and consistent
- [ ] Provider capabilities match model formats
- [ ] Variant sizes are reasonable
- [ ] No duplicate entries
- [ ] New models are properly categorized
- [ ] Existing models are not modified

## Field Mapping Reference

### ModelMetadata Structure
```rust
pub struct ModelMetadata {
    pub id: String,                              // Unique identifier
    pub family: String,                          // Model family name
    pub version: String,                         // Version number
    pub size: u64,                              // Parameters count
    pub context_length: u64,                    // Max context tokens
    pub model_type: ModelType,                  // Text/Vision/Speech/Embedding
    pub huggingface_repo: String,               // HF repo path
    pub required_provider_capabilities: Vec<String>, // Provider requirements
    pub variants: Vec<ModelVariant>,            // Available formats
    pub description: Option<String>,            // Human-readable description
    pub tags: Vec<String>,                      // Categorization tags
}

pub struct ModelVariant {
    pub format: String,             // File format (eg GGUF, safetensors)
    pub precision: String,          // Precision label (eg Q4_K_M, bfloat16)
    pub size_gb: f64,               // Size in GB to 3 decimal places
    pub huggingface_path: String,   // Path within hf.co to the individual model or file
}
```

## Error Handling

### API Rate Limits
If you encounter rate limits there are two things to try:

1. Ask the user to provide a huggingface token (`export HF_TOKEN=<token>`)
2. Increase delay between calls (`export HF_REQUEST_DELAY=2`)

### Validation Failures
```bash
# Validate generated YAML
./scripts/06-validate-yaml.sh data/models-new.yaml

# Common issues:
# - Duplicate IDs: Rename with qualifier
# - Missing required fields: Add manually
# - Invalid format: Check YAML syntax
```

## Advanced Usage

### Dry Run (No File Changes)
```bash
# Generate YAML without overwriting
./scripts/05-generate-yaml.sh data/models.json | tee data/models-preview.yaml
```

### Incremental Update
```bash
# Merge new models with existing ones
./scripts/07-merge-yaml.sh resources/models.yaml data/models-new.yaml > data/models-merged.yaml
```

## Troubleshooting

### Issue: Script fails with "jq: command not found"
**Solution:** Request that the user install jq: `brew install jq` (macOS) or `apt-get install jq` (Linux)

### Issue: HuggingFace API returns 403
**Solution:** Check if repo is private or rate-limited. Wait and retry.

### Issue: Ollama search returns no results
**Solution:** Model may not be on Ollama yet. Skip Ollama integration for that model.

### Issue: Generated YAML has syntax errors
**Solution:** Run `./scripts/06-validate-yaml.sh` to identify issues. Common causes:
- Unescaped special characters in descriptions
- Missing quotes around strings with colons
- Incorrect indentation

## Script Reference

### Core Scripts

| Script | Purpose | Input | Output |
|--------|---------|-------|--------|
| `01-list-collections.sh` | List HF collections | None | `collections.json` |
| `02-fetch-all-models.sh` | Fetch model metadata | `collections.json` | `models.json` |
| `03-fetch-quantized.sh` | Add quantized variants | `models.json` | Enriched `models.json` |
| `04-query-ollama.sh` | Add Ollama info | `models.json` | Enriched `models.json` |
| `05-generate-yaml.sh` | Generate YAML | `models.json` | `models-new.yaml` |
| `06-validate-yaml.sh` | Validate YAML | `models-new.yaml` | Validation report |
| `07-merge-yaml.sh` | Merge YAML files | Two YAML files | Merged YAML |

### Utility Scripts

| Script | Purpose |
|--------|---------|
| `infer-capabilities.sh` | Determine provider capabilities |
| `format-description.sh` | Generate description template |
| `suggest-tags.sh` | Suggest tags based on model type |
| `hf-curl.sh` | Run a curl call against huggingface with HF_TOKEN if available |

## Best Practices

1. **Always backup** `resources/models.yaml` before updating
2. **Review all flagged entries** marked with `[NEEDS REVIEW]` or `[SUGGESTED]`
3. **Test with subset** of models first to validate workflow
4. **Commit incrementally** - one model family per commit
5. **Document changes** in commit messages
6. **Validate YAML** before committing
7. **Check for duplicates** - ensure unique IDs
8. **Maintain consistency** - follow existing description/tag patterns

## Maintenance

### Updating the Skill
When HuggingFace or Ollama APIs change:
1. Update relevant scripts in `scripts/`
2. Test with current models
3. Update this documentation
4. Commit changes with clear description

### Adding New Model Types
To support new model types (e.g., "Granite Audio"):
1. Add to collection category mapping in `01-list-collections.sh`
2. Add ModelType variant in `src/models/base.rs`
3. Update field mapping rules in `05-generate-yaml.sh`
4. Document in this SKILL.md

## Related Documentation

- [ModelMetadata Structure](../../src/models/base.rs)
- [Provider Capabilities](../../src/providers/base.rs)
- [Registry System](../../docs/specs/0003-model-registry-implementation.md)

## Support

For issues or questions:
1. Check [Troubleshooting](#troubleshooting) section
2. Review script output for error messages
3. Consult [PLAN.md](./PLAN.md) for detailed workflow
4. Open an issue in the project repository