#!/usr/bin/env bash
set -euo pipefail

# Run from the repo root
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# ---------- helpers ----------

die() {
    git stash -- Cargo.toml Cargo.lock
    echo "error: $*" >&2; exit 1;
}

# Get the most recent git tag matching vX.Y.Z
latest_tag() {
    git tag --sort=-v:refname | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -n1 || echo ""
}

# ---------- argument parsing ----------

FORCE=0
CRATE_ARGS=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --force) FORCE=1; shift ;;
        *) CRATE_ARGS+=("$1"); shift ;;
    esac
done

# ---------- validate current state ----------

# Ensure working tree is clean
if ! git diff --quiet; then
    die "working tree is not clean — commit or stash changes first"
fi

# ---------- resolve version ----------

TAG="$(latest_tag)"
[[ "$TAG" != "" ]] || die "no vX.Y.Z git tags found"

# Strip leading 'v' to get the version string
VERSION="${TAG#v}"

if [[ "$FORCE" -eq 0 ]]; then
    # The tag must point to the current commit.
    COMMIT="$(git rev-parse HEAD)"
    TAG_COMMIT="$(git rev-list -n1 "$TAG" 2>/dev/null || true)"
    if [[ "$TAG_COMMIT" != "$COMMIT" ]]; then
        die "tag '$TAG' does not point to the current commit (HEAD=$COMMIT, tag=$TAG_COMMIT)" \
            "--force to publish anyway"
    fi
fi

echo "version: $VERSION  tag: $TAG"

# ---------- update Cargo.toml ----------

# Restore Cargo.toml on failure
trap 'git checkout HEAD -- Cargo.toml Cargo.lock"; echo "restored Cargo.toml"; exit' INT TERM EXIT

# Replace version = "0.0.0" (the sentinel value) with the real version
sed -i '' "s/^version = \"0\.0\.0\"/version = \"${VERSION}\"/" "Cargo.toml"

echo "updated Cargo.toml version to ${VERSION}"

# ---------- publish ----------

echo "publishing granite-cli ${VERSION}..."
cargo publish "${CRATE_ARGS[@]+"${CRATE_ARGS[@]}"}" --allow-dirty

# If publish succeeds, revert the local change
trap - INT TERM EXIT
git checkout HEAD -- Cargo.toml Cargo.lock

echo "published successfully"
