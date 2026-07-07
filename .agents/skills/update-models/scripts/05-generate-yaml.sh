#!/usr/bin/env bash
set -euo pipefail

# Generates YAML from model metadata JSON
# Input: models.json
# Output: models.yaml

MODELS_FILE="${1:-data/models.json}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
UTILS_DIR="${SCRIPT_DIR}/utils"

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

generate_model_entry() {
    local model="$1"

    # Extract fields
    local repo=$(echo "$model" | jq -r '.repo')
    local id=$(basename "$repo")
    local family=$(echo "$model" | jq -r '.family')
    local version=$(echo "$model" | jq -r '.version')
    local size=$(echo "$model" | jq -r '.size')
    local context_length=$(echo "$model" | jq -r '.context_length')
    local model_type=$(echo "$model" | jq -r '.model_type')
    local variants=$(echo "$model" | jq -c '.variants')

    # Infer capabilities
    local capabilities=$(echo "$model" | "${UTILS_DIR}/infer-capabilities.sh")

    # Generate description
    local description=$(echo "$model" | jq -r .description || echo "")
    if [ "$description" == "" ]; then
        description=$("${UTILS_DIR}/format-description.sh" "$id" "$family" "$version" "$model_type")
    fi
    description="$(echo "$description" | sed 's,^,    ,g')" # Indent 4 spaces for multiline yaml

    # Suggest tags
    local tags=$(echo "$model" | "${UTILS_DIR}/suggest-tags.sh")

    # Generate YAML entry
    cat <<EOF
- id: ${id}
  family: "${family}"
  version: "${version}"
  size: ${size}
  context_length: ${context_length}
  model_type: "${model_type}"
  huggingface_repo: "${repo}"
  required_provider_capabilities:
EOF
    echo "$capabilities" | jq -r '.[] | "    - \(.)"'
    cat <<EOF
  variants:
EOF
    echo "$variants" | jq -r '.[] | "    - format: \(.format)\n      precision: \(.precision)\n      size_gb: \(.size_gb)\n      huggingface_path: \"\(.huggingface_path)\""'
    cat <<EOF
  description: |
${description}
  tags:
EOF
    echo "$tags" | jq -r '.[] | "    - \(.)"'
    echo ""
}

# Header
echo "# Granite Models Catalog"
echo "# Generated: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
echo "# Source: HuggingFace ibm-granite organization"
echo ""

# Process all models
jq -c '.[]' "$MODELS_FILE" | while read -r model; do
    generate_model_entry "$model"
done
