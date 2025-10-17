# MeshBBS Admin Dashboard - Phase 1 (Test-Ready)

## Overview

The admin web dashboard provides a modern web interface for managing MeshBBS. This is a **Phase 1 test-ready implementation** with core security infrastructure in place.

## What's Implemented (Phase 1)

✅ **Authentication**
- Uses same credentials as BBS sysop account
- Session-based with JWT tokens
- 24-hour token expiry with automatic rotation

✅ **Security**
- Disabled by default (must explicitly enable)
- TLS/HTTPS with self-signed cert auto-generation
- Rate limiting (5 login attempts/15min, 1000 API calls/min)
- Token replay prevention via rotation
- CSRF and XSS protection

✅ **Audit Logging**
- Mandatory logging of all admin actions
- Separate log file with timestamps
- Tamper-resistant with checksums
- Automatic rotation (daily/weekly/size-based)

✅ **Configuration**
- Bind to any IPv4/IPv6 address
- Configurable port (default 9885)
- Feature flags to enable/disable sections
- Multiple TLS modes (self-signed, Let's Encrypt, custom)

✅ **Basic UI**
- Login page
- Dashboard landing page
- Stats placeholders

## Quick Start

### 1. Build with Web UI Support

```bash
cargo build --features webui
```

### 2. Enable in Configuration

Add to your `config.toml`:

```toml
[admin_dashboard]
enabled = true
bind_addresses = ["0.0.0.0:9885", "[::]:9885"]
session_timeout = 86400
tls_mode = "self_signed"
```

See `admin_dashboard.example.toml` for full configuration options.

### 3. Start MeshBBS

```bash
./target/debug/meshbbs start --config config.toml
```

### 4. Access Dashboard

Open your browser to:
- **HTTP**: http://localhost:9885
- **HTTPS** (with self-signed cert): https://localhost:9885

**Note**: Your browser will warn about the self-signed certificate. This is normal for development. Click "Advanced" and proceed.

### 5. Login

Use your BBS sysop credentials:
- **Username**: Value of `bbs.sysop` from config.toml
- **Password**: The password you set with `meshbbs sysop-passwd`

## Security Notes

### Default Configuration

The dashboard is **DISABLED BY DEFAULT** for security. You must explicitly set `enabled = true` in the config.

### TLS Modes

- **`self_signed`** (default): Auto-generates certificate on startup
  - Good for: Development, local testing
  - Certificate saved to: `data/webui_cert.pem` and `data/webui_key.pem`

- **`letsencrypt`**: Automatic certificate from Let's Encrypt (ACME)
  - Good for: Production with public domain
  - Requires: `letsencrypt_domain` and `letsencrypt_email`
  - Status: **Not yet implemented** - falls back to self-signed

- **`custom`**: Use your own certificate
  - Good for: Corporate environments with CA
  - Requires: `tls_cert` and `tls_key` paths

- **`disabled`**: HTTP only (NOT RECOMMENDED)
  - Good for: Localhost-only testing
  - Warning: All traffic unencrypted

### Network Binding

The default configuration binds to **all interfaces** (0.0.0.0 and ::) to support both local and remote access. For localhost-only:

```toml
bind_addresses = ["127.0.0.1:9885", "[::1]:9885"]
```

### Rate Limiting

Protects against brute force attacks:
- **Login**: 5 failed attempts per IP per 15 minutes
- **API**: 1000 requests per session per minute

After exceeding limits, the IP/session is temporarily blocked.

### Audit Logging

All admin actions are logged to `data/admin_dashboard.log` by default. Log entries include:
- Timestamp (ISO 8601 with timezone)
- Username
- Action type (LOGIN, LOGOUT, CREATE, UPDATE, DELETE, etc.)
- Resource affected
- IP address
- Session token (truncated)
- Success/failure status

**The audit log cannot be disabled** - it's a mandatory security feature.

## Architecture

```
Frontend (Vanilla JS)
    ↓ REST API / WebSocket
Axum Web Server
    ↓ Authentication & Authorization
Audit Logger (async) → admin_dashboard.log
    ↓ Database Operations
Sled Database (shared with BBS)
```

## What's NOT Yet Implemented

The following are planned for future phases:

❌ Content Management (NPCs, Achievements, Rooms, Objects, Quests, Companions)
❌ Player Management (view, kick, ban, edit stats)
❌ System Monitoring (real-time metrics, graphs)
❌ Configuration Editor (edit config.toml via UI)
❌ JSON Editor (edit seed files)
❌ Analytics (charts, statistics)
❌ WebSocket live updates
❌ Let's Encrypt ACME implementation

## Development Roadmap

- **Phase 1** (Current): ✅ Foundation - Authentication, audit logging, TLS
- **Phase 2** (Next): Content Management CRUD for all 6 types
- **Phase 3**: Visual editors (world map, dialogue trees, quest flows)
- **Phase 4**: Player management and system monitoring
- **Phase 5**: Polish, analytics, mobile responsive

## Testing

### Manual Testing Checklist

- [ ] Enable dashboard in config
- [ ] Build with `--features webui`
- [ ] Start BBS server
- [ ] Access https://localhost:9885 in browser
- [ ] Accept self-signed certificate warning
- [ ] Login with sysop credentials
- [ ] Verify dashboard page loads
- [ ] Check audit log created at `data/admin_dashboard.log`
- [ ] Test logout
- [ ] Test failed login (check rate limiting after 5 attempts)
- [ ] Verify session persists across page reloads (24 hours)

### Automated Testing

Integration tests will be added in Phase 2.

## Troubleshooting

### "Connection refused" or "Cannot access"

1. Check dashboard is enabled: `enabled = true` in `[admin_dashboard]`
2. Check port not in use: `lsof -i :9885` (macOS/Linux)
3. Check bind addresses match your network

### "Certificate error" in browser

This is normal with `tls_mode = "self_signed"`. Click "Advanced" → "Proceed to localhost" (text varies by browser).

For production, use `tls_mode = "custom"` with a CA-signed certificate.

### "Invalid credentials"

1. Verify sysop username matches `bbs.sysop` in config
2. Reset password: `meshbbs sysop-passwd`
3. Check audit log for failed login details

### "Rate limit exceeded"

Wait 15 minutes or restart BBS server to reset rate limits.

## Configuration Reference

See `admin_dashboard.example.toml` for complete configuration with all options documented.

Key settings:
- `enabled`: Must be `true` to start dashboard
- `bind_addresses`: Array of "ip:port" strings
- `session_timeout`: Seconds before auto-logout (default 24 hours)
- `require_admin_level`: Minimum BBS admin level (default 10 = sysop)
- `tls_mode`: "self_signed", "letsencrypt", "custom", or "disabled"
- `rate_limit_enabled`: Enable/disable rate limiting
- `features_*`: Control which dashboard sections are available

## API Documentation

Basic API endpoints currently implemented:

- `POST /api/auth/login` - Login with username/password
  - Request: `{"username": "...", "password": "..."}`
  - Response: `{"token": "...", "username": "...", "admin_level": 10}`

- `POST /api/auth/logout` - End session
  - Headers: `Authorization: Bearer <token>`

- `GET /api/npcs` - List NPCs (placeholder, returns empty)
  - Headers: `Authorization: Bearer <token>`

Full API documentation will be added as features are implemented.

## Contributing

This is a work in progress. Phase 1 establishes the security foundation. Contributions welcome for Phase 2+ features!

See `docs/development/WEBUI_DESIGN.md` for complete design specification.

## License

Same as main MeshBBS project: CC-BY-NC-4.0
