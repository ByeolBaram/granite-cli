#!/usr/bin/env bash
set -euo pipefail

# Generates description template for a model
# Input: id family version model_type
# Output: description string

ID="$1"
FAMILY="$2"
VERSION="$3"
MODEL_TYPE="$4"

# Extract size from ID
size=$(echo "$ID" | grep -oE '[0-9]+[bB]' || echo "" | head -1)
if [ "$size" == "" ]; then
    size=$(echo "$ID" | grep -oE '[0-9]+[mM]' || echo "" | head -1)
fi

desc=""

# Generate description based on model type
case "$MODEL_TYPE" in
    Text)
        if [[ "$ID" == *"instruct"* ]]; then
            desc="${FAMILY} ${VERSION} ${size} instruct-tuned model for text generation."
        else
            desc="${FAMILY} ${VERSION} ${size} base model for text generation."
        fi
        ;;
    Vision)
        desc="${FAMILY} ${VERSION} ${size} for visual analysis and image understanding."
        ;;
    Speech)
        desc="${FAMILY} ${VERSION} for audio transcription and translation."
        ;;
    Embedding)
        desc="${FAMILY} ${VERSION} embedding model for semantic search and retrieval."
        ;;
    *)
        desc="${FAMILY} ${VERSION} ${size} model."
        ;;
esac

echo "$desc" | sed 's,  *, ,g'
