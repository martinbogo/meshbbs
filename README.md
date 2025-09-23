# MeshBBS

A modern Bulletin Board System (BBS) designed for Meshtastic mesh networks, written in Rust.

## Overview

MeshBBS brings the classic BBS experience to modern mesh networks, allowing users to exchange messages and communicate over long-range, low-power radio networks using Meshtastic devices. This project aims to create resilient communication systems that work without traditional internet infrastructure.

## Features

- 📡 **Meshtastic Integration**: Direct communication with Meshtastic devices via serial or Bluetooth
- 💬 **Message Boards**: Traditional BBS-style message areas and forums
- 🎯 **Dynamic Contextual Prompts**: Smart prompts showing current state (`unauth>`, `user@area>`, `post@area>`)
- 📚 **Enhanced Help System**: Compact `HELP` + verbose `HELP+` with detailed command reference
- 🌤️ **Proactive Weather Updates**: Automatic 5-minute weather refresh for instant responses
- 👥 **User Management**: User accounts, roles (User, Moderator, Sysop)
- 🔐 **Security**: Argon2id password hashing, configurable parameters, sysop immutability
- 📊 **Statistics**: Network and usage statistics
- ⚡ **Async Design**: Built with Tokio for high performance
- 🛎️ **Public Discovery + DM Sessions**: Low-noise public channel handshake (HELP / LOGIN) leading to authenticated Direct Message sessions
- 🧷 **Persistent Area Locks**: Moderators can LOCK / UNLOCK areas; state survives restarts
- 📜 **Deletion Audit Log**: `DELLOG [page]` paginates moderator deletion events for accountability
- 🛂 **Per-Area Access Levels**: Config-driven read/post level gating (areas hidden if insufficient read level)
- 💡 **Smart User Experience**: One-time shortcuts reminder, streamlined login flow, contextual guidance

## Quick Start

### Prerequisites

- Rust 1.82+ (install from [rustup.rs](https://rustup.rs/))
- A Meshtastic device (T-Beam, Heltec, etc.)
- USB cable or Bluetooth connection to your Meshtastic device

### Installation

1. Clone the repository:
```bash
git clone https://github.com/martinbogo/meshbbs.git
cd meshbbs
```

2. Build the project:
```bash
cargo build --release
```

3. Initialize the BBS:
```bash
./target/release/meshbbs init
```

4. Start the BBS server:
```bash
./target/release/meshbbs start --port /dev/ttyUSB0
```

### Configuration

The BBS is configured via a `config.toml` file. Run `meshbbs init` to create a default configuration:

```toml
[bbs]
name = "MeshBBS Station"
sysop = "Your Name"
location = "Your Location"
zipcode = "97210"
description = "A bulletin board system for mesh networks"
max_users = 100             # Hard cap on concurrent logged-in sessions
session_timeout = 10        # Minutes of inactivity before auto-logout
welcome_message = "Welcome to MeshBBS! Type HELP for commands." # Shown on login then description appended

[meshtastic]
port = "/dev/ttyUSB0"
baud_rate = 115200
node_id = ""
channel = 0

[storage]
data_dir = "./data"
max_message_size = 230        # Protocol hard cap; higher values are clamped
message_retention_days = 30
max_messages_per_area = 1000

[message_areas.general]
name = "General Discussion"
description = "General chat and discussion"
read_level = 0
post_level = 0

[logging]
level = "info"
file = "meshbbs.log"
```

## Usage

### Command Line Interface

```bash
# Start the BBS server
meshbbs start --port /dev/ttyUSB0

# Initialize configuration
meshbbs init

# Show status and statistics
meshbbs status

# Run serial smoke test
meshbbs smoke-test

# Set/update sysop password
meshbbs sysop-passwd

# Enable verbose logging
meshbbs -vv start
```

### Connecting via Meshtastic

MeshBBS uses a two-phase interaction model that keeps the shared mesh channel quiet while enabling richer private sessions.

1. Public Broadcast (Discovery)
	- Supported commands: `^HELP`, `^LOGIN <username>` (caret prefix REQUIRED to address the BBS)
	- `^HELP` returns a short onboarding message.
	- `^LOGIN <username>` registers a pending login for the sender's node id (no session yet).
2. Direct Message (DM) Session
	- After a public `LOGIN`, open a direct/private message to the BBS node.
	- The pending login is consumed and a session starts under that username.
	- Further interactive commands occur privately (syntax is evolving; early forms may be simple verbs like `READ general`).

This design minimizes public spam, allows lightweight discovery, and reserves bandwidth for substantive interactions in DMs.

#### Example Flow

Public channel (note required caret prefix):
```
> ^HELP
< MeshBBS: Send LOGIN <name> then start a DM to begin.

> ^LOGIN alice
< MeshBBS: Pending login for 'alice'. Open a DM to start your session.
```

Direct message session commands:
```
LOGIN <user> [pass]       # Authenticate (set password if first time)
REGISTER <user> <pass>    # Create new account
HELP / H / ?              # Show compact help (with shortcuts reminder on first use)
HELP+ / HELP V            # Detailed verbose help with examples

AREAS / LIST              # List available message areas
READ <area>               # Read recent messages from area
POST <area> <message>     # Post a message to area
POST <area>               # Start multi-line post (end with '.' on new line)

M                         # Quick navigation to message areas
U                         # Quick navigation to user menu  
Q                         # Quit/logout
B                         # Back to previous menu

CHPASS <old> <new>        # Change password
SETPASS <new>             # Set initial password (passwordless accounts)
LOGOUT                    # End session
```

### Dynamic Prompts

MeshBBS 0.9.0+ shows contextual prompts that reflect your current state:

- `unauth>` - Not logged in
- `alice (lvl1)>` - Logged in as alice, user level 1
- `alice@general>` - Reading messages in 'general' area
- `post@general>` - Posting a message to 'general' area

Prompts automatically appear at the end of responses and adapt to very long area names (truncated with ellipsis).

### Help System

Two help modes are available:

- **`HELP`** - Compact, role-aware help showing only commands you can use
- **`HELP+`** or **`HELP V`** - Verbose help with detailed explanations and examples

The first time you use `HELP` after login, you'll see a shortcuts reminder: "Shortcuts: M=areas U=user Q=quit".

Legacy prototype `CMD:` prefixed message formats are deprecated in favor of this simpler discovery + DM approach.

### Moderator & Sysop Commands (Direct Message)

Moderators (level >=5):

```
DELETE <area> <id>    # Remove a message
LOCK <area>           # Prevent new posts
UNLOCK <area>         # Allow posts again
DELLOG [page]         # View recent deletion audit entries (page size 10)
```

Sysop (level 10) also:
```
PROMOTE <user>
DEMOTE <user>
```

Area permissions are defined in `config.toml` under `[message_areas]` entries with `read_level` and `post_level`. Users cannot see (LIST/AREAS) areas above their read level, nor post below the post level. Locked areas reject posts even if the user otherwise qualifies.

Example config excerpt:
```toml
[message_areas.general]
name = "General Discussion"
description = "General chat"
read_level = 0
post_level = 0

[message_areas.announcements]
name = "Announcements"
description = "Important updates (sysop only posts)"
read_level = 0
post_level = 10
```

### Message Size Limit

Each message is limited to a maximum of **230 bytes** (not characters). This mirrors the practical Meshtastic text payload constraint. The `max_message_size` setting in `[storage]` is clamped to this ceiling even if a higher value is configured. Multi‑byte UTF‑8 characters reduce the number of visible glyphs you can send. Oversized posts are rejected with an error.

## Architecture

MeshBBS is built with a modular architecture:

- **`bbs/`**: Core BBS functionality and user interface
- **`meshtastic/`**: Meshtastic device communication layer
	- Parses protobuf frames (when `meshtastic-proto` is enabled) and emits structured `TextEvent` items consumed by the BBS routing logic (public vs DM).
- **`storage/`**: Message and file storage subsystem
- **`config/`**: Configuration management
	(Web interface support was removed in 0.8.10; configuration stubs and default feature were dropped.)

## Development

### Building from Source

```bash
# Debug build
cargo build

# Release build with optimizations
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run -- start
```

### Features

The project uses Cargo features to enable optional functionality:

- `serial` (default): Serial port communication with Meshtastic devices
- `meshtastic-proto` (default): Enable protobuf parsing of native Meshtastic packets
- `weather` (default): Enable weather lookup (uses zipcode + wttr.in)
- `api-reexports` (default): Re-export internal types for downstream crates

```bash
# Build minimal version without serial & protobuf
cargo build --no-default-features

# Build with Meshtastic protobuf parsing (placeholder proto)
cargo build --features meshtastic-proto

# Build with real Meshtastic protos (after cloning upstream definitions)
MESHTASTIC_PROTO_DIR=third_party/meshtastic-protobufs/src proto \
	cargo build --features meshtastic-proto
```

### Meshtastic Protobuf Integration

By default MeshBBS uses a simplified text parsing heuristic for incoming frames. For richer
packet handling (positions, node info, channels, telemetry, etc.) enable the `meshtastic-proto` feature.

The upstream Meshtastic protobuf definitions are included as a **git submodule** at:
`third_party/meshtastic-protobufs`

#### Cloning with the Submodule

Fresh clone (preferred):
```bash
git clone --recurse-submodules https://github.com/martinbogo/meshbbs.git
cd meshbbs
```

If you already cloned without submodules:
```bash
git submodule update --init --recursive
```

#### Building With Protobuf Support

```bash
cargo build --features meshtastic-proto
```

If you want to point at a different proto directory (e.g. experimenting with a fork), override:
```bash
MESHTASTIC_PROTO_DIR=path/to/your/protos/src cargo build --features meshtastic-proto
```

#### Updating the Submodule

To pull latest upstream protobuf changes:
```bash
git submodule update --remote third_party/meshtastic-protobufs
```
(Optionally add `--merge` if you keep a local branch.) Then commit the updated gitlink:
```bash
git add third_party/meshtastic-protobufs
git commit -m "chore(deps): bump meshtastic protobufs"
```

#### Pinning a Specific Version

For reproducible builds you can pin a commit:
```bash
cd third_party/meshtastic-protobufs
git checkout <commit-sha>
cd ../..
git add third_party/meshtastic-protobufs
git commit -m "chore(deps): pin meshtastic protobufs to <commit-sha>"
```

#### Fallback Behavior

If the submodule is absent or not initialized, a placeholder proto is used so the build still succeeds, but rich packet decoding will be limited.

#### Roadmap

Future work: Map decoded packet types to BBS events (messages, user presence, telemetry ingestion, channel config synchronization, etc.)

### Project Structure

```
meshbbs/
├── src/
│   ├── main.rs           # Application entry point
│   ├── lib.rs            # Library exports
│   ├── bbs/              # Core BBS functionality
│   │   ├── mod.rs
│   │   ├── server.rs     # BBS server implementation
│   │   ├── session.rs    # User session management
│   │   ├── commands.rs   # BBS command processing
│   │   ├── public.rs     # Public channel command parsing
│   │   └── roles.rs      # User role definitions
│   ├── meshtastic/       # Meshtastic integration
│   │   └── mod.rs        # Device communication & protocol
│   ├── storage/          # Data persistence
│   │   └── mod.rs        # Message & user storage
│   ├── config/           # Configuration
│   │   └── mod.rs        # Settings management
│   └── protobuf/         # Protobuf definitions
│       └── mod.rs
├── tests/                # Integration tests
├── data/                 # BBS data directory (created at runtime)
├── third_party/          # Git submodules
│   └── meshtastic-protobufs/
├── config.toml           # Configuration file
├── Cargo.toml           # Rust dependencies
└── README.md            # This file
```

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Setup

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Run `cargo test` and `cargo clippy`
6. Submit a pull request

## Roadmap

Recent releases have focused on user experience improvements and core functionality:

- ✅ **v0.9.0** (2025-09-22): Dynamic prompts, enhanced help system, proactive weather updates
- ✅ **v0.8.11** (2025-09-22): Unread message notifications, role-aware help
- ✅ **v0.8.10** (2025-09-22): Session management, user limits, welcome banners
- ✅ **v0.8.0** (2025-09-22): Security features, moderation tools, area access control

Future development priorities:
- [ ] **File transfer capabilities**: Binary data transfer protocols optimized for mesh constraints
- [ ] **Enhanced encryption**: End-to-end message encryption beyond transport security
- [ ] **Web interface**: Optional web-based administration and monitoring
- [ ] **Federation**: Multi-node BBS networks with message routing
- [ ] **Mobile clients**: Native mobile apps for easier mesh BBS access

## Hardware Compatibility

MeshBBS is designed to work with various Meshtastic-compatible devices:

- **T-Beam**: ESP32 + LoRa + GPS
- **Heltec LoRa 32**: ESP32 + LoRa + OLED
- **TTGO LoRa32**: ESP32 + LoRa
- **LilyGO devices**: Various ESP32-based LoRa boards
- **RAK WisBlock**: Modular LoRa solutions

## License

This project is licensed under the Creative Commons Attribution-NonCommercial 4.0 International License.

You are free to:
- **Share** — copy and redistribute the material in any medium or format
- **Adapt** — remix, transform, and build upon the material

Under the following terms:
- **Attribution** — You must give appropriate credit, provide a link to the license, and indicate if changes were made
- **NonCommercial** — You may not use the material for commercial purposes

See the [LICENSE](LICENSE) file for the full license text or visit [Creative Commons BY-NC 4.0](https://creativecommons.org/licenses/by-nc/4.0/) for more details.

## Acknowledgments

- [Meshtastic](https://meshtastic.org/) - The open source mesh networking project
- [Tokio](https://tokio.rs/) - Asynchronous runtime for Rust
- The amateur radio and mesh networking communities

## Support

- 📧 Email: martinbogo@gmail.com
-  Issues: [GitHub Issues](https://github.com/martinbogo/meshbbs/issues)
- 📖 Documentation: [Wiki](https://github.com/martinbogo/meshbbs/wiki)

---

**MeshBBS** - Bringing bulletin board systems to the mesh networking age! 📡