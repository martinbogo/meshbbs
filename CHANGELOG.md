# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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