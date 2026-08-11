#!/usr/bin/env bash
set -euo pipefail

# Queries LM Studio's model catalog for matching Granite models
# Input: models.json
# Output: Enriched models.json with lmstudio_info

MODELS_FILE="${1:-data/models.json}"

if [ ! -f "$MODELS_FILE" ]; then
    echo "Error: Models file not found: $MODELS_FILE" >&2
    exit 1
fi

if ! command -v jq &> /dev/null; then
    echo "Error: jq is required for this script." >&2
    exit 1
fi

if ! command -v python3 &> /dev/null; then
    echo "Error: python3 is required to parse LM Studio's embedded model.yaml manifests." >&2
    exit 1
fi

# LM Studio model pages render a model.yaml manifest (https://modelyaml.org) as
# syntax-highlighted HTML inside the page's React Server Component payload.
# There is no public catalog API, so we scrape that manifest for the fields we
# need instead of hitting a REST endpoint like the HuggingFace scripts do.
extract_manifest() {
    local page_file="$1"
    python3 - "$page_file" <<'PYEOF'
import json
import re
import sys

with open(sys.argv[1], encoding="utf-8", errors="ignore") as f:
    data = f.read()

size_gb = None

anchor = data.find("model.yaml is an open standard")
if anchor != -1:
    # The manifest is streamed as a React Server Component payload, so its
    # distance from the anchor varies (models with several base sources push
    # the relevant keys much further out) - use a generous window rather than
    # relying on a literal closing tag showing up nearby.
    chunk = data[anchor:anchor + 60000]
    text = chunk.encode("utf-8", "ignore").decode("unicode_escape", "ignore")
    text = re.sub(r"<[^>]+>", "", text)

    # Note: the manifest's `compatibilityTypes` field (e.g. "gguf") is the
    # packaging *format*, not a quantization/precision value, and LM Studio's
    # server-rendered page has no per-quant listing at all -- precision is
    # inferred separately in infer_precision() below.
    m = re.search(r"minMemoryUsageBytes:\s*([0-9]+)", text)
    if m:
        size_gb = round(int(m.group(1)) / 1e9, 3)

json.dump({"size_gb": size_gb}, sys.stdout)
PYEOF
}

# Infers the LM Studio variant's quantization precision by finding the
# closest-sized GGUF variant already known for this model (fetched from the
# HF GGUF sibling repo by 03-fetch-quantized.sh) to LM Studio's reported
# minMemoryUsageBytes. There's no real per-quant precision data to scrape.
infer_precision() {
    local model="$1" size_gb="$2"

    if [ "$size_gb" = "null" ] || [ -z "$size_gb" ]; then
        echo "null"
        return
    fi

    echo "$model" | jq --argjson target "$size_gb" '
        def dist: (.size_gb - $target) | if . < 0 then -. else . end;
        ([.variants[]? | select(.format == "GGUF")] | sort_by(dist) | .[0].precision) // null
    '
}

get_lmstudio_info() {
    local owner="$1" name="$2" model="$3"
    local url="https://lmstudio.ai/models/${owner}/${name}"

    local page_file manifest size_gb precision
    page_file=$(mktemp)
    curl -sL "$url" -o "$page_file"
    manifest=$(extract_manifest "$page_file")
    rm -f "$page_file"

    size_gb=$(echo "$manifest" | jq -r '.size_gb // "null"')
    precision=$(infer_precision "$model" "$size_gb")

    jq -n \
        --arg name "$name" \
        --arg url "$url" \
        --argjson manifest "$manifest" \
        --argjson precision "$precision" \
        '{name: $name, url: $url, size_gb: $manifest.size_gb, precision: $precision}'
}

map_to_lmstudio() {
    local repo="$1" version="$2"
    local name=$(basename "$repo")

    # LM Studio drops a trailing ".0" from the version, same as Ollama's tag
    # convention (e.g. HF "granite-4.0-h-micro" -> LM Studio "granite-4-h-micro").
    # Model types that don't embed the version in their name (guardian, vision,
    # embedding) pass through unchanged, which also matches LM Studio's naming.
    local short_version=$(echo "$version" | sed 's/\.0$//')
    echo "${name/$version/$short_version}"
}

# Process each model
jq -c '.[]' "$MODELS_FILE" | while read -r model; do
    repo=$(echo "$model" | jq -r '.repo')
    version=$(echo "$model" | jq -r '.version')

    candidate=$(map_to_lmstudio "$repo" "$version")

    echo "Checking LM Studio for: ibm/${candidate}" >&2

    http_code=$(curl -sL -o /dev/null -w "%{http_code}" "https://lmstudio.ai/models/ibm/${candidate}")
    if [ "$http_code" = "404" ]; then
        lmstudio_info="[]"
    else
        lmstudio_info="[$(get_lmstudio_info ibm "$candidate" "$model")]"
    fi

    echo "$model" | jq --argjson lmstudio "$lmstudio_info" '. + {lmstudio_info: $lmstudio}'
done | jq -s '.'
