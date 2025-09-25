---
title: Releasing Meshbbs
description: A printable checklist and step-by-step guide for cutting releases, including 1.0.0-beta preparation.
---

# Releasing Meshbbs

This page is a printer-friendly guide for creating new meshbbs releases and preparing for the 1.0.0-beta series.

Tip for printing: Use your browser’s Print function to save this page as a PDF.

## Preflight

- [ ] Ensure working tree is clean and tests pass
  - `cargo clean && cargo build && cargo test`
- [ ] Verify Meshtastic features compile with defaults (proto, serial, weather)
- [ ] Confirm CHANGELOG.md and README.md reflect new behavior
- [ ] Update version in Cargo.toml

## 1.0.0-beta Prep

- [ ] Bump version: `0.9.x` → `1.0.0-beta.1`
- [ ] Update README badge and examples if needed
- [ ] Review compact help for size and content (≤230 B including prompt)
- [ ] Re-run all tests; verify flaky tests pass locally
- [ ] Consider enabling CI (GitHub Actions)
  - [ ] Rust toolchain setup, `cargo test`
  - [ ] Optional: Matrix for features (default / minimal)

## Tag and Release

- [ ] Commit with message: `v1.0.0-beta.1: summary`
- [ ] Create tag: `git tag -a v1.0.0-beta.1 -m "v1.0.0-beta.1"`
- [ ] Push: `git push origin main --tags`
- [ ] Create GitHub Release
  - Title: `v1.0.0-beta.1`
  - Notes: Paste CHANGELOG section
  - Attach binary (optional): build `meshbbs` for macOS/Linux if desired

## Post-Release

- [ ] Update docs site if applicable (GitHub Pages)
- [ ] Open follow-ups for any items deferred from the release

## Quick commands

```bash
# Build & test
cargo build --release
cargo test

# Tag & push
git tag -a v1.0.0-beta.1 -m "v1.0.0-beta.1"
git push origin main --tags
```

---

Source on GitHub: See the root-level `RELEASING.md` for the canonical text.
