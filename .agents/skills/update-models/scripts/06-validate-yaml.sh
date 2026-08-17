#!/usr/bin/env bash
set -euo pipefail

# Validates generated YAML file
# Input: models.yaml
# Output: Validation report

YAML_FILE="${1:-data/models-new.yaml}"

if [ ! -f "$YAML_FILE" ]; then
    echo "Error: YAML file not found: $YAML_FILE" >&2
    exit 1
fi

errors=0
warnings=0

echo "Validating: $YAML_FILE"
echo "========================"

# Check for duplicate IDs
echo "Checking for duplicate IDs..."
duplicates=$(grep "^- id:" "$YAML_FILE" | sort | uniq -d)
if [ -n "$duplicates" ]; then
    echo "ERROR: Duplicate IDs found:"
    echo "$duplicates"
    ((errors++))
else
    echo "  ✓ No duplicate IDs"
fi

# Check for required fields
echo "Checking required fields..."
required_fields=("id" "family" "version" "size" "context_length" "model_type" "huggingface_repo" "native_dtype" "architecture")

model_count=$(grep -c "^- id:" "$YAML_FILE" || echo "0")

field_errors=0
for field in "${required_fields[@]}"; do
    # Count occurrences of each field (with proper indentation)
    field_count=$(grep -c "^[- ] ${field}:" "$YAML_FILE" || echo "0")

    if [ "$model_count" -ne "$field_count" ]; then
        echo "ERROR: Field '${field}' count mismatch (expected: ${model_count}, found: ${field_count})"
        ((field_errors++))
    fi
done

if [ $field_errors -eq 0 ]; then
    echo "  ✓ All required fields present"
else
    ((errors++))
fi

# Check architecture blocks (layer_types counts sum correctly, shape fields present)
echo "Checking architecture blocks..."
if command -v python3 &> /dev/null; then
    arch_check_output=$(python3 - "$YAML_FILE" <<'PYEOF'
import sys
import yaml

with open(sys.argv[1]) as f:
    models = yaml.safe_load(f)

for m in models:
    mid = m.get("id", "<unknown>")
    if not m.get("native_dtype"):
        print(f"{mid}: missing native_dtype")

    arch = m.get("architecture")
    if not arch:
        print(f"{mid}: missing architecture block")
        continue

    layer_types = arch.get("layer_types") or []
    if not layer_types:
        print(f"{mid}: architecture.layer_types is empty")

    total = sum(lt.get("count", 0) for lt in layer_types)
    expected = arch.get("num_hidden_layers", 0)
    if total != expected:
        print(f"{mid}: layer_types counts sum to {total}, expected num_hidden_layers={expected}")

    for lt in layer_types:
        kind = lt.get("kind")
        if kind == "sliding_attention" and not lt.get("window"):
            print(f"{mid}: sliding_attention layer_type missing window")
        if kind == "recurrent" and not lt.get("mamba"):
            print(f"{mid}: recurrent layer_type missing mamba shape")
PYEOF
) || true

    if [ -n "$arch_check_output" ]; then
        echo "$arch_check_output" | sed 's/^/ERROR: /'
        ((errors++))
    else
        echo "  ✓ Architecture blocks valid"
    fi
else
    echo "WARNING: python3 not available, skipping deep architecture validation"
    ((warnings++))
fi

# Check for review flags
echo "Checking for review flags..."
review_count=$(grep -c "NEEDS REVIEW" "$YAML_FILE" || true)
suggest_count=$(grep -c "SUGGESTED" "$YAML_FILE" || true)

if [ $review_count -gt 0 ]; then
    echo "WARNING: $review_count entries need review"
    ((warnings++))
fi

if [ $suggest_count -gt 0 ]; then
    echo "WARNING: $suggest_count suggested changes"
    ((warnings++))
fi

# Summary
echo ""
echo "========================"
echo "Validation complete:"
echo "  Errors: $errors"
echo "  Warnings: $warnings"
echo "  Models: $model_count"

if [ $errors -gt 0 ]; then
    exit 1
fi

exit 0