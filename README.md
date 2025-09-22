# MeshBBS

A modern Bulletin Board System (BBS) designed for Meshtastic mesh networks, written in Rust.

## Overview

MeshBBS brings the classic BBS experience to modern mesh networks, allowing users to exchange messages, share files, and communicate over long-range, low-power radio networks using Meshtastic devices. This project aims to create resilient communication systems that work without traditional internet infrastructure.

## Features

- 📡 **Meshtastic Integration**: Direct communication with Meshtastic devices via serial or Bluetooth
- 💬 **Message Boards**: Traditional BBS-style message areas and forums
- 📁 **File Sharing**: Exchange files over the mesh network
- 👥 **User Management**: User accounts and permissions system
- 🔐 **Security**: Message encryption and user authentication
- 📊 **Statistics**: Network and usage statistics
- 🌐 **Web Interface**: Optional web-based administration panel
- ⚡ **Async Design**: Built with Tokio for high performance

## Quick Start

### Prerequisites

- Rust 1.70+ (install from [rustup.rs](https://rustup.rs/))
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
max_users = 100

[meshtastic]
port = "/dev/ttyUSB0"
baud_rate = 115200
node_id = "your_node_id"

[storage]
data_dir = "./data"
max_message_size = 1024
message_retention_days = 30

[web]
enabled = false
bind_address = "127.0.0.1:8080"
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

# Enable verbose logging
meshbbs -vv start
```

### Connecting via Meshtastic

Once the BBS is running, users can connect by sending messages to your Meshtastic node. The BBS will respond with a menu system allowing users to:

- Read and post messages
- Browse message areas
- Download/upload files
- View user statistics
- Send private messages

### Message Format

The BBS communicates using structured text messages optimized for LoRa transmission:

```
CMD:LOGIN user:johndoe
CMD:READ area:general
CMD:POST area:general msg:Hello mesh world!
CMD:LIST files
```

## Architecture

MeshBBS is built with a modular architecture:

- **`bbs/`**: Core BBS functionality and user interface
- **`meshtastic/`**: Meshtastic device communication layer
- **`storage/`**: Message and file storage subsystem
- **`config/`**: Configuration management
- **`web/`** (optional): Web administration interface

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
- `web`: Web interface for administration
 - `meshtastic-proto`: Enable protobuf parsing of native Meshtastic packets (requires Meshtastic .proto files)

```bash
# Build with web interface
cargo build --features web

# Build minimal version without serial support
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
│   ├── bbs/              # Core BBS functionality
│   │   ├── mod.rs
│   │   ├── server.rs     # BBS server implementation
│   │   ├── session.rs    # User session management
│   │   └── commands.rs   # BBS command processing
│   ├── meshtastic/       # Meshtastic integration
│   │   ├── mod.rs
│   │   ├── device.rs     # Device communication
│   │   └── protocol.rs   # Message protocol
│   ├── storage/          # Data persistence
│   │   ├── mod.rs
│   │   ├── messages.rs   # Message storage
│   │   └── users.rs      # User management
│   └── config/           # Configuration
│       ├── mod.rs
│       └── settings.rs
├── data/                 # BBS data directory
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

- [ ] **v0.1.0**: Basic BBS functionality with message boards
- [ ] **v0.2.0**: File sharing and transfer capabilities
- [ ] **v0.3.0**: User management and permissions
- [ ] **v0.4.0**: Web administration interface
- [ ] **v0.5.0**: Message encryption and security features
- [ ] **v1.0.0**: Production-ready release

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