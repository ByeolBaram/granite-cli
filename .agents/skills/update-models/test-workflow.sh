#!/usr/bin/env bash
set -euo pipefail

# Quick test of the update-models workflow with a single collection
# Usage: ./test-workflow.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="${SCRIPT_DIR}/data"
SCRIPTS_DIR="${SCRIPT_DIR}/scripts"

echo "Testing update-models workflow..."
echo "================================"

# Create test collection with just Granite 4.1
echo "Creating test collection..."
cat > "${DATA_DIR}/test-collections.json" <<EOF
[
{
  "name": "Granite 4.1",
  "url": "https://huggingface.co/collections/ibm-granite/granite-41-698cc9befe1d9ed2e1587777",
  "slug": "ibm-granite/granite-41-language-models-69d3b30986f23ed3d8597ff3",
  "category": "llm",
  "version": "4.1"
}
]
EOF

# Step 1: Fetch models (will get all models from collection)
echo ""
echo "Fetching model metadata..."
"${SCRIPTS_DIR}/02-fetch-all-models.sh" "${DATA_DIR}/test-collections.json" > "${DATA_DIR}/test-models.json"

# Count models manually
MODEL_COUNT=$(grep -c '"repo"' "${DATA_DIR}/test-models.json" || echo "0")
echo "  ✓ Fetched ${MODEL_COUNT} models"

# Step 2: Add variants
echo ""
echo "Adding quantized variants..."
"${SCRIPTS_DIR}/03-fetch-quantized.sh" "${DATA_DIR}/test-models.json" > "${DATA_DIR}/test-models-variants.json"
mv "${DATA_DIR}/test-models-variants.json" "${DATA_DIR}/test-models.json"
echo "  ✓ Variants added"

# Step 3: Query Ollama
echo ""
echo "Querying Ollama..."
"${SCRIPTS_DIR}/04-01-query-ollama.sh" "${DATA_DIR}/test-models.json" > "${DATA_DIR}/test-models-ollama.json"
mv "${DATA_DIR}/test-models-ollama.json" "${DATA_DIR}/test-models.json"
echo "  ✓ Ollama info added"

# Step 4: Query LM Studio
echo ""
echo "Querying LM Studio..."
"${SCRIPTS_DIR}/04-02-query-lmstudio.sh" "${DATA_DIR}/test-models.json" > "${DATA_DIR}/test-models-lmstudio.json"
mv "${DATA_DIR}/test-models-lmstudio.json" "${DATA_DIR}/test-models.json"
echo "  ✓ LM Studio info added"

# Step 5: Query OpenRouter
echo ""
echo "Querying OpenRouter..."
"${SCRIPTS_DIR}/04-03-query-openrouter.sh" "${DATA_DIR}/test-models.json" > "${DATA_DIR}/test-models-openrouter.json"
mv "${DATA_DIR}/test-models-openrouter.json" "${DATA_DIR}/test-models.json"
echo "  ✓ OpenRouter info added"

# Step 6: Generate YAML
echo ""
echo "Generating YAML..."
"${SCRIPTS_DIR}/05-generate-yaml.sh" "${DATA_DIR}/test-models.json" > "${DATA_DIR}/test-models.yaml"
echo "  ✓ YAML generated"

# Step 7: Validate
echo ""
echo "Validating YAML..."
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