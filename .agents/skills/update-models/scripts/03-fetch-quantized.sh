#!/usr/bin/env bash
set -euo pipefail

# Cross-references models with quantized variants
# Input: models.json
# Output: Enriched models.json with variants array

MODELS_FILE="${1:-data/models.json}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HF_ORG="ibm-granite"
HF_API="https://huggingface.co/api"

if [ ! -f "$MODELS_FILE" ]; then
    echo "Error: Models file not found: $MODELS_FILE" >&2
    exit 1
fi

# Check for JSON tool (jq or python)
if ! command -v jq &> /dev/null; then
    if ! command -v python3 &> /dev/null && ! command -v python &> /dev/null; then
        echo "Error: Neither jq nor Python is available. Please install one of them." >&2
        exit 1
    fi
fi

find_variants() {
    local base_repo="$1"
    local base_name=$(basename "$base_repo")

    # Look for GGUF repo
    local gguf_repo="${base_repo}-GGUF"

    variants="[]"

    # Check if GGUF repo exists
    http_code=$($SCRIPT_DIR/utils/hf-curl.sh -o /dev/null -w "%{http_code}" "https://huggingface.co/${gguf_repo}")

    if [ "$http_code" = "200" ]; then
        # Fetch GGUF files
        gguf_files=$($SCRIPT_DIR/utils/hf-curl.sh "${HF_API}/models/${gguf_repo}/tree/main" 2>/dev/null | \
            jq -c '[.[] | select(.type == "file" and (.path | endswith(".gguf")))]' 2>/dev/null || echo "[]")

        # Process each GGUF file
        if [ "$gguf_files" != "[]" ]; then
            variants=$(echo "$gguf_files" | jq -c '[.[] | {
                format: "GGUF",
                precision: (.path | split("-") | last | split(".") | first | ascii_upcase),
                size_gb: ((.size // 0) / 1073741824 | floor * 10 / 10),
                huggingface_path: "'"${gguf_repo}"'/blob/main/\(.path)"
            }]')
        fi
    fi

    # Add base model as safetensors variant
    base_variant=$(jq -n \
        --arg repo "$base_repo" \
        '[{
            format: "safetensors",
            precision: "BF16",
            size_gb: 0,
            huggingface_path: $repo
        }]')

    # Merge variants
    echo "$variants" | jq --argjson base "$base_variant" '. + $base'
}

# Process each model
jq -c '.[]' "$MODELS_FILE" | while read -r model; do
    repo=$(echo "$model" | jq -r '.repo')

    echo "Finding variants for: $repo" >&2

    variants=$(find_variants "$repo")

    # Add variants to model
    echo "$model" | jq --argjson variants "$variants" '. + {variants: $variants}'
done | jq -s '.'