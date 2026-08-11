# Raspberry Pi Setup Guide

## System Requirements

- Raspberry Pi 3B+ or newer (tested on Pi 4 and Pi 5)
- Raspberry Pi OS (64-bit recommended)
- At least 1GB RAM available
- Internet connection for initial setup

---

## Installation Steps

### 1. Install System Dependencies

MeshBBS requires several system libraries for compilation:

```bash
# Update package list
sudo apt update

# Install required development libraries
sudo apt install -y \
    build-essential \
    pkg-config \
    libudev-dev \
    libssl-dev \
    git

# Optional: Install Rust if not already present
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### 2. Clone and Build MeshBBS

```bash
# Clone the repository. --recurse-submodules is required: the Meshtastic protobuf
# definitions live in a submodule, and without them the build fails to compile.
git clone --recurse-submodules https://github.com/martinbogo/meshbbs.git
cd meshbbs

# Build the release version (this will take 10-20 minutes on Pi)
cargo build --release

# The binary will be at: target/release/meshbbs
```

### 3. Install MeshBBS

**Option A: Automated Installation (Recommended)**

```bash
# Install to default location (/opt/meshbbs)
sudo ./install.sh

# Or specify custom location
sudo ./install.sh /usr/local/meshbbs
```

The install script will:
- Create service user and directories
- Install binary and configuration
- Set up systemd service
- Configure permissions

**Option B: Manual Installation**

```bash
# Create installation directory
sudo mkdir -p /opt/meshbbs/data
sudo cp target/release/meshbbs /opt/meshbbs/
sudo cp config.example.toml /opt/meshbbs/config.toml

# Create service user
sudo useradd -r -s /bin/false -d /opt/meshbbs bbs
sudo usermod -a -G dialout bbs
sudo chown -R bbs:bbs /opt/meshbbs
```

### 4. Configuration

```bash
# Edit configuration
sudo nano /opt/meshbbs/config.toml
```

Key settings for Raspberry Pi:
- Set `serial_port` to your Meshtastic device (usually `/dev/ttyACM0` or `/dev/ttyUSB0`)
- Adjust `data_dir` if you want to store data on external storage
- Set `sysop_password` for admin access

### 5. Start MeshBBS

```bash
# Enable and start service (if using install.sh)
sudo systemctl enable meshbbs
sudo systemctl start meshbbs

# Check status
sudo systemctl status meshbbs

# View logs
sudo journalctl -u meshbbs -f
```

---

## Troubleshooting

### Error: `libudev` not found

**Solution:** Install the development package:
```bash
sudo apt install -y libudev-dev pkg-config
```

### Error: `libssl` not found

**Solution:** Install OpenSSL development package:
```bash
sudo apt install -y libssl-dev
```

### Serial Port Permission Denied

**Solution:** Add your user to the `dialout` group:
```bash
sudo usermod -a -G dialout $USER
# Log out and back in for changes to take effect
```

### Build Runs Out of Memory

**Solution:** Increase swap space or build with fewer parallel jobs:
```bash
# Build with only 1 parallel job
cargo build --release -j1
```

---

## Performance Notes

- **Raspberry Pi 3B+:** Expect 5-10 concurrent users comfortably
- **Raspberry Pi 4 (4GB+):** Can handle 20+ concurrent users
- **Raspberry Pi 5:** Excellent performance, 30+ users

The BBS is designed to be lightweight and runs efficiently on Raspberry Pi hardware.

---

## Next Steps

1. Read [Configuration Guide](CONFIGURATION.md)
2. Set up [Systemd Service](../administration/SYSTEMD_SETUP.md)
3. Review [Security Guide](../administration/SECURITY.md)
4. Configure [Backup System](../administration/BACKUP_RECOVERY.md)

---

## Quick Fix Reference

```bash
# Complete dependency installation command
sudo apt install -y build-essential pkg-config libudev-dev libssl-dev git

# Add user to serial port group
sudo usermod -a -G dialout $USER

# Build with memory constraints
cargo build --release -j1

# Check serial ports
ls -la /dev/ttyACM* /dev/ttyUSB*

# Test serial port permissions
groups | grep dialout
```
