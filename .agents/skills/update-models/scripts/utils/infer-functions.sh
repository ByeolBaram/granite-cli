#!/usr/bin/env bash
set -euo pipefail

# Infers supported model functions from model metadata
# Input: model JSON (via stdin)
# Output: JSON array of function names

model=$(cat)

# Check model type
model_type=$(echo "$model" | jq -r '.model_type')

# Build functions array
functions=()

# Chat is the base function for most models
if [ "$model_type" != "Embedding" ]; then
    functions+=("Chat")
fi

# Embedding models provide Embeddings function
if [ "$model_type" = "Embedding" ]; then
    functions+=("Embeddings")
fi

# Speech models provide Chat and Transcription
if [ "$model_type" = "Speech" ]; then
    functions+=("Transcription")
fi

# Vision models provide Chat and ImageUnderstanding
if [ "$model_type" = "Vision" ]; then
    functions+=("ImageUnderstanding")
fi

# Output as JSON array
printf '%s\n' "${functions[@]}" | jq -R . | jq -s .
