#!/usr/bin/env bash
set -euo pipefail

# Lists all HuggingFace collections from ibm-granite org
# Filters out excluded collections (Data, Experiments, Libraries)
# Output: JSON array of collection objects

HF_API="https://huggingface.co/api/collections?owner=ibm-granite&limit=100"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Excluded collection patterns
EXCLUDED_PATTERNS=(
    "Granite Data"
    "Granite Experiments"
    "Granite Libraries"
    "Granite Time Series"
    "Granite Geospatial Models"
    "Granite 2.0 Code Models (deprecated)"
)

# Fetch collections
collections_json=$(${SCRIPT_DIR}/utils/hf-curl.sh "${HF_API}")

echo "["
first=true

# Process each collection
echo "$collections_json" | jq -c '.[]' | while read -r collection; do
    name=$(echo "$collection" | jq -r '.title')
    slug=$(echo "$collection" | jq -r '.slug')

    # Check if excluded
    excluded=false
    for pattern in "${EXCLUDED_PATTERNS[@]}"; do
        if [[ "$name" == *"$pattern"* ]]; then
            excluded=true
            break
        fi
    done

    if [ "$excluded" = true ]; then
        continue
    fi

    # Determine category and version
    category="other"
    version=""

    if [[ "$name" =~ Granite[[:space:]]([0-9]+\.[0-9]+) ]]; then
        category="language"
        version="${BASH_REMATCH[1]}"
    elif [[ "$name" =~ Granite[[:space:]](Vision|Speech|Embedding|Guardian|Time\ Series|Geospatial|Docling) ]]; then
        category="multimodal"
    elif [[ "$name" == "Granite Quantized Models" ]]; then
        category="quantized"
    fi

    # Output JSON object
    if [ "$first" = true ]; then
        first=false
    else
        echo ","
    fi

    jq -n \
        --arg name "$name" \
        --arg url "https://huggingface.co/collections/ibm-granite/${slug}" \
        --arg slug "$slug" \
        --arg category "$category" \
        --arg version "$version" \
        '{name: $name, url: $url, slug: $slug, category: $category, version: $version}'
done

echo "]"