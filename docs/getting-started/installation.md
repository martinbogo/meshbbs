# Installation Guide

This guide will walk you through installing and setting up meshbbs on your system.

## Prerequisites

- **Rust 1.82+** - Install from [rustup.rs](https://rustup.rs/)
- **Meshtastic Device** - Any compatible device (T-Beam, Heltec, etc.)
- **Connection** - USB cable or Bluetooth capability

## Installation Steps

### 1. Clone the Repository

```bash
git clone --recurse-submodules https://github.com/martinbogo/meshbbs.git
cd meshbbs
```

> **Note**: The `--recurse-submodules` flag is important for including Meshtastic protobuf definitions.

### 2. Build the Project

```bash
# Debug build for development
cargo build

# Release build for production
cargo build --release
```

### 3. Initialize Configuration

```bash
# Create default configuration
./target/release/meshbbs init
```

This creates a `config.toml` file with default settings.

### 4. Configure Your BBS

Edit the generated `config.toml` file:

```toml
[bbs]
name = "Your BBS Name"
sysop = "your_admin_username"
location = "Your Location"
zipcode = "12345"

[meshtastic]
port = "/dev/ttyUSB0"  # Adjust for your system
baud_rate = 115200

[storage]
data_dir = "./data"
max_message_size = 230
```

### 5. Set Sysop Password

```bash
./target/release/meshbbs sysop-passwd
```

### 6. Start Your BBS

```bash
./target/release/meshbbs start
```

## Platform-Specific Notes

### Linux
- Device typically at `/dev/ttyUSB0` or `/dev/ttyACM0`
- May need to add user to `dialout` group: `sudo usermod -a -G dialout $USER`

### macOS  
- Device typically at `/dev/tty.usbserial-*`
- May need to install serial drivers for some devices

### Windows
- Device typically at `COM3`, `COM4`, etc.
- Check Device Manager for the correct port

## Verification

Once started, you should see output similar to:

```
INFO BBS 'Your BBS Name' started by your_admin_username
INFO Meshtastic device connected on /dev/ttyUSB0
INFO Ready for connections
```

Your BBS is now ready for users to connect via the Meshtastic network!

## Next Steps

- [Configuration Guide](configuration.md) - Detailed configuration options
- [First Run Guide](first-run.md) - Initial setup and testing
- [Troubleshooting](../user-guide/troubleshooting.md) - Common issues and solutions