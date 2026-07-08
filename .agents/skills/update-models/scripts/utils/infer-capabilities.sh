#!/usr/bin/env bash
set -euo pipefail

# Infers required provider capabilities from model metadata
# Input: model JSON (via stdin)
# Output: JSON array of capabilities

model=$(cat)

# Check for GGUF variants (Ollama support)
has_gguf=false
if echo "$model" | jq -e '.variants[] | select(.format == "GGUF")' >/dev/null 2>&1; then
    has_gguf=true
fi

# Check for safetensors/BF16 (OpenAI-compatible support)
has_safetensors=false
if echo "$model" | jq -e '.variants[] | select(.format == "safetensors")' >/dev/null 2>&1; then
    has_safetensors=true
fi

# Check model type
model_type=$(echo "$model" | jq -r '.model_type')

# Build capabilities array
capabilities=()

# Embedding models use OpenAIEmbeddings, not OpenAIChat
if [ "$model_type" = "Embedding" ]; then
    capabilities+=("OpenAIEmbeddings")
else
    if [ "$has_gguf" = true ]; then
        capabilities+=("OllamaChat")
    fi

    if [ "$has_safetensors" = true ]; then
        capabilities+=("OpenAIChat")
    fi

    # Vision models require OpenAI-compatible API
    if [ "$model_type" = "Vision" ]; then
        if [[ ! " ${capabilities[@]} " =~ " OpenAIChat " ]]; then
            capabilities+=("OpenAIChat")
        fi
    fi
fi

# Output as JSON array
printf '%s\n' "${capabilities[@]}" | jq -R . | jq -s .
