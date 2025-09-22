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

### Changed
- Nothing yet
- Default outbound Meshtastic requests now use length-prefixed framing on wired serial links
- Improved internal logging clarity and reduced noisy hex dumps unless in debug level

### Deprecated
- Nothing yet

### Removed
- Nothing yet

### Fixed
- Nothing yet
- Resolved inability to parse protobuf frames due to incorrect assumption of SLIP framing on wired serial

### Security
- Nothing yet

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