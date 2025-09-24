<div align="center">
  <img src="images/meshbbs_logo.png" alt="MeshBBS Logo" width="200" height="200">
  
  # MeshBBS
  
  **A modern Bulletin Board System for Meshtastic mesh networks**
  
   [![Version](https://img.shields.io/badge/version-0.9.90-blue.svg)](https://github.com/martinbogo/meshbbs/releases)
  [![License](https://img.shields.io/badge/license-CC--BY--NC--4.0-green.svg)](LICENSE)
  [![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
  [![Platform](https://img.shields.io/badge/platform-Meshtastic-purple.svg)](https://meshtastic.org/)
  
  *Bringing the classic BBS experience to modern mesh networks*
  
  [🚀 Quick Start](#quick-start) • [📖 User Guide](#usage) • [📚 Documentation](docs/) • [🔧 API Reference](https://martinbogo.github.io/meshbbs/meshbbs/) • [🤝 Contributing](#contributing) • [💬 Support](#support)
</div>

---

## 🌟 Overview

MeshBBS revolutionizes communication on mesh networks by bringing the beloved Bulletin Board System experience to Meshtastic devices. Exchange messages, participate in forums, and build communities over long-range, low-power radio networks—all without traditional internet infrastructure.

Perfect for emergency communications, remote areas, outdoor adventures, and building resilient community networks.

## 📚 Documentation

Comprehensive documentation is available in the [`docs/`](docs/) directory and hosted at [GitHub Pages](https://martinbogo.github.io/meshbbs):

- **[Installation Guide](docs/getting-started/installation.md)** - Complete setup instructions
- **[Command Reference](docs/user-guide/commands.md)** - All available commands and usage
- **[API Documentation](https://martinbogo.github.io/meshbbs/meshbbs/)** - Generated Rust API docs
- **[Administration Guide](docs/administration/)** - BBS setup and management
- **[Hardware Compatibility](docs/hardware/)** - Supported devices and setup

> The documentation is maintained alongside the code and automatically updated with each release.

## ✨ Features

### � **Connectivity & Integration**
- **📡 Meshtastic Integration**: Direct communication via serial or Bluetooth
- **🛎️ Public Discovery + DM Sessions**: Low-noise public channel handshake leading to authenticated Direct Message sessions
- **⚡ Async Design**: Built with Tokio for high performance

### 💬 **Communication & Messaging**
- **📚 Message Boards**: Traditional BBS-style message topics and forums
- **🎯 Dynamic Contextual Prompts**: Smart prompts showing current state (`unauth>`, `user@topic>`, `post@topic>`)
- **📜 Enhanced Help System**: Compact `HELP` + verbose `HELP+` with detailed command reference
- **📏 Optimized Message Size**: 230-byte limit optimized for Meshtastic constraints

### 👥 **User Management & Security**
- **🔐 Robust Security**: Argon2id password hashing with configurable parameters
- **👑 Role-Based Access**: User, Moderator, and Sysop roles with granular permissions
- **🛂 Per-Topic Access Levels**: Config-driven read/post level gating
- **💡 Smart User Experience**: One-time shortcuts reminder, streamlined login flow

### 🛠️ **Administration & Moderation**
- **🧷 Persistent Topic Locks**: Moderators can LOCK/UNLOCK topics; state survives restarts
- **📊 Deletion Audit Log**: `DELLOG` command for accountability tracking
- **📈 Network Statistics**: Usage and performance monitoring
- **🌤️ Proactive Weather Updates**: Automatic 5-minute weather refresh

## 🚀 Quick Start

> **Prerequisites**: Rust 1.82+, Meshtastic device, USB cable or Bluetooth connection

### 📦 Installation

```bash
# Clone the repository
git clone --recurse-submodules https://github.com/martinbogo/meshbbs.git
cd meshbbs

# Build the project
cargo build --release

# Initialize the BBS configuration
./target/release/meshbbs init
```

### ⚙️ Configure Your BBS

After initialization, edit the `config.toml` file to set up your BBS:

```bash
# Open config.toml in your preferred editor
nano config.toml  # or vim, code, etc.
```

**Critical settings to configure:**

1. **📡 Meshtastic Connection** - Update your serial port:
   ```toml
   [meshtastic]
   port = "/dev/ttyUSB0"  # Change to your device port
   # macOS: often /dev/tty.usbserial-*
   # Windows: often COM3, COM4, etc.
   # Linux: often /dev/ttyUSB0, /dev/ttyACM0
   ```

2. **👑 Sysop Information** - Set your admin details:
   ```toml
   [bbs]
   name = "Your BBS Name"
   sysop = "Your Name"  # This becomes your admin username
   location = "Your Location"
   zipcode = "12345"    # For weather features
   ```

3. **🔐 Set Sysop Password** - Secure your admin account:
   ```bash
   ./target/release/meshbbs sysop-passwd
   ```

### 🚀 Start Your BBS

```bash
# Start the BBS server (use your configured port)
./target/release/meshbbs start

# Or specify port if different from config
./target/release/meshbbs start --port /dev/ttyUSB0
```

### ⚡ Quick Commands

| Command | Description |
|---------|-------------|
| `meshbbs init` | Create initial configuration file |
| `meshbbs sysop-passwd` | Set/update sysop password (do this first!) |
| `meshbbs start` | Start BBS server with config.toml settings |
| `meshbbs start --port /dev/ttyUSB0` | Override port from command line |
| `meshbbs status` | Show server statistics and status |

## ⚙️ Configuration

MeshBBS uses a `config.toml` file for all settings. Run `meshbbs init` to create a default configuration.

<details>
<summary><strong>📋 View Example Configuration</strong></summary>

```toml
[bbs]
name = "MeshBBS Station"
sysop = "Your Name"
location = "Your Location" 
zipcode = "97210"
description = "A bulletin board system for mesh networks"
max_users = 100             # Hard cap on concurrent logged-in sessions
session_timeout = 10        # Minutes of inactivity before auto-logout
welcome_message = "Welcome to MeshBBS! Type HELP for commands."

[meshtastic]
port = "/dev/ttyUSB0"
baud_rate = 115200
node_id = ""
channel = 0
min_send_gap_ms = 2000                  # Enforced minimum between sends (ms)
dm_resend_backoff_seconds = [4, 8, 16]  # Reliable DM retry schedule (s)
post_dm_broadcast_gap_ms = 1200         # Delay broadcast after DM (ms)
dm_to_dm_gap_ms = 600                   # Gap between DMs (ms)
help_broadcast_delay_ms = 3500          # Delay HELP public broadcast after DM (ms)

[storage]
data_dir = "./data"
max_message_size = 230        # Protocol hard cap


[logging]
level = "info"
file = "meshbbs.log"
```
</details>

### 🎛️ Key Configuration Options

| Section | Purpose | Key Settings |
|---------|---------|--------------|
| `[bbs]` | Basic BBS settings | `name`, `sysop`, `max_users`, `session_timeout` |
| `[meshtastic]` | Device connection | `port`, `baud_rate`, `channel` |
### Fairness / Writer Tuning Fields

These pacing controls reduce airtime contention and avoid triggering device / network rate limits:

* `min_send_gap_ms` – Global enforced minimum between any two text sends (hard floor 2000ms)
* `dm_resend_backoff_seconds` – Retry schedule for reliable DM ACKs (default `[4,8,16]` seconds)
* `post_dm_broadcast_gap_ms` – Additional gap before a broadcast that immediately follows a reliable DM
* `dm_to_dm_gap_ms` – Gap enforced between consecutive reliable DMs
* `help_broadcast_delay_ms` – Higher-level scheduling delay for the public HELP notice after its DM reply; effective delay is `max(help_broadcast_delay_ms, min_send_gap_ms + post_dm_broadcast_gap_ms)` (default 3500ms) to prevent an immediate broadcast rate-limit right after a DM

| `[storage]` | Data management | `max_message_size` |
| `topics.json` | Forum topics (runtime) | Create/manage interactively; persisted to `data/topics.json` |

## 📖 Usage

### 🎮 Command Line Interface

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

### 📡 Connecting via Meshtastic

MeshBBS uses a **two-step interaction model** that keeps the shared mesh channel quiet while enabling rich private sessions.

#### 🔍 **Step 1: Say Hello on the Public Channel**
Commands require `^` prefix to address the BBS:
- `^HELP` - Returns onboarding message
- `^LOGIN <username>` - Registers pending login for your node ID
- `^WEATHER` - Get current weather information

#### 💬 **Step 2: Start Your Private Conversation**
After public `LOGIN`, open a private message to the BBS node to start your authenticated session.

<details>
<summary><strong>📋 Complete Command Reference</strong></summary>

**Authentication Commands:**
```bash
LOGIN <user> [pass]       # Authenticate (set password if first time)
REGISTER <user> <pass>    # Create new account
LOGOUT                    # End session
CHPASS <old> <new>        # Change password
SETPASS <new>             # Set initial password (passwordless accounts)
```

**Navigation & Help:**
```bash
HELP / H / ?              # Compact help with shortcuts
HELP+ / HELP V            # Detailed verbose help with examples
M                         # Quick navigation to message topics
U                         # Quick navigation to user menu
Q                         # Quit/logout
B                         # Back to previous menu
```

**Message Commands:**
```bash
TOPICS / LIST             # List available message topics
READ <topic>              # Read recent messages from topic
POST <topic> <message>    # Post a message to topic
POST <topic>              # Start multi-line post (end with '.' on new line)
```

**Moderator Commands** (level ≥5):
```bash
DELETE <topic> <id>       # Remove a message
LOCK <topic>              # Prevent new posts
UNLOCK <topic>            # Allow posts again  
DELLOG [page]             # View deletion audit entries
```

**Sysop Commands** (level 10):
```bash
PROMOTE <user>            # Promote user level
DEMOTE <user>             # Demote user level
```
</details>

### 🎯 Dynamic Prompts

MeshBBS shows contextual prompts that reflect your current state:

| Prompt | Meaning |
|--------|---------|
| `unauth>` | Not logged in |
| `alice (lvl1)>` | Logged in as alice, user level 1 |
| `alice@general>` | Reading messages in 'general' topic |
| `post@general>` | Posting a message to 'general' topic |

### 📏 Message Size Limit

Each message is limited to **230 bytes** (not characters) to mirror Meshtastic text payload constraints. Multi-byte UTF-8 characters reduce visible character count. Oversized posts are rejected with an error.

## 🏗️ Architecture

MeshBBS is built with a clean, modular architecture in Rust:

```mermaid
graph TD
    A[Meshtastic Device] --> B[Serial/Bluetooth Interface]
    B --> C[Meshtastic Module]
    C --> D[BBS Server]
    D --> E[Sessions HashMap]
    E --> F[Session State]
    F --> G[Command Processor]
    D --> H[Storage Layer]
    H --> I[Message Database]
    H --> J[User Database]
    D --> K[Configuration]
    D --> L[Device Interface]
    D --> M[Public State]
```

### 📁 Module Structure

- **`bbs/`**: Core BBS functionality and user interface
- **`meshtastic/`**: Meshtastic device communication layer
  - Parses protobuf frames and emits structured `TextEvent` items
- **`storage/`**: Message and file storage subsystem  
- **`config/`**: Configuration management

## 🛠️ Development

### 🔧 Building from Source

```bash
# Development build
cargo build

# Optimized release build
cargo build --release

# Run comprehensive test suite
cargo test

# Run with debug logging
RUST_LOG=debug cargo run -- start
```

### 🎛️ Feature Flags

Control optional functionality with Cargo features:

| Feature | Default | Description |
|---------|---------|-------------|
| `serial` | ✅ | Serial port communication |
| `meshtastic-proto` | ✅ | Protobuf parsing of Meshtastic packets |
| `weather` | ✅ | Weather lookup via wttr.in |
| `api-reexports` | ✅ | Re-export internal types |

```bash
# Minimal build without optional features
cargo build --no-default-features

# Build with specific features only
cargo build --features "serial,weather"
```

### 📡 Meshtastic Protobuf Integration

For rich packet handling, enable the `meshtastic-proto` feature. Upstream protobuf definitions are included as a git submodule.

<details>
<summary><strong>🔧 Protobuf Setup Instructions</strong></summary>

**Fresh clone with submodules:**
```bash
git clone --recurse-submodules https://github.com/martinbogo/meshbbs.git
```

**Initialize submodules in existing clone:**
```bash
git submodule update --init --recursive
```

**Build with protobuf support:**
```bash
cargo build --features meshtastic-proto
```

**Update submodules:**
```bash
git submodule update --remote third_party/meshtastic-protobufs
git add third_party/meshtastic-protobufs
git commit -m "chore(deps): bump meshtastic protobufs"
```

**Use custom proto directory:**
```bash
MESHTASTIC_PROTO_DIR=path/to/protos cargo build --features meshtastic-proto
```
</details>

### 📂 Project Structure

```
meshbbs/
├── 📄 src/
│   ├── main.rs           # Application entry point
│   ├── lib.rs            # Library exports
│   ├── 🎮 bbs/           # Core BBS functionality
│   │   ├── server.rs     # BBS server implementation
│   │   ├── session.rs    # User session management
│   │   ├── commands.rs   # BBS command processing
│   │   ├── public.rs     # Public channel command parsing
│   │   └── roles.rs      # User role definitions
│   ├── 📡 meshtastic/    # Meshtastic integration
│   ├── 💾 storage/       # Data persistence
│   ├── ⚙️ config/        # Configuration management
│   └── 📋 protobuf/      # Protobuf definitions
├── 🧪 tests/             # Integration tests
├── 📊 data/              # BBS data directory (runtime)
├── 🔧 third_party/       # Git submodules
│   └── meshtastic-protobufs/
└── 📝 config.toml        # Configuration file
```

## 🗺️ Roadmap

### ✅ Recent Releases
- **v0.9.70** (2025-09-23): Major architecture refactor implementing reader/writer pattern to resolve asynchronous I/O deadlock, improved message delivery reliability
- **v0.9.65** (2025-09-23): Fixed short name resolution bug - correctly extract actual short names (e.g., "WAP2") from Meshtastic network instead of generating wrong names from long names (e.g., "WREC")
- **v0.9.60** (2025-09-23): Enhanced weather debugging with full URL logging, improved DM delivery with hop limit fixes, persistent node cache system
- **v0.9.55** (2025-09-23): Complete AREA → TOPIC terminology refactor, enhanced default topics, documentation fixes
- **v0.9.50** (2025-09-23): Documentation accuracy improvements, streamlined roadmap, tested hardware clarity
- **v0.9.30** (2025-09-23): Critical security fix for public login password bypass vulnerability
- **v0.9.25** (2025-09-23): Documentation improvements, rustdoc fixes, ^WEATHER command addition
- **v0.9.20** (2025-09-23): Version consistency and stability improvements
- **v0.9.18** (2025-09-23): New user welcome system, enhanced security, sysop username support

### 🚀 Upcoming Features
- [ ] **� Locally encrypted data storage**: Enhanced security for stored messages and user data

## 💻 Hardware Compatibility

MeshBBS has been tested on the following Meshtastic devices:

| Device | Status |
|--------|--------|
| **Heltec V3** | ✅ Tested |
| **Heltec T114** | ✅ Tested |
| **LilyGO T-Deck** | ✅ Tested |
| **LilyGO T-Beam** | ✅ Tested |
| **RAK WisBlock** | ✅ Tested |

> **Other Meshtastic devices**: MeshBBS should work with any Meshtastic-compatible device, but we'd love to hear about your experiences adapting the BBS to other hardware! Please share your results in the discussions or issues.

## 🤝 Contributing

We welcome contributions from the community! Here's how to get started:

### 🚀 Quick Contribution Guide

1. **🍴 Fork** the repository
2. **🌟 Create** a feature branch: `git checkout -b feature/amazing-feature`
3. **💻 Make** your changes with tests
4. **✅ Test** your changes: `cargo test && cargo clippy`
5. **📝 Commit** with clear messages: `git commit -m 'feat: add amazing feature'`
6. **📤 Push** to your branch: `git push origin feature/amazing-feature`
7. **🔄 Submit** a Pull Request

### 📋 Development Guidelines

- Follow Rust best practices and idioms
- Add tests for new functionality
- Update documentation for user-facing changes
- Run `cargo fmt` and `cargo clippy` before committing
- Keep commits focused and atomic

**Note**: All code contributions require appropriate unit tests.

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed guidelines.

## 📄 License

<div align="center">

[![License: CC BY-NC 4.0](https://img.shields.io/badge/License-CC%20BY--NC%204.0-lightgrey.svg)](https://creativecommons.org/licenses/by-nc/4.0/)

</div>

This project is licensed under the **Creative Commons Attribution-NonCommercial 4.0 International License**.

**You are free to:**
- ✅ **Share** — copy and redistribute in any medium or format
- ✅ **Adapt** — remix, transform, and build upon the material

**Under these terms:**
- 🏷️ **Attribution** — Give appropriate credit and indicate changes
- 🚫 **NonCommercial** — No commercial use without permission

See the [LICENSE](LICENSE) file or visit [CC BY-NC 4.0](https://creativecommons.org/licenses/by-nc/4.0/) for details.

## 🙏 Acknowledgments

Special thanks to the projects and communities that make MeshBBS possible:

- 🌐 **[Meshtastic](https://meshtastic.org/)** - The open source mesh networking project
- ⚡ **[Tokio](https://tokio.rs/)** - Asynchronous runtime for Rust  
- 📻 **Amateur Radio Community** - For mesh networking innovations
- 🦀 **Rust Community** - For the amazing language and ecosystem

## 💬 Support

<div align="center">

**Need help? We're here for you!**

[![Email](https://img.shields.io/badge/Email-martinbogo%40gmail.com-blue?style=for-the-badge&logo=gmail)](mailto:martinbogo@gmail.com)
[![Issues](https://img.shields.io/badge/Issues-GitHub-orange?style=for-the-badge&logo=github)](https://github.com/martinbogo/meshbbs/issues)
[![Docs](https://img.shields.io/badge/Documentation-GitHub%20Pages-green?style=for-the-badge&logo=gitbook)](https://martinbogo.github.io/meshbbs)

</div>

### 🐛 Bug Reports
Found a bug? Please [open an issue](https://github.com/martinbogo/meshbbs/issues/new) with:
- Steps to reproduce
- Expected vs actual behavior  
- System information (OS, Rust version, device model)
- Relevant log output

### 💡 Feature Requests
Have an idea? We'd love to hear it! [Start a discussion](https://github.com/martinbogo/meshbbs/discussions) or create an issue.

### 🆘 Getting Help
- Check the [Documentation](docs/) for comprehensive guides
- Browse the [API Reference](https://martinbogo.github.io/meshbbs/meshbbs/) for technical details
- Search existing [Issues](https://github.com/martinbogo/meshbbs/issues) for solutions
- Join the discussion in [GitHub Discussions](https://github.com/martinbogo/meshbbs/discussions)

---

<div align="center">
  
**🎯 MeshBBS - Bringing bulletin board systems to the mesh networking age! 📡**

*Built with ❤️ for the mesh networking community*

[![Made with Rust](https://img.shields.io/badge/Made%20with-Rust-orange?style=flat&logo=rust)](https://www.rust-lang.org/)
[![Powered by Meshtastic](https://img.shields.io/badge/Powered%20by-Meshtastic-purple?style=flat)](https://meshtastic.org/)

</div>