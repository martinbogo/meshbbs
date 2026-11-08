# Utility tasks for meshbbs contributors

default:
	@just --list

utf8-check limit=200 *paths:
	python3 scripts/check_utf8_budget.py --limit {{limit}} {{paths}}

qa-utf8:
	python3 scripts/check_utf8_budget.py --limit 200 docs/qa tests/test-data-int

# Verify meshbbs still builds and passes the wire-compat tests against an older Meshtastic
# protobuf revision (default: the revision pinned before the v2.8.2 bump). Run before and
# after bumping third_party/meshtastic-protobufs.
proto-compat rev="46b81e822af1b8e408f437092337f129dee693e6":
	#!/usr/bin/env bash
	set -euo pipefail
	sub=third_party/meshtastic-protobufs
	git -C "$sub" cat-file -e "{{rev}}^{commit}" 2>/dev/null || git -C "$sub" fetch -q origin "{{rev}}"
	dir="$(mktemp -d)"
	trap 'git -C "$sub" worktree remove --force "$dir"' EXIT
	git -C "$sub" worktree add -q --detach "$dir" "{{rev}}"
	MESHTASTIC_PROTO_DIR="$dir/meshtastic" cargo build --all-targets
	MESHTASTIC_PROTO_DIR="$dir/meshtastic" cargo test --test proto_compat
	cargo test --test proto_compat
