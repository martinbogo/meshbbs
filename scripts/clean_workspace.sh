#!/usr/bin/env bash
set -euo pipefail

# meshbbs cleanup script
# - Cleans Cargo build artifacts
# - Removes test-generated files under tests/test-data-int (users/messages), preserving tracked fixtures
# - Removes common logs and temp files
# - Cleans runtime data: removes created users (data/users), messages (data/messages)
# - In data/, keeps topics.example.json and removes other .json files (e.g., topics.json, node_cache.json)
# - Removes generated logfiles such as meshbbs.log and data/admin_audit.log
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
for f in meshbbs.log meshbbs-security.log data/admin_audit.log; do
  if [[ -f $f ]]; then run "rm -v '$f'"; fi
done
# Remove stray macOS and editor temp files
run "find . -name .DS_Store -print -delete"
run "find . -type f \( -name '*.swp' -o -name '*.swo' -o -name '*.tmp' -o -name '*.temp' \) -print -delete"

echo "==> Cleaning tmp/ directory (if present)"
if [[ -d tmp ]]; then run "rm -rf tmp/*"; fi

echo "==> Cleaning runtime data under data/"
if [[ -d data ]]; then
  # Remove created users and messages
  if [[ -d data/users ]]; then run "rm -rf data/users"; fi
  if [[ -d data/messages ]]; then run "rm -rf data/messages"; fi

  # Remove all JSON files in data/ except topics.example.json
  while IFS= read -r -d '' file; do
    base="$(basename "$file")"
    if [[ "$base" == "topics.example.json" ]]; then
      echo "Preserved: $file"
      continue
    fi
    run "rm -v '$file'"
  done < <(find data -maxdepth 1 -type f -name '*.json' -print0)

  # Ensure empty directories exist after clean
  run "mkdir -p data/users"
  run "mkdir -p data/messages"
fi

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

echo "==> Preserving config example file (if present)"
if [[ -e config.example.toml ]]; then echo "Preserved: config.example.toml"; fi

if $DEEP_CLEAN; then
  echo "==> Deep clean: removing ALL untracked files (git clean -fdx)"
  echo "WARNING: This will remove every untracked file/dir, including local data under data/ if untracked."
  run "git clean -fdx"
fi

echo "==> Cleanup complete."
