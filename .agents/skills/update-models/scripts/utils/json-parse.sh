#!/usr/bin/env bash
# JSON parsing utilities using only grep/sed/awk
# No jq dependency required

# Extract a string value from JSON
# Usage: json_get_string "$json" "key"
json_get_string() {
    local json="$1"
    local key="$2"
    echo "$json" | grep -o "\"$key\"[[:space:]]*:[[:space:]]*\"[^\"]*\"" | sed "s/\"$key\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\"/\1/"
}

# Extract a number value from JSON
# Usage: json_get_number "$json" "key"
json_get_number() {
    local json="$1"
    local key="$2"
    echo "$json" | grep -o "\"$key\"[[:space:]]*:[[:space:]]*[0-9][0-9]*" | sed "s/\"$key\"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\)/\1/"
}

# Extract array items (one per line)
# Usage: json_get_array_items "$json" "key"
json_get_array_items() {
    local json="$1"
    local key="$2"
    # Extract array content between [ and ]
    echo "$json" | sed -n "s/.*\"$key\"[[:space:]]*:[[:space:]]*\[\([^]]*\)\].*/\1/p" | tr ',' '\n' | sed 's/^[[:space:]]*"\(.*\)"[[:space:]]*$/\1/'
}

# Check if key exists
# Usage: json_has_key "$json" "key"
json_has_key() {
    local json="$1"
    local key="$2"
    echo "$json" | grep -q "\"$key\""
}

# Extract nested object
# Usage: json_get_object "$json" "key"
json_get_object() {
    local json="$1"
    local key="$2"
    # This is simplified - works for single-level nesting
    echo "$json" | sed -n "s/.*\"$key\"[[:space:]]*:[[:space:]]*{\([^}]*\)}.*/\1/p"
}

# Build JSON string field
# Usage: json_build_string "key" "value"
json_build_string() {
    local key="$1"
    local value="$2"
    echo "\"$key\": \"$value\""
}

# Build JSON number field
# Usage: json_build_number "key" "value"
json_build_number() {
    local key="$1"
    local value="$2"
    echo "\"$key\": $value"
}

# Build JSON array field
# Usage: json_build_array "key" "item1" "item2" ...
json_build_array() {
    local key="$1"
    shift
    local items=""
    local first=true
    for item in "$@"; do
        if [ "$first" = true ]; then
            first=false
            items="\"$item\""
        else
            items="$items, \"$item\""
        fi
    done
    echo "\"$key\": [$items]"
}
