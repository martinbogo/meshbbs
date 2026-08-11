# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### 🔴 BREAKING CHANGES - Configuration Restructure (v1.2.0)

**All app configurations have been unified under `[apps.*]` sections for consistency and better WebUI management.**

#### Migration Required

**Old configuration structure:**
```toml
[games]
tinyhack_enabled = true
tinymush_enabled = true
tinymush_db_path = "./data/tinymush"

[weather]
enabled = false
api_key = "YOUR_KEY"
default_location = "Los Angeles"

[ident_beacon]
enabled = true
frequency = "15min"

[welcome]
enabled = false
public_greeting = true
```

**New configuration structure:**
```toml
[apps.fortune]
enabled = true

[apps.eightball]
enabled = true

[apps.slotmachine]
enabled = true

[apps.weather]
enabled = false
api_key = "YOUR_KEY"
location = "Los Angeles"  # Note: renamed from default_location

[apps.tinyhack]
enabled = true

[apps.tinymush]
enabled = true
db_path = "./data/tinymush"  # Note: renamed from tinymush_db_path

[apps.ident_beacon]
enabled = true
frequency = "15min"

[apps.welcome]
enabled = false
public_greeting = true
```

#### Migration Steps

1. **Backup your current config:**
   ```bash
   cp config.toml config.toml.backup
   ```

2. **Update config structure:**
   - Replace `[games]` section with `[apps.tinyhack]` and `[apps.tinymush]`
   - Replace `[weather]` with `[apps.weather]`
   - Replace `[ident_beacon]` with `[apps.ident_beacon]`
   - Replace `[welcome]` with `[apps.welcome]`
   - Add new app sections: `[apps.fortune]`, `[apps.eightball]`, `[apps.slotmachine]`

3. **Field name changes:**
   - `weather.default_location` → `apps.weather.location`
   - `games.tinymush_db_path` → `apps.tinymush.db_path`
   - `games.tinyhack_enabled` → `apps.tinyhack.enabled`
   - `games.tinymush_enabled` → `apps.tinymush.enabled`

4. **Restart meshbbs:**
   ```bash
   systemctl restart meshbbs
   # or if running manually:
   ./meshbbs
   ```

#### Benefits of New Structure

- **Unified Configuration**: All apps follow consistent `[apps.<name>]` pattern
- **Better WebUI Integration**: Admin dashboard can easily toggle all apps
- **Easier Extension**: Adding new apps is now straightforward
- **Clearer Organization**: Each app's config is self-contained
- **Future-Proof**: Scales better as more apps are added

#### What's Changed in Code

- New `AppsConfig` struct with 8 nested app configurations
- Updated 13 source files with 50+ reference changes
- All 680 tests passing (247 unit + 433 integration)
- WebUI toggle endpoint now supports all 8 apps
- `config.example.toml` fully updated with comprehensive documentation
- `install.sh` generates new config format automatically

### Added

- Fortune cookie broadcaster app configuration (`[apps.fortune]`)
- Magic 8-ball app configuration (`[apps.eightball]`)
- Slot machine app configuration (`[apps.slotmachine]`)
- Unified app management in admin WebUI

### Changed

- **BREAKING**: Configuration file structure requires migration (see above)
- Admin dashboard now shows all 8 apps with consistent toggle controls
- Config parsing now uses unified `AppsConfig` structure
- `[apps.tinyhack]` and `[apps.tinymush]` remain **disabled by default**, matching
  the old `[games]` behavior. Enabling them adds a `[G]ames` entry to the main menu,
  which pushes the registration reply past the 200-byte mesh message limit.

### Deprecated

- Old config sections `[games]`, `[weather]`, `[ident_beacon]`, `[welcome]` (still parsed for compatibility but deprecated)
- These will be removed in v2.0.0

---

## [1.1.4] - 2025-10-17

### Added - Vending Machine System & Documentation

#### Shop System Enhancement
- **Vending Machine Support**: Clone-based item purchasing system
  - Added `clone_items: bool` field to `ShopRecord` for vending vs regular shops
  - New `ShopRecord::new_vending_machine()` constructor for easy creation
  - `clone_object_internal()` helper for shop purchases (bypasses player clone security)
  - Each purchase creates unique clone with new UUID and owner tracking
  - Stock tracking separate from item cloning (limited vs infinite inventory)

- **Museum Trinket Vending Machine**: Example world content
  - Located in `mesh_museum` location
  - 4 commemorative trinkets with tiered pricing:
    - Mesh Network Keychain (25 coins, 50 stock)
    - Commemorative Antenna Pin (50 coins, 30 stock)
    - Storm '19 Memorial Coin (75 coins, 20 stock)
    - Museum Replica Node (100 coins, 10 stock)
  - All items are Clonable and takeable
  - Demonstrates clone-based purchasing mechanics

- **Shop Seeding System**: Automatic world initialization
  - Added `seed_shops_if_needed()` function in storage layer
  - Integrated into world initialization sequence
  - Shops seed automatically on first startup
  - Vending machine and trinkets created from `seed_starter_vending_machine()`

#### Documentation
- **DATA_DRIVEN_STATUS.md**: Comprehensive system status (360 lines)
  - Verification that all Phase 1-6 data-driven systems are 100% complete
  - Documentation of 6 admin command systems (@ACHIEVEMENT, @NPC, @COMPANION, @ROOM, @OBJECT, @QUEST)
  - JSON seed file status for all content types
  - Architecture overview and example usage
  - Confirms 387 tests passing with zero compiler warnings

- **COMBAT_TODO.md**: Complete combat system implementation plan (944 lines)
  - 6-week phased implementation roadmap
  - Data-driven approach extending existing NPC/Object systems
  - JSON-configurable enemies, loot tables, aggression types
  - PvE and PvP design with opt-in mechanics
  - Companion combat assist integration
  - Achievement tracking for kills
  - Boss fight mechanics and safe zone enforcement
  - Example NPCs: training dummy, goblin raider, dragon boss

- **TODO.md Cleanup**: Removed 1,610 lines of completed items
  - Cleaned from 1,762 lines to 152 lines
  - Removed all completed Phase 1-10 work
  - Now contains only "Future Enhancements (Post-Alpha Launch)"
  - Updated with recent completions (vending machine, vendor NPCs)
  - Documented vendor NPC dialog integration (Mira the Vendor)

### Changed
- **Shop Purchase Logic**: Modified `handle_buy()` to support dual modes
  - Checks `shop.clone_items` flag to decide clone vs transfer
  - Clone mode: Creates new item with `clone_object_internal()`
  - Transfer mode: Removes item from shop inventory (original behavior)
  - Maintains backward compatibility with existing shops

### Fixed
- **Test Configuration**: Added missing `require_device_at_startup` field
  - Updated 12 test files with proper MeshtasticConfig initialization
  - Ensures all tests compile with latest configuration schema
  - Files: admin_commands, deletion_log, ident_beacon_behavior, lock_persistence,
    moderation, password_enforcement, permissions, role_commands, sysop_seed,
    tmush_admin_command_handlers, unread_indicators, unread_messages

### Technical Notes
- All new fields use `#[serde(default)]` for backward compatibility
- No database migration required
- Clone tracking via `clone_source_id` for genealogy
- Ownership history preserved for all cloned items
- Security: Shop cloning bypasses player quotas/cooldowns (appropriate for purchases)
- Performance: Async database operations maintained for 500-1000 user scale

## [1.1.3] - 2025-01-17

### Added - Queue Health Monitoring & Graceful Device Startup

This release focuses on production reliability and operational visibility, addressing
long-running stability issues and improving service management integration.

#### Queue Health Monitoring System
- **Scheduler Health Checks**: Automatic monitoring every 30 seconds detects queue saturation
  - Warns when queue depth exceeds 80% capacity (>410/512 messages)
  - Alerts immediately when messages are dropped due to overflow
  - Logs recovery when queue returns to normal state
  - Tracks priority escalations and aging patterns

- **Circuit Breaker**: Prevents cascade failures at 95% queue capacity
  - Rejects new messages when queue critically full (>486/512 messages)
  - Returns clear error instead of silently dropping
  - Prevents system from accepting work it cannot complete
  - Users see "system overloaded" rather than timeout

- **Bounded Pending HashMap**: Limits awaiting ACKs to prevent unbounded memory growth
  - Maximum 100 pending reliable sends (previously unlimited)
  - Automatic cleanup every 5 minutes removes stale entries
  - Removes entries older than 10 minutes
  - Warns when approaching limit (>80 entries)

- **Observable Metrics**: Comprehensive logging of health indicators
  - Queue depth percentage and absolute counts
  - Drop counts (total and overflow-specific)
  - Priority escalation events
  - Pending HashMap size and cleanup statistics

**Problem Solved**: After ~2 hours of runtime, BBS would stop responding to commands
while ident beacons continued. Root cause was silent queue saturation where the bounded
512-message scheduler queue filled faster than messages could transmit over low-bandwidth
mesh radio, eventually dropping even high-priority command responses.

See [QUEUE_HEALTH_MONITORING.md](QUEUE_HEALTH_MONITORING.md) and 
[BUG_FIX_SUMMARY.md](BUG_FIX_SUMMARY.md) for complete details.

#### Graceful Device Startup Handling
- **New Configuration Option**: `require_device_at_startup` (default: false)
  - When `false`: BBS continues starting without device (backward compatible)
  - When `true`: BBS exits immediately with error code 2 if device unavailable

- **Proper Exit Codes for Service Managers**:
  - Exit code 0: Clean shutdown (normal operation)
  - Exit code 1: Configuration error (invalid config)
  - Exit code 2: Device connection failed (when required)

- **Clear Error Messages**: Guides troubleshooting
  - Reports exact connection error (permission denied, device not found, etc.)
  - Suggests configuration changes to allow startup without device
  - Recommends solutions (add user to dialout group, check device path)

- **Future-Ready Design**: Transport-agnostic architecture
  - Works with current serial connections
  - Will support future Bluetooth connections
  - Will support future TCP/UDP network connections
  - Single configuration option controls all transport types

- **Production-Friendly**: Service manager integration
  - systemd can detect device failures via exit code
  - Docker/Kubernetes health checks work correctly
  - No restart loops when device persistently unavailable
  - Clear "failed" status instead of appearing healthy

**Problem Solved**: When serial device unavailable at startup (USB disconnected, wrong
permissions, device busy), BBS would log warning and continue, appearing healthy to
service managers while being unable to communicate with mesh network.

See [DEVICE_STARTUP_HANDLING.md](DEVICE_STARTUP_HANDLING.md) and
[GRACEFUL_STARTUP_SUMMARY.md](GRACEFUL_STARTUP_SUMMARY.md) for complete details.

### Configuration
- **config.example.toml**: Added `require_device_at_startup` documentation
  - Defaults to `false` for backward compatibility
  - Recommended `true` for production deployments
  - Works with all current and future transport types

### Documentation
- **QUEUE_HEALTH_MONITORING.md**: Comprehensive monitoring guide
  - Health check frequency and thresholds
  - Log analysis commands
  - Tuning recommendations
  - Future enhancement roadmap

- **BUG_FIX_SUMMARY.md**: Bug analysis and fix summary
  - Root cause analysis
  - Before/after behavior comparison
  - Testing recommendations

- **DEVICE_STARTUP_HANDLING.md**: Complete device startup guide
  - Configuration options and behavior modes
  - Production deployment examples (systemd, docker)
  - Troubleshooting common errors
  - Future transport planning

- **GRACEFUL_STARTUP_SUMMARY.md**: Quick reference guide
  - Exit code meanings
  - Common errors and solutions
  - Migration guide for existing installations

### Changed
- **Writer Task**: Enhanced heartbeat interval with cleanup logic
  - Periodic pending HashMap cleanup (every 5 minutes)
  - Warns when pending HashMap exceeds 80% of limit
  - Removes entries older than 10 minutes
  - Removes oldest entries when limit exceeded

- **Main Event Loop**: Added periodic health monitoring
  - Scheduler health check every 30 seconds
  - Tracks cumulative drops and queue high warnings
  - Logs recovery when system stabilizes

- **Startup Sequence**: Device connection validation
  - Captures `require_device_at_startup` flag before moving config
  - Clear error paths for missing device and missing port
  - Consistent behavior in daemon and non-daemon modes

### Fixed
- **Queue Saturation After 2+ Hours**: Prevents silent lockup
  - Early warning when queue depth exceeds 80%
  - Circuit breaker prevents accepting work when overloaded
  - Bounded resources prevent unbounded memory growth
  - System degrades gracefully with clear errors

- **Silent Device Failures**: Proper exit codes and error messages
  - Service managers can detect device connection failures
  - Clear guidance for resolving permission issues
  - No zombie processes appearing healthy but non-functional

### Performance
- **Scheduler Efficiency**: Minimal overhead from health checks
  - Health check runs once per 30 seconds (negligible CPU impact)
  - Snapshot operation is fast (atomic read of stats)
  - No performance impact during normal operation

- **Memory Bounds**: Pending HashMap limited growth
  - Maximum 100 entries (previously unlimited)
  - Periodic cleanup prevents accumulation
  - Typical usage: 0-20 entries under normal load

## [1.1.2] - 2025-01-16

### Added
- **Configurable Help Command**: The public help command keyword can now be configured to use "HELP", "MENU", or "INFO" instead of the default "HELP". This addresses networks where !HELP might be reserved for emergency assistance (e.g., 911-style emergency calls). Configure via `help_command` in the `[bbs]` section of config.toml. All help text generation throughout the system (ident beacons, public broadcasts, DM help, verbose help, and welcome messages) now uses the configured keyword.

## [1.1.1] - 2025-01-15

### Added - Complete Data-Driven Admin System

This release completes the data-driven admin system expansion, implementing all missing
editable fields for rooms, objects, and players. Administrators can now manage all game
content at runtime without code modification.

#### New Admin Commands
- **@GIVE command**: Grant items and currency to players
  - `@GIVE <player> <object_id> [quantity]` - Grant items with optional quantity
  - `@GIVE <player> CURRENCY <amount>` - Grant currency to player
  - Supports item stacking for stackable objects
  - Uses proper CurrencyAmount type matching (Decimal/MultiTier)
  - Validates player and object existence
  - Requires admin level 2+

- **@TELEPORT command**: Admin teleportation of other players
  - `@TELEPORT <player> <room>` - Teleport player to specific room
  - Enhanced parsing to distinguish from @GOTO (self teleport)
  - Validates player and room existence
  - Persists player location changes
  - Requires admin level 2+

- **@STATS command**: Edit player statistics
  - `@STATS <player> EDIT <stat> <value>` - Modify player stats
  - Currently supports HP editing
  - Extensible architecture for future stats (max_HP, XP, etc.)
  - Requires admin level 2+

#### Enhanced Room Management
- **@ROOM EDIT VISIBILITY**: Added Hidden visibility mode for secret rooms
  - `@ROOM EDIT <room> VISIBILITY <public|private|hidden>` - Control room visibility
  - Hidden rooms don't appear in area maps or room lists
  - Useful for secret passages, puzzle rooms, admin areas
  
- **@ROOM EDIT LOCKED**: Lock/unlock rooms
  - `@ROOM EDIT <room> LOCKED <true|false>` - Toggle room lock state
  - Locked rooms prevent entry except by authorized users
  
- **@ROOM EDIT OWNER**: Transfer room ownership
  - `@ROOM EDIT <room> OWNER <player|world>` - Change room owner
  - Transfer to specific player or make world-owned
  - Validates player existence before transfer
  
- **@ROOM EDIT HOUSING_TAGS**: Configure housing filter tags
  - `@ROOM EDIT <room> HOUSING_TAGS <tag1,tag2,...>` - Set housing category tags
  - Used for filtering housing templates (medieval, modern, fantasy, etc.)
  
- **@ROOM EDIT EXIT REMOVE**: Delete exits
  - `@ROOM EDIT <room> EXIT <direction> REMOVE` - Remove existing exit
  - Bidirectional exit management
  - Prevents orphaned connections

#### Enhanced Object Management
- **@OBJECT EDIT OWNER**: Transfer object ownership
  - `@OBJECT EDIT <id> OWNER <player|world>` - Change object owner
  - Transfer to specific player or make world-owned
  - Validates player existence before transfer
  - Enables admin redistribution of items

### Changed
- **@ADMIN help text**: Updated to show all new admin commands with level requirements
  - Level 1+ commands: @PLAYERS, @GOTO
  - Level 2+ commands: @SETADMIN, @REMOVEADMIN, @GIVE, @TELEPORT, @STATS
  
- **@TELEPORT parsing**: Enhanced to support both self-teleport and admin modes
  - `@TELEPORT <target>` - Self teleport (like @GOTO)
  - `@TELEPORT <player> <room>` - Admin teleport another player

### Technical Improvements
- Added `ItemStack` import for proper inventory management
- Used `put_player_async()` for all player persistence operations
- Safe currency operations with `CurrencyAmount.add()` method
- Proper ownership handling (extracted display names before move)
- Added `RoomVisibility::Hidden` enum variant
- All 636+ tests pass
- Zero compilation errors or warnings

### Data-Driven System Status
All 10 identified gaps in the admin toolset have been implemented:
1. ✅ Room visibility control (public/private/hidden)
2. ✅ Room locking
3. ✅ Item granting to players
4. ✅ Currency granting to players
5. ✅ Admin player teleportation
6. ✅ Object ownership transfer
7. ✅ Room ownership transfer
8. ✅ Player stat editing
9. ✅ Housing tag configuration
10. ✅ Exit removal

Administrators now have complete runtime control over all rooms, objects, and player
properties through standardized @COMMAND interfaces without requiring code changes.

## [1.1.0] - 2025-10-15

### Added - Production Deployment & Advanced Object Management

This release adds production-ready systemd deployment support, security hardening,
admin workflow improvements, and comprehensive object management tools for large worlds.

#### Systemd Service Integration
- **Production-ready systemd service**: Full integration with systemd on Raspberry Pi and Linux systems
  - Automatic service installation via `install.sh`
  - Security hardening: `ProtectSystem=full`, `PrivateTmp=true`, memory limits
  - Graceful shutdown support (SIGTERM handling)
  - Service templates in `docs/administration/SYSTEMD_SETUP.md`
  - Compatible with older kernels (RPi 3/4 tested)
- **Fixed systemd compatibility issues**:
  - Removed `RestrictNamespaces=true` (incompatible with older kernels)
  - Changed `ProtectSystem=strict` → `ProtectSystem=full` (fixes 226/NAMESPACE error)
  - Changed `MemoryLimit` → `MemoryMax` (removed deprecation warning)
  - Added `start` subcommand to ExecStart for proper daemon mode
  - Created `meshbbs.log` during installation for mount namespace compatibility

#### Admin Command Standardization
- **All admin commands now use @ prefix**: Standardized to MUSH conventions
  - Converted 17 commands: @BUILDER, @SETBUILDER, @REMOVEBUILDER, @BUILDERS
  - Player monitoring: @PLAYERS, @WHERE, @GOTO
  - Clone management: @LISTCLONES, @CLONESTATS
  - Backup operations: @BACKUP, @RESTORE, @LISTBACKUPS, @VERIFYBACKUP, @DELETEBACKUP, @BACKUPCONFIG
  - World events: @CONVERT_CURRENCY
  - Updated all help text and documentation
  - Maintains consistency across 33 total admin commands

#### Clone Security Enhancements
- **Builder permission requirement for /CLONE**: Security fix for production worlds
  - Only builders (level 1+) can now use /CLONE command
  - Prevents general player abuse of cloning system
  - Added Check 0 (builder permission) before all other security checks
  - Updated help text to mention builder requirement
  - Added `test_non_builder_cannot_clone()` test case
- **Existing clone protections remain**:
  - UNIQUE flag prevents cloning entirely
  - Quest items blocked via Quest flag
  - High-value items (>100 gold) cannot be cloned
  - Max clone depth: 3 levels
  - Player quotas: 100 objects max, 20 clones/hour
  - 60-second cooldown between operations

#### Advanced Object Management (@OBJECT Enhancements)
- **@OBJECT LIST [pattern]**: Optional fuzzy search filter
  - Case-insensitive substring matching on ID, name, and description
  - Example: `@OBJECT LIST torch` shows all torch-related objects
  - Shows filtered count vs total count
- **@OBJECT SEARCH <pattern>**: Advanced fuzzy search with scoring
  - Intelligent relevance ranking (exact match > partial > description)
  - Word boundary matching for better results
  - Top 20 results with scores displayed
  - Truncated descriptions for readability
  - Example: `@OBJECT SEARCH singing mushroom` finds even with typos
- **@OBJECT INSTANCES <id>**: Track object usage across world
  - Scans all rooms (items field)
  - Scans all player inventories
  - Shows clone information for prototypes
  - Displays location summary with counts
  - Example output: "📍 Rooms (3), 🎒 Player Inventories (2), 📊 Total: 5"
- **@OBJECT EDIT warnings**: Smart warnings for heavily-used objects
  - Warns when editing prototypes (clone_count > 0)
  - Warns when object appears in multiple locations
  - Shows instance count breakdown (rooms/players)
  - Suggests using @OBJECT INSTANCES for details
  - Prevents accidental edits to widely-used items

### Changed
- **Install script improvements**: Enhanced path layout and systemd integration
- **Documentation updates**: Added systemd setup guide, updated admin command reference
- **Test suite enhancements**: Added 14 new tests for clone security and object management

### Fixed
- **Systemd service startup failures** on Raspberry Pi and older Linux kernels
- **Player monitoring commands** now use @ prefix consistently
- **Clone security vulnerability** allowing any player to clone objects

### Technical Notes
- All 636+ tests passing
- No breaking changes to existing functionality
- Backward compatible with existing game worlds and configurations
- Systemd service templates tested on Raspberry Pi 3/4

## [1.0.900] - 2025-10-15

### Added - Complete Data-Driven Content Management System

This release completes the comprehensive migration from hardcoded content to a fully
data-driven system with admin commands and JSON seed files. All game content is now
manageable without recompilation.

### Added - Data-Driven Seed Content Migration

#### Phase 6: JSON Seed Data Migration
- **Seed data now JSON-based**: All initial game content now loaded from editable JSON files in `data/seeds/`
  - `npcs.json`: 5 starter NPCs (Mayor Thompson, City Clerk, Gate Guard, Market Vendor, Museum Curator)
  - `companions.json`: 3 companions (Gentle Mare, Loyal Hound, Shadow Cat)
  - `rooms.json`: 24 world rooms (Old Towne Mesh complete world map)
  - `achievements.json`: 17 achievements across 6 categories
  - `quests.json`: 3 starter quests (Welcome, Market Exploration, Network Explorer)
  - `recipes.json`: 2 crafting recipes (Signal Booster, Basic Antenna)

- **seed_loader module**: New `src/tmush/seed_loader.rs` with 6 loading functions
  - `load_npcs_from_json()`: Deserialize NPCs with dialogues and flags
  - `load_companions_from_json()`: Deserialize companions with types and locations
  - `load_rooms_from_json()`: Deserialize rooms with exits, flags, and capacity
  - `load_achievements_from_json()`: Deserialize achievements with triggers
  - `load_quests_from_json()`: Deserialize quests with objectives and rewards
  - `load_recipes_from_json()`: Deserialize crafting recipes with materials

- **Fallback to hardcoded seeds**: If JSON files are missing, automatically falls back to hardcoded Rust seed functions for backwards compatibility

- **Installation support**: `install.sh` now creates `data/seeds/` directory and copies all seed JSON files

- **Package distribution**: All 6 seed JSON files included in cargo-deb package assets

#### Benefits
- **Admins can customize content**: Edit seed JSON files to customize initial world without recompiling
- **Version control friendly**: Seed data changes visible in git diffs as JSON
- **Easier testing**: Create custom test worlds by modifying JSON files
- **Modding support**: Community can share custom seed content as JSON files
- **No compilation required**: Adjust NPC dialogue, room descriptions, achievement triggers without rebuilding

### Added - Data-Driven Achievement Management

#### Phase 1: Achievement System (@ACHIEVEMENT Command)
- **@ACHIEVEMENT command**: Complete admin interface for data-driven achievement management (admin level 2+ required)
  - `CREATE <id> <name>`: Create new achievements with default settings
  - `EDIT <id> DESCRIPTION <text>`: Set achievement description
  - `EDIT <id> CATEGORY <category>`: Set category (Combat, Exploration, Social, Economic, Quest, Special)
  - `EDIT <id> TRIGGER <type> <params>`: Configure trigger conditions (9 trigger types supported)
  - `EDIT <id> TITLE <text>`: Set optional title reward
  - `EDIT <id> HIDDEN <true|false>`: Toggle hidden achievement visibility
  - `DELETE <id>`: Remove achievement from database
  - `LIST [category]`: List all achievements or filter by category
  - `SHOW <id>`: Display detailed achievement information
  - Alias: `@ACH` for shorter command usage

#### Achievement Trigger Types (9 variants)
- **KILLCOUNT <required>**: Track enemy defeats
- **ROOMVISITS <required>**: Track room exploration
- **FRIENDCOUNT <required>**: Track social connections
- **MESSAGESSENT <required>**: Track message activity
- **TRADECOUNT <required>**: Track trading activity
- **CURRENCYEARNED <amount>**: Track currency accumulation
- **QUESTCOMPLETION <required>**: Track quest completions
- **VISITLOCATION <room_id>**: Require visiting specific location
- **COMPLETEQUEST <quest_id>**: Require completing specific quest

#### Storage Layer Enhancements
- `achievement_exists()`: Fast existence check for achievements
- Achievement CRUD operations integrated with TinyMushStore
- Full support for all 6 achievement categories and 9 trigger types

#### Testing
- **7 new integration tests** in `tests/achievement_management.rs`:
  - CRUD operations validation
  - All field editing (description, category, trigger, title, hidden)
  - All 9 trigger type parsing and validation
  - Category validation for all 6 categories
  - Existence checks and error handling
  - List filtering by category
  - Seeded starter achievements verification
- **608 total tests passing** (including new achievement tests)

#### Phase 2: NPC System (@NPC Command)
- **@NPC command**: Complete admin interface for data-driven NPC management (admin level 2+ required)
  - `CREATE <id> <name>`: Create new NPCs with default settings (spawns in starting_room)
  - `EDIT <id> NAME <text>`: Update NPC display name
  - `EDIT <id> TITLE <text>`: Set NPC title/role
  - `EDIT <id> DESCRIPTION <text>`: Set NPC description
  - `EDIT <id> ROOM <room_id>`: Move NPC to different room
  - `EDIT <id> DIALOG <key> <response>`: Add dialogue response (key->response mapping)
  - `EDIT <id> FLAG <flag>`: Add NPC behavioral flag (5 flag types)
  - `DELETE <id>`: Remove NPC from database
  - `LIST`: List all NPCs with room location and flags
  - `SHOW <id>`: Display detailed NPC information including all dialogue

#### NPC Flag Types (5 variants)
- **TutorialNpc**: Provides new player guidance
- **QuestGiver**: Can offer quests to players
- **Vendor**: Can engage in trading
- **Guard**: Special patrol/security behavior
- **Immortal**: Cannot be removed or modified by standard means

#### Storage Layer Enhancements
- `npc_exists()`: Fast existence check for NPCs
- `companion_exists()`: Fast existence check for companions
- NPC CRUD operations integrated with TinyMushStore
- Companion CRUD operations integrated with TinyMushStore
- Support for dialogue HashMap and future dialog_tree system

#### Testing
- **7 new integration tests** in `tests/npc_management.rs`:
  - CRUD operations validation
  - All field editing (name, title, description, room, dialog)
  - Dialogue management with multiple key->response pairs
  - All 5 flag type management
  - Multiple flags per NPC
  - Existence checks and error handling
  - Seeded starter NPCs verification (5 NPCs: mayor_thompson, city_clerk, gate_guard, market_vendor, museum_curator)
- **7 new integration tests** in `tests/companion_management.rs`:
  - CRUD operations validation
  - All field editing (name, description, type, room)
  - All 6 CompanionType variants (Horse, Dog, Cat, Familiar, Mercenary, Construct)
  - All 7 CompanionBehavior types (AutoFollow, AlertDanger, ExtraStorage, CombatAssist, Healing, SkillBoost, IdleChatter)
  - Complex behavior management with parameters
  - Multiple behaviors per companion
  - Existence checks and error handling
  - Seeded starter companions verification (3 companions: gentle_mare, loyal_hound, shadow_cat)
- **622 total tests passing** (including 14 new NPC+Companion tests)

#### Phase 3: Companion System (@COMPANION Command)
- **@COMPANION command**: Complete admin interface for data-driven companion management (admin level 2+ required)
  - `CREATE <id> <name>`: Create new companions with default type (Dog) and default room (starting_room)
  - `EDIT <id> NAME <text>`: Update companion display name
  - `EDIT <id> DESCRIPTION <text>`: Set companion description
  - `EDIT <id> TYPE <type>`: Set companion type (6 types)
  - `EDIT <id> ROOM <room_id>`: Move companion to different room
  - `EDIT <id> BEHAVIOR <behavior> [params]`: Add companion behavior (7 behavior types with parameters)
  - `DELETE <id>`: Remove companion from database
  - `LIST`: List all companions with type, room location, and owner
  - `SHOW <id>`: Display detailed companion information including all behaviors and inventory
  - Aliases: `@COMPANIONS`, `@PET` for alternate command usage

#### Companion Types (6 variants)
- **Horse**: Mount with extra storage capacity, auto-follows owner
- **Dog**: Loyal follower with danger alerts and idle chatter
- **Cat**: Independent companion with idle chatter behaviors
- **Familiar**: Magical companion with skill boosts and auto-follow
- **Mercenary**: Combat assistant with damage bonuses
- **Construct**: Mechanical ally with storage and combat capabilities

#### Companion Behaviors (7 types with parameters)
- **AutoFollow**: Automatically follow owner between rooms
- **AlertDanger**: Warn player when danger is nearby
- **ExtraStorage <capacity>**: Provide additional inventory slots (e.g., saddlebags)
- **CombatAssist <damage_bonus>**: Add bonus damage in combat
- **Healing <heal_amount> <cooldown_seconds>**: Provide healing over time
- **SkillBoost <skill> <bonus>**: Boost specific skill (e.g., magic +10)
- **IdleChatter <message1> [message2...]**: Say ambient messages periodically

#### Phase 4: Room System (@ROOM Command)
- **@ROOM command**: Complete admin interface for data-driven room/location management (admin level 2+ required)
  - `CREATE <id> <name>`: Create new rooms with default world settings
  - `EDIT <id> NAME <text>`: Update room display name
  - `EDIT <id> SHORTDESC <text>`: Set short description (shown in exits/lists)
  - `EDIT <id> LONGDESC <text>`: Set long description (shown when entering room)
  - `EDIT <id> EXIT <direction> <destination_id>`: Add directional exit to another room (10 directions)
  - `EDIT <id> FLAG <flag>`: Add room behavioral flag (13 flag types)
  - `EDIT <id> CAPACITY <number>`: Set maximum occupancy (1-65535)
  - `DELETE <id>`: Remove room from database (with validation)
  - `LIST`: List all rooms with exit counts and active flags
  - `SHOW <id>`: Display detailed room information including all exits, flags, items, owner, visibility

#### Room Directions (10 variants)
- **Cardinal**: North, South, East, West
- **Vertical**: Up, Down
- **Diagonal**: Northeast, Northwest, Southeast, Southwest
- **Exit validation**: Destination room must exist before creating exit

#### Room Flags (13 variants)
- **Safe**: No combat allowed, safe zone for players
- **Dark**: Requires light source to navigate
- **Indoor**: Interior location (affects weather/time-of-day descriptions)
- **Shop**: Contains vendor NPCs or vending machines
- **QuestLocation**: Key location for quest objectives
- **PvpEnabled**: Player vs player combat allowed
- **PlayerCreated**: Built by players using housing system
- **Private**: Access restricted to owner and invited guests
- **Moderated**: Requires moderator approval for messages
- **Instanced**: Each player sees separate instance (e.g., personal landing room)
- **Crowded**: High traffic area with performance considerations
- **HousingOffice**: Location where players can purchase housing
- **NoTeleportOut**: Players cannot teleport/warp out of room

#### Storage Layer Enhancements
- `room_exists()`: Fast existence check for rooms using "rooms:world:{id}" key format
- `list_room_ids()`: Scan and return all room IDs from primary tree with "rooms:world:" prefix
- `delete_room()`: Remove room from database with flush (ensures persistence)
- Room CRUD operations use primary sled tree (not dedicated tree like NPCs/Companions)
- Support for bidirectional exit management with destination validation

#### Testing
- **7 new integration tests** in `tests/room_management.rs`:
  - CRUD operations validation (create, read, update, delete)
  - All field editing (name, short_desc, long_desc, capacity)
  - Exit management with 10 direction types and destination validation
  - Single and multiple exit handling
  - All 13 flag type management with validation
  - Multiple flags per room (combinations)
  - Existence checks and error handling
  - Seeded starter rooms verification (20+ rooms including gazebo_landing, town_square, all world locations)
- **629 total tests passing** (including 7 new room tests, 21 total from phases 2-4)

#### Phase 5: Object System (@OBJECT Command)
- **@OBJECT command**: Complete admin interface for data-driven object management (admin level 2+ required)
  - `CREATE <id> <name>`: Create new world objects with default properties
  - `EDIT <id> NAME <text>`: Update object display name
  - `EDIT <id> DESCRIPTION <text>`: Set object description
  - `EDIT <id> WEIGHT <number>`: Set weight in units (0-255)
  - `EDIT <id> VALUE <number>`: Set currency value in base units
  - `EDIT <id> FLAG <flag>`: Add object behavioral flag (12 flag types)
  - `EDIT <id> TAKEABLE <true|false>`: Toggle whether object can be picked up
  - `EDIT <id> USABLE <true|false>`: Toggle whether object can be used
  - `EDIT <id> LOCKED <true|false>`: Toggle lock status (prevents taking)
  - `DELETE <id>`: Remove world object from database
  - `LIST`: List all world objects with properties and flags
  - `SHOW <id>`: Display detailed object information including history
  - Aliases: `@OBJECTS`, `@OBJ` for alternate command usage

#### Object Flags (12 variants)
- **QuestItem**: Required for quest objectives
- **Consumable**: Single-use items (potions, food)
- **Equipment**: Can be equipped by players
- **KeyItem**: Important story or quest items
- **Container**: Can hold other objects
- **Magical**: Has magical properties or effects
- **Companion**: Pet or ally object
- **Clonable**: Players can create copies (opt-in)
- **Unique**: Cannot be cloned (artifacts, quest items)
- **NoValue**: Strip currency value to 0 on clone
- **NoCloneChildren**: Cannot clone if contains other objects
- **LightSource**: Provides illumination in dark rooms (torches, lanterns)

#### Storage Layer Enhancements
- `object_exists()`: Fast existence check using secondary index (oid:{id}) and fallback to "objects:world:{id}"
- `list_object_ids()`: Scan and return all world object IDs from objects tree with "objects:world:" prefix
- `delete_object_world()`: Remove world object with index cleanup and flush
- Object CRUD operations use dedicated objects sled tree with O(1) secondary index
- Support for both world-owned and player-owned objects with different key formats

#### Testing
- **7 new integration tests** in `tests/object_management.rs`:
  - CRUD operations validation (create, read, update, delete)
  - All field editing (name, description, weight, value, takeable, usable, locked)
  - All 12 flag type management with validation
  - Object flag combinations
  - Existence checks and error handling
  - List functionality with sorting validation
  - Seeded objects verification (37 objects present in initialized database)
- **636 total tests passing** (including 7 new object tests, 28 total from phases 2-5)

### Developer Notes
- **Pattern**: Achievement, NPC, Companion, Room, and Object systems follow proven @QUEST/@RECIPE command structure
- **Migration Status**: Phase 5 of 6-phase data-driven content migration (see TODO.md)
- **Next Steps**: Phase 6 (Polish & Documentation)
- **Implementation Notes**: Object system features include:
  - Secondary index for O(1) object lookups (oid:{id} → full key)
  - Support for world-owned vs player-owned objects with different key formats
  - Currency value using CurrencyAmount type (supports multi-tier and decimal)
  - Clone tracking with depth, source ID, and clone count fields
  - Ownership history for forensic tracking

## [1.0.115-beta] - 2025-10-14

### Added - Phase 4 Quest Content & Phase 5 Features

#### Quest System Enhancements
- **New ObjectiveType variants** for Phase 4 puzzle mechanics:
  - `ExamineSequence`: Track players examining objects in correct order (Phase 4.2 symbol puzzles)
  - `NavigateDarkRoom`: Require light sources for dark navigation (Phase 4.3)
  - `CraftItem`: Track crafting specific items (Phase 4.4)
  - `ObtainLightSource`: Require light source in inventory

#### Phase 4 Quests (4 new quests)
- **The Cipher** (difficulty 4): Examine seasonal symbols in correct sequence, rewards decoder lens
- **Into the Depths** (difficulty 4): Navigate dark caverns with light source, rewards ancient relay core
- **Master Artisan** (difficulty 5): Craft antenna, relay module, and advanced signal array, rewards master crafter badge
- **The Lost Artifact** (difficulty 5): Epic quest combining all Phase 4 mechanics - symbol sequences, dark navigation, and crafting special key

#### Phase 4 Rooms (7 new locations)
- **Cipher Chamber**: Ancient stone chamber with four seasonal glyphs
- **Deep Caverns Entrance**: Pitch-black entrance to underground caverns
- **Sunken Chamber**: Flooded chamber requiring light source navigation
- **Hidden Vault**: Secret vault containing ancient artifacts
- **Forgotten Ruins Entrance**: Ancient ruins with glyph pillars
- **Dark Passage**: Twisting passage through ruins requiring light
- **Artifact Chamber**: Inner sanctum with legendary communication device

#### Phase 4 Objects (20+ new objects)
- **Cipher Glyphs**: Spring, Summer, Autumn, Winter symbol tablets for puzzle quests
- **Ruins Glyphs**: Alpha, Beta, Gamma, Delta pillars for epic quest
- **Light Sources**: LED lantern, chemical glowstick (in addition to existing torch)
- **Crafting Materials**: Crystal oscillator, power cell, circuit board for advanced crafting

#### Phase 5: Reputation System
- **Faction tracking**: Players can build reputation (-100 to +100) with six factions:
  - Old Towne Citizens
  - Tinkers Guild
  - Wanderers League
  - Merchants Coalition
  - Scholars Circle
  - Underground Network
- **Reputation levels**: Hated, Hostile, Unfriendly, Neutral, Friendly, Honored, Revered
- **Faction benefits**: Each faction grants benefits at different reputation levels
- **PlayerRecord.faction_reputation**: HashMap tracking reputation with all factions
- **Helper methods**: `get_reputation()`, `add_reputation()`, `set_reputation()`, `get_reputation_level()`, `has_reputation_level()`

#### Integration Tests
- **phase_4_quest_integration.rs**: 9 comprehensive tests covering:
  - Cipher quest examine sequence objectives
  - Dark navigation quest light requirements
  - Crafting quest craft objectives
  - Epic quest combining all mechanics
  - Player symbol sequence tracking
  - Light source object flags
  - Reputation tracking and level calculation
  - Reputation bounds clamping

### Changed
- **Quest count**: Total quests increased from 7 to 11 (3 starter + 4 original content + 4 Phase 4 quests)
- **Test suite**: Updated quest count assertions in storage and integration tests

## [1.0.110-beta] - 2025-10-13

### Fixed
- **Tutorial Completion Flow**: Tutorial now requires explicit TALK interaction with Mayor Thompson to complete
  - Previously, tutorial auto-completed upon entering Mayor's Office room
  - Final tutorial step now shows hint to talk to the mayor instead of auto-advancing
  - Players must use `TALK MAYOR` command to complete the tutorial and receive rewards
- **Mayor Thompson Dialog**: Mayor Thompson's dialog tree is now accessible after tutorial completion
  - Removed blocking condition that prevented post-tutorial conversations
  - Mayor now offers branching dialog options for completed players
- **Borrow Checker Error**: Fixed compilation error in movement handler
  - Restructured destination resolution to occur before acquiring room manager
  - Eliminated mutable/immutable borrow conflict in `handle_move` function
- **Test Suite**: Fixed world_config test assertion to match actual default welcome message
  - Updated test to check for "Welcome to Old Towne Mesh" (mixed case) instead of all uppercase

### Changed
- **Code Quality**: Improved code formatting and readability across multiple files
  - Cleaned up formatting in commands.rs, storage.rs, state.rs, and server.rs
  - Better indentation and line breaks for complex method chains

### Added
- **Integration Tests**: Added comprehensive tests for tutorial completion behavior
  - Test for Mayor dialog availability after tutorial completion
  - Test verifying TALK requirement for final tutorial step
  - Ensures tutorial cannot be bypassed by simple room navigation

## [1.0.106] - 2025-10-13

### Added
- **Personal landing gazebo instances**: `TinyMushStore::ensure_personal_landing_room` now clones the landing template into per-player rooms that live exclusively in memory, giving each newcomer a private staging area.

### Changed
- **Movement & teleport resolution**: All movement paths (cardinal exits, teleport commands, tutorial warps) resolve landing template references to the caller's active instance and persist the new location before dispatching triggers.
- **Provisioning helpers**: Admin seeding, guest creation, and TinyMUSH initialization utilities guarantee every player record points at its personalized landing room.

### Fixed
- **Gazebo cleanup**: Leaving an instanced landing now clears its cache entry so temporary rooms vanish once players step into the wider world.

## [1.0.105-beta] - 2025-10-13

### Changed
- **Installer layout refresh**: `install.sh` now installs the `meshbbs` binary to `/opt/meshbbs/bin/` and helper scripts to `/opt/meshbbs/scripts/` for clearer separation and systemd compatibility. The generated service definition and summary output were updated to match the new structure.
- **TinyMUSH engine overhaul**: Refactored storage, builder, trigger, companion, and quest modules alongside extensive test updates to improve reliability, schema migrations, and future extensibility.

### Fixed
- **Admin seeding resilience**: TinyMUSH seeding now guarantees both the configured sysop account and the legacy `admin` handle receive proper privileges, preventing monitoring and prompt tests from regressing when historical data exists.
- **Game door prompts**: TinyHack and TinyMUSH sessions intentionally suppress BBS prompts while players are inside the games; prompts return immediately after exiting back to the main menu.

### Documentation
- Updated README release highlights and version badge to match 1.0.105-beta.

## [1.0.102-beta] - 2025-10-13

### Fixed
- **CRITICAL: Message Chunking**: Fixed large messages not being split properly
  - **Root Cause**: Mutable session borrow conflict prevented `send_session_message()` from accessing session
  - When session was mutably borrowed for command processing, subsequent call to get session for chunking failed
  - This caused `sessions.get(node_key)` to return `None`, falling through to non-chunked send
  - **Solution**: Explicitly drop mutable session borrow before calling `send_session_message()`
  - **Impact**: TinyMUSH welcome message (~810 bytes) now properly splits into 4 chunks under 230 byte limit
  - All messages exceeding Meshtastic 230 byte limit now chunk correctly
  - **Verified**: All 237 tests passing

### Added
- **Debug Logging**: Added logging to track message chunking behavior
  - Logs body length, budget, max size, prompt length, and session state
  - Warns when session not found (which triggers the bug)
  - Helps diagnose future chunking issues

## [1.0.101-beta] - 2025-10-12

### Added
- **New CLI Command**: `hash-password` - Non-interactive password hashing for scripts
  - Reads password from stdin
  - Outputs Argon2 hash for use in config files
  - Usage: `echo -n "password" | meshbbs hash-password`

### Changed
- **CLI Command Rename**: `smoke-test` → `check-device`
  - More descriptive name for device connectivity testing
  - Verifies Meshtastic device communication over serial
  - Updated documentation to reflect new command name

- **TinyMUSH Admin Synchronization**: Admin account now automatically uses BBS sysop username
  - TinyMUSH admin account is created using the `sysop` username from `config.toml`
  - Eliminates orphaned "admin" account that was previously unreachable
  - BBS sysop automatically receives TinyMUSH admin level 3 (Sysop) on first login
  - Breaking change: Existing TinyMUSH databases with "admin" account unaffected, but new installs will use configured sysop name
  - See `docs/administration/tinymush-admin-setup.md` for complete guide

### Fixed
- **Schema Alignment**: Corrected 6 demo trigger objects from schema v1 to v2
  - Objects now use `OBJECT_SCHEMA_VERSION` constant instead of hardcoded value
  - Prevents unnecessary migrations on fresh database initialization
  - Affected objects: healing_potion, ancient_key, mystery_box, quest_clue, teleport_stone, singing_mushroom
  
- **Constructor Future-Proofing**: Updated type constructors to use schema version constants
  - `NpcRecord::new()` now uses `NPC_SCHEMA_VERSION`
  - `QuestRecord::new()` now uses `QUEST_SCHEMA_VERSION`
  - `AchievementRecord::new()` now uses `ACHIEVEMENT_SCHEMA_VERSION`
  - Ensures automatic version updates when schema evolves

- **Build Configuration**: Removed archived binary reference from Cargo.toml
  - Removed `migrate_messages` binary entry (script was archived)
  - Fixes compilation error for archived script

- **Installation Script**: Fixed password hashing in `install.sh`
  - Now uses correct `hash-password` command instead of non-existent `--hash-password`
  - Properly generates Argon2 password hash during installation
  - Prevents `REPLACE_WITH_HASHED_PASSWORD` placeholder in config

- **Critical Config Bug**: Welcome section field names in `config.example.toml` and `install.sh`
  - **Wrong field names** that would cause config parsing to use defaults instead of user settings
  - Fixed `private_enabled` → `private_guide`
  - Fixed `public_enabled` → `public_greeting`
  - Fixed `rate_limit_minutes` → `cooldown_minutes`
  - Removed non-existent `private_template` and `public_template` fields
  - Added missing `max_welcomes_per_node` field
  - **Impact**: Users who configured welcome messages would have had their settings silently ignored

- **Critical Config Bug**: Invalid `[storage.backup]` section removed from config files
  - The `StorageConfig` struct only supports `data_dir` and `max_message_size`
  - Backup system uses separate JSON config (`data/backup_scheduler.json`), not config.toml
  - Backup configuration managed via in-game admin commands
  - Removed invalid fields: `enabled`, `retention_days`, `interval_hours` from both config.example.toml and install.sh
  - Added note explaining backup configuration location
  - **Impact**: Users who tried to configure backups via config.toml had settings completely ignored

- **Critical Config Bug**: TinyMUSH database path field name incorrect
  - Fixed `[games.tinymush]` section with `data_dir` field (invalid subsection)
  - Changed to optional `tinymush_db_path` field directly in `[games]` section
  - Correct usage: `tinymush_db_path = "./data/tinymush"` (commented by default)
  - Updated both config.example.toml and install.sh
  - **Impact**: Database path overrides were silently ignored due to wrong field name

### Deprecated
- **CLI Command Removed**: `meshbbs init` command has been fully removed
  - Use `install.sh` for automated setup on Linux/Raspberry Pi
  - Use `cp config.example.toml config.toml` for manual setup
  - Updated all documentation to reflect removal

### Documentation
- Updated README.md with Rust installation instructions
- Added links to official Rust installation guide (rust-lang.org)
- Updated Quick Start guide to reference `install.sh` instead of deprecated `init`
- Fixed all documentation references to removed `meshbbs init` command
- Added `hash-password` command to CLI reference
- Updated command descriptions for clarity

## [1.0.100-beta] - 2025-10-12

### 🎮 Major: TinyMUSH Game Engine - Production Ready

#### Added
- **Complete MUD/MUSH Engine**: Fully functional text-based adventure game
  - 20+ interactive rooms with dynamic descriptions
  - Room navigation system with exits and movement
  - Interactive world with persistent state
  - Complete room builder commands for world creation

- **Interactive NPC System**:
  - 5 unique NPCs with distinct personalities (Mayor Thompson, City Clerk, Gate Guard, Market Vendor, Museum Curator)
  - Multi-topic dialogue trees with branching conversations
  - Quest integration with NPCs as quest givers
  - Dynamic NPC interactions based on player state
  - NPC dialogue editor for administrators

- **Player Progression System**:
  - Complete inventory management (take, drop, examine, use items)
  - Skills system with experience and leveling
  - Achievement tracking with 20+ achievements
  - Quest system with multi-stage objectives
  - Title system with unlockable titles
  - Player statistics and progression tracking

- **Advanced Trigger System**:
  - 14 action types: say, emote, teleport, damage, heal, grant_item, remove_item, add_quest, complete_quest, unlock_achievement, change_state, random_response, sequence, conditional
  - Conditional logic with comparisons (eq, ne, gt, lt, gte, lte, contains)
  - Rate limiting to prevent spam
  - Trigger chains and sequences
  - Room-based and item-based triggers
  - Complete trigger documentation (73 pages)

- **Economy & Trading**:
  - Currency system (gold coins)
  - 5+ shops with dynamic inventories
  - Buy/sell/trade mechanics
  - Price negotiation system
  - Shop management for administrators
  - Economic balance testing

- **5 Interactive Games**:
  - 8ball: Magic 8-ball fortune telling
  - Fortune: Daily fortune cookies
  - Slots: Casino slot machine
  - TinyHack: Dungeon crawler mini-game
  - TinyMUSH: Complete MUD/MUSH experience

- **Production Infrastructure**:
  - Automated installation script (install.sh) with interactive setup
  - Automated uninstall script (uninstall.sh) with backup option
  - Systemd service configuration
  - Raspberry Pi deployment guide with dependency fixes
  - Backup system with automated retention policies
  - Configuration security hardening (no default passwords)
  - Complete API documentation (73 markdown files)

#### Security
- Zero critical vulnerabilities (cargo audit clean)
- Password hashing with Argon2
- Interactive password setup during installation
- No default passwords in repository
- Secure configuration defaults (public_login=false)
- API key placeholders to prevent exposure

#### Performance
- 458 database operations per second
- 14.58 concurrent users per second
- Release build 12x faster than debug
- Optimized backup ID generation (nanosecond precision)
- Async database operations via spawn_blocking

#### Testing
- 237 tests passing (100% success rate)
- 98% code coverage for TinyMUSH engine
- Integration tests for all major features
- Performance testing completed
- Security audit completed

#### Documentation
- 73 markdown documentation files
- Complete user guide for TinyMUSH
- Administrator's guide with all commands
- Trigger engine guide with examples
- API reference documentation
- Raspberry Pi setup guide
- Tutorial walkthrough
- Installation documentation

#### Fixed
- Backup ID collision bug (millisecond → nanosecond precision)
- Configuration structure (password_hash in correct section)
- Test data cleanup (625+ files removed)
- UTF-8 truncation in message chunking

#### Removed
- 625+ test-generated files from git tracking
- Old backup directories (data/users.old, data/messages.old)
- One-time migration scripts (moved to archive)
- Temporary diagnostic scripts

## [1.0.65-beta] - 2025-10-05

### Added
- **Production Daemon Mode**: Fully functional daemon mode with custom implementation
  - `--daemon` flag for background operation (Linux/macOS)
  - `--pid-file <path>` for custom PID file location
  - Clean process forking and terminal detachment
  - Automatic log file redirection
  - Management script at `scripts/meshbbs-daemon.sh` with start/stop/restart/status/logs commands

### Fixed
- **TTY-Aware Logging**: Eliminated duplicate log lines in daemon mode
  - Added `atty` crate for TTY detection
  - Daemon mode: logs written to file only (single copy)
  - Foreground mode: logs written to both file and console
  - Automatic behavior based on stdout TTY status

### Changed
- **Dependency Cleanup**: Removed 5 unused crates for smaller binary and faster builds
  - Removed `getrandom` (replaced with `rand::thread_rng()` in migration script)
  - Removed `unsigned-varint` (not used anywhere)
  - Removed `daemonize` (custom implementation instead)
  - Removed `axum` and `tower` (web feature not implemented)
  - Result: 220 fewer lines in Cargo.lock
- **Daemon Feature**: Now included in default features for production deployments
- **Cross-Platform Graceful Shutdown**: Enhanced signal handling
  - Unix: SIGTERM, SIGHUP, SIGINT (Ctrl+C)
  - Windows: Ctrl+C, Ctrl+Break
  - All signals trigger same shutdown sequence with proper cleanup

## [1.0.61-beta] - 2025-01-06

### Fixed
- **Daemon Mode Logging**: Eliminated duplicate log lines in daemon mode
  - Added TTY detection using `atty` crate
  - Daemon mode: logs written to file only (single copy)
  - Foreground mode: logs written to both file and console
  - Automatic behavior based on stdout TTY status
  - Validated both operational modes working correctly

## [1.0.60-beta] - 2025-01-06

### Added
- **Cross-Platform Graceful Shutdown**: Production-grade signal handling across all platforms
  - Unix (Linux/macOS): `SIGTERM`, `SIGHUP`, `SIGINT` (Ctrl+C)
  - Windows: `Ctrl+C`, `Ctrl+Break`
  - All signals trigger same shutdown sequence:
    1. Close all active user sessions with proper cleanup
    2. Notify reader/writer tasks to terminate
    3. Disconnect Meshtastic device cleanly
    4. Flush all pending writes
    5. Exit with return code 0
  - Made `Server::shutdown()` public API for programmatic shutdown
  - Full systemd/launchd/Windows Service compatibility

- **Optional Daemon Mode** (Linux/macOS only, requires `--features daemon`):
  - `--daemon` flag: Run as background process
  - `--pid-file <path>` flag: Custom PID file location (default: `/tmp/meshbbs.pid`)
  - Process forking and terminal detachment
  - Automatic log file redirection (respects `config.toml` logging settings)
  - PID file management with proper cleanup
  - Restrictive umask (0o027) for security
  - Example: `meshbbs start --daemon --pid-file /var/run/meshbbs.pid`

- **Management Script** (`scripts/meshbbs-daemon.sh`):
  - Commands: `start`, `stop`, `restart`, `status`, `logs [lines]`
  - Environment variables for configuration:
    - `MESHBBS_BIN`: Path to binary (default: `./target/release/meshbbs`)
    - `MESHBBS_CONFIG`: Path to config file (default: `config.toml`)
    - `MESHBBS_PID_FILE`: PID file location (default: `/tmp/meshbbs.pid`)
    - `MESHBBS_PORT`: Serial port override
  - Cross-platform (bash on Linux/macOS)
  - Graceful shutdown with SIGTERM, fallback to SIGKILL after 10s
  - Example: `./scripts/meshbbs-daemon.sh start`

- **Comprehensive Documentation** (`docs/administration/daemon-mode.md`):
  - Complete setup guide for Linux, macOS, and Windows
  - systemd service file template with security hardening
  - launchd plist templates (system and user level)
  - Windows service setup using NSSM
  - Signal handling reference
  - Troubleshooting guide
  - Security best practices

### Changed
- `Server::shutdown()` visibility changed from private to public
  - Now part of public API for programmatic server lifecycle management
  - Fully documented with usage examples

### Technical Notes
- **Dependencies Added**:
  - `daemonize = "0.5"` (optional, Unix-only)
- **Features Added**:
  - `daemon = ["dep:daemonize"]` (not in default features)
- **Build Requirements**:
  - Standard builds: No changes, works on all platforms
  - Daemon support: `cargo build --features daemon` (Linux/macOS only)
- **Platform Support**:
  - Graceful shutdown: Linux, macOS, Windows
  - Daemon mode: Linux, macOS (Windows can use NSSM for service mode)
- **Backward Compatibility**: 
  - No breaking changes
  - Default behavior unchanged (foreground mode)
  - Existing deployments continue to work

### Implementation Details
- Signal handlers use Tokio async signal API (`tokio::signal`)
- Platform-specific code isolated with `#[cfg(unix)]` and `#[cfg(windows)]`
- Signal handling integrated into main event loop via `tokio::select!`
- Async blocks with conditional compilation to support cfg inside select! branches
- All 51 unit tests passing, no regressions

## [1.0.60-beta] - 2025-10-05

### Changed
- **Welcome Queue Rate Limiting**: Optimized startup welcome processing
  - Startup queue welcomes now send every 30 seconds (previously throttled by 5-minute cooldown)
  - Real-time node detections (NODEINFO, public messages) still rate-limited at 5 minutes to prevent spam
  - Added `is_from_startup_queue` field to `NodeDetectionEvent` to distinguish detection sources
  - Modified `should_welcome()` to accept `skip_rate_limit` parameter for queue items
  - **Impact**: 17-node startup queue now processes in ~8.5 minutes instead of ~85 minutes
  - Queue monitoring: `data/welcome_queue.json` shows real-time countdown and queue length

### Technical Details
- Rate limiting now bifurcated based on detection source:
  - Queue items: Created during startup scan with `is_from_startup_queue=true`, bypass global cooldown
  - Spontaneous detections: NODEINFO packets and public messages set `is_from_startup_queue=false`, enforce 5-minute cooldown
- All detection code paths updated to explicitly set the queue flag
- Per-node welcome limits still enforced for all sources
- Backward compatible: No config changes required

## [1.0.55-beta] - 2025-10-04

### Added
- **Message Replication Infrastructure**: Foundation for future inter-BBS message distribution
  - `message_id`: 6-byte unique identifier (12-char hex string)
    - Format: 8 hex chars (4-byte timestamp) + 4 hex chars (2-byte random)
    - Example: "68e16405c370"
    - Provides natural temporal ordering and collision resistance
  - `crc16`: CRC-16-IBM-SDLC checksum for message integrity verification
    - Calculated over topic, author, content, and timestamp
    - Example: 41846
  - Backward compatibility: Both fields use `Option<>` with `skip_serializing_if`
  - Old messages without new fields continue to work seamlessly
  
- **Message Migration Tool**: Standalone binary for updating existing messages
  - Binary: `migrate_messages` (scripts/migrate_messages.rs)
  - Usage: `./target/release/migrate_messages /path/to/data`
  - Features:
    - Scans all JSON files in messages/* subdirectories
    - Generates unique message_id for each message
    - Calculates CRC-16 checksum for integrity
    - Skips already-migrated messages (idempotent)
    - Provides detailed migration statistics
    - Safe to run multiple times
  - Successfully migrated 4 production messages in /opt/meshbbs/data

### Changed
- Message struct now includes optional `message_id` and `crc16` fields
- `store_message()` automatically populates message_id and crc16 for new messages
- Added `getrandom` dependency for secure random number generation

### Technical Details
- New helper functions in `src/storage/mod.rs`:
  - `generate_message_id()`: Creates 6-byte ID from timestamp + random
  - `calculate_message_crc()`: Computes CRC-16 over message data
- CRC algorithm: CRC-16-IBM-SDLC (polynomial 0x1021, aka CRC-16-CCITT)
- 8-byte overhead per message (6-byte ID + 2-byte CRC)
- Message ID uses `SystemTime::now()` for timestamp component
- Random component uses `getrandom` for cryptographic randomness

### Testing
- New test suite: `tests/message_id_crc.rs` (155 lines)
  - `test_message_id_and_crc_generation`: Basic generation and format validation
  - `test_message_id_uniqueness`: Verifies 10 messages have unique IDs
  - `test_crc_different_for_different_content`: CRC variance validation
  - `test_backward_compatibility_with_old_messages`: JSON without new fields loads correctly
  - `test_message_id_format`: Timestamp and format verification
- All tests pass with 100% success rate
- Migration tool tested on production data with 4 messages

## [1.0.50-beta] - 2025-10-04

### Added
- **Welcome System**: Automatic onboarding for new mesh users
  - Detects default "Meshtastic XXXX" node names and sends friendly welcome messages
  - Private DM with setup instructions: CONFIG → USER → "Long Name" path
  - Fun personalized name suggestions: Adjective + Animal + Emoji (e.g., "🦊 Clever Fox")
  - 50 adjectives × 50 animals = 2,500 possible combinations
  - Full emoji support: 🦊 Fox, 🐻 Bear, 🦅 Eagle, 🦁 Lion, 🐼 Panda, and 45 more
  - Public mesh greeting broadcasts to announce new users
  - Persistent state tracking in `data/welcomed_nodes.json`
  - Rate limiting: 5-minute global cooldown + per-node max count
  - Configurable via `[welcome]` section: enabled, private_guide, public_greeting, cooldown_minutes, max_welcomes_per_node
  
- **Reliable Ping System**: TEXT_MESSAGE_APP with routing ACK verification
  - Verifies node reachability before sending welcome messages
  - Uses single "." character payload with `want_ack=true`
  - Leverages proven routing ACK system (not application-level replies)
  - 120-second timeout for slow mesh routing (2 minutes)
  - Tracks pending pings: `packet_id → (node_id, response_channel)`
  - Success: ACK received → send welcome
  - Failure: NoResponse/timeout → skip welcome (node unreachable)
  
- **Node Detection Pipeline**: Automatic discovery and welcome queueing
  - Reader task emits `NodeDetectionEvent` for NODEINFO packets
  - Server queues startup welcomes for recently active unwelcomed nodes
  - Integrates with existing node cache (`data/node_cache.json`)
  - Respects welcome configuration and rate limits

### Changed
- **Ping Implementation Evolution**:
  - Attempt 1: REPLY_APP (port 32) → Failed: ReplyModule commented out in Meshtastic firmware by default
  - Attempt 2: POSITION_APP (port 3) → Failed: Requires GPS/fixed position, many nodes return NoResponse
  - Attempt 3: TEXT_MESSAGE_APP (port 1) → **SUCCESS**: Always enabled, reliable routing ACKs, no dependencies
- AckReceived handler now checks `pending_pings` before `pending` messages
- RoutingError handler removes failed pings from tracking and notifies waiters
- Welcome messages chunked to 200-byte segments with 5-second delays
- 11-second gap between private DM completion and public greeting

### Technical Details
- New module: `src/bbs/welcome.rs` (430 lines)
  - `WelcomeState`: Persistent tracking with disk serialization
  - `is_default_name()`: Detects "Meshtastic XXXX" pattern (4 hex digits)
  - `generate_callsign()`: Returns (name, emoji) tuple
  - `get_animal_emoji()`: Maps 50 animals to Unicode emojis
  - `private_guide()`: Accepts `cmd_prefix` for correct !HELP or ^HELP
- Control message: `SendPing { to, channel, response_tx }`
- Writer methods: `send_ping_packet()` sends TEXT_MESSAGE_APP with want_ack
- Server: `handle_node_detection()` implements full welcome workflow
- Configuration: `public_command_prefix` used to show correct help command

### Testing
- New test suite: `tests/node_welcome_integration.rs` (522 lines)
  - `welcome_system_end_to_end`: Full flow from detection to welcome
  - `welcome_detects_default_names`: Pattern matching validation
  - `welcome_rate_limiting`: Global cooldown enforcement
  - `welcome_per_node_limit`: Max welcomes per node
  - `welcome_persistence`: State survives restarts
  - `welcome_state_thread_safety`: Concurrent access safety
  - `callsign_generation_variety`: Ensures diverse suggestions
  - `welcome_message_formatting`: Validates DM and greeting format
  - `config_prefix_in_welcome`: Correct prefix in help instructions
- Real-world validation: Node 0x433AF828 successfully pinged and welcomed (3-second ACK)

### Fixed
- Removed old POSITION_APP ping handler (no longer sends PingReply)
- Simplified PingReply control message handler (marked as unused)
- Fixed test warnings in `tests/tinyhack_minimap.rs` (removed unnecessary `mut`)

## [1.0.45-beta] - 2025-10-02

### Changed
- Replaced every em-dash with a plain hyphen after the punctuation union submitted a buffet bill for all those extra bytes—hyphen is now our cost-cutting consultant.

### Fixed
- TinyHack minimap tests no longer shout their internal monologue during CI; the southbound trip is silent but successful.

## [1.0.44-beta] - 2025-10-03

### Added
- **TinyHack mini-map feature** with fog of war exploration tracking
  - New **M** command displays compact ASCII mini-map (~165 chars, fits in Meshtastic 230-char limit)
  - 6×6 grid showing player position (@), unexplored fog (#), and room types
  - Room symbols: M=monster, X=dead, C=chest, D=door, V=vendor, S=stairs, F=fountain, H=shrine, T=trap
  - Visited rooms tracking persists across game sessions
  - Non-action command (doesn't advance turn counter like I and ?)
  - Backward compatible: old saves initialize visited vector on load

### Changed
- Updated TinyHack help text to document M (map) command
- Added M to compute_options() so it appears in available command list
- GameState now tracks visited rooms with Vec<bool> field (default empty for backward compat)

### Technical
- New `render_map()` function generates compact grid view
- `do_move()` marks rooms as visited when player enters
- `load_or_new_with_flag()` initializes visited vector for old saves
- Added 3 comprehensive tests in `tests/tinyhack_minimap.rs`

## [1.0.43-beta] - 2025-10-03

### Fixed
- **CRITICAL**: UTF-8 boundary panic in message chunking that caused crashes when chunk boundaries landed in the middle of multi-byte UTF-8 characters
  - Symptom: `panic: byte index 215 is not a char boundary; it is inside '—'`
  - Root cause: Incomplete manual UTF-8 boundary checking in `src/bbs/server.rs`
  - Solution: Replaced manual byte checking with Rust's `str::is_char_boundary()` method
  - Impact: Prevented crashes during TinyHack gameplay and any messages with em-dashes, emoji, or other multi-byte characters

### Added
- Comprehensive regression test `tests/utf8_chunking_crash.rs` validating chunking behavior with:
  - Em-dashes (3-byte UTF-8 characters)
  - Emoji (4-byte UTF-8 characters)
  - Chinese characters
  - Cyrillic characters
  - Mixed multi-byte content

### Changed
- Updated 10 test files to include `allow_public_login` field for compatibility with 1.0.41 config changes

## [1.0.42-beta] - 2025-10-02

### Fixed
- Navigation menu consistency across Topics, Subtopics, and Threads views
- Menu footers now consistently display "B back. Q quit" instead of inconsistent "X exit" messaging
- Command handler behavior standardized: B always goes back to previous level/main menu, Q (and X as alias) always quits/logs out
- Improved user experience with clear, predictable navigation commands throughout the message board interface

### Changed
- Topics menu footer: Changed from "X exit" to "B back. Q quit" for clarity
- Subtopics menu footer: Changed from "X exit" to "B back. Q quit" for consistency
- Threads menu help text: Changed from "X exit" to "Q quit" to match standardized behavior
- Q command in Threads view now properly logs out instead of returning to main menu

## [1.0.41-beta] - 2025-10-02

### Added
- Configuration option `allow_public_login` in `[bbs]` section to control whether public channel LOGIN commands are accepted
- When set to `false`, users must initiate login via direct message only, enhancing security by preventing username enumeration
- Comprehensive documentation in configuration guide explaining security implications
- Unit test `public_login_disabled_by_config` validating the feature behavior

### Changed
- Public LOGIN command handler now respects `allow_public_login` configuration setting
- When disabled, public LOGIN attempts are silently ignored with trace logging (no response to prevent enumeration attacks)
- Direct message LOGIN continues to work regardless of this setting

### Documentation
- Added "BBS Settings" section to configuration guide documenting all core BBS configuration options
- Updated `config.example.toml` with `allow_public_login` field and explanatory comments
- Created `FEATURE_PUBLIC_LOGIN_CONFIG.md` with complete implementation details and usage examples

### Security
- Defaults to `true` for backward compatibility with existing configurations
- When disabled, prevents potential username enumeration attacks on public channels
- All authentication remains functional via direct messages

## [1.0.40-beta] - 2025-10-02

### Added
- TinyHack roguelike DM mini-game accessible from the main menu when enabled, with per-user saves stored under `data/tinyhack`.

### Changed
- Retired legacy long-form commands (READ, POST, TOPICS, LIST); compact letter/number shortcuts now drive the entire UI, and help output mirrors the streamlined prompts.
- Added succinct first-login hints in DM onboarding so new users immediately see `M=messages` / `H=help`, reinforcing the shortcut workflow.

### Documentation
- Updated the user guide, QA plan, and README to describe the compact command set and TinyHack entry point.

### Tests
- Refreshed help/menu integration tests and TinyHack coverage to guard the new navigation flow.

## [1.0.36-beta] - 2025-10-01

### Changed
- Docs only: Fix README Mermaid diagram parse error and sanitize section heading characters so the GitHub page renders correctly.

## [1.0.35] - 2025-09-30

### Changed
- Public command DM replies now honor the incoming event channel with fallback to the configured primary channel, reducing `NoChannel` routing errors.
- Startup now emits an INFO log showing the configured Meshtastic primary channel to aid diagnostics.
- NodeCache maintenance: hourly cleanup of entries not seen in more than 90 days; persist `last_seen` on observed node info updates. Atomic save and resilient load remain in effect.

### Notes
- Direct Message encryption requires the recipient's public key to be known by the radio. If you see `RoutingError: PkiUnknownPubkey`, ensure a key exchange occurs (e.g., an initial DM or presence exchange) so the radio learns the peer's key.

## [1.0.32] - 2025-09-29

### Changed
- Fortune module: replaced built‑in database with ~400 user‑provided fortunes (haiku, limericks, proverbs, jokes). Ensured every entry is <= 200 chars for mesh compatibility.
- Tests: made fortune tests count‑agnostic and focused on invariants (non‑empty, <=200 chars, printable, randomness variety).
- Docs: updated module docs and README release notes to reflect the new fortune set.

## [1.0.31] - 2025-09-29

### Changed
- Storage hardening: implement atomic write-then-rename with fsync for JSON writes across the storage layer (users, runtime topics, append logs), and add read-side resilience by stripping accidental leading NUL bytes before JSON parse.
- Sysop seeding: replace direct file writes with a centralized atomic helper (upsert_user_with_hash), ensuring the sysop record is created/updated atomically.
- Meshtastic NodeCache: atomic save (temp + fsync + rename + dir fsync) and resilient load (leading-NUL trim) to prevent partial write issues.

### Notes
- These changes reduce the risk of data corruption under crashes or power loss during writes, and guard reads against previously observed leading-NUL artifacts.

## [1.0.30] - 2025-09-29

### Fixed
- Slot machine jackpot persistence: occasional jackpot.json reset due to non-seeked overwrite. We now seek to start before truncate/write to ensure atomic overwrite semantics across platforms.

### Changed
- Tests: updated integration tests to use writable temp copies of fixtures (alice.json, carol.json, etc.) to avoid mutating tracked files during test runs.

## [1.0.25] - 2025-09-28

### Changed
- Runtime: Parameterized user-facing public messages to respect the configured public command prefix:
  - IDENT beacon hint now says: `Type <prefix>HELP for commands` (default `^`)
  - Public broadcasts for Slot Machine, Magic 8‑Ball, Fortune, and Slot Stats include the configured prefix
- Documentation: Updated README, user guides, getting-started, QA plan, and rustdoc to describe and demonstrate the configurable prefix (default `^`).

### Notes
- No functional behavior change aside from message formatting consistency.

## [1.0.22] - 2025-09-28

### Added
- Unit tests to verify UTF‑8 safe truncation behavior for log previews:
  - Ensures truncation never splits multi‑byte characters (em‑dash, emoji)
  - Guards against regression of the 1.0.21 fix

### Notes
- No functional changes beyond tests and release metadata.

## [1.0.21] - 2025-09-28

### Fixed
- Critical: Prevent panic when logging long message previews containing multibyte UTF-8 characters (e.g., em dash, emoji).
  - Replaced byte-slicing in log previews with a UTF-8 safe truncation helper.
  - Hardened parser slices in public command parser to avoid accidental mid-char slicing.
  - Observed crash looked like: `byte index N is not a char boundary; it is inside '—' ...`.

### Notes
- Recommended immediate update if you run public commands that may produce non-ASCII content (e.g., `^FORTUNE`).


## [1.0.21] - 2025-09-27

### Fixed
- Ident beacon could fire twice within the same scheduled minute (e.g., at :00 and :56) due to an elapsed-time-only duplicate guard.
  - Implemented robust per-minute boundary deduplication using epoch-minute key; guarantees at most one ident per boundary.

### Internal
- Restored test data files inadvertently modified by local runs.

---

## [1.0.18] - 2025-09-28

### Added
- Ident Beacon: New frequency option "5min" (in addition to 15min default, 30min, 1hour, 2hours, 4hours)

### Fixed
- Ident could stall in reader/writer mode when no device instance was present to report initialization.
  - Added a startup grace period (~120s) that allows the first ident after boot if no device is detected,
    while still waiting for `initial_sync_complete()` when a device is present.
  - Preserves UTC boundary scheduling and duplicate suppression semantics.

### Changed
- Improved rustdoc around ident scheduling and startup gating behavior.
- Updated configuration docs to include the new "5min" option and clarified behavior.

### Tests & Docs
- Expanded tests to cover 5‑minute frequency and boundary logic; cleaned warnings.
- Added/updated documentation under `docs/getting-started/configuration.md`.

---

## [1.0.17] - 2025-09-27

### Fixed
- **Ident Beacon Timing Issue**: Fixed critical timing bug where ident beacons were sent before radio initialization completed
  - Added proper check for `device.initial_sync_complete()` before sending ident beacons
  - Prevents beacons from transmitting during "Requesting initial config from radio" phase
  - Added debug logging to track initialization status
  - Ensures ident beacons only start after Meshtastic device is fully ready

### Technical
- Enhanced ident beacon logic with radio initialization state validation
- Improved test configuration compatibility
- Added comprehensive debug logging for initialization sequence

---

## [1.0.16] - 2025-01-28

### Added
- **OpenWeatherMap Integration**: Complete replacement of wttr.in weather service with OpenWeatherMap API
  - Support for city names, ZIP codes, and OpenWeatherMap city IDs
  - Configurable API key and cache TTL settings
  - Comprehensive error handling and graceful fallbacks
  - Real-time weather data with temperature, conditions, and humidity
- **Enhanced Weather Configuration**: New `[weather]` section in config with `api_key` and `cache_ttl_minutes` settings
- **Weather Service Testing**: Comprehensive test suite including integration tests with real API validation

### Changed
- **Weather Command Improvements**: Better error messages and more reliable weather data retrieval
- **Configuration Validation**: Enhanced validation for weather service configuration
- **Documentation Updates**: Updated README and configuration examples for OpenWeatherMap setup

### Technical
- Migrated from wttr.in HTTP scraping to OpenWeatherMap REST API
- Added proper HTTP client with timeout and retry logic
- Implemented structured weather data parsing and caching
- Enhanced test coverage for weather functionality

---

### Security
- API keys properly excluded from git repository via .gitignore
- Example files load configuration from config.toml instead of hard-coded values
- Secure credential management for production deployments

## [1.0.15-beta] - 2025-09-27

### Added
- **New Feature**: Configurable ident beacon system for periodic station identification
- Ident beacon broadcasts BBS name, node ID, timestamp, and help hint to public channel
- Configurable frequency options: 15min, 30min, 1hour, 2hours, 4hours (default: 15min)
- Enable/disable control via `[ident_beacon]` section in config.toml
- Smart time-boundary scheduling (beacons fire exactly at configured intervals)
- Fallback node ID handling when dynamic ID unavailable

### Enhanced
- Added comprehensive unit tests for ident beacon configuration and timing logic
- 14 new tests covering frequency validation, serialization, and integration
- Improved configuration validation with clear error messages for invalid frequencies

### Technical
- New `IdentBeaconConfig` struct with serde support for TOML configuration
- Time-based scheduling using chrono for UTC boundary calculations
- Integration with existing public channel broadcast system
- Backward-compatible configuration (defaults to enabled with 15min frequency)

## [1.0.13-beta] - 2025-01-27

### Fixed
- **Critical**: Fixed BBS becoming unresponsive after extended runtime (~1 hour+)
- Reader task could permanently exit on serial I/O errors, causing complete message processing failure
- Implemented resilient error handling: reader continues operating despite transient serial errors
- Added small delays (50-100ms) in error paths to prevent tight error loops
- Enhanced error logging with "continuing operation" messages for transparency

### Technical
- Changed fatal `return Err()` to warning logs in serial read error paths
- Reader task now only exits on shutdown signal or EINTR (controlled shutdown)
- Improved production stability for long-running deployments

## [1.0.12-beta] - 2025-09-26

### Fixed
- **Critical**: Public HELP messages (e.g., `<prefix>HELP`, default `^HELP`) are now chunked to stay under Meshtastic's 230-byte transmission limit
- Previous help message was 302 bytes, potentially causing transmission failures
- Help content now split into multiple chunks (220-byte safety margin): first chunk with main commands + "DM for BBS access", continuation chunks with "More:" header
- Each chunk sent with 2.5-second delays to respect rate limiting
- Added comprehensive test coverage for message chunking in `tests/help_message_chunking.rs`

### Technical
- Implemented intelligent command distribution across chunks to maximize information density
- Maintains existing scheduler integration and fallback paths
- Verified worst-case scenarios: long BBS names result in 217 + 185 byte chunks (well under limit)

## [1.0.11-beta] - 2025-09-26

### Added
- Enhanced public HELP command (`<prefix>HELP`, default `^HELP`) now broadcasts all available public commands to improve discoverability
- New behavior: public HELP sends BBS instructions via DM while broadcasting public commands list to channel
- Comprehensive test coverage for new help behavior in `tests/help_public_commands.rs`

### Changed  
- HELP command now shows: "Public Commands (for {user}): <prefix>HELP - Show this help | <prefix>LOGIN <user> - Register for BBS | <prefix>SLOT - Play slot machine | <prefix>SLOTSTATS - Show your stats | <prefix>8BALL - Magic 8-Ball oracle | <prefix>FORTUNE - Random wisdom | DM for BBS access" (default prefix `^`)
- Updated documentation in README.md and user guides to reflect improved command discoverability
- Enhanced help system description in README from "Compact HELP + verbose HELP+" to "^HELP broadcasts all public commands for discovery, with BBS instructions via DM"

### Fixed
- Resolved critical UX issue where mesh network users had no way to discover available public commands without prior knowledge

## [1.0.10-beta] - 2025-09-26

### Added
- Public command `<prefix>FORTUNE` (Fortune Cookies; default `^FORTUNE`): returns random wisdom from curated Unix fortune database entries. All entries under 200 characters for mesh-friendly transmission. Broadcast-only with 5-second per-node cooldown.
- Comprehensive unit test coverage for Fortune module (11 test functions covering database validation, functionality, thread safety, and content quality)
- Helper functions for Fortune module: `fortune_count()` and `max_fortune_length()` for diagnostics and testing
- Extensive rustdoc documentation for Fortune module with examples and thread safety notes
- Development guide for Fortune module at `docs/development/fortune-module.md` with architecture, maintenance, and troubleshooting information

## [1.0.9-beta] - 2025-09-26

### Added
- Public command `<prefix>8BALL` (Magic 8‑Ball; default `^8BALL`): returns one of 20 classic responses (emoji‑prefixed). Broadcast‑only with a lightweight per‑node cooldown like `<prefix>SLOT`.

### Changed
- Docs: README badge bumped to 1.0.9‑beta; user docs updated to include Magic 8‑Ball.

### Fixed
- Slot machine docs/tests alignment: `<prefix>SLOT` clarified as broadcast‑only with a behavior test to prevent regressions (default `^SLOT`).

## [1.0.8-beta] - 2025-09-26

### Changed
- Logging hygiene: demote noisy INFO logs to DEBUG in HELP flow, weather fetch/success, cache loads, serial open, reader/writer init, resend attempts, and per-message delivered ACK logs.

### Fixed
- Public `<prefix>SLOT` behavior corrected to be broadcast-only (no DM). `<prefix>SLOTSTATS` remains broadcast-first with DM fallback for reliability. (Default `^SLOT` / `^SLOTSTATS`.)

## [1.0.7] - 2025-09-26


- README and docs tweaks to clarify public broadcast ACK confirmation semantics and command examples. No functional code changes.

## [1.0.6] - 2025-09-26
## [1.0.20] - 2025-09-27

### Added
- Expanded rustdoc coverage across modules (`bbs/commands.rs`, `bbs/public.rs`, `bbs/roles.rs`, `meshtastic/framer.rs`, crate binary docs)
- New docs pages added to close gaps referenced by the docs index:
  - Getting Started: First Run
  - User Guide: Connecting, Message Topics, Troubleshooting
  - Administration: Setup, User Management, Moderation
  - Hardware: Device Compatibility, Device Setup
  - Development: Architecture, Building from Source
- Added `docs/index.md` and updated Jekyll config for better site navigation

### Changed
- README: added local API docs build instructions and ensured API docs link prominence
- Documentation links updated/fixed for GitHub Pages compatibility (absolute links to repository files where needed)
- Minor rustdoc warnings resolved (escaped angle brackets; relaxed HTML tag checks for generated protobuf docs)

### Tests & Build
- `cargo doc` passes with all features
- Full test suite green (`cargo test`)

Broadcast reliability telemetry and optional confirmation for public messages.

### Added
- Optional broadcast ACK request: broadcasts can set `want_ack`; if any node ACKs, we consider it a success ("at least one hop").
- Lightweight broadcast ACK tracking in the writer with a short TTL (no retries) to avoid ACK storms.
- Metrics: `broadcast_ack_confirmed` and `broadcast_ack_expired` counters for trend visibility.

### Changed
- Updated HELP/public broadcast paths to request ACKs for visibility (DMs remain reliable with retries/backoff).
- Documentation updates in `README.md` to describe broadcast confirmation semantics and new metrics.

### Notes
- Direct messages remain fully reliable with ACK tracking, retries, and latency metrics; broadcasts remain best‑effort but can now indicate basic delivery when at least one ACK is observed.

## [1.0.5] - 2025-09-25

Added a public‑channel slot machine mini‑game and related documentation updates.

### Added
- New public commands:
  - `<prefix>SLOT` / `<prefix>SLOTMACHINE` — spin the slot machine (5 coins per spin; daily refill to 100 when at 0) (default `^SLOT`)
  - `<prefix>SLOTSTATS` — show your coin balance, total spins, wins, and jackpots (default `^SLOTSTATS`)
- Persistent per‑player state under `data/slotmachine/players.json` with safe file locking
- Jackpot and stats tracking (total_spins, total_wins, jackpots, last_spin, last_jackpot)
- Runtime packaging skeleton includes `data/slotmachine/.keep`

### Changed
- Updated user documentation (`docs/user-guide/commands.md`) and `README.md` to include new commands and feature blurb
- Bumped crate version to 1.0.5

## [1.0.0 BETA] - 2025-09-25

This is the first public beta for the 1.x series of meshbbs. It stabilizes the core architecture and user experience for Meshtastic devices while inviting broader testing and feedback before the final 1.0.0 release.

What’s in this beta (high level):
- Compact DM‑first UI optimized for Meshtastic frame limits (≤230 bytes), with Topics → Subtopics → Threads → Read navigation, paging, filtering, and breadcrumbs
- Robust help system: single‑frame compact HELP and multi‑chunk verbose HELP+
- Role‑based permissions: users, moderators, and sysop; moderation tools (pin/unpin, rename, lock/unlock, delete with audit log)
- Persistence: JSON‑backed users, topics/subtopics, messages, and replies; state survives restarts
- Meshtastic integration: protobuf support, hop‑limit 3, paced writer with scheduler to minimize airtime contention; optional weather integration
- Security and safety: Argon2id password hashing, command audit logging, UTF‑8 safe clamping/chunking, strict prompt‑aware size enforcement

Known limitations (to be refined before final 1.0):
- Some admin/dashboards are minimal or placeholders
- Performance tuning and CI coverage are ongoing
- Weather relies on an external service and may be rate‑limited

Upgrade and compatibility notes:
- On‑disk data persists in `data/` (topics at `data/topics.json`, messages under `data/messages/`)
- No schema migration is required from recent 0.9 builds; however, regenerating `config.toml` and reviewing new pacing/security options is recommended

Feedback welcome: please report issues and suggestions so we can tighten the final 1.0 release.