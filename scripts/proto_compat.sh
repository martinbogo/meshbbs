#!/usr/bin/env bash
# Build meshbbs and run the wire-compat tests against an older Meshtastic protobuf revision,
# then against the pinned submodule revision. Used by `just proto-compat` and CI.
#
# Usage: scripts/proto_compat.sh [rev]
#   rev  older protobufs commit to test against (default: the revision pinned before v2.8.2)
set -euo pipefail

rev="${1:-46b81e822af1b8e408f437092337f129dee693e6}"
sub=third_party/meshtastic-protobufs

cd "$(dirname "$0")/.."

# CI checkouts are shallow; fetch the old revision if it is not present.
if ! git -C "$sub" cat-file -e "${rev}^{commit}" 2>/dev/null; then
    git -C "$sub" fetch -q --depth 1 origin "$rev"
fi

dir="$(mktemp -d)"
trap 'git -C "$sub" worktree remove --force "$dir"' EXIT
git -C "$sub" worktree add -q --detach "$dir" "$rev"

echo "::group::Build against protobufs $rev"
MESHTASTIC_PROTO_DIR="$dir/meshtastic" cargo build --locked --all-targets
echo "::endgroup::"

echo "== proto_compat against $rev"
MESHTASTIC_PROTO_DIR="$dir/meshtastic" cargo test --locked --test proto_compat

echo "== proto_compat against pinned submodule revision"
cargo test --locked --test proto_compat
