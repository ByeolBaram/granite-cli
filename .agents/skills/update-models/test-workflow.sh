#!/usr/bin/env bash
set -euo pipefail

# Quick test of the update-models workflow with a single collection
# Usage: ./test-workflow.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="${SCRIPT_DIR}/data"
SCRIPTS_DIR="${SCRIPT_DIR}/scripts"

echo "Testing update-models workflow..."
echo "================================"

# Create test collection with just Granite 3.3
echo "Creating test collection..."
cat > "${DATA_DIR}/test-collections.json" <<EOF
[
{
  "name": "Granite Guardian",
  "url": "https://huggingface.co/collections/ibm-granite/ibm-granite/granite-guardian-66db06b1202a56cf7b079562",
  "slug": "ibm-granite/granite-guardian-66db06b1202a56cf7b079562",
  "category": "multimodal",
  "version": ""
}
]
EOF

# Step 1: Fetch models (will get all models from collection)
echo ""
echo "Step 1: Fetching model metadata..."
"${SCRIPTS_DIR}/02-fetch-all-models.sh" "${DATA_DIR}/test-collections.json" > "${DATA_DIR}/test-models.json"

# Count models manually
MODEL_COUNT=$(grep -c '"repo"' "${DATA_DIR}/test-models.json" || echo "0")
echo "  ✓ Fetched ${MODEL_COUNT} models"

# Step 2: Add variants
echo ""
echo "Step 2: Adding quantized variants..."
"${SCRIPTS_DIR}/03-fetch-quantized.sh" "${DATA_DIR}/test-models.json" > "${DATA_DIR}/test-models-variants.json"
mv "${DATA_DIR}/test-models-variants.json" "${DATA_DIR}/test-models.json"
echo "  ✓ Variants added"

# Step 3: Query Ollama
echo ""
echo "Step 3: Querying Ollama..."
"${SCRIPTS_DIR}/04-query-ollama.sh" "${DATA_DIR}/test-models.json" > "${DATA_DIR}/test-models-ollama.json"
mv "${DATA_DIR}/test-models-ollama.json" "${DATA_DIR}/test-models.json"
echo "  ✓ Ollama info added"

# Step 4: Generate YAML
echo ""
echo "Step 4: Generating YAML..."
"${SCRIPTS_DIR}/05-generate-yaml.sh" "${DATA_DIR}/test-models.json" > "${DATA_DIR}/test-models.yaml"
echo "  ✓ YAML generated"

# Step 5: Validate
echo ""
echo "Step 5: Validating YAML..."
"${SCRIPTS_DIR}/06-validate-yaml.sh" "${DATA_DIR}/test-models.yaml"

echo ""
echo "================================"
echo "Test complete! Generated files:"
echo "  - ${DATA_DIR}/test-models.json"
echo "  - ${DATA_DIR}/test-models.yaml"
echo ""
echo "Preview of generated YAML (first 50 lines):"
echo "================================"
head -50 "${DATA_DIR}/test-models.yaml"