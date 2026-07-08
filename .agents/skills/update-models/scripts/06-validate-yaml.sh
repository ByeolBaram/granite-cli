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
required_fields=("id" "family" "version" "size" "context_length" "model_type" "huggingface_repo")

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