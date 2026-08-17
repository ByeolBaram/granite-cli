#!/usr/bin/env bash
set -euo pipefail

# Main orchestration script for updating models.yaml
# Usage: ./run-update.sh [--dry-run]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="${SCRIPT_DIR}/data"
SCRIPTS_DIR="${SCRIPT_DIR}/scripts"
ROOT_DIR=$(cd $SCRIPT_DIR/../../.. && pwd)

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
log "Step 1/6: Listing HuggingFace collections..."
"${SCRIPTS_DIR}/01-list-collections.sh" > "${DATA_DIR}/collections.json"
COLLECTION_COUNT=$(jq 'length' "${DATA_DIR}/collections.json")
log "Found ${COLLECTION_COUNT} collections"

# Step 2: Fetch all models
log "Step 2/6: Fetching model metadata..."
"${SCRIPTS_DIR}/02-fetch-all-models.sh" "${DATA_DIR}/collections.json" > "${DATA_DIR}/models.json"
MODEL_COUNT=$(jq 'length' "${DATA_DIR}/models.json")
log "Found ${MODEL_COUNT} models"

# Step 3: Fetch quantized variants
log "Step 3/6: Cross-referencing quantized variants..."
"${SCRIPTS_DIR}/03-fetch-quantized.sh" "${DATA_DIR}/models.json" > "${DATA_DIR}/models-with-variants.json"
mv "${DATA_DIR}/models-with-variants.json" "${DATA_DIR}/models.json"

# Step 4.1: Query Ollama
log "Step 4.1/6: Querying Ollama registry..."
"${SCRIPTS_DIR}/04-01-query-ollama.sh" "${DATA_DIR}/models.json" > "${DATA_DIR}/models-with-ollama.json"
mv "${DATA_DIR}/models-with-ollama.json" "${DATA_DIR}/models.json"

# Step 4.2: Query LM Studio
log "Step 4.2/6: Querying LM Studio catalog..."
"${SCRIPTS_DIR}/04-02-query-lmstudio.sh" "${DATA_DIR}/models.json" > "${DATA_DIR}/models-with-lmstudio.json"
mv "${DATA_DIR}/models-with-lmstudio.json" "${DATA_DIR}/models.json"

# Step 6: Generate YAML
log "Step 6/6: Generating YAML..."
"${SCRIPTS_DIR}/06-generate-yaml.sh" "${DATA_DIR}/models.json" > "${DATA_DIR}/models-new.yaml"

# Validate
log "Validating generated YAML..."
"${SCRIPTS_DIR}/07-validate-yaml.sh" "${DATA_DIR}/models-new.yaml"

if [ "$DRY_RUN" = true ]; then
    log "Dry run complete. Generated file: ${ROOT_DIR}/${DATA_DIR}/models-new.yaml"
    log "Review the file and run without --dry-run to apply changes"
else
    log "Update complete!"
    cp ${DATA_DIR}/models-new.yaml ${ROOT_DIR}/resources/models.yaml
fi
