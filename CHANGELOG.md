# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.90] - 2025-09-24
### Changed
- Configuration: `message_topics` in `config.toml` is now optional. If omitted, MeshBBS starts with no pre-defined topics and expects runtime topic management. Any TOML-defined topics (if present) are merged into the runtime store at startup for backward compatibility.
- Documentation: README updated to reflect runtime topics persistence in `data/topics.json` and added writer tuning options in `[meshtastic]`.

### Added
- Meshtastic writer tuning (with enforced fairness):
  - Global minimum send gate (`min_send_gap_ms`), hard-clamped to ≥2000 ms.
  - Reliable DM retransmit backoff schedule (`dm_resend_backoff_seconds`, default [4,8,16]).
  - Pacing delays: `post_dm_broadcast_gap_ms` and `dm_to_dm_gap_ms`.
- Runtime topic management persisted to `data/topics.json` with interactive create/modify/delete.

### Fixed
- Startup failure when `message_topics` is missing from `config.toml`.

## [0.9.70] - 2025-09-23
### Changed
- **Major Architecture Refactor**: Implemented reader/writer pattern to resolve asynchronous I/O deadlock
  - Split Meshtastic device communication into separate reader and writer tasks
  - Reader task continuously reads from device, sends TextEvents to main server via channels
  - Writer task handles all outgoing messages via channels, preventing blocking operations
  - Eliminated deadlock where app would send messages but not process device responses/ACKs until another unrelated message arrived
  - Improved message delivery reliability and response times
  - Enhanced error handling with independent task failure management
  - Better separation of concerns between I/O operations and business logic

### Added
- **New Channel Architecture**: Clean communication between tasks
  - TextEvent channel for incoming messages (Reader → Server)
  - OutgoingMessage channel for outgoing messages (Server → Writer)
  - Control channels for task coordination and shutdown
  - Message priority system (High for DMs, Normal for broadcasts)

### Technical
- Complete separation of reading and writing operations prevents I/O blocking
- Async task spawning for concurrent reader/writer operation
- Improved shutdown coordination with proper task termination
- Enhanced logging for task lifecycle management

## [0.9.60] - 2025-09-23
### Added
- **Enhanced Weather Debug Logging**: Weather queries now include full URL in debug output
  - Main weather fetch log now shows complete wttr.in URL being used
  - Error messages include the specific URL that failed
  - Timeout scenarios now log the URL that timed out
  - Changed from trace! to debug! level for better visibility

### Fixed  
- **DM Delivery Improvements**: Fixed help command DM regression and routing issues
  - Enhanced logging for DM operations with detailed from/to/channel information
  - Fixed hop_limit from 0 to 3 hops for proper mesh routing
  - Node ID mismatch discovery and resolution (0x132BEE vs 0x0a132bee format)
- **Persistent Node Cache System**: Complete node management overhaul
  - JSON-based persistent storage with timestamps for each node
  - Automatic loading on device connection and periodic cleanup of stale nodes (7+ days)
  - Thread-safe operation with proper error handling
- **Integration Test Fixes**: Updated all tests for gated topic creation system
  - Fixed config usage by removing obsolete fields (message_retention_days, max_messages_per_area)
  - Added proper topic creation in tests that use POST commands
  - All 56+ tests now passing successfully

## [0.9.55] - 2025-09-23
### Changed
- **Complete AREA → TOPIC terminology refactor**: Comprehensive update across the entire codebase
  - Updated all user-facing text from "areas" to "topics" throughout help system and commands
  - Renamed functions: `moderator_lock_area` → `moderator_lock_topic`, `self_area_can_read` → `self_topic_can_read`, etc.
  - Updated parameter names and variable names from "area" to "topic" throughout the codebase
  - Updated README.md documentation with new terminology (command examples, configuration sections)
  - Updated code comments and documentation strings to use "topic" terminology
  - Updated command syntax examples: `READ <area>` → `READ <topic>`, `POST <area>` → `POST <topic>`
  - Configuration section renamed: `[message_areas.*]` → `[message_topics.*]`
- **Default Topics Enhancement**: All three default topics now properly available in working directory
  - `general/` - General discussions
  - `community/` - Events, meet-ups, and community discussions  
  - `technical/` - Tech, hardware, and administrative discussions

### Fixed
- **Documentation Examples**: Fixed all rustdoc examples to use correct API methods
  - Fixed `Config::load()` method calls instead of non-existent `from_file()`
  - Fixed `Storage::new()` and `get_messages()` method signatures
  - Fixed `MeshtasticDevice` examples to use correct return types
  - Added proper feature guards for conditional compilation examples
  - All 9 documentation examples now compile and pass doctest

### Technical
- Maintained backward compatibility for stored message data format
- All 57 tests continue to pass after refactor
- Build system remains stable with no breaking changes

## [0.9.18] - 2025-09-23
### Added
- **New User Welcome System**: Enhanced onboarding experience for new users
  - Registration welcome message with comprehensive quick start guide including key commands (HELP, LIST, POST, READ, WHO)
  - First login follow-up message with additional tips and command suggestions (LIST, WHO, RECENT)
  - Persistent tracking ensures welcome messages are shown only once per user
- **Sysop Username Support**: Fixed validation to allow "sysop" as a valid username for sysop role while maintaining security for regular users
- **Enhanced Security Features**: 
  - Increased minimum password length from 6 to 8 characters
  - File locking protection for concurrent access using fs2 crate
  - Comprehensive admin audit logging for administrative actions (PROMOTE, DEMOTE, KICK, BROADCAST)
  - New ADMINLOG command for sysops to view administrative action history
- **Reserved Username Documentation**: Complete list of 45 blocked system usernames for reference

### Changed
- Username validation system enhanced with role-aware reserved name checking
- User data structure extended with welcome message tracking fields
- Storage layer improved with secure file operations and audit trail support

### Security
- Password minimum length increased to 8 characters for better security
- File operations now use exclusive locking to prevent race conditions
- Admin actions now generate persistent audit logs for accountability
- Reserved system usernames properly enforced with sysop role exceptions

### Fixed
- Sysop can now use "sysop" as username (previously blocked by reserved name validation)
- Concurrent file access issues resolved through proper locking mechanisms
- User creation properly initializes new welcome tracking fields

## [0.9.0] - 2025-09-22
### Added
- **Dynamic contextual prompts**: Prompts now show current state and context instead of static '>'.
  - Unauthenticated: `unauth>`
  - Logged in: `<user> (lvl<level>)>`
  - Reading messages: `<user>@<area>>`
  - Posting messages: `post@<area>>`
- **Verbose help system**: `HELP+` or `HELP V` provides detailed multi-part help with all commands and examples.
- **One-time shortcuts reminder**: First `HELP` after login shows "Shortcuts: M=areas U=user Q=quit".
- **Proactive weather updates**: Weather now fetches automatically every 5 minutes (reduced from 15-minute on-demand cache).

### Changed
- Weather cache TTL reduced from 15 minutes to 5 minutes for fresher data.
- Unknown command response updated to "Unknown command. Type HELP" for better guidance.
- Login output streamlined to show only welcome message and unread count (removed redundant prompts).
- Centralized message sending with automatic prompt appending (internal architecture improvement).

### Removed
- Static '>' prompt characters embedded in message responses.
- Legacy banner text on first authenticated direct message.

### Fixed
- HELP command reliability improved with size-aware chunking (prevents delivery failures).
- Prompt consistency across all session states and command flows.

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