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

get_ollama_info() {
    ollama_name=$1
    ollama_tag=$2

    # Fetch the html for this listing and parse out size and precision
    model_page=$(curl -s "https://ollama.com/ibm/${ollama_name}:$ollama_tag")
    size_gb=$(echo "$model_page" | grep -oE "[0-9]+\.?[0-9]*GB" | head -1 | sed 's/GB//i' || echo "")
    size_mb=$(echo "$model_page" | grep -oE "[0-9]+\.?[0-9]*MB" | head -1 | sed 's/MB//i' || echo "")

    # Convert MB to GB if no GB size found
    if [ -z "$size_gb" ] && [ -n "$size_mb" ]; then
        size_gb=$(awk "BEGIN {printf \"%.3f\", $size_mb / 1024}")
    fi

    precision=$(echo "$model_page" | grep "quantization" | head -1 | sed 's/<[^>]*>//g' | sed 's/.*quantization//i' | tr -d '\n' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

    jq -n \
        --arg name "$ollama_name" \
        --arg url "https://ollama.com/ibm/${ollama_name}:$ollama_tag" \
        --arg size_gb "${size_gb:-null}" \
        --arg precision "${precision:-null}" \
        '{name: $name, url: $url, size_gb: (if $size_gb == "null" then null else ($size_gb | tonumber) end), precision: (if $precision == "null" then null else $precision end)}'
}

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

    # Map to tag conventions
    local ollama_tag="latest"
    if [[ "$name" =~ .*-h-([0-9]+[bm]).* ]]; then
        ollama_tag="${BASH_REMATCH[1]}-h"
    elif [[ "$name" =~ .*-([0-9]+[bm]).* ]]; then
        ollama_tag="${BASH_REMATCH[1]}"
    elif [[ "$name" =~ .*-h-([a-z]+) ]]; then
        ollama_tag="${BASH_REMATCH[1]}-h"
    elif [[ "$name" =~ .*[0-9]-([a-z]+) ]]; then
        ollama_tag="${BASH_REMATCH[1]}"
    fi

    # First look in un-scoped (library)
    echo "["
    first=true
    for ollama_name in ${ollama_names[@]}; do
        # Check if model exists on Ollama
        http_code=$(curl -s -o /dev/null -w "%{http_code}" "https://ollama.com/library/${ollama_name}:$ollama_tag")

        if [ "$http_code" = "404" ]; then
            continue
        else

            if [ "$first" = true ]; then
                first=false
            else
                echo ","
            fi

            get_ollama_info $ollama_name $ollama_tag
        fi
    done

    # If not found in library, look in ibm/
    if [ "$first" = true ]; then
        for ollama_name in ${ollama_names[@]}; do
            # Check if model exists on Ollama
            http_code=$(curl -s -o /dev/null -w "%{http_code}" "https://ollama.com/ibm/${ollama_name}:$ollama_tag")

            if [ "$http_code" = "404" ]; then
                continue
            else

                if [ "$first" = true ]; then
                    first=false
                else
                    echo ","
                fi

                get_ollama_info $ollama_name $ollama_tag
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
