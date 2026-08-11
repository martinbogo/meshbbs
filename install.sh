#!/bin/bash
# MeshBBS Installation Script
# Installs MeshBBS to /opt/meshbbs (or custom location)
# Usage: sudo ./install.sh [install_path]

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Default installation path
INSTALL_PATH="${1:-/opt/meshbbs}"
SERVICE_USER="${MESHBBS_USER:-bbs}"
SERVICE_GROUP="${MESHBBS_GROUP:-bbs}"

echo -e "${GREEN}MeshBBS Installation Script${NC}"
echo "========================================"
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then 
    echo -e "${RED}Error: This script must be run as root (use sudo)${NC}"
    exit 1
fi

# Check if binary exists
if [ ! -f "target/release/meshbbs" ]; then
    echo -e "${RED}Error: Binary not found at target/release/meshbbs${NC}"
    echo "Please run 'cargo build --release' first"
    exit 1
fi

echo -e "${YELLOW}Installation Configuration:${NC}"
echo "  Install path: $INSTALL_PATH"
echo "  Service user: $SERVICE_USER"
echo "  Service group: $SERVICE_GROUP"
echo ""

# Confirm installation
read -p "Continue with installation? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Installation cancelled."
    exit 0
fi

echo ""
echo "Installing MeshBBS..."

# Create service user if it doesn't exist
if ! id "$SERVICE_USER" &>/dev/null; then
    echo -e "${YELLOW}Creating service user: $SERVICE_USER${NC}"
    useradd -r -s /bin/false -d "$INSTALL_PATH" -c "MeshBBS Service User" "$SERVICE_USER"
fi

# Add user to dialout group for serial port access
echo -e "${YELLOW}Adding $SERVICE_USER to dialout group${NC}"
usermod -a -G dialout "$SERVICE_USER" 2>/dev/null || true

# Create installation directory
echo -e "${YELLOW}Creating installation directory: $INSTALL_PATH${NC}"
mkdir -p "$INSTALL_PATH"
mkdir -p "$INSTALL_PATH/bin"
mkdir -p "$INSTALL_PATH/scripts"
mkdir -p "$INSTALL_PATH/data"
mkdir -p "$INSTALL_PATH/data/messages"
mkdir -p "$INSTALL_PATH/data/users"
mkdir -p "$INSTALL_PATH/data/files"
mkdir -p "$INSTALL_PATH/data/tinymush"
mkdir -p "$INSTALL_PATH/data/backups"
mkdir -p "$INSTALL_PATH/data/seeds"

# Copy binary
echo -e "${YELLOW}Installing binary${NC}"
cp target/release/meshbbs "$INSTALL_PATH/bin/meshbbs"
chmod 755 "$INSTALL_PATH/bin/meshbbs"

# Copy seed data files. Sourced from packaging/runtime-skel rather than data/,
# which is gitignored and therefore absent from a fresh clone.
echo -e "${YELLOW}Installing seed data files${NC}"
if [ -d "packaging/runtime-skel/data/seeds" ]; then
    cp packaging/runtime-skel/data/seeds/*.json "$INSTALL_PATH/data/seeds/"
else
    echo -e "${YELLOW}Warning: seed data not found; TinyMUSH will use built-in defaults${NC}"
fi

# Copy daemon helper script
if [ -f "scripts/meshbbs-daemon.sh" ]; then
    echo -e "${YELLOW}Installing daemon helper script${NC}"
    cp scripts/meshbbs-daemon.sh "$INSTALL_PATH/scripts/meshbbs-daemon.sh"
    chmod 755 "$INSTALL_PATH/scripts/meshbbs-daemon.sh"
fi

# Copy or create configuration
if [ ! -f "$INSTALL_PATH/config.toml" ]; then
    echo ""
    echo -e "${GREEN}Configuration Setup${NC}"
    echo "===================="
    echo ""
    
    # Ask for sysop password
    echo -e "${YELLOW}Please set a sysop password (for administrative access)${NC}"
    echo "This password will be hashed and stored securely."
    echo ""
    
    SYSOP_PASSWORD=""
    while [ -z "$SYSOP_PASSWORD" ]; do
        read -s -p "Sysop password (min 8 characters): " SYSOP_PASSWORD
        echo ""
        if [ ${#SYSOP_PASSWORD} -lt 8 ]; then
            echo -e "${RED}Password must be at least 8 characters${NC}"
            SYSOP_PASSWORD=""
            continue
        fi
        read -s -p "Confirm password: " SYSOP_PASSWORD_CONFIRM
        echo ""
        if [ "$SYSOP_PASSWORD" != "$SYSOP_PASSWORD_CONFIRM" ]; then
            echo -e "${RED}Passwords do not match${NC}"
            SYSOP_PASSWORD=""
        fi
    done
    
    # Hash the password using the meshbbs binary
    echo -e "${YELLOW}Hashing password...${NC}"
    SYSOP_HASH=$(echo -n "$SYSOP_PASSWORD" | "$INSTALL_PATH/bin/meshbbs" hash-password 2>/dev/null || echo "")
    
    if [ -z "$SYSOP_HASH" ]; then
        echo -e "${YELLOW}Warning: Could not hash password automatically. Will use plaintext marker.${NC}"
        echo -e "${YELLOW}You'll need to run: meshbbs sysop-passwd${NC}"
        SYSOP_HASH="REPLACE_WITH_HASHED_PASSWORD"
    fi
    
    # Ask for serial port
    echo ""
    echo -e "${YELLOW}Select serial port for Meshtastic device:${NC}"
    echo "Common ports:"
    echo "  1) /dev/ttyUSB0 (default)"
    echo "  2) /dev/ttyACM0"
    echo "  3) /dev/ttyUSB1"
    echo "  4) Custom"
    echo ""
    read -p "Select option (1-4) [1]: " SERIAL_CHOICE
    SERIAL_CHOICE=${SERIAL_CHOICE:-1}
    
    case $SERIAL_CHOICE in
        1) SERIAL_PORT="/dev/ttyUSB0" ;;
        2) SERIAL_PORT="/dev/ttyACM0" ;;
        3) SERIAL_PORT="/dev/ttyUSB1" ;;
        4) 
            read -p "Enter custom serial port: " SERIAL_PORT
            ;;
        *) SERIAL_PORT="/dev/ttyUSB0" ;;
    esac
    
    echo -e "${YELLOW}Creating config.toml${NC}"
    
    # Create config file
    cat > "$INSTALL_PATH/config.toml" <<EOF
# MeshBBS Configuration File
# Generated by install script on $(date)

[bbs]
# Display name for your BBS
name = "Your MeshBBS Name"

# Location (shown to users)
location = "90210"

# Description of your BBS
description = "Some information about your MeshBBS"

# Sysop (system operator) username
sysop = "sysop"

# Sysop password hash (generated during installation)
sysop_password_hash = "$SYSOP_HASH"

# Maximum concurrent users
max_users = 100

# Session timeout in minutes
session_timeout = 30

# Welcome message shown at login
welcome_message = "Welcome to this Mesh BBS"

# Public command prefix (single character: ^ ! + $ / >)
public_command_prefix = "^"

# Allow public channel LOGIN command (security consideration)
# When false, users must initiate login via direct message only
allow_public_login = false

[meshtastic]
# Serial port for Meshtastic device
port = "$SERIAL_PORT"

# Baud rate (usually 115200)
baud_rate = 115200

# Channel to monitor (0 = primary channel)
channel = 0

[storage]
# Data directory (absolute path)
data_dir = "$INSTALL_PATH/data"

# Maximum message size in bytes (accounting for ~30 bytes Meshtastic protocol overhead)
max_message_size = 200

# Note: Automatic backups are configured via in-game admin commands
# and stored in data/backup_scheduler.json (not in this config file)

[logging]
# Log level: trace, debug, info, warn, error
level = "info"

# Optional: Log to file (recommended for production)
file = "$INSTALL_PATH/meshbbs.log"

# ============================================================================
# [apps] - Application & Feature Configuration
# ============================================================================
# All BBS apps use the unified [apps.*] configuration structure
# Each app can be independently enabled/disabled

[apps.fortune]
# Fortune cookie broadcaster
enabled = true

[apps.eightball]
# Magic 8-ball fortune telling
enabled = true

[apps.slotmachine]
# Slot machine mini-game
enabled = true

[apps.weather]
# OpenWeatherMap API key - Get one at https://openweathermap.org/api
# IMPORTANT: Replace this with your actual API key!
api_key = "REPLACE_WITH_YOUR_API_KEY"

# Enable weather functionality (requires valid API key)
enabled = false

# Default location for weather queries
location = "Los Angeles"

# Location type: "city", "zipcode", or "city_id"
location_type = "city"

# Country code for zipcode lookups (e.g., "US", "GB")
country_code = "US"

# Cache duration in minutes
cache_ttl_minutes = 10

# Request timeout in seconds
timeout_seconds = 5

[apps.tinyhack]
# TinyHack roguelike game
enabled = true

[apps.tinymush]
# TinyMUSH multi-user game
enabled = true

# Optional: Override TinyMUSH database path (defaults to <data_dir>/tinymush)
# db_path = "$INSTALL_PATH/data/tinymush"

[apps.ident_beacon]
# Station identification beacon
enabled = true

# Beacon frequency: "5min", "15min", "30min", "1hr", "2hr", "4hr"
frequency = "15min"

[apps.welcome]
# Welcome message system (greets new nodes on the mesh)
enabled = false

# Send public greeting via public channel
public_greeting = true

# Send private guide via DM
private_guide = true

# Minimum time between any welcomes (minutes) to prevent spam
cooldown_minutes = 5

# Maximum times to welcome the same node (prevents repeated welcomes)
max_welcomes_per_node = 1
EOF
    
    echo -e "${GREEN}Configuration created!${NC}"
    echo ""
    echo "Important: Your sysop password has been hashed and stored securely."
    echo ""
    
else
    echo -e "${YELLOW}Existing config.toml found, not overwriting${NC}"
fi

chmod 644 "$INSTALL_PATH/config.toml" 2>/dev/null || true

# Copy topics example if data/topics.json doesn't exist
if [ ! -f "$INSTALL_PATH/data/topics.json" ]; then
    if [ -f "topics.example.json" ]; then
        echo -e "${YELLOW}Installing topics.json${NC}"
        cp topics.example.json "$INSTALL_PATH/data/topics.json"
    fi
fi

# The fortune app loads exclusively from data/fortunes.json and has no built-in
# fallback, so without this file the feature stays disabled at every startup.
if [ ! -f "$INSTALL_PATH/data/fortunes.json" ]; then
    if [ -f "packaging/runtime-skel/data/fortunes.json" ]; then
        echo -e "${YELLOW}Installing fortunes.json${NC}"
        cp packaging/runtime-skel/data/fortunes.json "$INSTALL_PATH/data/fortunes.json"
    else
        echo -e "${YELLOW}Warning: fortunes.json not found; fortune feature will be disabled${NC}"
    fi
fi

# Same story for the 8-ball: no data file means the feature stays unavailable.
if [ ! -f "$INSTALL_PATH/data/8ball_responses.json" ]; then
    if [ -f "packaging/runtime-skel/data/8ball_responses.json" ]; then
        echo -e "${YELLOW}Installing 8ball_responses.json${NC}"
        cp packaging/runtime-skel/data/8ball_responses.json "$INSTALL_PATH/data/8ball_responses.json"
    else
        echo -e "${YELLOW}Warning: 8ball_responses.json not found; 8-ball will be unavailable${NC}"
    fi
fi

# Create log file for systemd mount namespace
echo -e "${YELLOW}Creating log file${NC}"
touch "$INSTALL_PATH/meshbbs.log"

# Set ownership
echo -e "${YELLOW}Setting ownership${NC}"
chown -R "$SERVICE_USER:$SERVICE_GROUP" "$INSTALL_PATH"

# Set permissions
chmod 755 "$INSTALL_PATH"
chmod 755 "$INSTALL_PATH/bin"
chmod 755 "$INSTALL_PATH/scripts"
chmod 755 "$INSTALL_PATH/data"
chmod 755 "$INSTALL_PATH/bin/meshbbs"

# Install systemd service if systemd is available
if command -v systemctl &> /dev/null; then
    echo -e "${YELLOW}Installing systemd service${NC}"
    
    # Create service file
    cat > /etc/systemd/system/meshbbs.service <<EOF
[Unit]
Description=MeshBBS - Bulletin Board System for Meshtastic
Documentation=https://martinbogo.github.io/meshbbs/
After=network.target
Wants=network.target

[Service]
Type=simple
User=$SERVICE_USER
Group=$SERVICE_GROUP
WorkingDirectory=$INSTALL_PATH
ExecStart=$INSTALL_PATH/bin/meshbbs start
Restart=always
RestartSec=10
TimeoutStopSec=30

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=meshbbs

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=full
ProtectHome=true
ReadWritePaths=$INSTALL_PATH/data $INSTALL_PATH/meshbbs.log
ProtectKernelTunables=true
ProtectControlGroups=true
RestrictRealtime=true

# Environment
Environment="RUST_LOG=info"

# Resource limits
MemoryMax=512M
TasksMax=100

[Install]
WantedBy=multi-user.target
EOF

    # Reload systemd
    systemctl daemon-reload
    
    echo ""
    echo -e "${GREEN}Systemd service installed!${NC}"
    echo "To enable and start the service:"
    echo "  sudo systemctl enable meshbbs"
    echo "  sudo systemctl start meshbbs"
    echo ""
    echo "To check status:"
    echo "  sudo systemctl status meshbbs"
    echo "  sudo journalctl -u meshbbs -f"
fi

echo ""
echo -e "${GREEN}Installation complete!${NC}"
echo ""
echo "Installation details:"
echo "  Binary:        $INSTALL_PATH/bin/meshbbs"
if [ -f "$INSTALL_PATH/scripts/meshbbs-daemon.sh" ]; then
    echo "  Daemon script: $INSTALL_PATH/scripts/meshbbs-daemon.sh"
fi
echo "  Config:        $INSTALL_PATH/config.toml"
echo "  Data:          $INSTALL_PATH/data"
echo "  User:          $SERVICE_USER"
echo ""
echo "Next steps:"
echo "  1. Edit configuration: sudo nano $INSTALL_PATH/config.toml"
echo "  2. Set sysop password in config.toml"
echo "  3. Configure serial port (usually /dev/ttyACM0 or /dev/ttyUSB0)"
if command -v systemctl &> /dev/null; then
    echo "  4. Enable service: sudo systemctl enable meshbbs"
    echo "  5. Start service: sudo systemctl start meshbbs"
else
    echo "  4. Run manually: cd $INSTALL_PATH && sudo -u $SERVICE_USER ./scripts/meshbbs-daemon.sh start"
fi
echo ""
echo "Documentation: $INSTALL_PATH/docs/ or https://martinbogo.github.io/meshbbs/"
echo ""
