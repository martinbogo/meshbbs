# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Changed
- Default `storage.max_message_size` reduced to 230 bytes (Meshtastic practical payload limit).
### Added
- Hard enforcement: messages larger than 230 bytes are rejected (config values above 230 are clamped).

## [0.8.11] - 2025-09-22
### Added
- Unread message notification at login: shows "<n> new messages since your last login" when applicable.
- Role-aware HELP output: HELP now tailors command list for guests, users, moderators, and sysop (moderation/admin commands hidden unless authorized).

### Changed
- Login banner construction centralized (internal refactor) to ensure consistent truncation and future extensibility.
- Passwordless legacy accounts must now set a password on first LOGIN attempt (guidance message provided); enforcement clarified.

### Removed
- Obsolete "(private)" tag in initial Direct Message welcome (reduced clutter).

### Fixed
- HELP output no longer exposes moderator/sysop commands to lower roles.
- Banner duplication reduced preventing subtle divergences during future changes.

### Migration Notes
- No schema changes. Existing user records remain compatible. Legacy passwordless users will be prompted to set a password upon next login attempt.


## [0.8.10] - 2025-09-22
### Added
- `bbs.session_timeout` configuration with idle session pruning (auto logout after inactivity).
- `bbs.max_users` enforcement blocking additional logins when capacity reached.
- `bbs.welcome_message` now appended with `bbs.description` on successful login (combined banner clamped to 230 bytes).
- Tests for session limits and welcome banner.

### Changed
- Login banner now includes BBS description line.
- Removed deprecated web configuration section from `config.toml` and default feature list.
- Modernized chrono usage in tests (replaced deprecated `timestamp_nanos`).
- Reduced unnecessary imports and cleaned warnings.

### Removed
- `WebConfig` struct and `[web]` section (feature deferred); `web` feature no longer enabled by default.

### Fixed
- Auto-login path now respects `max_users` capacity.
- Minor borrow checker and initialization issues from prior refactor.

### Migration Notes
- Remove any `[web]` section from existing `config.toml` (ignored if present).
- Add new fields to your `[bbs]` section if upgrading: `max_users`, `session_timeout`, `welcome_message`, `description` (if not already present). Existing installs should regenerate or manually merge.

## [0.8.0] - 2025-09-22
- Security: Added configurable Argon2 password hashing parameters (memory_kib, time_cost, parallelism) under [security.argon2] in config.
- Security: Introduced dedicated security log target duplication to optional `logging.security_file` with PROMOTE/DEMOTE and password related events.
- Security: Enforced immutability of sysop access level (cannot demote or change via storage API).
- Moderation: Added DELETE <area> <id> command for moderators (level >=5) to remove messages.
- Moderation: Added LOCK <area> / UNLOCK <area> commands to control posting in specific areas.
- Moderation: Added in-memory locked area tracking and storage enforcement preventing posts to locked areas.
- Moderation: HELP text updated to advertise new moderator commands.
- Moderation: Locked areas now persist across restarts via `locked_areas.json`.
- Moderation: Deletion audit log with pagination (`DELLOG [page]`) for moderators (level >=5).
- Access Control: Enforced per-area read/post levels from configuration (areas hidden if read_level unmet; posting denied if post_level unmet).
- Access Control: Locked area state overrides post permissions (cannot post even if post_level satisfied).

### Added
- Initial project structure and skeleton
- Basic Rust project with Cargo.toml configuration
- Command line interface with clap
- Modular architecture with separate modules for BBS, Meshtastic, storage, and config
- Comprehensive README.md with project overview and usage instructions
- This CHANGELOG.md file
- Meshtastic length-prefixed (0x94 0xC3 + u16 length) serial framing support (in addition to legacy SLIP fallback)
- Stateful sync logic: stable want_config_id generation, periodic resend, heartbeat integration
- Protobuf parsing for MyInfo, Config, ModuleConfig, NodeInfo, and ConfigCompleteId with internal state flags
- Enhanced smoke test JSON summary (node count, config flags, binary detection, request id)
- Automatic detection and resynchronization of misaligned or partial binary frames with recovery heuristics
- Diagnostics and warnings when only ASCII/ANSI log output is seen (helps distinguish framing vs absence of data)
- Public broadcast discovery model with minimal commands (caret‑prefixed `^HELP`, `^LOGIN <username>`) on the shared channel
- Pending login handshake recorded by node id and fulfilled upon first Direct Message (DM)
- Direct Message session creation gated on prior public `LOGIN` (creates authenticated BBS session)
- Rate limiting of public replies to reduce channel noise
- Structured `TextEvent` extraction layer decoupling frame parsing from session routing
- Unit tests for `PublicCommandParser` covering help, login, invalid login, and unknown inputs
- Inline Direct Message (DM) command set: `READ [area]`, `POST [area] <text>`, `AREAS`/`LIST` for quick interactions without menu traversal
- Integration test simulating public `LOGIN` then DM session with inline commands
- `proto-silence` feature flag to suppress unused warnings from generated Meshtastic protobuf surface
- Password management commands: `SETPASS <new>` (if no password set) and `CHPASS <old> <new>` to change existing password
- Sysop password management via out-of-band CLI subcommand `sysop-passwd` (argon2 hashed, stored in config, seeded at server startup)
- Moderator role (level 5) and role constants; Sysop retains level 10
- `PROMOTE <user>` / `DEMOTE <user>` commands (Sysop only) to manage moderator status
- Config section `[security.argon2]` (placeholders) for future tunable hashing parameters
- Structured security log target for role change events (logged with target `security`)

### Security
- Redacts sensitive password material from logs (no plaintext for REGISTER/LOGIN/SETPASS/CHPASS)
- Security-targeted logs for privilege changes (promotion/demotion) to aid audit trails

### Changed
- Nothing yet
- Default outbound Meshtastic requests now use length-prefixed framing on wired serial links
- Improved internal logging clarity and reduced noisy hex dumps unless in debug level
- Refactored async server loop to drain parsed text events prior to awaiting new IO to satisfy borrow checker
- Parser now classifies bare `login` without username as Invalid instead of Unknown
- Build script now reliably generates `meshtastic.rs` when upstream protos are available (avoids placeholder mismatch)
- Added re-export layer for `proto-silence` feature and replaced deprecated `PortNum::from_i32` with `TryFrom<i32>` usage
- Public channel commands now require a leading caret (`^`) to address the BBS (reduces ambient channel noise and accidental triggers)

### Deprecated
- Nothing yet

### Removed
- Nothing yet

### Fixed
- Nothing yet
- Resolved inability to parse protobuf frames due to incorrect assumption of SLIP framing on wired serial
- Missing `meshtastic.rs` generation caused by incorrect placeholder proto package name
- Borrow checker violations (E0499/E0500) in server loop due to simultaneous mutable borrows
- Channel handling bug in text message parsing (invalid `.map` on integer replaced with cast)

### Security
- Nothing yet

## [0.6.0] - 2025-09-22

### Added
- Password-based authentication with Argon2id hashing (argon2 + password-hash crates)
- `REGISTER <user> <pass>` direct message command to create new user accounts (node auto-bound)
- Enhanced `LOGIN <user> [pass]` semantics: password required only if user has a password hash and is logging in from a new (unbound) node
- Node binding persistence (first successful login/registration binds node id to user record)
- `LOGOUT` direct command to end the session (binding retained for future passwordless login from same node)
- Updated HELP and welcome messaging to guide new authentication flow

### Changed
- User schema: added `password_hash` (optional), `node_id` made optional; `access_level` internally renamed to `user_level` (serde alias preserves backward compatibility)
- Direct session bootstrap no longer depends on prior public-channel LOGIN; users can immediately REGISTER or LOGIN via DM
- HELP DM now references authentication commands

### Fixed
- Eliminated ambiguity where legacy LOGIN path accepted any username without credential checks (now gated by stored hash when present)

### Migration Notes
- Existing user JSON files (without `password_hash`) remain valid; those users can LOGIN without a password and become node-bound on first login
- To enforce passwords for existing users, delete their JSON file and have them re-REGISTER, or implement a future SETPASS command

### Security
- Introduces hashed credential storage using Argon2 default parameters; no plaintext passwords stored


## [0.1.0] - 2025-09-21

### Added
- Project initialization by Martin Bogomolni
- Basic project structure with Rust skeleton
- Core dependencies for async runtime, CLI, serialization, and logging
- Optional features for serial communication and web interface
- Development documentation and contribution guidelines
- Creative Commons Attribution-NonCommercial 4.0 International License

---

## Template for Future Releases

### [Version] - YYYY-MM-DD

### Added
- New features

### Changed
- Changes in existing functionality

### Deprecated
- Soon-to-be removed features

### Removed
- Now removed features

### Fixed
- Bug fixes

### Security
- Vulnerability fixes