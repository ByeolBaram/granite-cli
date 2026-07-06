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
size=$(echo "$ID" | grep -oE '[0-9]+b' | head -1 | tr -d 'b')

# Generate description based on model type
case "$MODEL_TYPE" in
    Text)
        if [[ "$ID" == *"instruct"* ]]; then
            echo "[NEEDS REVIEW] ${FAMILY} ${VERSION} ${size}B instruct-tuned model for text generation."
        else
            echo "[NEEDS REVIEW] ${FAMILY} ${VERSION} ${size}B base model for text generation."
        fi
        ;;
    Vision)
        echo "[NEEDS REVIEW] ${FAMILY} ${VERSION} ${size}B for visual analysis and image understanding."
        ;;
    Speech)
        echo "[NEEDS REVIEW] ${FAMILY} ${VERSION} for audio transcription and translation."
        ;;
    Embedding)
        echo "[NEEDS REVIEW] ${FAMILY} ${VERSION} embedding model for semantic search and retrieval."
        ;;
    *)
        echo "[NEEDS REVIEW] ${FAMILY} ${VERSION} ${size}B model."
        ;;
esac
