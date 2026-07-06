#!/usr/bin/env bash
set -euo pipefail

# Queries Ollama registry for matching Granite models
# Input: models.json
# Output: Enriched models.json with ollama_info

MODELS_FILE="${1:-data/models.json}"

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

map_to_ollama() {
    local repo="$1"
    local version="$2"
    local family="$3"

    # Extract model name components
    local name=$(basename "$repo")

    # Map to Ollama naming convention
    local ollama_names=()

    if [[ "$name" == *"vision"* ]]; then
        ollama_names=("granite${version}-vision")
    elif [[ "$name" == *"guardian"* ]]; then
        ollama_names=("granite${version}-guardian")
    elif [[ "$name" == *"embedding"* ]]; then
        ollama_names=("granite-embedding")
    else
        # Standard language model
        # Drop .0 from version if present
        local short_version=$(echo "$version" | sed 's/\.0$//')
        ollama_names=(
            "granite${short_version}"
            "granite${short_version}-moe"
            "granite${short_version}-dense"
        )
    fi

    # First look in un-scoped (library)
    echo "["
    first=true
    for ollama_name in ${ollama_names[@]}; do
        # Check if model exists on Ollama
        http_code=$(curl -s -o /dev/null -w "%{http_code}" "https://ollama.com/library/${ollama_name}")

        if [ "$http_code" = "404" ]; then
            continue
        else

            if [ "$first" = true ]; then
                first=false
            else
                echo ","
            fi

            jq -n \
                --arg name "$ollama_name" \
                --arg url "https://ollama.com/library/${ollama_name}" \
                '{name: $name, url: $url, available: true}'
        fi
    done

    # If not found in library, look in ibm/
    if [ "$first" = true ]; then
        for ollama_name in ${ollama_names[@]}; do
            # Check if model exists on Ollama
            http_code=$(curl -s -o /dev/null -w "%{http_code}" "https://ollama.com/ibm/${ollama_name}")

            if [ "$http_code" = "404" ]; then
                continue
            else

                if [ "$first" = true ]; then
                    first=false
                else
                    echo ","
                fi

                jq -n \
                    --arg name "$ollama_name" \
                    --arg url "https://ollama.com/ibm/${ollama_name}" \
                    '{name: $name, url: $url, available: true}'
            fi
        done
    fi
    echo "]"
}

# Process each model
jq -c '.[]' "$MODELS_FILE" | while read -r model; do
    repo=$(echo "$model" | jq -r '.repo')
    version=$(echo "$model" | jq -r '.version')
    family=$(echo "$model" | jq -r '.family')

    echo "Checking Ollama for: $repo" >&2

    ollama_info=$(map_to_ollama "$repo" "$version" "$family")

    # Add ollama_info to model
    echo "$model" | jq --argjson ollama "$ollama_info" '. + {ollama_info: $ollama}'
done | jq -s '.'
