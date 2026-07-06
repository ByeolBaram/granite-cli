#!/usr/bin/env bash
# JSON tool wrapper - uses jq if available, falls back to Python
# Provides common JSON operations with consistent interface

# Check which JSON tool is available
if command -v jq &> /dev/null; then
    JSON_TOOL="jq"
elif command -v python3 &> /dev/null; then
    JSON_TOOL="python3"
elif command -v python &> /dev/null; then
    JSON_TOOL="python"
else
    echo "Error: Neither jq nor Python is available. Please install one of them." >&2
    echo "  macOS: brew install jq" >&2
    echo "  Linux: apt-get install jq" >&2
    exit 1
fi

# Pretty print JSON
json_pretty() {
    if [ "$JSON_TOOL" = "jq" ]; then
        jq '.'
    else
        $JSON_TOOL -m json.tool
    fi
}

# Extract string value: json_get "key"
json_get() {
    local key="$1"
    if [ "$JSON_TOOL" = "jq" ]; then
        jq -r ".$key // empty"
    else
        $JSON_TOOL -c "import sys, json; data=json.load(sys.stdin); print(data.get('$key', ''))"
    fi
}

# Extract nested value: json_get_nested "key1.key2"
json_get_nested() {
    local path="$1"
    if [ "$JSON_TOOL" = "jq" ]; then
        jq -r ".$path // empty"
    else
        # Convert dot notation to Python dict access
        local py_path=$(echo "$path" | sed "s/\./']['/g")
        $JSON_TOOL -c "import sys, json; data=json.load(sys.stdin); print(data['$py_path'])" 2>/dev/null || echo ""
    fi
}

# Extract array items: json_array_items "key"
json_array_items() {
    local key="$1"
    if [ "$JSON_TOOL" = "jq" ]; then
        jq -r ".${key}[]? // empty"
    else
        $JSON_TOOL -c "import sys, json; data=json.load(sys.stdin); [print(item) for item in data.get('$key', [])]"
    fi
}

# Filter array: json_filter "condition"
json_filter() {
    local condition="$1"
    if [ "$JSON_TOOL" = "jq" ]; then
        jq -c ".[] | select($condition)"
    else
        echo "Error: Python fallback for json_filter not implemented" >&2
        return 1
    fi
}

# Build JSON object from key-value pairs
json_build() {
    if [ "$JSON_TOOL" = "jq" ]; then
        jq -n "$@"
    else
        # Simple Python JSON builder
        $JSON_TOOL -c "import json; print(json.dumps({$@}))"
    fi
}

# Compact JSON (remove whitespace)
json_compact() {
    if [ "$JSON_TOOL" = "jq" ]; then
        jq -c '.'
    else
        $JSON_TOOL -c "import sys, json; print(json.dumps(json.load(sys.stdin), separators=(',', ':')))"
    fi
}

# Get array length
json_array_length() {
    if [ "$JSON_TOOL" = "jq" ]; then
        jq 'length'
    else
        $JSON_TOOL -c "import sys, json; print(len(json.load(sys.stdin)))"
    fi
}

# Export functions for use in other scripts
export -f json_pretty json_get json_get_nested json_array_items json_filter json_build json_compact json_array_length
export JSON_TOOL
