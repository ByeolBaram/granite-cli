#!/usr/bin/env bash
set -euo pipefail

# Fetches a model's chat template Jinja source from HuggingFace.
# Tries the standalone chat_template.jinja file first (newer models), then
# falls back to the embedded chat_template field in tokenizer_config.json
# (older models).
# Input: $1 = repo (e.g. ibm-granite/granite-4.0-h-small)
# Output: raw template text on stdout, or empty string if neither exists

repo="$1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

jinja_url="https://huggingface.co/${repo}/raw/main/chat_template.jinja"
jinja=$("${SCRIPT_DIR}/hf-curl.sh" "$jinja_url")
if [ "$jinja" != "Entry not found" ] && [ -n "$jinja" ]; then
    echo "$jinja"
    exit 0
fi

config_url="https://huggingface.co/${repo}/raw/main/tokenizer_config.json"
tokenizer_config=$("${SCRIPT_DIR}/hf-curl.sh" "$config_url")
if [ "$tokenizer_config" == "Entry not found" ]; then
    exit 0
fi

echo "$tokenizer_config" | jq -r '.chat_template as $ct | if ($ct | type) == "string" then $ct else "" end'
