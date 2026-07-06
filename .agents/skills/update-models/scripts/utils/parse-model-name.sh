#!/usr/bin/env bash
set -euo pipefail

# Parses model name to extract family, version, size
# Input: model name (via stdin or arg)
# Output: JSON object

NAME="${1:-$(cat)}"

# Extract version (e.g., 3.1, 4.0)
version=$(echo "$NAME" | grep -oE '[0-9]+\.[0-9]+' | head -1)

# Extract size (e.g., 3b, 8b, 30b)
size=$(echo "$NAME" | grep -oE '[0-9]+b' | head -1)

# Extract family
family="Granite"
if [[ "$NAME" == *"vision"* ]]; then
    family="Granite Vision"
elif [[ "$NAME" == *"speech"* ]]; then
    family="Granite Speech"
elif [[ "$NAME" == *"guardian"* ]]; then
    family="Granite Guardian"
elif [[ "$NAME" == *"embedding"* ]]; then
    family="Granite Embedding"
fi

jq -n \
    --arg family "$family" \
    --arg version "${version:-unknown}" \
    --arg size "${size:-unknown}" \
    '{family: $family, version: $version, size: $size}'
