# Remote Restart Feature

## Overview

The MeshBBS server now supports remote restart functionality through the admin dashboard, allowing system operators to restart the server without SSH access.

## Implementation

### Backend Components

1. **`src/restart.rs`** - Core restart module
   - `restart_server()` - Spawns new server instance and exits current one
   - `is_restarting()` - Detects if current process is a restart
   - Platform: Works on Unix/Linux/macOS (uses `std::process::Command`)

2. **`src/webui/api/system.rs`** - System control API endpoints
   - `POST /api/system/restart` - Initiates server restart (JWT protected)
   - `GET /api/system/status` - Health check endpoint for reconnection polling

3. **Route Registration** - Added to `src/webui/server.rs`
   ```rust
   .route("/api/system/restart", post(restart_server))
   .route("/api/system/status", get(get_status))
   ```

### Frontend Components

1. **Restart Button** - Added to admin dashboard header
   - Location: Next to Logout button
   - Styled with amber/warning color scheme
   - Disabled state during restart process

2. **User Experience Flow**
   ```
   User clicks "🔄 Restart" button
   → Confirmation dialog with details
   → API call to /api/system/restart
   → Success toast: "Server restarting..."
   → Polls /api/system/status every 1s
   → On reconnect: Success toast + page reload
   ```

3. **Polling Logic** - `checkServerStatus()` function
   - Waits 3 seconds before first check
   - Retries every 1 second until server responds
   - Handles connection errors gracefully
   - Reloads dashboard when server is back online

## Usage

### For System Operators

1. Log into admin dashboard at `https://[server]:9885`
2. Click the **🔄 Restart** button in the top-right corner
3. Confirm the restart in the dialog
4. Wait 2-3 seconds for automatic reconnection

### For Applying Config Changes

When you toggle apps (Fortune, 8-Ball, etc.) on/off, the config file is updated immediately, but changes only take effect after restart:

1. Toggle app enabled/disabled
2. See toast: "⚠️ App enabled/disabled. Restart MeshBBS for changes to take effect."
3. Click **🔄 Restart** button
4. Server restarts with new configuration loaded

## Technical Details

### Restart Process

1. **API Request** (500ms delay for response delivery)
2. **Spawn New Process** - `std::process::Command::new(current_exe).spawn()`
3. **Current Process Exits** - `exit(0)` triggers graceful shutdown via Drop impls
4. **New Process Starts** - Loads fresh config from disk
5. **Total Downtime** - Typically 1-2 seconds

### Environment Flag

The new process sets `MESHBBS_RESTARTING=1` environment variable, which can be used to:
- Detect automated restarts vs. manual starts
- Log restart events differently
- Skip certain initialization steps if needed

### Error Handling

- **Spawn Failure**: Error toast shown, button re-enabled
- **Connection Timeout**: Polling continues indefinitely (server may be under load)
- **Auth Failure**: JWT middleware blocks unauthorized restart attempts

## Security

- Restart endpoint is protected by JWT authentication middleware
- Only users with valid auth tokens can trigger restarts
- No privilege escalation - runs as same user/permissions
- Audit logging can be added to track who initiated restarts

## Production Deployment

### Systemd Integration (Recommended)

For production Linux servers, integrate with systemd for automatic recovery:

```ini
# /etc/systemd/system/meshbbs.service
[Unit]
Description=MeshBBS Server
After=network.target

[Service]
Type=simple
User=meshbbs
WorkingDirectory=/opt/meshbbs
ExecStart=/opt/meshbbs/meshbbs
Restart=always
RestartSec=2s

[Install]
WantedBy=multi-user.target
```

This ensures:
- Automatic restart on crash
- Proper logging via journalctl
- System-level process management

### Docker/Containers

For containerized deployments:
- Use `--restart=unless-stopped` policy
- Mount config directory as volume
- Restart container updates config and restarts server

## Future Enhancements

Potential improvements for consideration:

1. **Hot Reload** - Reload config without full restart (see `WEBUI_IMPLEMENTATION.md` for Arc<RwLock<Config>> approach)
2. **Graceful Connection Transfer** - Zero-downtime restarts using socket passing
3. **Scheduled Restarts** - Cron-like scheduling for maintenance windows
4. **Restart History** - Log all restart events with timestamps and initiators
5. **Health Checks** - Pre-restart validation to prevent broken config from being loaded

## Testing

To test the restart functionality:

```bash
# Start server with WebUI
cargo run --features webui -- --daemon

# In admin dashboard:
# 1. Toggle an app on/off
# 2. Click Restart button
# 3. Verify reconnection happens automatically
# 4. Check app is enabled/disabled as expected
```

## Troubleshooting

### Restart button not visible
- Ensure you're logged in with valid JWT token
- Check browser console for JavaScript errors

### Server doesn't restart
- Check `meshbbs.log` for spawn errors
- Verify executable permissions
- Ensure sufficient system resources

### Reconnection fails
- Server may have crashed (check logs)
- Port may be blocked (firewall issue)
- Certificate mismatch (if using HTTPS)

## Related Documentation

- `WEBUI_IMPLEMENTATION.md` - Hot-reload architecture discussion
- `WEBUI_API_GUIDE.md` - API endpoint documentation
- `docs/administration/webui.md` - Admin dashboard guide
