#!/usr/bin/env bash
set -euo pipefail

DOCS_DIR="docs/specs"
MAIN_BRANCH="main"
TMPDIR_PREFIX=$(mktemp -d)
trap 'rm -rf "$TMPDIR_PREFIX"' EXIT

# Collect all spec files in the directory
shopt -s nullglob
all_files=("$DOCS_DIR"/*-*.md)
shopt -u nullglob

if [ ${#all_files[@]} -eq 0 ]; then
    echo "No spec files found in $DOCS_DIR"
    exit 0
fi

# Extract numeric prefix from filename
get_prefix() {
    local filename
    filename=$(basename "$1")
    echo "$filename" | sed -n 's/^\([0-9]*\)-.*/\1/p'
}

# Get the base name without the leading number
get_suffix() {
    local filename
    filename=$(basename "$1")
    echo "$filename" | sed 's/^[0-9]*-//'
}

# Get the number of leading zeros in the prefix to preserve formatting
get_prefix_len() {
    local filename
    filename=$(basename "$1")
    local prefix
    prefix=$(echo "$filename" | sed -n 's/^\([0-9]*\)-.*/\1/p')
    echo "${#prefix}"
}

# Get files currently on main branch
git ls-tree -r "$MAIN_BRANCH" --name-only -- "$DOCS_DIR/" 2>/dev/null > "$TMPDIR_PREFIX/main_files.txt" || true

# Get files currently tracked on HEAD
git ls-tree -r HEAD --name-only -- "$DOCS_DIR/" 2>/dev/null > "$TMPDIR_PREFIX/head_files.txt" || true

# Extract unique prefixes and sort them numerically
for f in "${all_files[@]}"; do
    get_prefix "$f"
done | sort -n -u > "$TMPDIR_PREFIX/prefixes.txt"

echo "=== Respecifying spec document numbering ==="
echo ""

next_num=0

while IFS= read -r prefix; do
    # Get all files with this prefix
    grep_files=()
    for f in "${all_files[@]}"; do
        if [[ "$(get_prefix "$f")" == "$prefix" ]]; then
            grep_files+=("$f")
        fi
    done

    file_count=${#grep_files[@]}
    prefix_len=$(get_prefix_len "${grep_files[0]}")

    echo "Prefix $prefix: $file_count file(s)"

    # Separate files into categories
    main_files=()
    branch_files=()
    uncommitted_files=()

    for f in "${grep_files[@]}"; do
        if grep -qxF "$f" "$TMPDIR_PREFIX/main_files.txt" 2>/dev/null; then
            main_files+=("$f")
        elif grep -qxF "$f" "$TMPDIR_PREFIX/head_files.txt" 2>/dev/null; then
            branch_files+=("$f")
        else
            uncommitted_files+=("$f")
        fi
    done

    # Build sorted order in a temp file
    > "$TMPDIR_PREFIX/final_order.txt"

    # Sort main files by first appearance on main
    if [ ${#main_files[@]} -gt 0 ]; then
        for f in "${main_files[@]}"; do
            first_date=$(git log --all --diff-filter=A --format="%ai" -- "$f" | tail -1)
            echo "${first_date}|${f}"
        done | sort | cut -d'|' -f2 >> "$TMPDIR_PREFIX/final_order.txt"
    fi

    # Sort branch files by first appearance on current branch (excluding main)
    if [ ${#branch_files[@]} -gt 0 ]; then
        for f in "${branch_files[@]}"; do
            first_date=$(git log "$MAIN_BRANCH"..HEAD --diff-filter=A --format="%ai" -- "$f" | tail -1)
            if [[ -z "$first_date" ]]; then
                first_date="1970-01-01 00:00:00"
            fi
            echo "${first_date}|${f}"
        done | sort | cut -d'|' -f2 >> "$TMPDIR_PREFIX/final_order.txt"
    fi

    # Sort uncommitted files by modification time (oldest first)
    if [ ${#uncommitted_files[@]} -gt 0 ]; then
        for f in "${uncommitted_files[@]}"; do
            mtime=$(stat -f "%Sm" -t "%Y-%m-%d %H:%M:%S" "$f" 2>/dev/null || stat -c "%y" "$f" 2>/dev/null)
            echo "${mtime}|${f}"
        done | sort | cut -d'|' -f2 >> "$TMPDIR_PREFIX/final_order.txt"
    fi

    # Rename files in sorted order
    while IFS= read -r f; do
        suffix=$(get_suffix "$f")
        new_name=$(printf "%0${prefix_len}s" "$next_num" | tr ' ' '0')-"$suffix"
        new_path="$DOCS_DIR/$new_name"

        if [[ "$f" != "$new_path" ]]; then
            echo "  $f -> $new_path"
            mv "$f" "$new_path"
        else
            echo "  $f (unchanged)"
        fi

        ((next_num++)) || true
    done < "$TMPDIR_PREFIX/final_order.txt"

    echo ""
done < "$TMPDIR_PREFIX/prefixes.txt"

echo "=== Done ==="
