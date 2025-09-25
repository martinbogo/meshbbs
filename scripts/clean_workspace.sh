#!/usr/bin/env bash
set -euo pipefail

# meshbbs cleanup script
# - Cleans Cargo build artifacts
# - Removes test-generated files under tests/test-data-int (users/messages), preserving tracked fixtures
# - Removes common logs and temp files
# - Does NOT touch config.toml or data/topics.json
#
# Usage:
#   bash scripts/clean_workspace.sh          # perform cleanup
#   bash scripts/clean_workspace.sh --dry    # dry-run (show what would be removed)
#   bash scripts/clean_workspace.sh --deep   # additionally run `git clean -fdx` (DANGEROUS)

DRY_RUN=false
DEEP_CLEAN=false
for arg in "$@"; do
  case "$arg" in
    --dry|--dry-run) DRY_RUN=true ;;
    --deep) DEEP_CLEAN=true ;;
  esac
done

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

if [[ ! -f Cargo.toml ]]; then
  echo "Error: This script must be run from the repo (found no Cargo.toml in $ROOT_DIR)" >&2
  exit 1
fi

run() {
  if $DRY_RUN; then echo "DRY: $*"; else eval "$@"; fi
}

echo "==> Cleaning Cargo build artifacts"
run "cargo clean"

echo "==> Removing target/ if present (after cargo clean)"
if [[ -d target ]]; then run "rm -rf target"; fi

echo "==> Removing logs and temp files"
for f in meshbbs.log meshbbs-security.log; do
  if [[ -f $f ]]; then run "rm -v '$f'"; fi
done
# Remove stray macOS files
run "find . -name .DS_Store -print -delete"

echo "==> Cleaning tmp/ directory (if present)"
if [[ -d tmp ]]; then run "rm -rf tmp/*"; fi

echo "==> Cleaning test-generated users (tests/test-data-int/users)"
USERS_DIR="tests/test-data-int/users"
if [[ -d "$USERS_DIR" ]]; then
  # Remove untracked JSON files and known ephemeral patterns
  while IFS= read -r -d '' file; do
    if git ls-files --error-unmatch "$file" >/dev/null 2>&1; then
      # Tracked file, keep
      continue
    fi
    # Extra guard: keep alice.json and carol.json if present even if untracked
    base="$(basename "$file")"
    if [[ "$base" == "alice.json" || "$base" == "carol.json" ]]; then
      continue
    fi
    run "rm -v '$file'"
  done < <(find "$USERS_DIR" -type f -name '*.json' -print0)
fi

echo "==> Cleaning test-generated messages (tests/test-data-int/messages)"
MSG_DIR="tests/test-data-int/messages"
if [[ -d "$MSG_DIR" ]]; then
  while IFS= read -r -d '' file; do
    if git ls-files --error-unmatch "$file" >/dev/null 2>&1; then
      # Tracked fixture, keep
      continue
    fi
    run "rm -v '$file'"
  done < <(find "$MSG_DIR" -type f -name '*.json' -print0)
fi

echo "==> Preserving config and topics (as requested)"
for p in config.toml data/topics.json; do
  if [[ -e "$p" ]]; then echo "Preserved: $p"; fi
done

if $DEEP_CLEAN; then
  echo "==> Deep clean: removing ALL untracked files (git clean -fdx)"
  echo "WARNING: This will remove every untracked file/dir, including local data under data/ if untracked."
  run "git clean -fdx"
fi

echo "==> Cleanup complete."
