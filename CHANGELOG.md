# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

This file records notable changes for meshbbs. Starting with the 1.0.0 BETA baseline, new entries will be added above this section over time (e.g., 1.0.1, 1.0.2).

## [1.0.10-beta] - 2025-09-26

### Added
- Public command `^FORTUNE` (Fortune Cookies): returns random wisdom from 140 curated Unix fortune database entries including programming quotes, philosophy, literature, and clean humor. All entries under 200 characters for mesh-friendly transmission. Broadcast-only with 5-second per-node cooldown.
- Comprehensive unit test coverage for Fortune module (11 test functions covering database validation, functionality, thread safety, and content quality)
- Helper functions for Fortune module: `fortune_count()` and `max_fortune_length()` for diagnostics and testing
- Extensive rustdoc documentation for Fortune module with examples and thread safety notes
- Development guide for Fortune module at `docs/development/fortune-module.md` with architecture, maintenance, and troubleshooting information

## [1.0.9-beta] - 2025-09-26

### Added
- Public command `^8BALL` (Magic 8‑Ball): returns one of 20 classic responses (emoji‑prefixed). Broadcast‑only with a lightweight per‑node cooldown like `^SLOT`.

### Changed
- Docs: README badge bumped to 1.0.9‑beta; user docs updated to include Magic 8‑Ball.

### Fixed
- Slot machine docs/tests alignment: `^SLOT` clarified as broadcast‑only with a behavior test to prevent regressions.

## [1.0.8-beta] - 2025-09-26

### Changed
- Logging hygiene: demote noisy INFO logs to DEBUG in HELP flow, weather fetch/success, cache loads, serial open, reader/writer init, resend attempts, and per-message delivered ACK logs.

### Fixed
- Public `^SLOT` behavior corrected to be broadcast-only (no DM). `^SLOTSTATS` remains broadcast-first with DM fallback for reliability.

## [1.0.7] - 2025-09-26

Documentation alignment follow-up for the 1.0.6 release.

### Changed
- README and docs tweaks to clarify public broadcast ACK confirmation semantics and command examples. No functional code changes.

## [1.0.6] - 2025-09-26

Broadcast reliability telemetry and optional confirmation for public messages.

### Added
- Optional broadcast ACK request: broadcasts can set `want_ack`; if any node ACKs, we consider it a success ("at least one hop").
- Lightweight broadcast ACK tracking in the writer with a short TTL (no retries) to avoid ACK storms.
- Metrics: `broadcast_ack_confirmed` and `broadcast_ack_expired` counters for trend visibility.

### Changed
- Updated HELP/public broadcast paths to request ACKs for visibility (DMs remain reliable with retries/backoff).
- Documentation updates in `README.md` to describe broadcast confirmation semantics and new metrics.

### Notes
- Direct messages remain fully reliable with ACK tracking, retries, and latency metrics; broadcasts remain best‑effort but can now indicate basic delivery when at least one ACK is observed.

## [1.0.5] - 2025-09-25

Added a public‑channel slot machine mini‑game and related documentation updates.

### Added
- New public commands:
	- `^SLOT` / `^SLOTMACHINE` — spin the slot machine (5 coins per spin; daily refill to 100 when at 0)
	- `^SLOTSTATS` — show your coin balance, total spins, wins, and jackpots
- Persistent per‑player state under `data/slotmachine/players.json` with safe file locking
- Jackpot and stats tracking (total_spins, total_wins, jackpots, last_spin, last_jackpot)
- Runtime packaging skeleton includes `data/slotmachine/.keep`

### Changed
- Updated user documentation (`docs/user-guide/commands.md`) and `README.md` to include new commands and feature blurb
- Bumped crate version to 1.0.5

## [1.0.0 BETA] - 2025-09-25

This is the first public beta for the 1.x series of meshbbs. It stabilizes the core architecture and user experience for Meshtastic devices while inviting broader testing and feedback before the final 1.0.0 release.

What’s in this beta (high level):
- Compact DM‑first UI optimized for Meshtastic frame limits (≤230 bytes), with Topics → Subtopics → Threads → Read navigation, paging, filtering, and breadcrumbs
- Robust help system: single‑frame compact HELP and multi‑chunk verbose HELP+
- Role‑based permissions: users, moderators, and sysop; moderation tools (pin/unpin, rename, lock/unlock, delete with audit log)
- Persistence: JSON‑backed users, topics/subtopics, messages, and replies; state survives restarts
- Meshtastic integration: protobuf support, hop‑limit 3, paced writer with scheduler to minimize airtime contention; optional weather integration
- Security and safety: Argon2id password hashing, command audit logging, UTF‑8 safe clamping/chunking, strict prompt‑aware size enforcement

Known limitations (to be refined before final 1.0):
- Some admin/dashboards are minimal or placeholders
- Performance tuning and CI coverage are ongoing
- Weather relies on an external service and may be rate‑limited

Upgrade and compatibility notes:
- On‑disk data persists in `data/` (topics at `data/topics.json`, messages under `data/messages/`)
- No schema migration is required from recent 0.9 builds; however, regenerating `config.toml` and reviewing new pacing/security options is recommended

Feedback welcome: please report issues and suggestions so we can tighten the final 1.0 release.