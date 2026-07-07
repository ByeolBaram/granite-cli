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
cd .agents/skills/update-models
./run-update.sh

# Or run individual steps
./scripts/01-list-collections.sh > data/collections.json
./scripts/02-fetch-all-models.sh data/collections.json > data/models.json
./scripts/03-generate-yaml.sh data/models.json > data/models-new.yaml

# Review and merge
diff resources/models.yaml data/models-new.yaml
cp data/models-new.yaml resources/models.yaml
```

## Workflow Overview

### Phase 1: Data Collection (Automated)

#### Step 1: List HuggingFace Collections
```bash
./scripts/01-list-collections.sh
```

Fetches all collections from `hf.co/ibm-granite` and filters them into categories:

**Included Collections:**
- Granite [VERSION] Language Models (e.g., "Granite 4.1 Language Models")
- Granite [MODALITY] (e.g., "Granite Vision", "Granite Speech")

**Excluded Collections:**
- Granite Data (datasets, not models)
- Granite Experiments (forward-looking, not production-ready)
- Granite Libraries (adapters, not standalone models)

**Output:** `data/collections.json`
```json
[
  {
    "name": "Granite 4.1 Language Models",
    "url": "https://huggingface.co/collections/ibm-granite/granite-41-language-models",
    "category": "language",
    "version": "4.1"
  },
  ...
]
```

#### Step 2: Fetch Models from Collections
```bash
./scripts/02-fetch-all-models.sh data/collections.json
```

For each collection, lists all models and fetches their metadata:
- Model card (README.md)
- Configuration (config.json)
- File list (to identify available formats)

**Output:** `data/models.json`
```json
[
  {
    "repo": "ibm-granite/granite-4.1-8b-instruct",
    "family": "Granite",
    "version": "4.1",
    "size": 8290000000,
    "context_length": 8192,
    "model_type": "Text",
    "files": [...],
    "config": {...}
  },
  ...
]
```

#### Step 3: Cross-Reference Quantized Variants
```bash
./scripts/03-fetch-quantized.sh data/models.json
```

Matches models with their quantized versions from "Granite Quantized Models" collection:
- GGUF formats (Q4_K_M, Q8_0, etc.)
- FP8 variants
- Other low-precision formats

**Output:** Enriches `data/models.json` with `variants` array

#### Step 4: Query Ollama Registry
```bash
./scripts/04-query-ollama.sh data/models.json
```

Searches Ollama for matching Granite models and extracts:
- Available model names
- Size/quantization tags
- Model URLs

**Output:** Enriches `data/models.json` with `ollama_info`

### Phase 2: YAML Generation (Semi-Automated)

#### Step 5: Generate YAML Entries
```bash
./scripts/05-generate-yaml.sh data/models.json
```

Converts JSON metadata to YAML format following the `ModelMetadata` structure:

**Automated Fields:**
- `id`: Derived from repo name
- `family`: From collection name
- `version`: Extracted from model name
- `size`: From config.json
- `context_length`: From config.json
- `model_type`: Inferred from collection
- `huggingface_repo`: Direct from API
- `variants`: Built from quantized models + file list

**Semi-Automated Fields (Flagged for Review):**
- `required_provider_capabilities`: Inferred from formats
  - GGUF → OllamaChat
  - Safetensors/BF16 → OpenAIChat
- `description`: Template generated, needs refinement
- `tags`: Suggested based on characteristics

**Output:** `data/models-new.yaml`

### Phase 3: Manual Review & Finalization

#### Step 6: Review Generated YAML
```bash
# Compare with current models.yaml
diff resources/models.yaml data/models-new.yaml

# Review flagged entries
grep "NEEDS REVIEW" data/models-new.yaml
grep "SUGGESTED" data/models-new.yaml
```

**Review Checklist:**
- [ ] All model IDs are unique
- [ ] Descriptions are accurate and informative
- [ ] Tags are appropriate and consistent
- [ ] Provider capabilities match model formats
- [ ] Variant sizes are reasonable
- [ ] No duplicate entries
- [ ] New models are properly categorized

#### Step 7: Refine Entries

Edit `data/models-new.yaml` to:
- Improve descriptions
- Add/remove tags
- Adjust provider capabilities if needed
- Fix any inconsistencies

#### Step 8: Update models.yaml
```bash
# Backup current file
cp resources/models.yaml resources/models.yaml.backup

# Replace with new version
cp data/models-new.yaml resources/models.yaml

# Commit changes
git add resources/models.yaml
git commit -m "Update models.yaml with latest Granite models"
```

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
```

### Extraction Rules

| Field | Source | Extraction Method |
|-------|--------|-------------------|
| `id` | HF repo name | Last segment, lowercase with hyphens |
| `family` | Collection name | Extract "Granite [Family]" pattern |
| `version` | Model name or collection | Regex: `\d+\.\d+` |
| `size` | config.json | `num_parameters` or calculate from architecture |
| `context_length` | config.json | `max_position_embeddings` |
| `model_type` | Collection category | Map: language→Text, vision→Vision, etc. |
| `huggingface_repo` | API response | Full repo path |
| `required_provider_capabilities` | File formats | GGUF→OllamaChat, safetensors→OpenAIChat |
| `variants` | Quantized collection + files | Build array of ModelVariant objects |
| `description` | Model card | Extract from README.md, refine manually |
| `tags` | Model characteristics | Infer from name/type, validate manually |

### ModelVariant Structure
```rust
pub struct ModelVariant {
    pub format: String,        // GGUF, safetensors, etc.
    pub precision: String,     // Q4_K_M, BF16, FP8, etc.
    pub size_gb: f64,         // File size in GB
    pub huggingface_path: String, // Full path to file/repo
}
```

## Collection Categories

### Language Models (Text)
- **Pattern:** "Granite [VERSION] (QUALIFIER)"
- **Examples:**
  - Granite 4.1 Language Models
  - Granite 4.0 Nano Language Models
  - Granite 3.3
  - Granite 3.1 Dense
  - Granite 3.1 MoE

### Multimodal Models
- **Pattern:** "Granite [MODALITY]"
- **Examples:**
  - Granite Vision (Vision)
  - Granite Speech (Speech)
  - Granite Embedding (Embedding)
  - Granite Docling (Vision/Document)

### Special Purpose Models
- **Granite Guardian** (Text, safety/moderation)
- **Granite Time Series** (Time Series)
- **Granite Geospatial Models** (Geospatial)

## Ollama Naming Convention

Ollama models follow the pattern: `granite[VERSION](-qualifier)`

**Examples:**
- `granite4.1` → Granite 4.1 Language Models
- `granite4` → Granite 4.0 Language Models (minor version dropped if 0)
- `granite3.2-vision` → Granite Vision 3.2
- `granite-code` → Granite Code models
- `granite-embedding` → Granite Embedding models

**Tag Format:** `<size><quant>`
- Size: `3b`, `8b`, `30b`, etc.
- Quant: `q4_k_m`, `q8_0`, etc. (optional)

**Example Tags:**
- `8b` - 8B parameters, default precision
- `8b-q4_k_m` - 8B parameters, Q4_K_M quantization

## Error Handling

### API Rate Limits
If you encounter rate limits:
```bash
# Add delay between requests
export HF_REQUEST_DELAY=2  # seconds
./scripts/02-fetch-all-models.sh data/collections.json
```

### Missing Metadata
When fields cannot be extracted:
- `size`: Flag as `[UNKNOWN]`, estimate from model name
- `context_length`: Use family default (8192 for most)
- `description`: Generate template: "[NEEDS REVIEW] Granite [version] [size] [type] model"

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

### Update Specific Collection
```bash
# Only update Granite 4.1 models
./scripts/02-fetch-all-models.sh \
  <(jq '.[] | select(.name | contains("4.1"))' data/collections.json)
```

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
**Solution:** Install jq: `brew install jq` (macOS) or `apt-get install jq` (Linux)

### Issue: HuggingFace API returns 403
**Solution:** Check if repo is private or rate-limited. Wait and retry.

### Issue: Ollama search returns no results
**Solution:** Model may not be on Ollama yet. Skip Ollama integration for that model.

### Issue: Generated YAML has syntax errors
**Solution:** Run `./scripts/06-validate-yaml.sh` to identify issues. Common causes:
- Unescaped special characters in descriptions
- Missing quotes around strings with colons
- Incorrect indentation

## Examples

### Example 1: Complete Update
```bash
cd .agents/skills/update-models

# Run full workflow
./run-update.sh

# Review output
less data/models-new.yaml

# Check diff
diff resources/models.yaml data/models-new.yaml

# Apply changes
cp data/models-new.yaml resources/models.yaml
git add resources/models.yaml
git commit -m "Update models.yaml: Add Granite 4.1 models"
```

### Example 2: Add Single Model
```bash
# Fetch specific model
./scripts/fetch-model-metadata.sh ibm-granite/granite-4.1-30b-instruct > data/single-model.json

# Generate YAML entry
./scripts/generate-yaml-entry.sh data/single-model.json

# Manually add to resources/models.yaml
```

### Example 3: Update Quantized Variants Only
```bash
# Re-fetch quantized models
./scripts/03-fetch-quantized.sh data/models.json

# Regenerate YAML
./scripts/05-generate-yaml.sh data/models.json > data/models-new.yaml

# Review variant changes
diff <(yq '.[] | .variants' resources/models.yaml) \
     <(yq '.[] | .variants' data/models-new.yaml)
```

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
| `parse-model-name.sh` | Extract family/version/size from name |
| `infer-capabilities.sh` | Determine provider capabilities |
| `calculate-size.sh` | Estimate model size from config |
| `format-description.sh` | Generate description template |
| `suggest-tags.sh` | Suggest tags based on model type |

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