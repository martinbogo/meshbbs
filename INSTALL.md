# Quick Installation Guide

## For Raspberry Pi / Linux

### Prerequisites

```bash
# Install system dependencies
sudo apt update
sudo apt install -y build-essential pkg-config libudev-dev libssl-dev git

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### Installation

```bash
# 1. Clone repository (submodules carry the Meshtastic protobufs; without them
#    the build fails to compile)
git clone --recurse-submodules https://github.com/martinbogo/meshbbs.git
cd meshbbs

# 2. Build release binary
cargo build --release

# 3. Install to /opt/meshbbs (requires sudo)
sudo ./install.sh

# 4. Configure
sudo nano /opt/meshbbs/config.toml
# Set serial_port, sysop_password, etc.

# 5. Start service
sudo systemctl enable meshbbs
sudo systemctl start meshbbs

# 6. Check status
sudo systemctl status meshbbs
sudo journalctl -u meshbbs -f
```

### Custom Installation Path

```bash
# Install to custom location
sudo ./install.sh /usr/local/meshbbs

# Or set environment variable
sudo MESHBBS_USER=myuser ./install.sh /home/myuser/meshbbs
```

---

## For macOS

```bash
# 1. Install Homebrew (if not installed)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# 2. Clone and build (--recurse-submodules is required for the protobufs)
git clone --recurse-submodules https://github.com/martinbogo/meshbbs.git
cd meshbbs
cargo build --release

# 3. Run directly (no install script needed on macOS)
./target/release/meshbbs

# 4. Or copy to /usr/local/bin for system-wide access
sudo cp target/release/meshbbs /usr/local/bin/
```

---

## Docker (Coming Soon)

```bash
docker pull martinbogo/meshbbs:latest
docker run -d --device=/dev/ttyACM0 \
  -v /opt/meshbbs/data:/data \
  --name meshbbs \
  martinbogo/meshbbs:latest
```

---

## Uninstallation

```bash
cd meshbbs
sudo ./uninstall.sh

# Optionally remove user
sudo userdel bbs
```

---

## Detailed Documentation

- [Raspberry Pi Setup](docs/getting-started/RASPBERRY_PI_SETUP.md)
- [Systemd Configuration](docs/administration/SYSTEMD_SETUP.md)
- [Configuration Guide](docs/getting-started/CONFIGURATION.md)
- [Security Best Practices](docs/administration/SECURITY.md)

---

## Troubleshooting

### Build fails with "libudev not found"
```bash
sudo apt install -y libudev-dev pkg-config
cargo clean && cargo build --release
```

### Permission denied on serial port
```bash
sudo usermod -a -G dialout $USER
# Log out and back in
```

### Out of memory during build
```bash
# Build with fewer parallel jobs
cargo build --release -j1
```

---

## Quick Test

After installation, test the BBS:

```bash
# Check if service is running
sudo systemctl status meshbbs

# View logs
sudo journalctl -u meshbbs -n 50

# Test serial port
ls -la /dev/ttyACM* /dev/ttyUSB*
```

---

## Support

- GitHub Issues: https://github.com/martinbogo/meshbbs/issues
- Documentation: https://martinbogo.github.io/meshbbs/
- Discord: [Link TBD]
