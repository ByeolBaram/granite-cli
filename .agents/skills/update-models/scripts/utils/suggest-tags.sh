#!/usr/bin/env bash
set -euo pipefail

# Suggests tags based on model characteristics
# Input: model JSON (via stdin)
# Output: JSON array of tags

model=$(cat)

id=$(echo "$model" | jq -r '.repo' | xargs basename)
model_type=$(echo "$model" | jq -r '.model_type')
size=$(echo "$model" | jq -r '.size')

tags=()

# Add type-based tags
case "$model_type" in
    Text)
        tags+=("text")
        if [[ "$id" == *"instruct"* ]]; then
            tags+=("instruct")
        fi
        if [[ "$id" == *"code"* ]]; then
            tags+=("code")
        fi
        ;;
    Vision)
        tags+=("vision" "image" "multimodal")
        ;;
    Speech)
        tags+=("speech" "audio" "transcription")
        ;;
    Embedding)
        tags+=("embedding" "retrieval")
        ;;
esac

# Add size-based tags
if [ "$size" -lt 5000000000 ]; then
    tags+=("efficient")
elif [ "$size" -gt 15000000000 ]; then
    tags+=("reasoning")
else
    tags+=("general-purpose")
fi

# Add special tags
if [[ "$id" == *"guardian"* ]]; then
    tags+=("guardian" "safety" "moderation")
fi

if [[ "$id" == *"moe"* ]]; then
    tags+=("mixture-of-experts")
fi

# Output as JSON array
printf '%s\n' "${tags[@]}" | jq -R . | jq -s . | jq 'unique'