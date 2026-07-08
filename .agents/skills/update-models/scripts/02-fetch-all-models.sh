#!/usr/bin/env bash
set -euo pipefail

# Fetches model metadata for all models in collections
# Input: collections.json
# Output: JSON array of model metadata objects

COLLECTIONS_FILE="${1:-data/collections.json}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HF_API="https://huggingface.co/api"

if [ ! -f "$COLLECTIONS_FILE" ]; then
    echo "Error: Collections file not found: $COLLECTIONS_FILE" >&2
    exit 1
fi

# Check for JSON tool (jq or python)
if ! command -v jq &> /dev/null; then
    if ! command -v python3 &> /dev/null && ! command -v python &> /dev/null; then
        echo "Error: Neither jq nor Python is available. Please install one of them." >&2
        echo "  macOS: brew install jq" >&2
        echo "  Linux: apt-get install jq" >&2
        exit 1
    fi
fi

# Request delay to avoid rate limiting
REQUEST_DELAY="${HF_REQUEST_DELAY:-1}"

fetch_model_metadata() {
    local repo="$1"
    local family="$2"
    local version="$3"

    # If version is not set from the collection, parse from the model name
    if [ "$version" == "" ]; then
        if [[ "$repo" =~ [a-z\-]+-([0-9\.]+)-.* ]]; then
            version="${BASH_REMATCH[1]}"
        # Handle embedding model versioning
        elif [[ "$repo" =~ .*-(r[0-9\.]+).* ]]; then
            version="${BASH_REMATCH[1]}"
        fi
    fi

    # Fetch config.json if available
    config_url="https://huggingface.co/${repo}/raw/main/config.json"
    config=$($SCRIPT_DIR/utils/hf-curl.sh "$config_url")
    if [ "$config" == "Entry not found" ]; then
        echo "No config.json found" >&2
        config="{}"
    fi

    # Extract fields
    local size=0
    local context_length=8192
    local model_type="Text"
    local description=""

    if [ "$config" != "{}" ]; then
        context_length=$(echo "$config" | jq -r '.max_position_embeddings // .n_positions // 8192')
    fi

    # Get the model metadata to fetch the size
    md_url="https://huggingface.co/api/models/${repo}"
    md_size=$($SCRIPT_DIR/utils/hf-curl.sh "$md_url" | jq .safetensors.total)
    if [ "$md_size" != "null" ]; then
        size=$md_size
    fi

    # Get the model description from the README if available
    readme_url="https://huggingface.co/$repo/raw/main/README.md"
    readme=$($SCRIPT_DIR/utils/hf-curl.sh $readme_url)
    description=$(echo -e "$readme" | awk '/\*\*Model Summary:\*\*/ {found=1; sub(/.*\*\*Model Summary:\*\*[[:space:]]*/, ""); if ($0 != "") {seen=1; print}; next} /## Model Summary/ {found=1; next} found && seen && /^$/ {exit} found && /^$/ {next} found && NF > 0 {seen=1; print}')

    # Infer model type from family
    if [[ "$family" == *"Vision"* ]] || [[ "$family" == *"Docling"* ]]; then
        model_type="Vision"
    elif [[ "$family" == *"Speech"* ]]; then
        model_type="Speech"
    elif [[ "$family" == *"Embedding"* ]]; then
        model_type="Embedding"
    fi

    # Output JSON
    jq -n \
        --arg repo "$repo" \
        --arg family "$family" \
        --arg version "$version" \
        --argjson size "$size" \
        --argjson context_length "$context_length" \
        --arg model_type "$model_type" \
        --arg description "$description" \
        --argjson config "$config" \
        '{
            repo: $repo,
            family: $family,
            version: $version,
            size: $size,
            context_length: $context_length,
            model_type: $model_type,
            description: $description,
            config: $config
        }'

    sleep "$REQUEST_DELAY"
}

echo "["
first=true

# Process each collection
jq -c '.[]' "$COLLECTIONS_FILE" | while read -r collection; do
    name=$(echo "$collection" | jq -r '.name')
    slug=$(echo "$collection" | jq -r '.slug')
    category=$(echo "$collection" | jq -r '.category')
    version=$(echo "$collection" | jq -r '.version')

    # Skip quantized collection (processed separately)
    if [ "$category" = "quantized" ]; then
        continue
    fi

    echo "Processing collection: $name" >&2

    # Fetch models in collection
    collection_api="${HF_API}/collections/${slug}"
    models=$($SCRIPT_DIR/utils/hf-curl.sh "$collection_api" 2>/dev/null | jq -r '.items[] | select(.type == "model") | .id' || echo "")

    # Process each model
    while IFS= read -r repo; do
        if [ -z "$repo" ]; then
            continue
        fi
        if [[ "$repo" =~ .*-base ]]; then
            echo "  Skipping base model: $repo" >&2
            continue
        fi
        if [[ "$repo" =~ .*-lora-.* ]]; then
            echo "  Skipping lora adapter: $repo" >&2
            continue
        fi

        echo "  Fetching: $repo" >&2

        if [ "$first" = true ]; then
            first=false
        else
            echo ","
        fi

        fetch_model_metadata "$repo" "$name" "$version"
    done <<< "$models"
done

echo "]"