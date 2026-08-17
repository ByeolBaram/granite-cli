#!/usr/bin/env bash
set -euo pipefail

# Queries OpenRouter API for Granite models
# Input: models.json
# Output: Enriched models.json with openrouter_info
#
# Strategy: Fetch all OpenRouter models, filter for hugging_face_id starting
# with "ibm-granite/", then match by that field against the repo in models.json.

MODELS_FILE="${1:-data/models.json}"

if [ ! -f "$MODELS_FILE" ]; then
    echo "Error: Models file not found: $MODELS_FILE" >&2
    exit 1
fi

if ! command -v python3 &> /dev/null; then
    echo "Error: python3 is required for this script." >&2
    exit 1
fi

# Fetch all models from OpenRouter (no auth required), filter for ibm-granite
# hugging_face_ids, then merge with the input models.json
python3 - "$MODELS_FILE" <<'PYEOF'
import sys, json, urllib.request, urllib.error

# Fetch full OpenRouter catalog
try:
    with urllib.request.urlopen(
        'https://openrouter.ai/api/v1/models',
        timeout=60
    ) as resp:
        data = json.loads(resp.read().decode())
        # Build lookup keyed by hugging_face_id for ibm-granite models
        or_lookup = {}
        for m in data.get('data', []):
            hf_id = m.get('hugging_face_id')
            if hf_id and str(hf_id).startswith('ibm-granite/'):
                or_lookup[hf_id] = {
                    'id': m.get('id'),
                    'name': m.get('name'),
                    'context_length': m.get('context_length'),
                    'pricing': m.get('pricing'),
                }
except urllib.error.URLError as e:
    sys.stderr.write(f'Warning: failed to fetch OpenRouter catalog: {e.reason}\n')
    or_lookup = {}
except Exception as e:
    sys.stderr.write(f'Warning: unexpected error fetching OpenRouter catalog: {e}\n')
    or_lookup = {}

# Read input models
with open(sys.argv[1], 'r') as f:
    models = json.load(f)

for model in models:
    repo = model.get('repo', '')
    if repo in or_lookup:
        or_info = or_lookup[repo]
        model['openrouter_info'] = [{
            'id': or_info['id'],
            'name': or_info['name'],
            'context_length': or_info['context_length'],
            'url': f"https://openrouter.ai/{or_info['id']}",
        }]
    else:
        model['openrouter_info'] = []

print(json.dumps(models, indent=2))
PYEOF
