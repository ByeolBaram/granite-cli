#!/usr/bin/env bash
set -euo pipefail

# Merges new models with existing models.yaml
# Input: existing.yaml new.yaml
# Output: merged.yaml

EXISTING="${1:-resources/models.yaml}"
NEW="${2:-data/models-new.yaml}"

if [ ! -f "$EXISTING" ]; then
    echo "Error: Existing file not found: $EXISTING" >&2
    exit 1
fi

if [ ! -f "$NEW" ]; then
    echo "Error: New file not found: $NEW" >&2
    exit 1
fi

# Extract IDs from existing file
existing_ids=$(grep "^- id:" "$EXISTING" | sed 's/^- id: //' | sort)

# Process new file, keeping only new models
echo "# Merged models.yaml"
echo "# Generated: $(date)"
echo ""

# First, output existing models
cat "$EXISTING"

echo ""
echo "# New models added by update-models skill"
echo ""

# Then, output new models that don't exist
grep "^- id:" "$NEW" | sed 's/^- id: //' | sort | while read -r id; do
    if ! echo "$existing_ids" | grep -q "^${id}$"; then
        # Extract this model's entry from new file
        awk "/^- id: ${id}$/,/^- id:/" "$NEW" | sed '$d'
        echo ""
    fi
done
