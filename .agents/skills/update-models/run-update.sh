#!/usr/bin/env bash
set -euo pipefail

# Main orchestration script for updating models.yaml
# Usage: ./run-update.sh [--dry-run]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="${SCRIPT_DIR}/data"
SCRIPTS_DIR="${SCRIPT_DIR}/scripts"

# Configuration
DRY_RUN=false
VERBOSE=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --verbose|-v)
            VERBOSE=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--dry-run] [--verbose]"
            exit 1
            ;;
    esac
done

# Create data directory
mkdir -p "${DATA_DIR}"

log() {
    echo "[$(date +'%Y-%m-%d %H:%M:%S')] $*" >&2
}

log "Starting model update workflow..."

# Step 1: List collections
log "Step 1/5: Listing HuggingFace collections..."
"${SCRIPTS_DIR}/01-list-collections.sh" > "${DATA_DIR}/collections.json"
COLLECTION_COUNT=$(jq 'length' "${DATA_DIR}/collections.json")
log "Found ${COLLECTION_COUNT} collections"

# Step 2: Fetch all models
log "Step 2/5: Fetching model metadata..."
"${SCRIPTS_DIR}/02-fetch-all-models.sh" "${DATA_DIR}/collections.json" > "${DATA_DIR}/models.json"
MODEL_COUNT=$(jq 'length' "${DATA_DIR}/models.json")
log "Found ${MODEL_COUNT} models"

# Step 3: Fetch quantized variants
log "Step 3/5: Cross-referencing quantized variants..."
"${SCRIPTS_DIR}/03-fetch-quantized.sh" "${DATA_DIR}/models.json" > "${DATA_DIR}/models-with-variants.json"
mv "${DATA_DIR}/models-with-variants.json" "${DATA_DIR}/models.json"

# Step 4: Query Ollama
log "Step 4/5: Querying Ollama registry..."
"${SCRIPTS_DIR}/04-query-ollama.sh" "${DATA_DIR}/models.json" > "${DATA_DIR}/models-with-ollama.json"
mv "${DATA_DIR}/models-with-ollama.json" "${DATA_DIR}/models.json"

# Step 5: Generate YAML
log "Step 5/5: Generating YAML..."
"${SCRIPTS_DIR}/05-generate-yaml.sh" "${DATA_DIR}/models.json" > "${DATA_DIR}/models-new.yaml"

# Validate
log "Validating generated YAML..."
"${SCRIPTS_DIR}/06-validate-yaml.sh" "${DATA_DIR}/models-new.yaml"

if [ "$DRY_RUN" = true ]; then
    log "Dry run complete. Generated file: ${DATA_DIR}/models-new.yaml"
    log "Review the file and run without --dry-run to apply changes"
else
    log "Update complete!"
    log "Next steps:"
    log "  1. Review: diff resources/models.yaml ${DATA_DIR}/models-new.yaml"
    log "  2. Check flagged entries: grep 'NEEDS REVIEW' ${DATA_DIR}/models-new.yaml"
    log "  3. Apply: cp ${DATA_DIR}/models-new.yaml resources/models.yaml"
fi
