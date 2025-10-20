//! TinyMUSH command parsing and session integration for MeshBBS.
//!
//! This module extends the BBS command processor to handle TinyMUSH-specific
//! verbs and routing. It integrates with the existing SessionState machine
//! and provides the bridge between mesh input and TinyMUSH world interactions.

use anyhow::Result;
use log::{debug, info};

use crate::bbs::session::{Session, SessionState};
use crate::config::Config;
use crate::logutil::escape_log;
use crate::metrics;
use crate::storage::Storage;
use crate::tmush::inventory::format_inventory_compact;
use crate::tmush::room_manager::RoomManager;
use crate::tmush::trigger::{
    execute_on_look, execute_on_poke, execute_on_use, execute_room_on_enter,
};
use crate::tmush::types::{
    AchievementCategory, AchievementRecord, AchievementTrigger, BulletinBoard, BulletinMessage,
    CurrencyAmount, Direction as TmushDirection, ItemStack, ObjectRecord, ObjectTrigger, RoomFlag,
    TutorialState, TutorialStep,
};
use crate::tmush::{PlayerRecord, TinyMushError, TinyMushStore};

/// TinyMUSH command categories for parsing and routing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TinyMushCommand {
    // Core navigation
    Look(Option<String>),  // L, L chest
    Move(Direction),       // N, S, E, W, U, D, NE, NW, SE, SW
    Where(Option<String>), // WHERE - show current location, WHERE player (admin) - locate player
    Map,                   // MAP - show area overview

    // Inventory and items
    Inventory,       // I - show inventory
    Take(String),    // T item - pick up item
    Drop(String),    // D item - drop item
    Use(String),     // U item - use/activate item
    Poke(String),    // POKE item - poke/prod an interactive object
    Examine(String), // X item - detailed examination
    Craft(String),   // CRAFT recipe - craft item from materials (Phase 4.4)

    // Economy and shops (Phase 5)
    Buy(String, Option<u32>),  // BUY item [quantity] - purchase from shop
    Sell(String, Option<u32>), // SELL item [quantity] - sell to shop
    List,                      // LIST/WARES - view shop inventory

    // Social interactions
    Say(String),             // SAY text - speak to room
    Whisper(String, String), // W player text - private message
    Emote(String),           // EMOTE action - perform emote
    Pose(String),            // POSE action - strike a pose
    Ooc(String),             // OOC text - out of character

    // Information
    Who,   // WHO - list online players
    Score, // SCORE - show player stats
    Time,  // TIME - show game time

    // Bulletin board commands (Phase 4 feature)
    Board(Option<String>), // BOARD, BOARD stump - view bulletin board
    Post(String, String),  // POST subject message - post to bulletin board
    Read(u64),             // READ 123 - read specific bulletin message

    // Mail system commands (Phase 4 feature)
    Mail(Option<String>),         // MAIL, MAIL inbox - view mail folder
    Send(String, String, String), // SEND player subject message - send mail
    ReadMail(u64),                // RMAIL 123 - read specific mail message
    DeleteMail(u64),              // DMAIL 123 - delete mail message

    // Banking commands (Phase 5 Week 4)
    Balance,                      // BALANCE - show pocket and bank balance
    Deposit(String),              // DEPOSIT amount - deposit currency to bank
    Withdraw(String),             // WITHDRAW amount - withdraw currency from bank
    BankTransfer(String, String), // BTRANSFER player amount - transfer to another player

    // Trading commands (Phase 5 Week 4)
    Trade(String), // TRADE player - initiate trade with player
    Offer(String), // OFFER item/amount - offer item or currency in active trade
    Accept,        // ACCEPT - accept trade
    Reject,        // REJECT - reject/cancel trade
    TradeHistory,  // THISTORY - view trade history

    // Tutorial & NPC commands (Phase 6 Week 1)
    Tutorial(Option<String>), // TUTORIAL, TUTORIAL SKIP, TUTORIAL RESTART - manage tutorial
    Talk(String, Option<String>), // TALK npc [topic] - interact with NPC, optionally specify topic
    Talked(Option<String>),   // TALKED [npc] - view conversation history with NPCs

    // Quest commands (Phase 6 Week 2)
    Quest(Option<String>), // QUEST, QUEST LIST, QUEST ACCEPT id - manage quests
    Abandon(String),       // ABANDON quest_id - abandon active quest

    // Achievement & Title commands (Phase 6 Week 3)
    Achievements(Option<String>), // ACHIEVEMENTS, ACHIEVEMENTS LIST, ACHIEVEMENTS EARNED - manage achievements
    Title(Option<String>),        // TITLE, TITLE LIST, TITLE EQUIP name - manage titles

    // Companion commands (Phase 6 Week 4)
    Companion(Option<String>), // COMPANION, COMPANION TAME/STAY/COME/INVENTORY/RELEASE - manage companions
    Feed(String),              // FEED horse - feed companion
    Pet(String),               // PET dog - interact with companion
    Mount(String),             // MOUNT horse - mount companion
    Dismount,                  // DISMOUNT - dismount from companion
    Train(String, String),     // TRAIN horse speed - train companion skill

    // Housing commands (Phase 7 Week 1-2)
    Housing(Option<String>), // HOUSING, HOUSING LIST, HOUSING INFO - manage housing
    Rent(String),            // RENT template_id - rent/purchase housing from template
    Home(Option<String>),    // HOME, HOME LIST, HOME <id>, HOME SET <id> - teleport to housing
    Invite(String),          // INVITE player - add guest to housing
    Uninvite(String),        // UNINVITE player - remove guest from housing
    Describe(Option<String>), // DESCRIBE <text> - edit current room description (housing only)
    // DESCRIBE - show current description and permissions
    Lock(Option<String>), // LOCK - lock current room, LOCK <item> - lock item (Phase 2)
    Unlock(Option<String>), // UNLOCK - unlock current room, UNLOCK <item> - unlock item (Phase 2)
    Kick(Option<String>), // KICK <player> - remove player from housing, KICK ALL (Phase 3)
    History(String),      // HISTORY <item> - view ownership audit trail (Phase 5)
    Reclaim(Option<String>), // RECLAIM - view reclaim box, RECLAIM <item> - retrieve item (Phase 6)

    /// Builder Commands (Phase 7 Week 3-4)
    ///
    /// These commands enable world building and modification:
    /// - `/DIG <direction> <room_name>`: Create a new room and link it in the specified direction
    /// - `/DESCRIBE <target> <text>`: Set description on room or object
    /// - `/LINK <direction> <destination>`: Create exit from current room
    /// - `/UNLINK <direction>`: Remove exit from current room
    /// - `/SETFLAG <target> <flag>`: Modify object or room flags
    /// - `/CREATE <object_name>`: Create new object in current room
    /// - `/DESTROY <object>`: Delete object (with safeguards)
    ///
    /// Permission requirements:
    /// - All commands require builder level 1+ (Apprentice or higher)
    /// - Some commands require higher levels (Architect for world structure changes)
    Dig(String, String), // /DIG <direction> <room_name> - create room and link (builder 1+)
    DescribeTarget(String, String), // /DESCRIBE <target> <text> - set description (builder 1+)
    Link(String, String),           // /LINK <direction> <destination> - create exit (builder 2+)
    Unlink(String),                 // /UNLINK <direction> - remove exit (builder 2+)
    SetFlag(String, String),        // /SETFLAG <target> <flag> - modify flags (builder 2+)
    Create(String),                 // /CREATE <object_name> - create object (builder 1+)
    Destroy(String),                // /DESTROY <object> - delete object (builder 3+)
    Clone(String),                  // /CLONE <object> - create copy of owned object (Phase 6)

    /// Builder permission management (Phase 7 Week 3)
    ///
    /// These commands manage builder privileges:
    /// - `@BUILDER`: Display builder status and level
    /// - `@SETBUILDER <player> <level>`: Grant builder privileges (level 0-3)
    /// - `@REMOVEBUILDER <player>`: Revoke builder privileges
    /// - `@BUILDERS`: List all builders
    ///
    /// Builder Levels:
    /// - Level 0: No builder permissions
    /// - Level 1: Apprentice - create objects, basic room editing
    /// - Level 2: Builder - create rooms, link exits, modify flags
    /// - Level 3: Architect - full world editing, deletion powers
    Builder, // @BUILDER - show builder status
    SetBuilder(String, u8), // @SETBUILDER player level - grant builder privileges (0-3)
    RemoveBuilder(String),  // @REMOVEBUILDER player - revoke builder privileges
    Builders,               // @BUILDERS - list all builders

    // System
    Help(Option<String>), // HELP, HELP topic
    Quit,                 // QUIT - leave TinyMUSH
    Save,                 // SAVE - force save player state

    // Meta/admin (future phases)
    Debug(String),                          // DEBUG - admin diagnostics
    SetConfig(String, String),              // @SETCONFIG field value - set world configuration
    GetConfig(Option<String>),              // @GETCONFIG [field] - view world configuration
    EditRoom(String, String), // @EDITROOM <room_id> <description> - edit any room description (admin only)
    EditNpc(String, String, String), // @EDITNPC <npc_id> <field> <value> - edit NPC properties (admin only)
    ListAbandoned, // @LISTABANDONED - view abandoned/at-risk housing (admin, Phase 7)
    Dialog(String, String, Option<String>), // @DIALOG <npc> <subcommand> [args] - manage NPC dialogue trees (admin only)
    Recipe(String, Vec<String>), // @RECIPE <subcommand> [args] - manage crafting recipes (admin only)
    QuestAdmin(String, Vec<String>), // @QUEST <subcommand> [args] - manage quests (admin only)
    AchievementAdmin(String, Vec<String>), // @ACHIEVEMENT <subcommand> [args] - manage achievements (admin only)
    NPCAdmin(String, Vec<String>),         // @NPC <subcommand> [args] - manage NPCs (admin only)
    CompanionAdmin(String, Vec<String>), // @COMPANION <subcommand> [args] - manage companions (admin only)
    RoomAdmin(String, Vec<String>),      // @ROOM <subcommand> [args] - manage rooms (admin only)
    ObjectAdmin(String, Vec<String>), // @OBJECT <subcommand> [args] - manage objects (admin only)

    /// Admin permission commands (Phase 9.2)
    ///
    /// These commands manage administrative privileges in TinyMUSH:
    /// - `Admin`: Display admin status, level, and available commands
    /// - `SetAdmin(username, level)`: Grant admin privileges (level 0-3: 0=none, 1=moderator, 2=admin, 3=sysop)
    /// - `RemoveAdmin(username)`: Revoke admin privileges
    /// - `Admins`: List all administrators (public command)
    ///
    /// Permission requirements:
    /// - `Admin` and `Admins`: Available to all players
    /// - `SetAdmin` and `RemoveAdmin`: Require admin level 2+
    ///
    /// See handler documentation for detailed usage and examples.
    Admin, // @ADMIN - show admin status
    SetAdmin(String, u8), // @SETADMIN player level - grant admin privileges (0-3)
    RemoveAdmin(String),  // @REMOVEADMIN / @REVOKEADMIN player - revoke admin privileges
    Admins,               // @ADMINS / @ADMINLIST - list all admins

    /// Player monitoring commands (Phase 9.3)
    ///
    /// These commands enable administrators to monitor and manage player activity:
    /// - `@PLAYERS`: List all players in TinyMUSH (online/offline status, location)
    /// - `@GOTO <target>`: Teleport admin to a player's location or specific room
    ///
    /// Permission requirements:
    /// - All commands require admin level 1+ (Moderator or higher)
    /// - Transparency: Actions are logged for accountability
    Players, // @PLAYERS - list all players with status and location
    Goto(String), // @GOTO <player|room> - teleport to player or room

    /// Item and currency management commands
    ///
    /// These commands enable administrators to grant items and currency to players:
    /// - `@GIVE <player> <object_id> [quantity]`: Grant items to player inventory
    /// - `@GIVE <player> CURRENCY <amount>`: Grant currency to player
    ///
    /// Permission requirements:
    /// - Requires admin level 2+ (Admin or higher)
    Give(String, Vec<String>), // @GIVE player object/currency [amount]

    /// Player teleportation and stat management commands
    ///
    /// These commands enable administrators to manage players:
    /// - `@TELEPORT <player> <room>`: Teleport another player to a room
    /// - `@STATS <player> EDIT <stat> <value>`: Modify player stats (HP, etc.)
    ///
    /// Permission requirements:
    /// - Requires admin level 2+ (Admin or higher)
    Teleport(String, String), // @TELEPORT player room - teleport player to room
    Stats(String, Vec<String>), // @STATS player edit/show [stat] [value]

    /// Clone monitoring commands (Phase 6 Admin Tools)
    ///
    /// These commands enable administrators to monitor cloning activity:
    /// - `@LISTCLONES [player]`: List all clones owned by a player with genealogy
    /// - `@CLONESTATS`: Server-wide cloning statistics (totals, top cloners, suspicious patterns)
    ///
    /// Permission requirements:
    /// - All commands require admin level 1+ (Moderator or higher)
    /// - Used for detecting clone abuse and quota violations
    ListClones(Option<String>), // @LISTCLONES [player] - list player's clones
    CloneStats, // @CLONESTATS - server-wide clone statistics

    /// World Event Commands (Phase 9.5)
    ///
    /// These commands enable administrators to manage world-wide events and migrations:
    /// - `@CONVERT_CURRENCY <decimal|multitier> [--dry-run]`: Convert all currency in the world
    ///   - Converts player wallets, bank accounts, item values, and shop inventories
    ///   - Use --dry-run flag to preview changes without applying them
    ///   - Logs all conversions for audit purposes
    ///   - Requires admin level 3 (sysop)
    ConvertCurrency(String, bool), // @CONVERT_CURRENCY <type> [--dry-run] - migrate all currency (sysop only)

    /// Backup & Recovery Commands (Phase 9.5)
    ///
    /// These commands enable administrators to backup and restore the world database:
    /// - `@BACKUP [name]`: Create a manual backup with optional name
    ///   - Creates compressed tar.gz archive with SHA256 checksum
    ///   - Manual backups are protected from automatic deletion
    ///   - Requires admin level 2+ (admin or sysop)
    /// - `@RESTORE <id>`: Restore world from backup ID
    ///   - Verifies backup integrity before restoration
    ///   - Requires confirmation and admin level 3 (sysop only)
    ///   - Server restart required after restore
    /// - `@LISTBACKUPS`: List all available backups with metadata
    ///   - Shows backup ID, name, date, size, type, and verification status
    ///   - Sorted by date (newest first)
    ///   - Requires admin level 2+
    /// - `@VERIFYBACKUP <id>`: Verify backup integrity via checksum
    ///   - Validates SHA256 checksum of backup archive
    ///   - Updates verification status in metadata
    ///   - Requires admin level 2+
    /// - `@DELETEBACKUP <id>`: Delete specific backup by ID
    ///   - Manual backups require confirmation
    ///   - Cannot delete only remaining backup
    ///   - Requires admin level 2+
    /// - `@BACKUPCONFIG [enable|disable|frequency|status]`: Configure automatic backups
    ///   - Enable/disable automatic backups
    ///   - Set backup frequency (hourly, 2h, 4h, 6h, 12h, daily)
    ///   - View current configuration
    ///   - Requires admin level 2+
    Backup(Option<String>), // @BACKUP [name] - create manual backup
    RestoreBackup(String), // @RESTORE <id> - restore from backup (sysop only)
    ListBackups,           // @LISTBACKUPS - list all backups
    VerifyBackup(String),  // @VERIFYBACKUP <id> - verify backup integrity
    DeleteBackup(String),  // @DELETEBACKUP <id> - delete specific backup
    BackupConfig(Vec<String>), // @BACKUPCONFIG [subcommand] - configure automatic backups

    // Unrecognized command
    Unknown(String),
}

/// Cardinal and intercardinal directions for movement
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Direction {
    North,
    South,
    East,
    West,
    Up,
    Down,
    Northeast,
    Northwest,
    Southeast,
    Southwest,
}

/// TinyMUSH session state and command processor
pub struct TinyMushProcessor {
    store: TinyMushStore,
    room_manager: Option<RoomManager>,
}

#[allow(unused_variables)] // Many config parameters reserved for future use
impl TinyMushProcessor {
    /// Create a new processor with a shared store instance.
    ///
    /// Using a shared store ensures all processors see consistent data without
    /// the multi-handle caching issues that occur when each processor opens
    /// its own Sled database handle.
    pub fn new(store: TinyMushStore) -> Self {
        Self {
            store,
            room_manager: None,
        }
    }

    /// Get the TinyMUSH store reference
    fn store(&self) -> &TinyMushStore {
        &self.store
    }

    /// Initialize or get the room manager for this session
    async fn get_room_manager(&mut self) -> Result<&mut RoomManager, TinyMushError> {
        if self.room_manager.is_none() {
            // Clone the store for the room manager
            // Cloning TinyMushStore is cheap - all internal Sled types are Arc-based
            let store = self.store.clone();

            debug!("Creating TinyMUSH room manager (using shared store)");
            let room_manager = RoomManager::new(store);
            self.room_manager = Some(room_manager);
        }

        Ok(self.room_manager.as_mut().unwrap())
    }

    /// Get world configuration
    async fn get_world_config(&self) -> Result<crate::tmush::types::WorldConfig, TinyMushError> {
        self.store().get_world_config()
    }

    /// Initialize a player when entering TinyMUSH and return welcome screen
    pub async fn initialize_player(
        &mut self,
        session: &mut Session,
        _storage: &mut Storage,
        _config: &Config,
    ) -> Result<String> {
        // Create or load player
        let mut player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Player initialization failed: {}", e)),
        };

        // Ensure onboarding steps begin in the Landing Gazebo
        let should_be_at_landing = matches!(
            player.tutorial_state,
            TutorialState::NotStarted
                | TutorialState::InProgress {
                    step: TutorialStep::WelcomeAtGazebo,
                }
        );

        if should_be_at_landing {
            match self.store().ensure_personal_landing_room(&player.username) {
                Ok(landing_id) => {
                    if player.current_room != landing_id {
                        player.current_room = landing_id.clone();
                        if let Err(e) = self.store().put_player(player.clone()) {
                            return Ok(format!("Player initialization failed: {}", e));
                        }
                    }
                }
                Err(e) => {
                    return Ok(format!("Player initialization failed: {}", e));
                }
            }
        }

        // Show welcome message - use tutorial welcome for new players
        let mut response = String::new();
        let is_new_player = crate::tmush::tutorial::should_auto_start_tutorial(&player);

        if is_new_player {
            // New player - start tutorial and show tutorial welcome
            use crate::tmush::tutorial::start_tutorial;
            match start_tutorial(self.store(), &player.username) {
                Ok(state) => {
                    player.tutorial_state = state;
                    if let Ok(updated_player) = self.store().get_player(&player.username) {
                        player = updated_player;
                    }
                }
                Err(e) => return Ok(format!("Tutorial start failed: {}", e)),
            }

            // Show tutorial welcome message from world config
            if let Ok(world_config) = self.store().get_world_config() {
                response.push_str(&world_config.welcome_message);
                response.push_str("\n\n");
            }
        } else {
            // Returning player - show standard welcome
            response.push_str("*** Welcome back to TinyMUSH! ***\n");
            response.push_str("Type HELP for commands, B or QUIT to exit.\n\n");
        }

        // Add initial room description
        if let Ok(room) = self.store().get_room(&player.current_room) {
            response.push_str(&format!("{}\n", room.name));
            response.push_str(&format!("{}\n", room.long_desc));

            // Show exits
            if !room.exits.is_empty() {
                response.push_str("Exits: ");
                let exit_names: Vec<String> = room
                    .exits
                    .keys()
                    .map(|dir| format!("{:?}", dir).to_lowercase())
                    .collect();
                response.push_str(&exit_names.join(", "));
                response.push('\n');
            }
        } else {
            response.push_str("You find yourself in a mysterious void...\n");
        }

        Ok(response)
    }

    /// Process a TinyMUSH command and return the response
    pub async fn process_command(
        &mut self,
        session: &mut Session,
        command: &str,
        _storage: &mut Storage,
        config: &Config,
    ) -> Result<String> {
        // Check if this is dialog input before parsing as a command
        // If player has an active dialog and enters a number or dialog keyword,
        // route it to the dialog handler
        if let Ok(username) = session.username.as_deref().ok_or("no username") {
            let trimmed = command.trim().to_uppercase();

            // First, check for disambiguation session (highest priority for numeric input)
            if let Ok(Some(disambiguation)) = self.store().get_disambiguation_session(username) {
                if let Ok(choice) = trimmed.parse::<usize>() {
                    // User selected a number - process the disambiguation
                    if let Some((object_id, _object_name)) = disambiguation.get_selection(choice) {
                        // Delete the disambiguation session
                        let _ = self.store().delete_disambiguation_session(username);

                        // Route back to the original command with the specific object ID
                        match disambiguation.command.as_str() {
                            "take" => {
                                // Execute take command with the selected object ID
                                return self.handle_take_by_id(session, object_id, config).await;
                            }
                            "drop" => {
                                return self.handle_drop_by_id(session, object_id, config).await;
                            }
                            "use" => {
                                return self.handle_use_by_id(session, object_id, config).await;
                            }
                            "examine" => {
                                return self.handle_examine_by_id(session, object_id, config).await;
                            }
                            _ => {
                                return Ok(format!(
                                    "Unknown disambiguation command: {}",
                                    disambiguation.command
                                ));
                            }
                        }
                    } else {
                        return Ok(format!(
                            "Invalid selection. Please choose a number between 1 and {}.",
                            disambiguation.matched_ids.len()
                        ));
                    }
                }
            }

            // Check if input looks like dialog navigation
            let is_dialog_input = trimmed.parse::<usize>().is_ok()
                || trimmed == "EXIT"
                || trimmed == "QUIT"
                || trimmed == "BYE"
                || trimmed == "BACK";

            if is_dialog_input {
                // Check if player has any active dialog sessions
                if let Ok(Some((npc_id, _))) = self.get_active_dialog_session(username) {
                    // Route to dialog handler with the NPC
                    return self
                        .handle_talk(session, npc_id, Some(trimmed), config)
                        .await;
                }
            }
        }

        let parsed_command = self.parse_command(command);
        debug!(
            "TinyMUSH command parsed: session={} command={:?}",
            escape_log(&session.id),
            parsed_command
        );

        match parsed_command {
            TinyMushCommand::Look(target) => self.handle_look(session, target, config).await,
            TinyMushCommand::Move(direction) => self.handle_move(session, direction, config).await,
            TinyMushCommand::Where(target_username) => {
                self.handle_where(session, target_username, config).await
            }
            TinyMushCommand::Map => self.handle_map(session, config).await,
            TinyMushCommand::Inventory => self.handle_inventory(session, config).await,
            TinyMushCommand::Take(item) => self.handle_take(session, item, config).await,
            TinyMushCommand::Drop(item) => self.handle_drop(session, item, config).await,
            TinyMushCommand::Use(item) => self.handle_use(session, item, config).await,
            TinyMushCommand::Poke(target) => self.handle_poke(session, target, config).await,
            TinyMushCommand::Examine(target) => self.handle_examine(session, target, config).await,
            TinyMushCommand::Craft(recipe) => self.handle_craft(session, recipe, config).await,
            TinyMushCommand::Buy(item, quantity) => {
                self.handle_buy(session, item, quantity, config).await
            }
            TinyMushCommand::Sell(item, quantity) => {
                self.handle_sell(session, item, quantity, config).await
            }
            TinyMushCommand::List => self.handle_list(session, config).await,
            TinyMushCommand::Who => self.handle_who(session, config).await,
            TinyMushCommand::Score => self.handle_score(session, config).await,
            TinyMushCommand::Say(text) => self.handle_say(session, text, config).await,
            TinyMushCommand::Whisper(target, text) => {
                self.handle_whisper(session, target, text, config).await
            }
            TinyMushCommand::Emote(text) => self.handle_emote(session, text, config).await,
            TinyMushCommand::Pose(text) => self.handle_pose(session, text, config).await,
            TinyMushCommand::Ooc(text) => self.handle_ooc(session, text, config).await,
            TinyMushCommand::Board(board_id) => self.handle_board(session, board_id, config).await,
            TinyMushCommand::Post(subject, message) => {
                self.handle_post(session, subject, message, config).await
            }
            TinyMushCommand::Read(message_id) => {
                self.handle_read(session, message_id, config).await
            }
            TinyMushCommand::Mail(folder) => self.handle_mail(session, folder, config).await,
            TinyMushCommand::Send(recipient, subject, message) => {
                self.handle_send(session, recipient, subject, message, config)
                    .await
            }
            TinyMushCommand::ReadMail(message_id) => {
                self.handle_read_mail(session, message_id, config).await
            }
            TinyMushCommand::DeleteMail(message_id) => {
                self.handle_delete_mail(session, message_id, config).await
            }
            TinyMushCommand::Balance => self.handle_balance(session, config).await,
            TinyMushCommand::Deposit(amount) => self.handle_deposit(session, amount, config).await,
            TinyMushCommand::Withdraw(amount) => {
                self.handle_withdraw(session, amount, config).await
            }
            TinyMushCommand::BankTransfer(recipient, amount) => {
                self.handle_bank_transfer(session, recipient, amount, config)
                    .await
            }
            TinyMushCommand::Trade(target) => self.handle_trade(session, target, config).await,
            TinyMushCommand::Offer(item) => self.handle_offer(session, item, config).await,
            TinyMushCommand::Accept => self.handle_accept(session, config).await,
            TinyMushCommand::Reject => self.handle_reject(session, config).await,
            TinyMushCommand::TradeHistory => self.handle_trade_history(session, config).await,
            TinyMushCommand::Tutorial(subcommand) => {
                self.handle_tutorial(session, subcommand, config).await
            }
            TinyMushCommand::Talk(npc, topic) => {
                self.handle_talk(session, npc, topic, config).await
            }
            TinyMushCommand::Talked(npc) => self.handle_talked(session, npc, config).await,
            TinyMushCommand::Quest(subcommand) => {
                self.handle_quest(session, subcommand, config).await
            }
            TinyMushCommand::Abandon(quest_id) => {
                self.handle_abandon(session, quest_id, config).await
            }
            TinyMushCommand::Achievements(subcommand) => {
                self.handle_achievements(session, subcommand, config).await
            }
            TinyMushCommand::Title(subcommand) => {
                self.handle_title(session, subcommand, config).await
            }
            TinyMushCommand::Companion(subcommand) => {
                self.handle_companion(session, subcommand, config).await
            }
            TinyMushCommand::Feed(name) => self.handle_feed(session, name, config).await,
            TinyMushCommand::Pet(name) => self.handle_pet(session, name, config).await,
            TinyMushCommand::Mount(name) => self.handle_mount(session, name, config).await,
            TinyMushCommand::Dismount => self.handle_dismount(session, config).await,
            TinyMushCommand::Train(companion, skill) => {
                self.handle_train(session, companion, skill, config).await
            }
            TinyMushCommand::Housing(subcommand) => {
                self.handle_housing(session, subcommand, config).await
            }
            TinyMushCommand::Rent(template_id) => {
                self.handle_rent(session, template_id, config).await
            }
            TinyMushCommand::Home(subcommand) => {
                self.handle_home(session, subcommand, config).await
            }
            TinyMushCommand::Invite(player) => self.handle_invite(session, player, config).await,
            TinyMushCommand::Uninvite(player) => {
                self.handle_uninvite(session, player, config).await
            }
            TinyMushCommand::Describe(description) => {
                self.handle_describe(session, description, config).await
            }
            TinyMushCommand::Lock(target) => self.handle_lock(session, target, config).await,
            TinyMushCommand::Unlock(target) => self.handle_unlock(session, target, config).await,
            TinyMushCommand::Kick(target) => self.handle_kick(session, target, config).await,
            TinyMushCommand::History(item_name) => {
                self.handle_history(session, item_name, config).await
            }
            TinyMushCommand::Reclaim(item_name) => {
                self.handle_reclaim(session, item_name, config).await
            }
            TinyMushCommand::SetConfig(field, value) => {
                self.handle_set_config(session, field, value, config).await
            }
            TinyMushCommand::GetConfig(field) => {
                self.handle_get_config(session, field, config).await
            }
            TinyMushCommand::EditRoom(room_id, description) => {
                self.handle_edit_room(session, room_id, description, config)
                    .await
            }
            TinyMushCommand::EditNpc(npc_id, field, value) => {
                self.handle_edit_npc(session, npc_id, field, value, config)
                    .await
            }
            TinyMushCommand::Dialog(npc_id, subcommand, args) => {
                self.handle_dialog(session, npc_id, subcommand, args, config)
                    .await
            }
            TinyMushCommand::Recipe(subcommand, args) => {
                self.handle_recipe(session, subcommand, args, config).await
            }
            TinyMushCommand::QuestAdmin(subcommand, args) => {
                self.handle_quest_admin(session, subcommand, args, config)
                    .await
            }
            TinyMushCommand::AchievementAdmin(subcommand, args) => {
                self.handle_achievement_admin(session, subcommand, args, config)
                    .await
            }
            TinyMushCommand::NPCAdmin(subcommand, args) => {
                self.handle_npc_admin(session, subcommand, args, config)
                    .await
            }
            TinyMushCommand::CompanionAdmin(subcommand, args) => {
                self.handle_companion_admin(session, subcommand, args, config)
                    .await
            }
            TinyMushCommand::RoomAdmin(subcommand, args) => {
                self.handle_room_admin(session, subcommand, args, config)
                    .await
            }
            TinyMushCommand::ObjectAdmin(subcommand, args) => {
                self.handle_object_admin(session, subcommand, args, config)
                    .await
            }
            TinyMushCommand::ListAbandoned => {
                self.handle_list_abandoned(session, _storage, config).await
            }
            TinyMushCommand::Admin => self.handle_admin(session, config).await,
            TinyMushCommand::SetAdmin(username, level) => {
                self.handle_set_admin(session, username, level, config)
                    .await
            }
            TinyMushCommand::RemoveAdmin(username) => {
                self.handle_remove_admin(session, username, config).await
            }
            TinyMushCommand::Admins => self.handle_admins(session, config).await,
            TinyMushCommand::Give(player, args) => {
                self.handle_give(session, player, args, config).await
            }
            TinyMushCommand::Stats(player, args) => {
                self.handle_stats(session, player, args, config).await
            }
            TinyMushCommand::Teleport(player, room) => {
                self.handle_teleport(session, player, room, config).await
            }
            TinyMushCommand::Players => self.handle_players(session, config).await,
            TinyMushCommand::Goto(target) => self.handle_goto(session, target, config).await,
            TinyMushCommand::ConvertCurrency(currency_type, dry_run) => {
                self.handle_convert_currency(session, currency_type, dry_run, config)
                    .await
            }
            // Backup & Recovery commands (Phase 9.5)
            TinyMushCommand::Backup(name) => self.handle_backup(session, name, config).await,
            TinyMushCommand::RestoreBackup(backup_id) => {
                self.handle_restore_backup(session, backup_id, config).await
            }
            TinyMushCommand::ListBackups => self.handle_list_backups(session, config).await,
            TinyMushCommand::VerifyBackup(backup_id) => {
                self.handle_verify_backup(session, backup_id, config).await
            }
            TinyMushCommand::DeleteBackup(backup_id) => {
                self.handle_delete_backup(session, backup_id, config).await
            }
            TinyMushCommand::BackupConfig(args) => {
                self.handle_backup_config(session, args, config).await
            }
            // Clone monitoring commands (Phase 6 Admin Tools)
            TinyMushCommand::ListClones(username) => {
                self.handle_list_clones(session, username, config).await
            }
            TinyMushCommand::CloneStats => self.handle_clone_stats(session, config).await,
            // Builder permission commands (Phase 7)
            TinyMushCommand::Builder => self.handle_builder(session, config).await,
            TinyMushCommand::SetBuilder(username, level) => {
                self.handle_set_builder(session, username, level, config)
                    .await
            }
            TinyMushCommand::RemoveBuilder(username) => {
                self.handle_remove_builder(session, username, config).await
            }
            TinyMushCommand::Builders => self.handle_builders(session, config).await,
            // Builder world manipulation commands (Phase 7)
            TinyMushCommand::Dig(direction, room_name) => {
                self.handle_dig(session, direction, room_name, config).await
            }
            TinyMushCommand::DescribeTarget(target, description) => {
                self.handle_describe_target(session, target, description, config)
                    .await
            }
            TinyMushCommand::Link(direction, destination) => {
                self.handle_link(session, direction, destination, config)
                    .await
            }
            TinyMushCommand::Unlink(direction) => {
                self.handle_unlink(session, direction, config).await
            }
            TinyMushCommand::SetFlag(target, flag) => {
                self.handle_set_flag(session, target, flag, config).await
            }
            TinyMushCommand::Create(object_name) => {
                self.handle_create(session, object_name, config).await
            }
            TinyMushCommand::Destroy(object_name) => {
                self.handle_destroy(session, object_name, config).await
            }
            TinyMushCommand::Clone(object_name) => {
                self.handle_clone(session, object_name, config).await
            }
            TinyMushCommand::Help(topic) => self.handle_help(session, topic, config).await,
            TinyMushCommand::Quit => self.handle_quit(session, config).await,
            TinyMushCommand::Save => self.handle_save(session, config).await,
            TinyMushCommand::Unknown(cmd) => Ok(format!(
                "Unknown command: '{}'\nType HELP for available commands.",
                cmd
            )),
            _ => Ok(
                "That command isn't implemented yet.\nType HELP for available commands."
                    .to_string(),
            ),
        }
    }

    /// Parse raw input into TinyMUSH command enum
    pub fn parse_command(&self, input: &str) -> TinyMushCommand {
        let input_trimmed = input.trim();
        let input_upper = input_trimmed.to_uppercase();

        // Determine if we need to preserve case for this command's arguments.
        // Most game commands should be case-insensitive, but user-facing text
        // (chat, emotes, descriptions, trigger scripts) must preserve case.
        let first_char = input_trimmed.chars().next();
        let first_word = input_upper.split_whitespace().next().unwrap_or("");
        let is_text_command = matches!(first_char, Some('\'') | Some(':') | Some(';'))
            || matches!(
                first_word,
                "SAY" | "EMOTE" | "POSE" | "OOC" | "WHISPER" | "WHIS"
            );
        let preserve_case = input_trimmed.starts_with('@') || is_text_command;

        let parts: Vec<&str> = if preserve_case {
            input_trimmed.split_whitespace().collect()
        } else {
            input_upper.split_whitespace().collect()
        };

        if parts.is_empty() {
            return TinyMushCommand::Unknown(input_upper);
        }

        // For case-preserved commands, uppercase just the command for pattern matching
        let cmd_for_match = if preserve_case {
            parts[0].to_uppercase()
        } else {
            parts[0].to_string()
        };

        match cmd_for_match.as_str() {
            // Movement (single letters)
            "N" | "NORTH" => TinyMushCommand::Move(Direction::North),
            "S" | "SOUTH" => TinyMushCommand::Move(Direction::South),
            "E" | "EAST" => TinyMushCommand::Move(Direction::East),
            "W" | "WEST" => TinyMushCommand::Move(Direction::West),
            "U" | "UP" => TinyMushCommand::Move(Direction::Up),
            "D" | "DOWN" => TinyMushCommand::Move(Direction::Down),
            "NE" | "NORTHEAST" => TinyMushCommand::Move(Direction::Northeast),
            "NW" | "NORTHWEST" => TinyMushCommand::Move(Direction::Northwest),
            "SE" | "SOUTHEAST" => TinyMushCommand::Move(Direction::Southeast),
            "SW" | "SOUTHWEST" => TinyMushCommand::Move(Direction::Southwest),

            // Look commands
            "L" | "LOOK" => {
                if parts.len() > 1 {
                    TinyMushCommand::Look(Some(parts[1..].join(" ")))
                } else {
                    TinyMushCommand::Look(None)
                }
            }

            // Information commands
            "I" | "INV" | "INVENTORY" => TinyMushCommand::Inventory,
            "WHO" => TinyMushCommand::Who,
            "WHERE" => {
                // WHERE with no args shows your location
                // WHERE <player> (admin only) locates another player
                if parts.len() > 1 {
                    TinyMushCommand::Where(Some(parts[1].to_lowercase()))
                } else {
                    TinyMushCommand::Where(None)
                }
            }
            "MAP" => TinyMushCommand::Map,
            "SCORE" => TinyMushCommand::Score,
            "TIME" => TinyMushCommand::Time,

            // Social commands
            "SAY" | "'" => {
                if parts.len() > 1 {
                    TinyMushCommand::Say(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Say("".to_string())
                }
            }
            "WHISPER" | "WHIS" => {
                if parts.len() > 2 {
                    let target = parts[1].to_string();
                    let message = parts[2..].join(" ");
                    TinyMushCommand::Whisper(target, message)
                } else {
                    TinyMushCommand::Unknown("Usage: WHISPER <player> <message>".to_string())
                }
            }
            "EMOTE" | ":" => {
                if parts.len() > 1 {
                    TinyMushCommand::Emote(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Emote("".to_string())
                }
            }
            "POSE" | ";" => {
                if parts.len() > 1 {
                    TinyMushCommand::Pose(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Pose("".to_string())
                }
            }
            "OOC" => {
                if parts.len() > 1 {
                    TinyMushCommand::Ooc(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Ooc("".to_string())
                }
            }

            // Object interaction
            "T" | "TAKE" | "GET" => {
                if parts.len() > 1 {
                    TinyMushCommand::Take(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Unknown(input_upper.to_string())
                }
            }
            "DROP" => {
                if parts.len() > 1 {
                    TinyMushCommand::Drop(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Unknown(input_upper.to_string())
                }
            }
            "USE" => {
                if parts.len() > 1 {
                    TinyMushCommand::Use(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Unknown(input_upper.to_string())
                }
            }
            "POKE" | "PROD" => {
                if parts.len() > 1 {
                    TinyMushCommand::Poke(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Unknown(input_upper.to_string())
                }
            }
            "X" | "EXAMINE" => {
                if parts.len() > 1 {
                    TinyMushCommand::Examine(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Unknown(input_upper.to_string())
                }
            }
            "CRAFT" => {
                if parts.len() > 1 {
                    TinyMushCommand::Craft(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Unknown(input_upper.to_string())
                }
            }

            // Economy commands
            "BUY" | "PURCHASE" => {
                if parts.len() > 1 {
                    // BUY item [quantity]
                    let item_name = parts[1].to_string();
                    let quantity = if parts.len() > 2 {
                        parts[2].parse::<u32>().ok()
                    } else {
                        Some(1) // Default to 1 if no quantity specified
                    };
                    TinyMushCommand::Buy(item_name, quantity)
                } else {
                    TinyMushCommand::Unknown("Usage: BUY <item> [quantity]".to_string())
                }
            }
            "SELL" => {
                if parts.len() > 1 {
                    // SELL item [quantity]
                    let item_name = parts[1].to_string();
                    let quantity = if parts.len() > 2 {
                        parts[2].parse::<u32>().ok()
                    } else {
                        Some(1) // Default to 1 if no quantity specified
                    };
                    TinyMushCommand::Sell(item_name, quantity)
                } else {
                    TinyMushCommand::Unknown("Usage: SELL <item> [quantity]".to_string())
                }
            }
            "LIST" | "WARES" | "SHOP" => TinyMushCommand::List,

            // System commands
            "HELP" | "H" => {
                if parts.len() > 1 {
                    TinyMushCommand::Help(Some(parts[1..].join(" ")))
                } else {
                    TinyMushCommand::Help(None)
                }
            }
            "QUIT" | "Q" | "EXIT" => TinyMushCommand::Quit,
            "SAVE" => TinyMushCommand::Save,

            // Bulletin board commands
            "BOARD" | "BB" => {
                if parts.len() > 1 {
                    TinyMushCommand::Board(Some(parts[1].to_string()))
                } else {
                    TinyMushCommand::Board(None)
                }
            }
            "POST" => {
                if parts.len() > 2 {
                    let subject = parts[1].to_string();
                    let message = parts[2..].join(" ");
                    TinyMushCommand::Post(subject, message)
                } else {
                    TinyMushCommand::Unknown("Usage: POST <subject> <message>".to_string())
                }
            }
            "READ" => {
                if parts.len() > 1 {
                    if let Ok(message_id) = parts[1].parse::<u64>() {
                        TinyMushCommand::Read(message_id)
                    } else {
                        TinyMushCommand::Unknown("Usage: READ <message_id>".to_string())
                    }
                } else {
                    TinyMushCommand::Unknown("Usage: READ <message_id>".to_string())
                }
            }

            // Mail system commands
            "MAIL" => {
                if parts.len() > 1 {
                    TinyMushCommand::Mail(Some(parts[1].to_string()))
                } else {
                    TinyMushCommand::Mail(None)
                }
            }
            "SEND" => {
                if parts.len() > 3 {
                    let recipient = parts[1].to_string();
                    let subject = parts[2].to_string();
                    let message = parts[3..].join(" ");
                    TinyMushCommand::Send(recipient, subject, message)
                } else {
                    TinyMushCommand::Unknown("Usage: SEND <player> <subject> <message>".to_string())
                }
            }
            "RMAIL" => {
                if parts.len() > 1 {
                    if let Ok(message_id) = parts[1].parse::<u64>() {
                        TinyMushCommand::ReadMail(message_id)
                    } else {
                        TinyMushCommand::Unknown("Usage: RMAIL <message_id>".to_string())
                    }
                } else {
                    TinyMushCommand::Unknown("Usage: RMAIL <message_id>".to_string())
                }
            }
            "DMAIL" => {
                if parts.len() > 1 {
                    if let Ok(message_id) = parts[1].parse::<u64>() {
                        TinyMushCommand::DeleteMail(message_id)
                    } else {
                        TinyMushCommand::Unknown("Usage: DMAIL <message_id>".to_string())
                    }
                } else {
                    TinyMushCommand::Unknown("Usage: DMAIL <message_id>".to_string())
                }
            }

            // Banking commands (Phase 5 Week 4)
            "BALANCE" | "BAL" => TinyMushCommand::Balance,
            "DEPOSIT" | "DEP" => {
                if parts.len() > 1 {
                    TinyMushCommand::Deposit(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Unknown("Usage: DEPOSIT <amount>".to_string())
                }
            }
            "WITHDRAW" | "WITH" => {
                if parts.len() > 1 {
                    TinyMushCommand::Withdraw(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Unknown("Usage: WITHDRAW <amount>".to_string())
                }
            }
            "BTRANSFER" | "BTRANS" => {
                if parts.len() > 2 {
                    let recipient = parts[1].to_string();
                    let amount = parts[2..].join(" ");
                    TinyMushCommand::BankTransfer(recipient, amount)
                } else {
                    TinyMushCommand::Unknown("Usage: BTRANSFER <player> <amount>".to_string())
                }
            }

            // Trading commands (Phase 5 Week 4)
            "TRADE" => {
                if parts.len() > 1 {
                    TinyMushCommand::Trade(parts[1].to_string())
                } else {
                    TinyMushCommand::Unknown("Usage: TRADE <player>".to_string())
                }
            }
            "OFFER" => {
                if parts.len() > 1 {
                    TinyMushCommand::Offer(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Unknown("Usage: OFFER <item/amount>".to_string())
                }
            }
            "ACCEPT" | "ACC" => TinyMushCommand::Accept,
            "REJECT" | "REJ" | "CANCEL" => TinyMushCommand::Reject,
            "THISTORY" | "THIST" => TinyMushCommand::TradeHistory,

            // Tutorial & NPC commands (Phase 6 Week 1)
            "TUTORIAL" | "TUT" => {
                if parts.len() > 1 {
                    TinyMushCommand::Tutorial(Some(parts[1].to_uppercase()))
                } else {
                    TinyMushCommand::Tutorial(None)
                }
            }
            "TALK" | "GREET" => {
                if parts.len() > 2 {
                    // TALK NPC TOPIC
                    TinyMushCommand::Talk(parts[1].to_uppercase(), Some(parts[2].to_uppercase()))
                } else if parts.len() > 1 {
                    // TALK NPC
                    TinyMushCommand::Talk(parts[1].to_uppercase(), None)
                } else {
                    TinyMushCommand::Unknown("Usage: TALK <npc> [topic]".to_string())
                }
            }
            "TALKED" => {
                if parts.len() > 1 {
                    TinyMushCommand::Talked(Some(parts[1].to_uppercase()))
                } else {
                    TinyMushCommand::Talked(None)
                }
            }

            // Quest commands (Phase 6 Week 2)
            "QUEST" | "QUESTS" => {
                if parts.len() > 1 {
                    TinyMushCommand::Quest(Some(parts[1..].join(" ").to_uppercase()))
                } else {
                    TinyMushCommand::Quest(None)
                }
            }
            "ABANDON" | "ABAND" => {
                if parts.len() > 1 {
                    TinyMushCommand::Abandon(parts[1].to_lowercase())
                } else {
                    TinyMushCommand::Unknown("Usage: ABANDON <quest_id>".to_string())
                }
            }

            // Achievement & Title commands (Phase 6 Week 3)
            "ACHIEVEMENTS" | "ACHIEVE" | "ACHIEV" | "ACH" => {
                if parts.len() > 1 {
                    TinyMushCommand::Achievements(Some(parts[1..].join(" ").to_uppercase()))
                } else {
                    TinyMushCommand::Achievements(None)
                }
            }
            "TITLE" | "TITLES" => {
                if parts.len() > 1 {
                    TinyMushCommand::Title(Some(parts[1..].join(" ")))
                } else {
                    TinyMushCommand::Title(None)
                }
            }

            // Companion commands (Phase 6 Week 4)
            "COMPANION" | "COMP" => {
                if parts.len() > 1 {
                    TinyMushCommand::Companion(Some(parts[1..].join(" ")))
                } else {
                    TinyMushCommand::Companion(None)
                }
            }
            "FEED" => {
                if parts.len() > 1 {
                    TinyMushCommand::Feed(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Unknown("Usage: FEED <companion>".to_string())
                }
            }
            "PET" => {
                if parts.len() > 1 {
                    TinyMushCommand::Pet(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Unknown("Usage: PET <companion>".to_string())
                }
            }
            "MOUNT" => {
                if parts.len() > 1 {
                    TinyMushCommand::Mount(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Unknown("Usage: MOUNT <horse>".to_string())
                }
            }
            "DISMOUNT" => TinyMushCommand::Dismount,
            "TRAIN" => {
                if parts.len() > 2 {
                    let companion = parts[1].to_string();
                    let skill = parts[2..].join(" ");
                    TinyMushCommand::Train(companion, skill)
                } else {
                    TinyMushCommand::Unknown("Usage: TRAIN <companion> <skill>".to_string())
                }
            }

            // Housing commands (Phase 7 Week 1-2)
            "HOUSING" | "HOUSE" => {
                if parts.len() > 1 {
                    TinyMushCommand::Housing(Some(parts[1..].join(" ").to_uppercase()))
                } else {
                    TinyMushCommand::Housing(None)
                }
            }
            "RENT" => {
                if parts.len() > 1 {
                    TinyMushCommand::Rent(parts[1].to_lowercase())
                } else {
                    TinyMushCommand::Unknown("Usage: RENT <template_id>".to_string())
                }
            }
            "HOME" => {
                if parts.len() > 1 {
                    TinyMushCommand::Home(Some(parts[1..].join(" ").to_uppercase()))
                } else {
                    TinyMushCommand::Home(None)
                }
            }
            "INVITE" => {
                if parts.len() > 1 {
                    TinyMushCommand::Invite(parts[1].to_lowercase())
                } else {
                    TinyMushCommand::Unknown("Usage: INVITE <player>".to_string())
                }
            }
            "UNINVITE" => {
                if parts.len() > 1 {
                    TinyMushCommand::Uninvite(parts[1].to_lowercase())
                } else {
                    TinyMushCommand::Unknown("Usage: UNINVITE <player>".to_string())
                }
            }
            "DESCRIBE" | "DESC" => {
                if parts.len() > 1 {
                    // Join all parts after DESCRIBE as the description
                    let description = parts[1..].join(" ");
                    TinyMushCommand::Describe(Some(description))
                } else {
                    // No args - show current description and permissions
                    TinyMushCommand::Describe(None)
                }
            }
            "LOCK" => {
                if parts.len() > 1 {
                    // LOCK <item> - lock a specific item
                    TinyMushCommand::Lock(Some(parts[1..].join(" ")))
                } else {
                    // LOCK - lock current room
                    TinyMushCommand::Lock(None)
                }
            }
            "UNLOCK" => {
                if parts.len() > 1 {
                    // UNLOCK <item> - unlock a specific item
                    TinyMushCommand::Unlock(Some(parts[1..].join(" ")))
                } else {
                    // UNLOCK - unlock current room
                    TinyMushCommand::Unlock(None)
                }
            }
            "KICK" => {
                if parts.len() > 1 {
                    let target = parts[1].to_uppercase();
                    if target == "ALL" {
                        TinyMushCommand::Kick(Some("ALL".to_string()))
                    } else {
                        TinyMushCommand::Kick(Some(parts[1].to_lowercase()))
                    }
                } else {
                    TinyMushCommand::Unknown("Usage: KICK <player> or KICK ALL".to_string())
                }
            }
            "HISTORY" | "HIST" => {
                if parts.len() > 1 {
                    TinyMushCommand::History(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Unknown("Usage: HISTORY <item>".to_string())
                }
            }
            "RECLAIM" => {
                if parts.len() > 1 {
                    // RECLAIM <item> - retrieve specific item
                    TinyMushCommand::Reclaim(Some(parts[1..].join(" ")))
                } else {
                    // RECLAIM - view reclaim box contents
                    TinyMushCommand::Reclaim(None)
                }
            }

            // Admin/debug
            "DEBUG" => {
                if parts.len() > 1 {
                    TinyMushCommand::Debug(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Debug("".to_string())
                }
            }
            "@SETCONFIG" | "@SETCONF" => {
                if parts.len() > 2 {
                    let field = parts[1].to_lowercase();
                    let value = parts[2..].join(" ");
                    TinyMushCommand::SetConfig(field, value)
                } else {
                    TinyMushCommand::Unknown("Usage: @SETCONFIG <field> <value>\nFields: welcome_message, motd, world_name, world_description".to_string())
                }
            }
            "@EDITROOM" => {
                if parts.len() > 2 {
                    let room_id = parts[1].to_string();
                    let description = parts[2..].join(" ");
                    TinyMushCommand::EditRoom(room_id, description)
                } else {
                    TinyMushCommand::Unknown("Usage: @EDITROOM <room_id> <description>\nExample: @EDITROOM gazebo_landing A new description here".to_string())
                }
            }
            "@EDITNPC" => {
                if parts.len() > 3 {
                    let npc_id = parts[1].to_string();
                    let field = parts[2].to_lowercase();
                    let value = parts[3..].join(" ");
                    TinyMushCommand::EditNpc(npc_id, field, value)
                } else {
                    TinyMushCommand::Unknown("Usage: @EDITNPC <npc_id> <field> <value>\nFields: dialog.<key>, description, room\nExample: @EDITNPC mayor_thompson dialog.greeting Hello there!".to_string())
                }
            }
            "@DIALOG" | "@DLG" => {
                if parts.len() >= 3 {
                    let npc_id = parts[1].to_string();
                    let subcommand = parts[2].to_uppercase();
                    let args = if parts.len() > 3 {
                        Some(parts[3..].join(" "))
                    } else {
                        None
                    };
                    TinyMushCommand::Dialog(npc_id, subcommand, args)
                } else {
                    TinyMushCommand::Unknown("Usage: @DIALOG <npc> <subcommand> [args]\nSubcommands: LIST, VIEW <topic>, ADD <topic> <text>, EDIT <topic> <json>, DELETE <topic>, TEST <topic>\nExample: @DIALOG merchant VIEW greeting".to_string())
                }
            }
            "@RECIPE" | "@RCP" => {
                if parts.len() >= 2 {
                    let subcommand = parts[1].to_uppercase();
                    let args: Vec<String> = parts[2..].iter().map(|s| s.to_string()).collect();
                    TinyMushCommand::Recipe(subcommand, args)
                } else {
                    TinyMushCommand::Unknown("Usage: @RECIPE <subcommand> [args]\n\nSubcommands:\n  CREATE <id> <name> - Create new recipe\n  EDIT <id> MATERIAL ADD <item_id> <qty> - Add material\n  EDIT <id> MATERIAL REMOVE <item_id> - Remove material\n  EDIT <id> RESULT <item_id> [qty] - Set result item\n  EDIT <id> STATION <station_id> - Set crafting station\n  EDIT <id> DESCRIPTION <text> - Set description\n  DELETE <id> - Delete recipe\n  LIST [station] - List all recipes\n  SHOW <id> - Show recipe details\n\nExample: @RECIPE CREATE goat_cheese \"Goat Milk Cheese\"".to_string())
                }
            }
            "@QUEST" | "@QST" => {
                if parts.len() >= 2 {
                    let subcommand = parts[1].to_uppercase();
                    let args: Vec<String> = parts[2..].iter().map(|s| s.to_string()).collect();
                    TinyMushCommand::QuestAdmin(subcommand, args)
                } else {
                    TinyMushCommand::Unknown("Usage: @QUEST <subcommand> [args]\n\nSubcommands:\n  CREATE <id> <name> - Create new quest\n  EDIT <id> DESCRIPTION <text> - Set description\n  EDIT <id> GIVER <npc_id> - Set quest giver NPC\n  EDIT <id> LEVEL <num> - Set recommended level\n  EDIT <id> OBJECTIVE ADD <type> <details> - Add objective\n  EDIT <id> OBJECTIVE REMOVE <index> - Remove objective\n  EDIT <id> REWARD CURRENCY <amount> - Set currency reward\n  EDIT <id> REWARD XP <amount> - Set experience reward\n  EDIT <id> REWARD ITEM <item_id> - Set item reward\n  EDIT <id> PREREQUISITE <quest_id> - Set prerequisite quest\n  DELETE <id> - Delete quest\n  LIST - List all quests\n  SHOW <id> - Show quest details\n\nExample: @QUEST CREATE fetch_water \"Fetch Water for the Village\"".to_string())
                }
            }
            "@ACHIEVEMENT" | "@ACH" => {
                if parts.len() >= 2 {
                    let subcommand = parts[1].to_uppercase();
                    let args: Vec<String> = parts[2..].iter().map(|s| s.to_string()).collect();
                    TinyMushCommand::AchievementAdmin(subcommand, args)
                } else {
                    TinyMushCommand::Unknown("Usage: @ACHIEVEMENT <subcommand> [args]\n\nSubcommands:\n  CREATE <id> <name> - Create new achievement\n  EDIT <id> DESCRIPTION <text> - Set description\n  EDIT <id> CATEGORY <type> - Set category (Combat/Exploration/Social/Economic/Quest/Special)\n  EDIT <id> TRIGGER <type> <params> - Set trigger condition\n    Trigger types:\n      KILLCOUNT <num> - Defeat N enemies\n      ROOMVISITS <num> - Visit N unique rooms\n      FRIENDCOUNT <num> - Make N friends\n      MESSAGESSENT <num> - Send N messages\n      TRADECOUNT <num> - Complete N trades\n      CURRENCYEARNED <amount> - Earn X currency\n      QUESTCOMPLETION <num> - Complete N quests\n      VISITLOCATION <room_id> - Visit specific location\n      COMPLETEQUEST <quest_id> - Complete specific quest\n  EDIT <id> TITLE <text> - Set title reward (optional)\n  EDIT <id> HIDDEN <true|false> - Toggle hidden status\n  DELETE <id> - Delete achievement\n  LIST [category] - List achievements (optional filter)\n  SHOW <id> - Show achievement details\n\nExample: @ACHIEVEMENT CREATE first_kill \"First Blood\"".to_string())
                }
            }
            "@NPC" => {
                if parts.len() >= 2 {
                    let subcommand = parts[1].to_uppercase();
                    let args: Vec<String> = parts[2..].iter().map(|s| s.to_string()).collect();
                    TinyMushCommand::NPCAdmin(subcommand, args)
                } else {
                    TinyMushCommand::Unknown("Usage: @NPC <subcommand> [args]\n\nSubcommands:\n  CREATE <id> <name> - Create new NPC\n  EDIT <id> NAME <text> - Set NPC name\n  EDIT <id> TITLE <text> - Set NPC title\n  EDIT <id> DESCRIPTION <text> - Set NPC description\n  EDIT <id> ROOM <room_id> - Set NPC location\n  EDIT <id> DIALOG <key> <text> - Add simple dialogue response\n  DELETE <id> - Delete NPC\n  LIST - List all NPCs\n  SHOW <id> - Show NPC details\n\nNPC Flags (use @NPC EDIT <id> FLAG <flag>):\n  VENDOR - NPC can trade items\n  GUARD - NPC provides security\n  TUTORIALNPC - NPC helps with tutorials\n  QUESTGIVER - NPC gives quests\n\nExample: @NPC CREATE blacksmith \"Forge Master Grimm\"\nExample: @NPC EDIT blacksmith ROOM town_forge\nExample: @NPC EDIT blacksmith DIALOG greeting Welcome to my forge!".to_string())
                }
            }
            "@COMPANION" | "@COMPANIONS" | "@PET" => {
                if parts.len() >= 2 {
                    let subcommand = parts[1].to_uppercase();
                    let args: Vec<String> = parts[2..].iter().map(|s| s.to_string()).collect();
                    TinyMushCommand::CompanionAdmin(subcommand, args)
                } else {
                    TinyMushCommand::Unknown("Usage: @COMPANION <subcommand> [args]\n\nSubcommands:\n  CREATE <id> <name> - Create new companion\n  EDIT <id> NAME <text> - Set companion name\n  EDIT <id> DESCRIPTION <text> - Set companion description\n  EDIT <id> TYPE <type> - Set companion type\n  EDIT <id> ROOM <room_id> - Set companion location\n  EDIT <id> BEHAVIOR <behavior> [params] - Add companion behavior\n  DELETE <id> - Delete companion\n  LIST - List all companions\n  SHOW <id> - Show companion details\n\nCompanion Types:\n  HORSE - Mount, extra storage\n  DOG - Loyal follower, alert danger\n  CAT - Independent, idle chatter\n  FAMILIAR - Magic boost, auto-follow\n  MERCENARY - Combat assist\n  CONSTRUCT - Mechanical ally\n\nBehavior Examples:\n  @COMPANION EDIT loyal_dog BEHAVIOR AutoFollow\n  @COMPANION EDIT war_horse BEHAVIOR ExtraStorage 30\n  @COMPANION EDIT guard_dog BEHAVIOR CombatAssist 5\n  @COMPANION EDIT healing_cat BEHAVIOR Healing 10 300\n\nExample: @COMPANION CREATE war_horse \"Battle Steed\"\nExample: @COMPANION EDIT war_horse TYPE HORSE\nExample: @COMPANION EDIT war_horse ROOM armory".to_string())
                }
            }
            "@ROOM" | "@ROOMS" => {
                if parts.len() >= 2 {
                    let subcommand = parts[1].to_uppercase();
                    let args: Vec<String> = parts[2..].iter().map(|s| s.to_string()).collect();
                    TinyMushCommand::RoomAdmin(subcommand, args)
                } else {
                    TinyMushCommand::Unknown("Usage: @ROOM <subcommand> [args]\n\nSubcommands:\n  CREATE <id> <name> - Create new room\n  EDIT <id> NAME <text> - Set room name\n  EDIT <id> SHORTDESC <text> - Set room short description\n  EDIT <id> LONGDESC <text> - Set room long description\n  EDIT <id> EXIT <direction> <dest_room> - Add exit\n  EDIT <id> EXIT <direction> REMOVE - Remove exit\n  EDIT <id> FLAG <flag> - Add room flag\n  EDIT <id> CAPACITY <number> - Set max occupancy\n  EDIT <id> VISIBILITY <public|private|hidden> - Set room visibility\n  EDIT <id> LOCKED <true|false> - Lock/unlock room\n  EDIT <id> OWNER <player|world> - Transfer ownership\n  EDIT <id> HOUSING_TAGS <tag1,tag2,...> - Set housing filter tags\n  DELETE <id> - Delete room\n  LIST - List all rooms\n  SHOW <id> - Show room details\n\nRoom Flags:\n  SAFE - No combat allowed\n  DARK - Requires light source\n  INDOOR - Protected from weather\n  SHOP - Commercial location\n  QUESTLOCATION - Quest-related room\n  PVPENABLED - PvP combat allowed\n  PLAYERCREATED - Player-made room\n  PRIVATE - Restricted access\n  MODERATED - Admin-monitored\n  INSTANCED - Separate copy per player\n  CROWDED - High traffic area\n  HOUSINGOFFICE - Housing services\n  NOTELEPORTOUT - Cannot teleport out\n\nVisibility Types:\n  PUBLIC - Visible to all, anyone can enter\n  PRIVATE - Visible only to owner/guests\n  HIDDEN - Not listed, requires knowledge of ID\n\nExamples:\n  @ROOM CREATE dark_cave \"Mysterious Cave\"\n  @ROOM EDIT dark_cave FLAG DARK\n  @ROOM EDIT dark_cave CAPACITY 10\n  @ROOM EDIT tavern EXIT NORTH town_square\n  @ROOM EDIT tavern EXIT SOUTH REMOVE\n  @ROOM EDIT private_study VISIBILITY PRIVATE\n  @ROOM EDIT vault LOCKED true\n  @ROOM EDIT player_house OWNER alice\n  @ROOM EDIT housing_office HOUSING_TAGS cozy,small".to_string())
                }
            }
            "@OBJECT" | "@OBJECTS" | "@OBJ" => {
                if parts.len() >= 2 {
                    let subcommand = parts[1].to_uppercase();
                    let args: Vec<String> = parts[2..].iter().map(|s| s.to_string()).collect();
                    TinyMushCommand::ObjectAdmin(subcommand, args)
                } else {
                    TinyMushCommand::Unknown("Usage: @OBJECT <subcommand> [args]\n\nSubcommands:\n  CREATE <id> <name> - Create new world object\n  EDIT <id> NAME <text> - Set object name\n  EDIT <id> DESCRIPTION <text> - Set object description\n  EDIT <id> WEIGHT <number> - Set weight (0-255)\n  EDIT <id> VALUE <amount> - Set currency value\n  EDIT <id> FLAG <flag> - Add object flag\n  EDIT <id> TAKEABLE <true|false> - Set takeable property\n  EDIT <id> USABLE <true|false> - Set usable property\n  EDIT <id> LOCKED <true|false> - Lock to prevent taking\n  EDIT <id> TRIGGER <type> <script> - Set object trigger\n  EDIT <id> TRIGGER <type> REMOVE - Remove object trigger\n  EDIT <id> OWNER <player|world> - Transfer ownership\n  DELETE <id> - Delete object\n  LIST - List all world objects\n  SHOW <id> - Show object details\n\nObject Flags:\n  QUESTITEM - Required for quests\n  CONSUMABLE - Single-use item\n  EQUIPMENT - Can be equipped\n  KEYITEM - Important story item\n  CONTAINER - Can hold other items\n  MAGICAL - Has magical properties\n  COMPANION - Companion pet/ally\n  CLONABLE - Can be cloned by players\n  UNIQUE - Cannot be cloned\n  NOVALUE - Strip value on clone\n  NOCLONECHILDREN - Cannot clone with contents\n  LIGHTSOURCE - Provides light in dark rooms\n\nTrigger Types:\n  ONENTER - Fires when player enters room with object\n  ONLOOK - Fires when player examines object\n  ONTAKE - Fires when player takes object\n  ONDROP - Fires when player drops object\n  ONUSE - Fires when player uses object\n  ONPOKE - Fires when player pokes object\n  ONFOLLOW - Fires when player follows something\n  ONIDLE - Fires periodically when idle\n  ONCOMBAT - Fires during combat\n  ONHEAL - Fires when healing occurs\n\nTrigger Script Commands:\n  message(\"text\") - Display message to player\n  heal(amount) - Heal the player\n  consume() - Destroy object after use\n  teleport(\"room_id\") - Move player to room\n  random_chance(percent) - Probability gate\n  has_quest(\"quest_id\") - Check quest status\n  unlock_exit(\"direction\") - Unlock exit\n  Multiple commands: cmd1 && cmd2 && cmd3\n\nValue Examples:\n  @OBJECT EDIT torch VALUE 5gc - Sets value to 5 gold, 0 silver, 0 copper\n  @OBJECT EDIT sword VALUE 2gc,50sc - Sets value to 2 gold, 50 silver, 0 copper\n\nExamples:\n  @OBJECT CREATE basic_torch \"Wooden Torch\"\n  @OBJECT EDIT basic_torch DESCRIPTION \"A simple torch that provides light.\"\n  @OBJECT EDIT basic_torch FLAG LIGHTSOURCE\n  @OBJECT EDIT basic_torch TAKEABLE true\n  @OBJECT EDIT basic_torch WEIGHT 5\n  @OBJECT EDIT basic_torch OWNER alice\n  @OBJECT EDIT singing_mushroom TRIGGER ONENTER message(\"🍄 Chimes!\")\n  @OBJECT EDIT healing_potion TRIGGER ONUSE heal(50) && consume()\n  @OBJECT EDIT mystery_box TRIGGER ONPOKE random_chance(50) && message(\"✨ Click!\")\n  @OBJECT EDIT singing_mushroom TRIGGER ONENTER REMOVE".to_string())
                }
            }
            "@GETCONFIG" | "@GETCONF" | "@CONFIG" => {
                if parts.len() > 1 {
                    TinyMushCommand::GetConfig(Some(parts[1].to_lowercase()))
                } else {
                    TinyMushCommand::GetConfig(None)
                }
            }
            "@LISTABANDONED" | "@ABANDONED" | "@INACTIVE" => TinyMushCommand::ListAbandoned,
            "@ADMIN" => TinyMushCommand::Admin,
            "@SETADMIN" => {
                if parts.len() > 2 {
                    let username = parts[1].to_lowercase();
                    match parts[2].parse::<u8>() {
                        Ok(level) if level <= 3 => TinyMushCommand::SetAdmin(username, level),
                        Ok(_) => TinyMushCommand::Unknown("Admin level must be 0-3 (0=none, 1=moderator, 2=admin, 3=sysop)".to_string()),
                        Err(_) => TinyMushCommand::Unknown("Usage: @SETADMIN <player> <level>\nLevel: 0=none, 1=moderator, 2=admin, 3=sysop\nExample: @SETADMIN alice 2".to_string()),
                    }
                } else {
                    TinyMushCommand::Unknown("Usage: @SETADMIN <player> <level>\nLevel: 0=none, 1=moderator, 2=admin, 3=sysop".to_string())
                }
            }
            "@REMOVEADMIN" | "@REVOKEADMIN" => {
                if parts.len() > 1 {
                    TinyMushCommand::RemoveAdmin(parts[1].to_lowercase())
                } else {
                    TinyMushCommand::Unknown("Usage: @REMOVEADMIN <player>".to_string())
                }
            }
            "@ADMINS" | "@ADMINLIST" => TinyMushCommand::Admins,
            "@GIVE" => {
                if parts.len() < 3 {
                    TinyMushCommand::Unknown("Usage: @GIVE <player> <object_id> [quantity]\n       @GIVE <player> CURRENCY <amount>\nExample: @GIVE alice basic_torch\nExample: @GIVE alice basic_torch 5\nExample: @GIVE alice CURRENCY 1000".to_string())
                } else {
                    let player = parts[1].to_lowercase();
                    TinyMushCommand::Give(
                        player,
                        parts[2..].iter().map(|s| s.to_string()).collect(),
                    )
                }
            }
            "@STATS" => {
                if parts.len() < 2 {
                    TinyMushCommand::Unknown("Usage: @STATS <player> EDIT <stat> <value>\nExample: @STATS alice EDIT HP 100".to_string())
                } else {
                    let player = parts[1].to_lowercase();
                    TinyMushCommand::Stats(
                        player,
                        parts[2..].iter().map(|s| s.to_string()).collect(),
                    )
                }
            }
            "@PLAYERS" | "@WHO" => TinyMushCommand::Players,
            "@WHERE" => {
                if parts.len() > 1 {
                    TinyMushCommand::Where(Some(parts[1].to_lowercase()))
                } else {
                    TinyMushCommand::Unknown(
                        "Usage: @WHERE <player>\nExample: @WHERE alice".to_string(),
                    )
                }
            }
            "@GOTO" | "@TEL" => {
                // @GOTO is for self-teleportation only
                if parts.len() > 1 {
                    TinyMushCommand::Goto(parts[1..].join(" "))
                } else {
                    TinyMushCommand::Unknown(
                        "Usage: @GOTO <player|room>\nExample: @GOTO alice or @GOTO town_square"
                            .to_string(),
                    )
                }
            }
            "@TELEPORT" => {
                // @TELEPORT can be used for self or to teleport others (admin only)
                if parts.len() == 2 {
                    // @TELEPORT <target> - self teleport (like @GOTO)
                    TinyMushCommand::Goto(parts[1..].join(" "))
                } else if parts.len() > 2 {
                    // @TELEPORT <player> <room> - admin teleporting another player
                    let player = parts[1].to_lowercase();
                    let room = parts[2..].join(" ");
                    TinyMushCommand::Teleport(player, room)
                } else {
                    TinyMushCommand::Unknown(
                        "Usage: @TELEPORT <target>  (self teleport)\n       @TELEPORT <player> <room>  (admin only)\nExample: @TELEPORT town_square\nExample: @TELEPORT alice tavern"
                            .to_string(),
                    )
                }
            }
            "@LISTCLONES" | "@CLONES" => {
                if parts.len() > 1 {
                    let username = parts[1].to_lowercase();
                    TinyMushCommand::ListClones(Some(username))
                } else {
                    TinyMushCommand::ListClones(None)
                }
            }
            "@CLONESTATS" | "@CLONESTATUS" => TinyMushCommand::CloneStats,
            "@CONVERT_CURRENCY" | "@CONVERTCURRENCY" | "@MIGRATE" => {
                if parts.len() < 2 {
                    return TinyMushCommand::Unknown("Usage: @CONVERT_CURRENCY <decimal|multitier> [--dry-run]\nExample: @CONVERT_CURRENCY multitier\nExample: @CONVERT_CURRENCY decimal --dry-run".to_string());
                }

                let currency_type = parts[1].to_lowercase();
                if currency_type != "decimal" && currency_type != "multitier" {
                    return TinyMushCommand::Unknown("Invalid currency type. Must be 'decimal' or 'multitier'.\nUsage: @CONVERT_CURRENCY <decimal|multitier> [--dry-run]".to_string());
                }

                // Check for --dry-run flag
                let dry_run = parts.len() > 2 && parts[2].eq_ignore_ascii_case("--dry-run");

                TinyMushCommand::ConvertCurrency(currency_type, dry_run)
            }

            // Backup & Recovery commands (Phase 9.5)
            "@BACKUP" => {
                if parts.len() > 1 {
                    // Backup with custom name
                    TinyMushCommand::Backup(Some(parts[1..].join(" ")))
                } else {
                    // Backup with auto-generated name
                    TinyMushCommand::Backup(None)
                }
            }
            "@RESTORE" | "@RESTOREBACKUP" => {
                if parts.len() > 1 {
                    TinyMushCommand::RestoreBackup(parts[1].to_string())
                } else {
                    TinyMushCommand::Unknown("Usage: @RESTORE <backup_id>\nUse @LISTBACKUPS to see available backups\nExample: @RESTORE backup_20250112_143022".to_string())
                }
            }
            "@LISTBACKUPS" | "@BACKUPS" | "@LSBACKUP" => TinyMushCommand::ListBackups,
            "@VERIFYBACKUP" | "@VERIFY" => {
                if parts.len() > 1 {
                    TinyMushCommand::VerifyBackup(parts[1].to_string())
                } else {
                    TinyMushCommand::Unknown("Usage: @VERIFYBACKUP <backup_id>\nExample: @VERIFYBACKUP backup_20250112_143022".to_string())
                }
            }
            "@DELETEBACKUP" | "@DELBACKUP" | "@RMBACKUP" => {
                if parts.len() > 1 {
                    TinyMushCommand::DeleteBackup(parts[1].to_string())
                } else {
                    TinyMushCommand::Unknown("Usage: @DELETEBACKUP <backup_id>\nExample: @DELETEBACKUP backup_20250112_143022".to_string())
                }
            }
            "@BACKUPCONFIG" | "@BACKUPCFG" | "@AUTOBACKUP" => {
                // Collect all arguments after the command
                let args = if parts.len() > 1 {
                    parts[1..].iter().map(|s| s.to_string()).collect()
                } else {
                    Vec::new()
                };
                TinyMushCommand::BackupConfig(args)
            }

            // Builder permission management commands (Phase 7)
            "@BUILDER" => TinyMushCommand::Builder,
            "@SETBUILDER" => {
                if parts.len() > 2 {
                    let username = parts[1].to_lowercase();
                    match parts[2].parse::<u8>() {
                        Ok(level) if level <= 3 => TinyMushCommand::SetBuilder(username, level),
                        Ok(_) => TinyMushCommand::Unknown("Builder level must be 0-3 (0=none, 1=apprentice, 2=builder, 3=architect)".to_string()),
                        Err(_) => TinyMushCommand::Unknown("Usage: @SETBUILDER <player> <level>\nLevel: 0=none, 1=apprentice, 2=builder, 3=architect\nExample: @SETBUILDER alice 2".to_string()),
                    }
                } else {
                    TinyMushCommand::Unknown("Usage: @SETBUILDER <player> <level>\nLevel: 0=none, 1=apprentice, 2=builder, 3=architect".to_string())
                }
            }
            "@REMOVEBUILDER" | "@REVOKEBUILDER" => {
                if parts.len() > 1 {
                    TinyMushCommand::RemoveBuilder(parts[1].to_lowercase())
                } else {
                    TinyMushCommand::Unknown("Usage: @REMOVEBUILDER <player>".to_string())
                }
            }
            "@BUILDERS" | "@BUILDERLIST" => TinyMushCommand::Builders,

            // Builder world manipulation commands (Phase 7)
            "/DIG" => {
                if parts.len() > 2 {
                    let direction = parts[1].to_string();
                    let room_name = parts[2..].join(" ");
                    TinyMushCommand::Dig(direction, room_name)
                } else {
                    TinyMushCommand::Unknown(
                        "Usage: /DIG <direction> <room_name>\nExample: /DIG north Mysterious Cave"
                            .to_string(),
                    )
                }
            }
            "/DESCRIBE" => {
                if parts.len() > 2 {
                    let target = parts[1].to_string();
                    let description = parts[2..].join(" ");
                    TinyMushCommand::DescribeTarget(target, description)
                } else {
                    TinyMushCommand::Unknown("Usage: /DESCRIBE <target> <description>\nExample: /DESCRIBE here A dusty abandoned room\nExample: /DESCRIBE sword An ancient blade".to_string())
                }
            }
            "/LINK" => {
                if parts.len() > 2 {
                    let direction = parts[1].to_string();
                    let destination = parts[2].to_string();
                    TinyMushCommand::Link(direction, destination)
                } else {
                    TinyMushCommand::Unknown(
                        "Usage: /LINK <direction> <destination>\nExample: /LINK south town_square"
                            .to_string(),
                    )
                }
            }
            "/UNLINK" => {
                if parts.len() > 1 {
                    let direction = parts[1].to_string();
                    TinyMushCommand::Unlink(direction)
                } else {
                    TinyMushCommand::Unknown(
                        "Usage: /UNLINK <direction>\nExample: /UNLINK north".to_string(),
                    )
                }
            }
            "/SETFLAG" => {
                if parts.len() > 2 {
                    let target = parts[1].to_string();
                    let flag = parts[2].to_string();
                    TinyMushCommand::SetFlag(target, flag)
                } else {
                    TinyMushCommand::Unknown("Usage: /SETFLAG <target> <flag>\nExample: /SETFLAG here safe\nExample: /SETFLAG sword magical".to_string())
                }
            }
            "/CREATE" => {
                if parts.len() > 1 {
                    let object_name = parts[1..].join(" ");
                    TinyMushCommand::Create(object_name)
                } else {
                    TinyMushCommand::Unknown(
                        "Usage: /CREATE <object_name>\nExample: /CREATE Ancient Sword".to_string(),
                    )
                }
            }
            "/DESTROY" => {
                if parts.len() > 1 {
                    let object_name = parts[1..].join(" ");
                    TinyMushCommand::Destroy(object_name)
                } else {
                    TinyMushCommand::Unknown(
                        "Usage: /DESTROY <object>\nExample: /DESTROY sword".to_string(),
                    )
                }
            }
            "/CLONE" => {
                if parts.len() > 1 {
                    let object_name = parts[1..].join(" ");
                    TinyMushCommand::Clone(object_name)
                } else {
                    TinyMushCommand::Unknown("Usage: /CLONE <object>\nExample: /CLONE sword\nNote: Requires builder privileges (level 1+). Only owned clonable objects can be cloned".to_string())
                }
            }

            _ => TinyMushCommand::Unknown(input_upper),
        }
    }

    /// Handle LOOK command - examine room or object
    async fn handle_look(
        &mut self,
        session: &Session,
        target: Option<String>,
        _config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // If no target specified, look at current room
        if target.is_none() {
            return self.describe_current_room(&player).await;
        }

        // Look at specific target (object in room or inventory)
        let target_name = target.unwrap().to_uppercase();
        let room_id = player.current_room.clone();

        // First, try to find object in player's inventory
        for object_id in &player.inventory {
            if let Ok(object) = self.store().get_object(object_id) {
                if object.name.to_uppercase() == target_name {
                    // Found object in inventory - display description
                    let mut response = format!("📦 {}\n{}", object.name, object.description);

                    // Execute OnLook trigger if present
                    let trigger_messages =
                        execute_on_look(&object, &player.username, &room_id, self.store());

                    // Append trigger messages
                    for msg in trigger_messages {
                        response.push_str("\n");
                        response.push_str(&msg);
                    }

                    return Ok(response);
                }
            }
        }

        // Next, try to find object in current room
        if let Ok(room) = self.store().get_room(&room_id) {
            for object_id in &room.items {
                if let Ok(object) = self.store().get_object(object_id) {
                    if object.name.to_uppercase() == target_name {
                        // Found object in room - display description
                        let mut response = format!("👁️  {}\n{}", object.name, object.description);

                        // Execute OnLook trigger if present
                        let trigger_messages =
                            execute_on_look(&object, &player.username, &room_id, self.store());

                        // Append trigger messages
                        for msg in trigger_messages {
                            response.push_str("\n");
                            response.push_str(&msg);
                        }

                        return Ok(response);
                    }
                }
            }
        }

        // Object not found
        Ok(format!(
            "You don't see '{}' here.\nType LOOK to see the room, or INVENTORY to check what you're carrying.",
            target_name
        ))
    }

    /// Handle movement commands
    async fn handle_move(
        &mut self,
        session: &Session,
        direction: Direction,
        config: &Config,
    ) -> Result<String> {
        let mut player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Track previous location for instance cleanup
        let previous_room_id = player.current_room.clone();

        // Resolve destination before getting room manager (avoids borrow conflict)
        let destination_id = {
            let current_room = match self.store().get_room(&player.current_room) {
                Ok(room) => room,
                Err(_) => {
                    return Ok(format!(
                        "You seem to be lost! Your current location '{}' doesn't exist.\nType WHERE for help.",
                        player.current_room
                    ));
                }
            };

            // Check if exit exists in that direction
            let tmush_direction = match direction {
                Direction::North => TmushDirection::North,
                Direction::South => TmushDirection::South,
                Direction::East => TmushDirection::East,
                Direction::West => TmushDirection::West,
                Direction::Up => TmushDirection::Up,
                Direction::Down => TmushDirection::Down,
                Direction::Northeast => TmushDirection::Northeast,
                Direction::Northwest => TmushDirection::Northwest,
                Direction::Southeast => TmushDirection::Southeast,
                Direction::Southwest => TmushDirection::Southwest,
            };

            let dest_id = match current_room.exits.get(&tmush_direction) {
                Some(dest) => dest.clone(),
                None => {
                    let dir_str = format!("{:?}", direction).to_lowercase();
                    return Ok(format!("You can't go {} from here.", dir_str));
                }
            };

            match self
                .store()
                .resolve_destination_for_player(&player.username, &dest_id)
            {
                Ok(room_id) => room_id,
                Err(e) => {
                    return Ok(format!("Movement failed: {}", e));
                }
            }
        };

        // Get room manager after resolving destination
        let room_manager = self.get_room_manager().await?;

        // Use room manager to move player (includes capacity and permission checks)
        match room_manager.move_player_to_room(&mut player, &destination_id) {
            Ok(true) => {
                // Movement successful
                debug!(
                    "Player {} moved to room {}",
                    player.username, destination_id
                );
            }
            Ok(false) => {
                // Movement blocked (capacity or permissions)
                let dir_str = format!("{:?}", direction).to_lowercase();
                return Ok(format!(
                    "You can't go {} right now. The area might be full or restricted.",
                    dir_str
                ));
            }
            Err(e) => {
                return Ok(format!("Movement failed: {}", e));
            }
        }

        // Save updated player state
        if let Err(e) = self.store().put_player(player.clone()) {
            return Ok(format!("Movement failed to save: {}", e));
        }

        self.store().cleanup_landing_instance_after_move(
            &player.username,
            &previous_room_id,
            &player.current_room,
        );

        // Execute OnEnter triggers for all objects in the new room
        let enter_messages = execute_room_on_enter(&player.username, &destination_id, self.store());

        // Check for tutorial progression after movement
        use crate::tmush::tutorial::{
            advance_tutorial_step, can_advance_from_location, get_tutorial_hint,
        };
        use crate::tmush::types::{TutorialState, TutorialStep};

        let mut tutorial_message = String::new();
        let mut tutorial_advanced = false;
        if let TutorialState::InProgress { step } = &player.tutorial_state {
            let reached_target = can_advance_from_location(step, &destination_id);
            if reached_target && matches!(step, TutorialStep::MeetTheMayor) {
                // Final step requires speaking with the mayor
                tutorial_message = format!("\n💡 Tutorial: {}\n", get_tutorial_hint(step));
            } else if reached_target {
                let current_step = step.clone();
                match advance_tutorial_step(self.store(), &player.username, current_step) {
                    Ok(new_state) => {
                        tutorial_advanced = true;
                        match new_state {
                            TutorialState::InProgress { step: new_step } => {
                                tutorial_message = format!(
                                    "\n🎯 Tutorial Progress!\n{}\n",
                                    get_tutorial_hint(&new_step)
                                );
                            }
                            TutorialState::Completed { .. } => {
                                // Tutorial completed - rewards are given when talking to mayor
                                tutorial_message =
                                    "\n✨ Tutorial area complete! Great job!\n".to_string();
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        debug!("Tutorial advancement error: {}", e);
                    }
                }
            } else {
                // Still in same step - show reminder hint
                tutorial_message = format!("\n💡 Tutorial: {}\n", get_tutorial_hint(step));
            }
        }

        // Reload player if tutorial advanced (to get updated state)
        if tutorial_advanced {
            player = match self.store().get_player(&player.username) {
                Ok(p) => p,
                Err(_) => player, // Fall back to old player if reload fails
            };
        }

        // Show the new room
        let mut response = String::new();
        response.push_str(&format!(
            "You go {}.\n\n",
            format!("{:?}", direction).to_lowercase()
        ));

        // Add room description
        match self.describe_current_room(&player).await {
            Ok(desc) => response.push_str(&desc),
            Err(_) => response.push_str("The room description is unavailable."),
        }

        // Add OnEnter trigger messages if any
        if !enter_messages.is_empty() {
            response.push_str("\n");
            for msg in enter_messages {
                response.push_str(&msg);
                response.push_str("\n");
            }
        }

        // Add tutorial hint if in progress
        response.push_str(&tutorial_message);

        Ok(response)
    }

    /// Handle WHERE command - show current location
    async fn handle_where(
        &mut self,
        session: &Session,
        target_username: Option<String>,
        config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // If no target specified, show own location (original WHERE behavior)
        if target_username.is_none() {
            let room_manager = self.get_room_manager().await?;

            match room_manager.get_room(&player.current_room) {
                Ok(room) => {
                    let occupancy = room_manager.get_room_occupancy(&player.current_room);
                    let capacity_limit = if room.max_capacity > 0 {
                        room.max_capacity
                    } else {
                        // Use default capacity based on room type
                        if room.flags.contains(&crate::tmush::types::RoomFlag::Shop) {
                            10
                        } else if room.flags.contains(&crate::tmush::types::RoomFlag::Indoor) {
                            20
                        } else {
                            50
                        }
                    };

                    Ok(format!(
                        "You are in: {} ({})\n{}\nOccupancy: {}/{}",
                        room.name, player.current_room, room.short_desc, occupancy, capacity_limit
                    ))
                }
                Err(_) => Ok(format!(
                    "You are lost in: {}\n(Room not found - contact admin)",
                    player.current_room
                )),
            }
        } else {
            // Target specified - admin command to locate another player
            let store = self.store();

            // Check if caller is admin
            if !player.is_admin() {
                return Ok("⛔ Permission denied: Not an administrator\n\nThis command is for administrators only.".to_string());
            }

            let target = target_username.unwrap();

            // Look up target player
            let target_player = match store.get_player(&target) {
                Ok(p) => p,
                Err(_) => {
                    return Ok(format!(
                        "❌ Player not found: {}\n\nCheck the spelling and try again.",
                        target
                    ))
                }
            };

            let mut response = String::from("📍 PLAYER LOCATION\n\n");
            response.push_str(&format!(
                "Player: {} ({})\n",
                target_player.display_name, target_player.username
            ));

            if target_player.is_admin() {
                let level_name = match target_player.admin_level() {
                    1 => "Moderator",
                    2 => "Admin",
                    3 => "Sysop",
                    _ => "Admin",
                };
                response.push_str(&format!(
                    "Admin Level: {} ({})\n",
                    target_player.admin_level(),
                    level_name
                ));
            }

            response.push_str(&format!(
                "\nCurrent Location: {}\n",
                target_player.current_room
            ));

            // Try to get room details
            let room_mgr = self.get_room_manager().await?;
            if let Ok(room) = room_mgr.get_room(&target_player.current_room) {
                response.push_str(&format!("Room: {}\n", room.name));
            }

            response.push_str(&format!(
                "\nYou can teleport there with: /GOTO {}\n",
                target_player.current_room
            ));
            response.push_str(&format!(
                "Or teleport to the player with: /GOTO {}\n",
                target_player.username
            ));

            Ok(response)
        }
    }

    /// Handle MAP command - show overview of the game world
    async fn handle_map(&mut self, session: &Session, _config: &Config) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        let current_room_id = &player.current_room;
        let room_manager = self.get_room_manager().await?;

        let current_room = match room_manager.get_room(current_room_id) {
            Ok(room) => room,
            Err(_) => return Ok("You can't see a map from here.".to_string()),
        };

        let mut response = String::new();
        response.push_str(&format!("=== Local Area: {} ===\n\n", current_room.name));

        if current_room.exits.is_empty() {
            response.push_str("There are no visible exits from here.\n");
        } else {
            response.push_str("You can see paths leading:\n");

            // Sort exits for consistent display
            let mut exits: Vec<_> = current_room.exits.iter().collect();
            exits.sort_by_key(|(direction, _)| format!("{:?}", direction));

            for (direction, destination_id) in exits {
                if let Ok(dest_room) = room_manager.get_room(destination_id) {
                    response.push_str(&format!("  {:?} → {}\n", direction, dest_room.name));
                } else {
                    response.push_str(&format!("  {:?} → somewhere...\n", direction));
                }
            }
        }

        Ok(response)
    }

    /// Handle INVENTORY command
    async fn handle_inventory(&mut self, session: &Session, _config: &Config) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Use new inventory system if available
        if !player.inventory_stacks.is_empty() {
            let store = self.store();
            let get_item = |object_id: &str| store.get_object(object_id).ok();
            let inventory_lines = format_inventory_compact(&player, get_item);
            Ok(inventory_lines.join("\n"))
        } else if player.inventory.is_empty() {
            Ok("You are carrying nothing.".to_string())
        } else {
            // Fallback to legacy inventory display
            let mut response = "=== INVENTORY ===\n".to_string();
            response.push_str(&format!("Gold: {}\n", player.credits));
            response.push_str(&format!("Items: {}\n", player.inventory.len()));

            for (i, item_id) in player.inventory.iter().enumerate() {
                response.push_str(&format!("{}. {}\n", i + 1, item_id));
            }

            Ok(response)
        }
    }

    /// Handle TAKE/GET command - pick up items from room
    async fn handle_take(
        &mut self,
        session: &Session,
        item_name: String,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::inventory::{add_item_to_inventory, can_add_item};
        use crate::tmush::trigger::integration::execute_on_take;
        use crate::tmush::types::InventoryConfig;

        let player_node_id = &session.node_id;

        // Parse item name - support "get 5 coins" or "get coins"
        let (quantity, item_name) = if let Some(first_space) = item_name.find(' ') {
            let (first, rest) = item_name.split_at(first_space);
            if let Ok(qty) = first.trim().parse::<u32>() {
                (qty, rest.trim().to_uppercase())
            } else {
                (1, item_name.to_uppercase())
            }
        } else {
            (1, item_name.to_uppercase())
        };

        // Get player
        let mut player = match self.get_or_create_player(session).await {
            Ok(p) => p,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        let room_id = player.current_room.clone();

        // Get current room to access items list
        let mut room = match self.store().get_room(&room_id) {
            Ok(r) => r,
            Err(_) => return Ok("Error: Cannot access current room.".to_string()),
        };

        // Use fuzzy matching to find objects
        let matches = self.find_objects_by_partial_name(&item_name, &room.items);

        // Handle different match scenarios
        let object = match matches.len() {
            0 => return Ok(format!("There is no '{}' here to take.", item_name)),
            1 => matches.into_iter().next().unwrap(), // Auto-select single match
            _ => {
                // Multiple matches - create disambiguation session
                let matched_ids: Vec<String> = matches.iter().map(|o| o.id.clone()).collect();
                let matched_names: Vec<String> = matches.iter().map(|o| o.name.clone()).collect();

                let disambiguation = crate::tmush::types::DisambiguationSession::new(
                    &player.username,
                    "take",
                    &item_name,
                    matched_ids,
                    matched_names,
                    crate::tmush::types::DisambiguationContext::Room,
                );

                // Store session
                if let Err(e) = self
                    .store()
                    .put_disambiguation_session(disambiguation.clone())
                {
                    return Ok(format!("Error creating disambiguation: {}", e));
                }

                return Ok(disambiguation.format_prompt());
            }
        };

        // Check if object is takeable
        if !object.takeable {
            return Ok(format!("You cannot take the {}.", object.name));
        }

        // Check if locked by another player
        if object.locked {
            if let crate::tmush::types::ObjectOwner::Player {
                username: ref owner,
            } = object.owner
            {
                if owner != &player.username {
                    return Ok(format!("The {} is locked by its owner.", object.name));
                }
            }
        }

        // Use inventory config
        let inventory_config = InventoryConfig::default();

        // Check if player can add item to inventory
        let store = self.store();
        let get_item = |object_id: &str| store.get_object(object_id).ok();
        if let Err(reason) = can_add_item(&player, &object, quantity, &inventory_config, get_item) {
            return Ok(reason);
        }

        // Fire OnTake trigger before taking
        let trigger_messages = execute_on_take(&object, &player.username, &room_id, self.store());

        // Remove from room
        room.items.retain(|id| id != &object.id);
        if let Err(e) = self.store().put_room(room) {
            return Ok(format!("Error updating room: {}", e));
        }

        // Add to player inventory using new system
        let _result = add_item_to_inventory(&mut player, &object, quantity, &inventory_config);

        // Record ownership transfer
        let mut updated_object = object.clone();
        Self::record_ownership_transfer(
            &mut updated_object,
            Some(format!("{:?}", object.owner)),
            player.username.clone(),
            crate::tmush::types::OwnershipReason::PickedUp,
        );
        updated_object.owner = crate::tmush::types::ObjectOwner::Player {
            username: player.username.clone(),
        };

        // Save updated object
        if let Err(e) = self.store().put_object(updated_object) {
            return Ok(format!("Error saving object: {}", e));
        }

        // Save updated player
        if let Err(e) = self.store().put_player(player) {
            return Ok(format!("Error saving player: {}", e));
        }

        // Build response with trigger messages
        let mut response = format!("You take the {}.", object.name);
        if quantity > 1 {
            response = format!("You take {} {}.", quantity, object.name);
        }

        // Append trigger messages
        for msg in trigger_messages {
            response.push_str("\n");
            response.push_str(&msg);
        }

        Ok(response)
    }

    /// Handle TAKE command with specific object ID (used after disambiguation)
    async fn handle_take_by_id(
        &mut self,
        session: &Session,
        object_id: String,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::trigger::integration::execute_on_take;

        let mut player = match self.get_or_create_player(session).await {
            Ok(p) => p,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        let room_id = player.current_room.clone();

        // Get current room
        let mut room = match self.store().get_room(&room_id) {
            Ok(r) => r,
            Err(_) => return Ok("Error: Cannot access current room.".to_string()),
        };

        // Verify object is in the room
        if !room.items.contains(&object_id) {
            return Ok("That object is no longer here.".to_string());
        }

        // Get the object
        let object = match self.store().get_object(&object_id) {
            Ok(obj) => obj,
            Err(_) => return Ok("Error loading object.".to_string()),
        };

        // Check if object is takeable
        if !object.takeable {
            return Ok(format!("You cannot take the {}.", object.name));
        }

        // Check if locked by another player
        if object.locked {
            if let crate::tmush::types::ObjectOwner::Player {
                username: ref owner,
            } = object.owner
            {
                if owner != &player.username {
                    return Ok(format!("The {} is locked by its owner.", object.name));
                }
            }
        }

        // Use inventory system
        use crate::tmush::inventory::{add_item_to_inventory, can_add_item};
        use crate::tmush::types::InventoryConfig;

        let inventory_config = InventoryConfig::default();
        let store = self.store();
        let get_item = |object_id: &str| store.get_object(object_id).ok();

        if let Err(reason) = can_add_item(&player, &object, 1, &inventory_config, get_item) {
            return Ok(reason);
        }

        // Fire OnTake trigger
        let trigger_messages = execute_on_take(&object, &player.username, &room_id, self.store());

        // Remove from room
        room.items.retain(|id| id != &object.id);
        if let Err(e) = self.store().put_room(room) {
            return Ok(format!("Error updating room: {}", e));
        }

        // Add to player inventory using new system
        let _result = add_item_to_inventory(&mut player, &object, 1, &inventory_config);

        // Record ownership transfer
        let mut updated_object = object.clone();
        Self::record_ownership_transfer(
            &mut updated_object,
            Some(format!("{:?}", object.owner)),
            player.username.clone(),
            crate::tmush::types::OwnershipReason::PickedUp,
        );
        updated_object.owner = crate::tmush::types::ObjectOwner::Player {
            username: player.username.clone(),
        };

        // Save updated object and player
        if let Err(e) = self.store().put_object(updated_object) {
            return Ok(format!("Error saving object: {}", e));
        }
        if let Err(e) = self.store().put_player(player) {
            return Ok(format!("Error saving player: {}", e));
        }

        // Build response with trigger messages
        let mut response = format!("You take the {}.", object.name);
        for msg in trigger_messages {
            response.push_str("\n");
            response.push_str(&msg);
        }

        Ok(response)
    }

    /// Handle DROP command - drop items into current room
    async fn handle_drop(
        &mut self,
        session: &Session,
        item_name: String,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::trigger::integration::execute_on_drop;

        let player_node_id = &session.node_id;

        // Parse item name - support "drop 5 coins" or "drop coins"
        let (quantity, item_name) = if let Some(first_space) = item_name.find(' ') {
            let (first, rest) = item_name.split_at(first_space);
            if let Ok(qty) = first.trim().parse::<u32>() {
                (qty, rest.trim().to_uppercase())
            } else {
                (1, item_name.to_uppercase())
            }
        } else {
            (1, item_name.to_uppercase())
        };

        // Get player
        let mut player = match self.get_or_create_player(session).await {
            Ok(p) => p,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        let room_id = player.current_room.clone();

        // Extract object IDs from inventory_stacks for fuzzy matching
        let inventory_ids: Vec<String> = player
            .inventory_stacks
            .iter()
            .map(|stack| stack.object_id.clone())
            .collect();

        // Use fuzzy matching to find objects in inventory
        let matches = self.find_objects_by_partial_name(&item_name, &inventory_ids);

        // Handle different match scenarios
        let object = match matches.len() {
            0 => return Ok(format!("You don't have a '{}' to drop.", item_name)),
            1 => matches.into_iter().next().unwrap(), // Auto-select single match
            _ => {
                // Multiple matches - create disambiguation session
                let matched_ids: Vec<String> = matches.iter().map(|o| o.id.clone()).collect();
                let matched_names: Vec<String> = matches.iter().map(|o| o.name.clone()).collect();

                let disambiguation = crate::tmush::types::DisambiguationSession::new(
                    &player.username,
                    "drop",
                    &item_name,
                    matched_ids,
                    matched_names,
                    crate::tmush::types::DisambiguationContext::Inventory,
                );

                // Store session
                if let Err(e) = self
                    .store()
                    .put_disambiguation_session(disambiguation.clone())
                {
                    return Ok(format!("Error creating disambiguation: {}", e));
                }

                return Ok(disambiguation.format_prompt());
            }
        };

        // Get current room
        let mut room = match self.store().get_room(&room_id) {
            Ok(r) => r,
            Err(_) => return Ok("Error: Cannot access current room.".to_string()),
        };

        // Fire OnDrop trigger before dropping
        let trigger_messages = execute_on_drop(&object, &player.username, &room_id, self.store());

        // Remove from player inventory using inventory system
        if !Self::remove_item_from_player(&mut player, &object.id, quantity) {
            return Ok(format!(
                "Error: Failed to remove {} from inventory.",
                object.name
            ));
        }

        // Add to room
        room.items.push(object.id.clone());
        if let Err(e) = self.store().put_room(room) {
            return Ok(format!("Error updating room: {}", e));
        }

        // Record ownership transfer
        let mut updated_object = object.clone();
        Self::record_ownership_transfer(
            &mut updated_object,
            Some(player.username.clone()),
            "WORLD".to_string(),
            crate::tmush::types::OwnershipReason::Dropped,
        );
        updated_object.owner = crate::tmush::types::ObjectOwner::World;

        // Save updated object
        if let Err(e) = self.store().put_object(updated_object) {
            return Ok(format!("Error saving object: {}", e));
        }

        // Save updated player
        if let Err(e) = self.store().put_player(player) {
            return Ok(format!("Error saving player: {}", e));
        }

        // Build response with trigger messages
        let mut response = format!("You drop the {}.", object.name);
        if quantity > 1 {
            response = format!("You drop {} {}.", quantity, object.name);
        }

        // Append trigger messages
        for msg in trigger_messages {
            response.push_str("\n");
            response.push_str(&msg);
        }

        Ok(response)
    }

    /// Handle DROP command with specific object ID (used after disambiguation)
    async fn handle_drop_by_id(
        &mut self,
        session: &Session,
        object_id: String,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::trigger::integration::execute_on_drop;

        let mut player = match self.get_or_create_player(session).await {
            Ok(p) => p,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        let room_id = player.current_room.clone();

        // Verify object is in player's inventory
        if !Self::player_has_item(&player, &object_id) {
            return Ok("You don't have that item anymore.".to_string());
        }

        // Get the object
        let object = match self.store().get_object(&object_id) {
            Ok(obj) => obj,
            Err(_) => return Ok("Error loading object.".to_string()),
        };

        // Get current room
        let mut room = match self.store().get_room(&room_id) {
            Ok(r) => r,
            Err(_) => return Ok("Error: Cannot access current room.".to_string()),
        };

        // Fire OnDrop trigger
        let trigger_messages = execute_on_drop(&object, &player.username, &room_id, self.store());

        // Remove from player inventory using inventory system
        if !Self::remove_item_from_player(&mut player, &object.id, 1) {
            return Ok(format!(
                "Error: Failed to remove {} from inventory.",
                object.name
            ));
        }

        // Add to room
        room.items.push(object.id.clone());
        if let Err(e) = self.store().put_room(room) {
            return Ok(format!("Error updating room: {}", e));
        }

        // Record ownership transfer
        let mut updated_object = object.clone();
        Self::record_ownership_transfer(
            &mut updated_object,
            Some(format!("Player:{}", player.username)),
            "World".to_string(),
            crate::tmush::types::OwnershipReason::Dropped,
        );
        updated_object.owner = crate::tmush::types::ObjectOwner::World;

        // Save updated object and player
        if let Err(e) = self.store().put_object(updated_object) {
            return Ok(format!("Error saving object: {}", e));
        }
        if let Err(e) = self.store().put_player(player) {
            return Ok(format!("Error saving player: {}", e));
        }

        // Build response with trigger messages
        let mut response = format!("You drop the {}.", object.name);
        for msg in trigger_messages {
            response.push_str("\n");
            response.push_str(&msg);
        }

        Ok(response)
    }

    /// Handle EXAMINE command - show detailed item information
    async fn handle_examine(
        &mut self,
        session: &Session,
        target: String,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::inventory::{format_item_examination, get_item_quantity};

        let target = target.to_uppercase();

        let mut player = match self.get_or_create_player(session).await {
            Ok(p) => p,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Extract object IDs from inventory_stacks for fuzzy matching
        let inventory_ids: Vec<String> = player
            .inventory_stacks
            .iter()
            .map(|stack| stack.object_id.clone())
            .collect();

        // First, try inventory with fuzzy matching
        let inv_matches = self.find_objects_by_partial_name(&target, &inventory_ids);

        if !inv_matches.is_empty() {
            match inv_matches.len() {
                1 => {
                    let object = inv_matches.into_iter().next().unwrap();
                    let quantity = get_item_quantity(&player, &object.id);
                    let examination = format_item_examination(&object, quantity);

                    // Track carved symbol examination for grove mystery puzzle (Phase 4.2)
                    if object.id.starts_with("carved_symbol_") {
                        self.track_symbol_examination(&mut player, &object.id)
                            .await?;
                    }

                    return Ok(examination.join("\n"));
                }
                _ => {
                    // Multiple matches in inventory
                    let matched_ids: Vec<String> =
                        inv_matches.iter().map(|o| o.id.clone()).collect();
                    let matched_names: Vec<String> =
                        inv_matches.iter().map(|o| o.name.clone()).collect();

                    let disambiguation = crate::tmush::types::DisambiguationSession::new(
                        &player.username,
                        "examine",
                        &target,
                        matched_ids,
                        matched_names,
                        crate::tmush::types::DisambiguationContext::Inventory,
                    );

                    if let Err(e) = self
                        .store()
                        .put_disambiguation_session(disambiguation.clone())
                    {
                        return Ok(format!("Error creating disambiguation: {}", e));
                    }

                    return Ok(disambiguation.format_prompt());
                }
            }
        }

        // Then try to find object in current room with fuzzy matching
        let room = match self.store().get_room(&player.current_room) {
            Ok(r) => r,
            Err(_) => return Ok("Error: Cannot access current room.".to_string()),
        };

        let room_matches = self.find_objects_by_partial_name(&target, &room.items);

        match room_matches.len() {
            0 => {
                return Ok(format!(
                    "You don't see '{}' here.\nType LOOK to see the room, or INVENTORY to check what you're carrying.",
                    target
                ));
            }
            1 => {
                let object = room_matches.into_iter().next().unwrap();
                let examination = format_item_examination(&object, 1);

                // Track carved symbol examination for grove mystery puzzle (Phase 4.2)
                if object.id.starts_with("carved_symbol_") {
                    self.track_symbol_examination(&mut player, &object.id)
                        .await?;
                }

                return Ok(examination.join("\n"));
            }
            _ => {
                // Multiple matches in room
                let matched_ids: Vec<String> = room_matches.iter().map(|o| o.id.clone()).collect();
                let matched_names: Vec<String> =
                    room_matches.iter().map(|o| o.name.clone()).collect();

                let disambiguation = crate::tmush::types::DisambiguationSession::new(
                    &player.username,
                    "examine",
                    &target,
                    matched_ids,
                    matched_names,
                    crate::tmush::types::DisambiguationContext::Room,
                );

                if let Err(e) = self
                    .store()
                    .put_disambiguation_session(disambiguation.clone())
                {
                    return Ok(format!("Error creating disambiguation: {}", e));
                }

                return Ok(disambiguation.format_prompt());
            }
        }
    }

    /// Handle EXAMINE command with specific object ID (used after disambiguation)
    async fn handle_examine_by_id(
        &mut self,
        session: &Session,
        object_id: String,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::inventory::{format_item_examination, get_item_quantity};

        let mut player = match self.get_or_create_player(session).await {
            Ok(p) => p,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Get the object
        let object = match self.store().get_object(&object_id) {
            Ok(obj) => obj,
            Err(_) => return Ok("Error loading object.".to_string()),
        };

        // Determine quantity if in inventory
        let quantity = if Self::player_has_item(&player, &object_id) {
            get_item_quantity(&player, &object_id)
        } else {
            1
        };

        let examination = format_item_examination(&object, quantity);

        // Track carved symbol examination for grove mystery puzzle (Phase 4.2)
        if object.id.starts_with("carved_symbol_") {
            self.track_symbol_examination(&mut player, &object.id)
                .await?;
        }

        Ok(examination.join("\n"))
    }

    /// Handle CRAFT command - craft items from materials (Phase 4.4)
    async fn handle_craft(
        &mut self,
        session: &Session,
        recipe_name: String,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::inventory::{add_item_to_inventory, remove_item_from_inventory};
        use crate::tmush::quest::update_quest_objective;
        use crate::tmush::types::{ObjectiveType, QuestState};

        let recipe_name = recipe_name.to_lowercase();

        let mut player = match self.get_or_create_player(session).await {
            Ok(p) => p,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Look up recipe in database (data-driven as of crafting refactor)
        let recipe = match self.find_recipe_by_name_or_id(&recipe_name) {
            Some(r) => r,
            None => {
                // List available recipes
                let all_recipes = self.store().list_recipes(None)?;
                let recipe_names: Vec<String> =
                    all_recipes.iter().map(|r| r.name.clone()).collect();

                return Ok(format!(
                    "Unknown recipe: '{}'\nAvailable recipes: {}",
                    recipe_name,
                    if recipe_names.is_empty() {
                        "(none)".to_string()
                    } else {
                        recipe_names.join(", ")
                    }
                ));
            }
        };

        // Check if station requirement is met
        if let Some(required_station) = &recipe.requires_station {
            // Check if player has the station in inventory or is in a room with it
            let has_station = player
                .inventory_stacks
                .iter()
                .any(|stack| stack.object_id.starts_with(required_station));

            if !has_station {
                return Ok(format!(
                    "You need a {} to craft {}.",
                    required_station, recipe.name
                ));
            }
        }

        // Check if player has all required materials
        let mut missing_materials = Vec::new();
        for material in &recipe.materials {
            if !material.consumed {
                // Tool requirement - just need to have it
                let has_tool = player
                    .inventory_stacks
                    .iter()
                    .any(|stack| stack.object_id.starts_with(&material.item_id));
                if !has_tool {
                    missing_materials.push(format!("{} (tool)", material.item_id));
                }
            } else {
                // Material requirement - need enough to consume
                let owned_qty: u32 = player
                    .inventory_stacks
                    .iter()
                    .filter(|stack| stack.object_id.starts_with(&material.item_id))
                    .map(|stack| stack.quantity)
                    .sum();

                if owned_qty < material.quantity {
                    missing_materials.push(format!(
                        "{} (need {}, have {})",
                        material.item_id, material.quantity, owned_qty
                    ));
                }
            }
        }

        if !missing_materials.is_empty() {
            return Ok(format!(
                "You don't have all the required materials to craft {}.\n\
Missing: {}",
                recipe.name,
                missing_materials.join(", ")
            ));
        }

        // Consume materials from inventory (but not tools)
        for material in &recipe.materials {
            if material.consumed {
                // Find matching items and remove required quantity
                let matching_stacks: Vec<String> = player
                    .inventory_stacks
                    .iter()
                    .filter(|stack| stack.object_id.starts_with(&material.item_id))
                    .map(|stack| stack.object_id.clone())
                    .collect();

                let mut remaining = material.quantity;
                for object_id in matching_stacks {
                    if remaining == 0 {
                        break;
                    }

                    match remove_item_from_inventory(&mut player, &object_id, remaining) {
                        crate::tmush::types::InventoryResult::Removed { quantity, .. } => {
                            remaining = remaining.saturating_sub(quantity);
                        }
                        crate::tmush::types::InventoryResult::Failed { reason } => {
                            return Ok(format!(
                                "Failed to remove material {}: {}",
                                material.item_id, reason
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }

        // Create the crafted item(s)
        let inv_config = crate::tmush::types::InventoryConfig {
            allow_stacking: true,
            max_weight: 1000,
            max_stacks: 100,
        };

        for _ in 0..recipe.result_quantity {
            let crafted_item = self.create_crafted_item(&recipe.result_item_id)?;
            self.store().put_object(crafted_item.clone())?;

            match add_item_to_inventory(&mut player, &crafted_item, 1, &inv_config) {
                crate::tmush::types::InventoryResult::Added { .. } => {}
                crate::tmush::types::InventoryResult::Failed { reason } => {
                    return Ok(format!(
                        "Failed to add crafted item to inventory: {}",
                        reason
                    ));
                }
                _ => {}
            }
        }

        // Save player
        self.store().put_player(player.clone())?;

        // Update quest objective if player has first_craft quest
        let has_craft_quest = player
            .quests
            .iter()
            .any(|q| q.quest_id == "first_craft" && matches!(q.state, QuestState::Active { .. }));

        if has_craft_quest {
            let _ = update_quest_objective(
                self.store(),
                &player.username,
                "first_craft",
                &ObjectiveType::UseItem {
                    item_id: "crafting_bench".to_string(),
                    target: "craft".to_string(),
                },
                1,
            );
        }

        let result_msg = if recipe.result_quantity > 1 {
            format!(
                "You successfully craft {} x{}!",
                recipe.name, recipe.result_quantity
            )
        } else {
            format!("You successfully craft {}!", recipe.name)
        };

        let description = if recipe.description.is_empty() {
            "".to_string()
        } else {
            format!("\n{}", recipe.description)
        };

        Ok(format!("{}{}", result_msg, description))
    }

    /// Find a recipe by ID or name (case-insensitive)
    fn find_recipe_by_name_or_id(
        &self,
        search: &str,
    ) -> Option<crate::tmush::types::CraftingRecipe> {
        let all_recipes = self.store().list_recipes(None).ok()?;
        let search_lower = search.to_lowercase();

        // First try exact ID match
        for recipe in &all_recipes {
            if recipe.id == search_lower {
                return Some(recipe.clone());
            }
        }

        // Then try case-insensitive name match
        for recipe in &all_recipes {
            if recipe.name.to_lowercase() == search_lower {
                return Some(recipe.clone());
            }
        }

        None
    }

    /// Create a crafted item (Phase 4.4 helper)
    fn create_crafted_item(&self, item_id: &str) -> Result<crate::tmush::types::ObjectRecord> {
        use crate::tmush::types::{ObjectOwner, ObjectRecord, OBJECT_SCHEMA_VERSION};
        use chrono::Utc;
        use uuid::Uuid;

        let now = Utc::now();
        let unique_id = format!("{}_{}", item_id, Uuid::new_v4());

        let (name, description, weight, value) = match item_id {
            "signal_booster" => (
                "Signal Booster",
                "A hand-crafted signal booster that extends mesh network range by 50%. \
The copper coils are wrapped precisely, and the circuit board hums with potential. \
This represents your growing mastery of mesh technology.",
                3,
                100,
            ),
            "basic_antenna" => (
                "Basic Antenna",
                "A simple but functional antenna you crafted yourself. The copper wire \
is wound carefully around the rod, creating a reliable signal receiver. \
Not fancy, but it gets the job done.",
                2,
                25,
            ),
            _ => return Err(anyhow::anyhow!("Unknown crafted item: {}", item_id)),
        };

        Ok(ObjectRecord {
            id: unique_id,
            name: name.to_string(),
            description: description.to_string(),
            owner: ObjectOwner::Player {
                username: "crafter".to_string(),
            },
            created_at: now,
            weight,
            currency_value: crate::tmush::types::CurrencyAmount::decimal(value),
            value: value as u32,
            takeable: true,
            usable: false,
            actions: std::collections::HashMap::new(),
            flags: vec![],
            locked: false,
            clone_depth: 0,
            clone_source_id: None,
            clone_count: 0,
            created_by: "crafting".to_string(),
            ownership_history: vec![],
            schema_version: OBJECT_SCHEMA_VERSION,
        })
    }

    /// Track examination of carved symbols for grove mystery puzzle (Phase 4.2)
    /// Updates player's examined_symbol_sequence and checks quest progress
    async fn track_symbol_examination(
        &mut self,
        player: &mut crate::tmush::types::PlayerRecord,
        symbol_id: &str,
    ) -> Result<()> {
        use crate::tmush::quest::update_quest_objective;
        use crate::tmush::types::{ObjectiveType, QuestState};

        // Add symbol to sequence if not already the last one examined
        if player.examined_symbol_sequence.last() != Some(&symbol_id.to_string()) {
            player.examined_symbol_sequence.push(symbol_id.to_string());
            player.touch();
            self.store().put_player(player.clone())?;
        }

        // Check if player has grove_mystery quest active
        let has_grove_quest = player
            .quests
            .iter()
            .any(|q| q.quest_id == "grove_mystery" && matches!(q.state, QuestState::Active { .. }));

        if !has_grove_quest {
            return Ok(());
        }

        // Correct sequence: oak -> elm -> willow -> ash
        let correct_sequence = vec![
            "carved_symbols_oak",
            "carved_symbols_elm",
            "carved_symbols_willow",
            "carved_symbols_ash",
        ];

        // Check each position in the sequence
        for (idx, expected) in correct_sequence.iter().enumerate() {
            if player.examined_symbol_sequence.len() > idx
                && &player.examined_symbol_sequence[idx] == expected
            {
                // Update corresponding quest objective using UseItem type
                let objective = ObjectiveType::UseItem {
                    item_id: expected.to_string(),
                    target: "examine".to_string(),
                };

                // Update quest objective
                let _ = update_quest_objective(
                    self.store(),
                    &player.username,
                    "grove_mystery",
                    &objective,
                    1,
                );
            }
        }

        // If sequence is wrong, reset it
        if player.examined_symbol_sequence.len() >= correct_sequence.len() {
            let is_correct = player.examined_symbol_sequence[..correct_sequence.len()]
                .iter()
                .zip(correct_sequence.iter())
                .all(|(a, b)| a == b);

            if !is_correct {
                // Wrong sequence - reset
                player.examined_symbol_sequence.clear();
                player.touch();
                self.store().put_player(player.clone())?;
            }
        }

        Ok(())
    }

    /// Check if player has a light source in inventory (Phase 4.3)
    fn player_has_light_source(&self, player: &crate::tmush::types::PlayerRecord) -> bool {
        for object_id in &player.inventory {
            if let Ok(object) = self.store().get_object(object_id) {
                if object
                    .flags
                    .contains(&crate::tmush::types::ObjectFlag::LightSource)
                {
                    return true;
                }
            }
        }
        false
    }

    /// Handle USE command - use/activate an object with trigger execution
    async fn handle_use(
        &mut self,
        session: &Session,
        item_name: String,
        _config: &Config,
    ) -> Result<String> {
        let item_name = item_name.to_uppercase();

        // Get player
        let player = match self.get_or_create_player(session).await {
            Ok(p) => p,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Extract object IDs from inventory_stacks for fuzzy matching
        let inventory_ids: Vec<String> = player
            .inventory_stacks
            .iter()
            .map(|stack| stack.object_id.clone())
            .collect();

        // Use fuzzy matching to find objects in inventory
        let matches = self.find_objects_by_partial_name(&item_name, &inventory_ids);

        // Handle different match scenarios
        let object = match matches.len() {
            0 => return Ok(format!("You don't have '{}' in your inventory.", item_name)),
            1 => matches.into_iter().next().unwrap(), // Auto-select single match
            _ => {
                // Multiple matches - create disambiguation session
                let matched_ids: Vec<String> = matches.iter().map(|o| o.id.clone()).collect();
                let matched_names: Vec<String> = matches.iter().map(|o| o.name.clone()).collect();

                let disambiguation = crate::tmush::types::DisambiguationSession::new(
                    &player.username,
                    "use",
                    &item_name,
                    matched_ids,
                    matched_names,
                    crate::tmush::types::DisambiguationContext::Inventory,
                );

                // Store session
                if let Err(e) = self
                    .store()
                    .put_disambiguation_session(disambiguation.clone())
                {
                    return Ok(format!("Error creating disambiguation: {}", e));
                }

                return Ok(disambiguation.format_prompt());
            }
        };

        // Check if object is usable
        if !object.usable {
            return Ok(format!("{} cannot be used.", object.name));
        }

        // Execute OnUse trigger if present
        let trigger_messages = execute_on_use(
            &object,
            &session.display_name(),
            &player.current_room,
            self.store(),
        );

        // Build response
        let mut response = format!("You use {}.", object.name);

        // Add trigger messages
        for msg in trigger_messages {
            response.push_str("\n");
            response.push_str(&msg);
        }

        Ok(response)
    }

    /// Handle USE command with specific object ID (used after disambiguation)
    async fn handle_use_by_id(
        &mut self,
        session: &Session,
        object_id: String,
        _config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(p) => p,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Verify object is in player's inventory
        if !Self::player_has_item(&player, &object_id) {
            return Ok("You don't have that item anymore.".to_string());
        }

        // Get the object
        let object = match self.store().get_object(&object_id) {
            Ok(obj) => obj,
            Err(_) => return Ok("Error loading object.".to_string()),
        };

        // Check if object is usable
        if !object.usable {
            return Ok(format!("{} cannot be used.", object.name));
        }

        // Execute OnUse trigger if present
        let trigger_messages = execute_on_use(
            &object,
            &session.display_name(),
            &player.current_room,
            self.store(),
        );

        // Build response
        let mut response = format!("You use {}.", object.name);

        // Add trigger messages
        for msg in trigger_messages {
            response.push_str("\n");
            response.push_str(&msg);
        }

        Ok(response)
    }

    /// Handle POKE command - poke/prod an interactive object with trigger execution
    async fn handle_poke(
        &mut self,
        session: &Session,
        target_name: String,
        _config: &Config,
    ) -> Result<String> {
        let target_name = target_name.to_uppercase();

        // Get player and current room
        let player = match self.get_or_create_player(session).await {
            Ok(p) => p,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        let room = match self.store().get_room(&player.current_room) {
            Ok(r) => r,
            Err(e) => return Ok(format!("Error loading room: {}", e)),
        };

        // Search for object in room by name
        let object = match self.find_object_by_name(&target_name, &room.items) {
            Some(obj) => obj,
            None => {
                // Also check inventory
                let inventory_ids: Vec<String> = player
                    .inventory_stacks
                    .iter()
                    .map(|stack| stack.object_id.clone())
                    .collect();
                match self.find_object_by_name(&target_name, &inventory_ids) {
                    Some(obj) => obj,
                    None => {
                        return Ok(format!("You don't see '{}' here.", target_name));
                    }
                }
            }
        };

        // Execute OnPoke trigger if present
        let trigger_messages = execute_on_poke(
            &object,
            &session.display_name(),
            &player.current_room,
            self.store(),
        );

        // Build response
        let mut response = format!("You poke {}.", object.name);

        // Add trigger messages
        for msg in trigger_messages {
            response.push_str("\n");
            response.push_str(&msg);
        }

        Ok(response)
    }

    /// Handle BUY command - purchase items from shops in current room
    async fn handle_buy(
        &mut self,
        session: &Session,
        item_name: String,
        quantity: Option<u32>,
        _config: &Config,
    ) -> Result<String> {
        // Get player
        let mut player = match self.get_or_create_player(session).await {
            Ok(p) => p,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Get shops in current location
        let location = player.current_room.clone();
        let shops = match self.store().get_shops_in_location(&location) {
            Ok(s) => s,
            Err(e) => return Ok(format!("Error finding shops: {}", e)),
        };

        if shops.is_empty() {
            return Ok("There are no shops here.".to_string());
        }

        // Find shop that has the item (search by name, case-insensitive)
        let item_name_upper = item_name.to_uppercase();
        let mut found_shop = None;
        let mut found_object = None;

        for shop in shops {
            // Look through shop inventory for matching item name
            for object_id in shop.inventory.keys() {
                if let Ok(obj) = self.store().get_object(object_id) {
                    if obj.name.to_uppercase() == item_name_upper {
                        found_shop = Some(shop);
                        found_object = Some(obj);
                        break;
                    }
                }
            }
            if found_shop.is_some() {
                break;
            }
        }

        let (mut shop, object) = match (found_shop, found_object) {
            (Some(s), Some(obj)) => (s, obj),
            _ => return Ok(format!("No shop here sells '{}'.", item_name)),
        };

        let qty = quantity.unwrap_or(1);

        // Get the shop item to calculate price
        let shop_item = match shop.get_item(&object.id) {
            Some(item) => item,
            None => return Ok(format!("Shop doesn't sell '{}'.", object.name)),
        };

        // Calculate total price using shop's pricing logic (includes quantity)
        let total_price = shop.calculate_buy_price(&object, qty, shop_item);

        // Check if player can afford it
        if !player.currency.can_afford(&total_price) {
            return Ok(format!(
                "You cannot afford {} x {} (need {:?}, have {:?}).",
                qty, object.name, total_price, player.currency
            ));
        }

        // Process the purchase (updates shop inventory and currency)
        match shop.process_buy(&object.id, qty, &object) {
            Ok((price, actual_qty)) => {
                // Deduct currency from player
                player.currency = match player.currency.subtract(&price) {
                    Ok(new_balance) => new_balance,
                    Err(e) => return Ok(format!("Payment failed: {}", e)),
                };

                // Add items to player inventory using inventory system
                use crate::tmush::inventory::add_item_to_inventory;
                use crate::tmush::types::InventoryConfig;
                let config = InventoryConfig::default();

                for _ in 0..actual_qty {
                    let item_to_add = if shop.clone_items {
                        // Vending machine mode: clone the template object
                        match self.clone_object_internal(&object.id, &player.username, &object) {
                            Ok(cloned) => cloned,
                            Err(e) => {
                                return Ok(format!("Failed to clone item: {}", e));
                            }
                        }
                    } else {
                        // Traditional shop: prepare object for ownership transfer
                        let mut owned_object = object.clone();
                        owned_object.owner = crate::tmush::types::ObjectOwner::Player {
                            username: player.username.clone(),
                        };
                        Self::record_ownership_transfer(
                            &mut owned_object,
                            None, // Purchased from shop
                            player.username.clone(),
                            crate::tmush::types::OwnershipReason::Purchased,
                        );
                        owned_object
                    };

                    add_item_to_inventory(&mut player, &item_to_add, 1, &config);
                }

                // Capture balance before moving player
                let final_balance = format!("{:?}", player.currency);

                // Save shop and player (consume values)
                if let Err(e) = self.store().put_shop(shop) {
                    return Ok(format!("Failed to save shop: {}", e));
                }
                if let Err(e) = self.store().put_player(player) {
                    return Ok(format!("Failed to save player: {}", e));
                }

                Ok(format!(
                    "You buy {} x {} for {:?}. Balance: {}",
                    actual_qty, object.name, price, final_balance
                ))
            }
            Err(e) => Ok(format!("Purchase failed: {}", e)),
        }
    }

    /// Handle SELL command - sell items from inventory to shops
    async fn handle_sell(
        &mut self,
        session: &Session,
        item_name: String,
        quantity: Option<u32>,
        _config: &Config,
    ) -> Result<String> {
        // Get player
        let mut player = match self.get_or_create_player(session).await {
            Ok(p) => p,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Find item in player's inventory by name
        let item_name_upper = item_name.to_uppercase();
        let mut found_object_id = None;
        let mut found_object = None;

        for stack in &player.inventory_stacks {
            if let Ok(obj) = self.store().get_object(&stack.object_id) {
                if obj.name.to_uppercase() == item_name_upper {
                    found_object_id = Some(stack.object_id.clone());
                    found_object = Some(obj);
                    break;
                }
            }
        }

        let (object_id, object) = match (found_object_id, found_object) {
            (Some(id), Some(obj)) => (id, obj),
            _ => return Ok(format!("You don't have any '{}'.", item_name)),
        };

        let qty = quantity.unwrap_or(1);

        // Check if player has enough quantity
        let player_qty = player
            .inventory_stacks
            .iter()
            .find(|s| s.object_id == object_id)
            .map(|s| s.quantity)
            .unwrap_or(0);
        if player_qty < qty {
            return Ok(format!("You only have {} x {}.", player_qty, object.name));
        }

        // Get shops in current location
        let location = player.current_room.clone();
        let shops = match self.store().get_shops_in_location(&location) {
            Ok(s) => s,
            Err(e) => return Ok(format!("Error finding shops: {}", e)),
        };

        if shops.is_empty() {
            return Ok("There are no shops here to sell to.".to_string());
        }

        // Find a shop willing to buy this item (shop must have item in inventory to accept it)
        let mut found_shop = None;
        for shop in shops {
            if shop.get_item(&object_id).is_some() {
                found_shop = Some(shop);
                break;
            }
        }

        let mut shop = match found_shop {
            Some(s) => s,
            None => return Ok(format!("No shop here buys '{}'.", object.name)),
        };

        // Get shop item to calculate price
        let shop_item = shop.get_item(&object_id).unwrap(); // safe: we just checked

        // Calculate sell price using shop's pricing logic (includes quantity)
        let total_price = shop.calculate_sell_price(&object, qty, shop_item);

        // Check if shop can afford it
        if !shop.currency.can_afford(&total_price) {
            return Ok(format!(
                "The shop cannot afford to buy {} x {} (need {:?}, have {:?}).",
                qty, object.name, total_price, shop.currency
            ));
        }

        // Process the sale (updates shop inventory and currency)
        match shop.process_sell(&object_id, qty, &object) {
            Ok(price) => {
                // Add currency to player
                player.currency = match player.currency.add(&price) {
                    Ok(new_balance) => new_balance,
                    Err(e) => return Ok(format!("Payment failed: {}", e)),
                };

                // Remove items from player inventory using inventory system
                use crate::tmush::inventory::remove_item_from_inventory;
                remove_item_from_inventory(&mut player, &object_id, qty);

                // Capture balance before moving player
                let final_balance = format!("{:?}", player.currency);

                // Save shop and player (consume values)
                if let Err(e) = self.store().put_shop(shop) {
                    return Ok(format!("Failed to save shop: {}", e));
                }
                if let Err(e) = self.store().put_player(player) {
                    return Ok(format!("Failed to save player: {}", e));
                }

                Ok(format!(
                    "You sell {} x {} for {:?}. Balance: {}",
                    qty, object.name, price, final_balance
                ))
            }
            Err(e) => Ok(format!("Sale failed: {}", e)),
        }
    }

    /// Handle LIST/WARES command - display shop inventory with prices
    async fn handle_list(&mut self, session: &Session, _config: &Config) -> Result<String> {
        use crate::tmush::shop::format_shop_listing;

        // Get player to determine current location
        let player = match self.get_or_create_player(session).await {
            Ok(p) => p,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Get shops in current location
        let location = player.current_room.clone();
        let shops = match self.store().get_shops_in_location(&location) {
            Ok(s) => s,
            Err(e) => return Ok(format!("Error finding shops: {}", e)),
        };

        if shops.is_empty() {
            return Ok("There are no shops here.".to_string());
        }

        // Build listing for each shop
        let mut response = String::new();

        for (idx, shop) in shops.iter().enumerate() {
            if idx > 0 {
                response.push_str("\n---\n");
            }

            response.push_str(&format!("=== {} ===\n", shop.name));
            if !shop.description.is_empty() {
                response.push_str(&format!("{}\n\n", shop.description));
            }

            // Get object resolver closure
            let store = self.store();
            let get_object = |object_id: &str| store.get_object(object_id).ok();

            // Use shop formatting function
            let lines = format_shop_listing(shop, get_object);
            response.push_str(&lines.join("\n"));
        }

        if response.is_empty() {
            Ok("No shops available.".to_string())
        } else {
            Ok(response)
        }
    }

    /// Handle WHO command - list online players
    async fn handle_who(&mut self, _session: &Session, _config: &Config) -> Result<String> {
        let player_ids = match self.store().list_player_ids() {
            Ok(ids) => ids,
            Err(e) => return Ok(format!("Error listing players: {}", e)),
        };

        if player_ids.is_empty() {
            return Ok("No players found.".to_string());
        }

        let mut response = "=== ONLINE PLAYERS ===\n".to_string();
        for (i, username) in player_ids.iter().take(10).enumerate() {
            response.push_str(&format!("{}. {}\n", i + 1, username));
        }

        if player_ids.len() > 10 {
            response.push_str(&format!("... and {} more\n", player_ids.len() - 10));
        }

        Ok(response)
    }

    /// Handle SCORE command - show player stats
    async fn handle_score(&mut self, session: &Session, _config: &Config) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        let mut response = format!("=== {} ===\n", player.display_name);
        response.push_str(&format!("Location: {}\n", player.current_room));
        response.push_str(&format!("Credits: {}\n", player.credits));
        response.push_str(&format!(
            "HP: {}/{}\n",
            player.stats.hp, player.stats.max_hp
        ));
        response.push_str(&format!(
            "MP: {}/{}\n",
            player.stats.mp, player.stats.max_mp
        ));
        response.push_str(&format!("Items: {}\n", player.inventory.len()));
        response.push_str(&format!("State: {:?}\n", player.state));

        Ok(response)
    }

    /// Handle SAY command - speak to room
    async fn handle_say(
        &mut self,
        session: &Session,
        text: String,
        config: &Config,
    ) -> Result<String> {
        if text.trim().is_empty() {
            let store = self.store();
            let world_config = store.get_world_config()?;
            return Ok(world_config.err_say_what);
        }

        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Get other players in the same room
        let room_manager = self.get_room_manager().await?;
        let players_in_room = room_manager.get_players_in_room(&player.current_room);
        let others_count = players_in_room.len().saturating_sub(1); // Exclude self

        let mut response = format!("You say: \"{}\"\n", text);
        if others_count > 0 {
            response.push_str(&format!(
                "({} other player{} in room will also see this)\n",
                others_count,
                if others_count == 1 { "" } else { "s" }
            ));
        } else {
            response.push_str("(No other players in room)\n");
        }

        Ok(response)
    }

    /// Handle WHISPER command - private message to another player  
    async fn handle_whisper(
        &mut self,
        session: &Session,
        target: String,
        text: String,
        config: &Config,
    ) -> Result<String> {
        let world_config = self.get_world_config().await?;

        if text.trim().is_empty() {
            return Ok("Whisper what?".to_string());
        }

        if target.trim().is_empty() {
            return Ok("Whisper to whom?".to_string());
        }

        let speaker = session.display_name();
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Check if target player exists and is in same room
        let room_manager = self.get_room_manager().await?;
        let players_in_room = room_manager.get_players_in_room(&player.current_room);

        let target_lower = target.to_lowercase();
        let target_found = players_in_room
            .iter()
            .find(|p| p.to_lowercase().starts_with(&target_lower));

        if let Some(target_player) = target_found {
            if target_player.to_lowercase() == speaker.to_lowercase() {
                return Ok(world_config.err_whisper_self);
            }

            Ok(format!(
                "You whisper to {}: \"{}\"\n(Private message - only {} will see this)",
                target_player, text, target_player
            ))
        } else {
            Ok(format!(
                "Player '{}' not found in this room.\nPlayers here: {}",
                target,
                if players_in_room.is_empty() {
                    "none".to_string()
                } else {
                    players_in_room.join(", ")
                }
            ))
        }
    }

    /// Handle EMOTE command - perform an action
    async fn handle_emote(
        &mut self,
        session: &Session,
        text: String,
        config: &Config,
    ) -> Result<String> {
        let world_config = self.get_world_config().await?;

        if text.trim().is_empty() {
            return Ok(world_config.err_emote_what);
        }

        let speaker = session.display_name();
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Get other players in the same room
        let room_manager = self.get_room_manager().await?;
        let players_in_room = room_manager.get_players_in_room(&player.current_room);
        let others_count = players_in_room.len().saturating_sub(1); // Exclude self

        let mut response = format!("{} {}\n", speaker, text);
        if others_count > 0 {
            response.push_str(&format!(
                "(Action visible to {} other player{})\n",
                others_count,
                if others_count == 1 { "" } else { "s" }
            ));
        } else {
            response.push_str("(No other players in room to see your action)\n");
        }

        Ok(response)
    }

    /// Handle POSE command - strike a pose  
    async fn handle_pose(
        &mut self,
        session: &Session,
        text: String,
        config: &Config,
    ) -> Result<String> {
        if text.trim().is_empty() {
            return Ok("Strike what pose?".to_string());
        }

        let speaker = session.display_name();
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Get other players in the same room
        let room_manager = self.get_room_manager().await?;
        let players_in_room = room_manager.get_players_in_room(&player.current_room);
        let others_count = players_in_room.len().saturating_sub(1); // Exclude self

        // Pose format is different from emote - it's a descriptive action
        let mut response = format!("{} {}\n", speaker, text);
        if others_count > 0 {
            response.push_str(&format!(
                "(Pose visible to {} other player{})\n",
                others_count,
                if others_count == 1 { "" } else { "s" }
            ));
        } else {
            response.push_str("(No other players in room to see your pose)\n");
        }

        Ok(response)
    }

    /// Handle OOC command - out of character communication
    async fn handle_ooc(
        &mut self,
        session: &Session,
        text: String,
        config: &Config,
    ) -> Result<String> {
        if text.trim().is_empty() {
            return Ok("Say what out of character?".to_string());
        }

        let speaker = session.display_name();
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Get other players in the same room
        let room_manager = self.get_room_manager().await?;
        let players_in_room = room_manager.get_players_in_room(&player.current_room);
        let others_count = players_in_room.len().saturating_sub(1); // Exclude self

        let mut response = format!("[OOC] {}: {}\n", speaker, text);
        if others_count > 0 {
            response.push_str(&format!(
                "(OOC message visible to {} other player{})\n",
                others_count,
                if others_count == 1 { "" } else { "s" }
            ));
        } else {
            response.push_str("(No other players in room to see your OOC message)\n");
        }

        Ok(response)
    }

    /// Handle TUTORIAL command - manage tutorial progress
    async fn handle_tutorial(
        &mut self,
        session: &Session,
        subcommand: Option<String>,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::tutorial::{
            format_tutorial_status, restart_tutorial, skip_tutorial, start_tutorial,
        };

        let username = session.node_id.to_string();
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        match subcommand.as_deref() {
            None => {
                // Show current tutorial status
                Ok(format_tutorial_status(&player.tutorial_state))
            }
            Some("SKIP") => {
                // Skip tutorial
                match skip_tutorial(self.store(), &username) {
                    Ok(_) => Ok(
                        "Tutorial skipped. You can restart anytime with TUTORIAL RESTART."
                            .to_string(),
                    ),
                    Err(e) => Ok(format!("Error skipping tutorial: {}", e)),
                }
            }
            Some("RESTART") => {
                // Restart tutorial from beginning
                match restart_tutorial(self.store(), &username) {
                    Ok(_) => Ok("Tutorial restarted. Head to the Gazebo to begin!".to_string()),
                    Err(e) => Ok(format!("Error restarting tutorial: {}", e)),
                }
            }
            Some("START") => {
                // Manually start tutorial
                match start_tutorial(self.store(), &username) {
                    Ok(_) => Ok("Tutorial started! Look around and follow the hints.".to_string()),
                    Err(e) => Ok(format!("Error starting tutorial: {}", e)),
                }
            }
            Some(unknown) => Ok(format!(
                "Unknown subcommand: {}\nUsage: TUTORIAL [SKIP|RESTART|START]",
                unknown
            )),
        }
    }

    /// Check if player has an active dialog session and return (npc_id, dialog_session)
    fn get_active_dialog_session(
        &self,
        username: &str,
    ) -> Result<Option<(String, crate::tmush::types::DialogSession)>, TinyMushError> {
        self.store().get_active_dialog_for_player(username)
    }

    /// Handle TALK command - interact with NPCs
    async fn handle_talk(
        &mut self,
        session: &Session,
        npc_name: String,
        topic: Option<String>,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::tutorial::{advance_tutorial_step, distribute_tutorial_rewards};
        use crate::tmush::types::{DialogSession, TutorialState, TutorialStep};

        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        let username = player.username.clone();

        // Check if there's already an active dialog session with this NPC ID
        // This handles the case where we're continuing a conversation via numbered input
        if let Some(mut dialog_session) = self.store().get_dialog_session(&username, &npc_name)? {
            // We have an active session - npc_name is actually the npc_id
            let npc_id = npc_name;

            // Get the NPC to access its name
            let npc = self.store().get_npc(&npc_id)?;

            // Player is in an active conversation
            if let Some(ref input) = topic {
                // Check for special keywords
                match input.as_str() {
                    "EXIT" | "QUIT" | "BYE" => {
                        self.store().delete_dialog_session(&username, &npc_id)?;
                        return Ok(format!("You end your conversation with {}.", npc.name));
                    }
                    "BACK" => {
                        if dialog_session.go_back() {
                            self.store().put_dialog_session(dialog_session.clone())?;
                            return self.render_dialog_node(
                                &npc.name,
                                &npc_id,
                                &dialog_session,
                                &player,
                            );
                        } else {
                            return Ok("You're at the start of the conversation.".to_string());
                        }
                    }
                    _ => {}
                }

                // Try to parse as choice number
                if let Ok(choice_num) = input.parse::<usize>() {
                    let goto_node = {
                        let node = dialog_session.get_current_node();
                        if let Some(node) = node {
                            // Filter visible choices based on conditions
                            let visible_choices: Vec<_> = node
                                .choices
                                .iter()
                                .filter(|c| {
                                    self.evaluate_conditions(&c.conditions, &player, &npc_id)
                                        .unwrap_or(false)
                                })
                                .collect();

                            if choice_num > 0 && choice_num <= visible_choices.len() {
                                let choice = visible_choices[choice_num - 1];

                                if choice.exit {
                                    self.store().delete_dialog_session(&username, &npc_id)?;
                                    return Ok(format!(
                                        "You end your conversation with {}.",
                                        npc.name
                                    ));
                                }

                                choice.goto.clone()
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };

                    if let Some(goto_node) = goto_node {
                        if dialog_session.navigate_to(&goto_node) {
                            // Execute actions for the new node
                            let action_messages =
                                if let Some(node) = dialog_session.get_current_node() {
                                    self.execute_actions(&node.actions, &username, &npc_id)
                                        .await?
                                } else {
                                    Vec::new()
                                };

                            self.store().put_dialog_session(dialog_session.clone())?;

                            let mut response = self.render_dialog_node(
                                &npc.name,
                                &npc_id,
                                &dialog_session,
                                &player,
                            )?;

                            // Prepend action messages
                            if !action_messages.is_empty() {
                                let actions_text = action_messages.join("\n");
                                response = format!("{}\n\n{}", actions_text, response);
                            }

                            return Ok(response);
                        } else {
                            self.store().delete_dialog_session(&username, &npc_id)?;
                            return Ok("Conversation error - dialogue tree too deep.".to_string());
                        }
                    }

                    return Ok("Invalid choice number.".to_string());
                }
            }

            // No input or invalid input - show current node again
            return self.render_dialog_node(&npc.name, &npc_id, &dialog_session, &player);
        }

        // No active session - need to find NPC in room by name
        // Get NPCs in current room
        let npcs = self.store().get_npcs_in_room(&player.current_room)?;

        if npcs.is_empty() {
            return Ok("There's nobody here to talk to.".to_string());
        }

        // Find matching NPC (case-insensitive partial match)
        let npc = npcs.iter().find(|n| {
            n.name.to_uppercase().contains(&npc_name) || n.id.to_uppercase().contains(&npc_name)
        });

        let Some(npc) = npc else {
            let available: Vec<_> = npcs.iter().map(|n| n.name.as_str()).collect();
            return Ok(format!(
                "I don't see '{}' here.\nAvailable: {}",
                npc_name,
                available.join(", ")
            ));
        };

        // Handle LIST keyword to show available topics
        if let Some(ref t) = topic {
            if t == "LIST" {
                let topics: Vec<_> = npc
                    .dialog
                    .keys()
                    .filter(|k| *k != "default")
                    .map(|k| k.as_str())
                    .collect();

                if topics.is_empty() {
                    return Ok(format!(
                        "{} doesn't have any specific topics to discuss.",
                        npc.name
                    ));
                }

                return Ok(format!(
                    "{} can talk about:\n  {}",
                    npc.name,
                    topics.join(", ")
                ));
            }
        }

        // Tutorial-specific NPC dialogs
        if npc.id == "mayor_thompson" {
            if let TutorialState::InProgress { step } = &player.tutorial_state {
                if matches!(step, TutorialStep::MeetTheMayor) {
                    // Complete tutorial and give rewards
                    if let Err(e) =
                        advance_tutorial_step(self.store(), &username, TutorialStep::MeetTheMayor)
                    {
                        return Ok(format!("Tutorial error: {}", e));
                    }

                    // Get world currency system from config or player
                    let currency_system = player.currency.clone();

                    if let Err(e) =
                        distribute_tutorial_rewards(self.store(), &username, &currency_system)
                    {
                        return Ok(format!("Reward error: {}", e));
                    }

                    return Ok(
                        "Mayor Thompson:\n'Welcome, citizen! Here's a starter purse and town map. \
                        Good luck in Old Towne Mesh!'\n\n\
                        [Tutorial Complete! Rewards granted.]"
                            .to_string(),
                    );
                }
            }
        }

        // Check if NPC has a dialog tree - start branching conversation
        if !npc.dialog_tree.is_empty() && topic.is_none() {
            // Start new dialog session
            let start_node = "greeting";
            if npc.dialog_tree.contains_key(start_node) {
                let dialog_session =
                    DialogSession::new(&username, &npc.id, start_node, npc.dialog_tree.clone());

                // Execute actions for the greeting node
                let action_messages = if let Some(node) = dialog_session.get_current_node() {
                    self.execute_actions(&node.actions, &username, &npc.id)
                        .await?
                } else {
                    Vec::new()
                };

                self.store().put_dialog_session(dialog_session.clone())?;

                let mut response =
                    self.render_dialog_node(&npc.name, &npc.id, &dialog_session, &player)?;

                // Prepend action messages
                if !action_messages.is_empty() {
                    let actions_text = action_messages.join("\n");
                    response = format!("{}\n\n{}", actions_text, response);
                }

                return Ok(response);
            }
        }

        // Fall back to simple topic-based dialog
        let (dialog_text, actual_topic) = if let Some(topic_name) = topic {
            // Try to find the specific topic (case-insensitive)
            let topic_key = npc
                .dialog
                .keys()
                .find(|k| k.to_uppercase() == topic_name)
                .cloned();

            if let Some(key) = topic_key {
                (npc.dialog.get(&key).map(|s| s.as_str()), Some(key))
            } else {
                // Topic not found, suggest available topics
                let topics: Vec<_> = npc
                    .dialog
                    .keys()
                    .filter(|k| *k != "default" && *k != "greeting")
                    .map(|k| k.as_str())
                    .collect();

                if topics.is_empty() {
                    return Ok(format!("{} doesn't know about '{}'.", npc.name, topic_name));
                } else {
                    return Ok(format!(
                        "{} doesn't know about '{}'.\nTry: {}",
                        npc.name,
                        topic_name,
                        topics.join(", ")
                    ));
                }
            }
        } else {
            // No topic specified, use greeting or default
            let greeting_key = if npc.dialog.contains_key("greeting") {
                Some("greeting".to_string())
            } else {
                Some("default".to_string())
            };
            (
                npc.dialog
                    .get("greeting")
                    .or_else(|| npc.dialog.get("default"))
                    .map(|s| s.as_str()),
                greeting_key,
            )
        };

        let dialog = dialog_text.unwrap_or("...");

        // Track conversation state (Phase 8.5)
        if let Some(topic) = actual_topic {
            use crate::tmush::types::ConversationState;

            let mut conv_state = self
                .store()
                .get_conversation_state(&username, &npc.id)?
                .unwrap_or_else(|| ConversationState::new(&username, &npc.id));

            conv_state.discuss_topic(&topic);

            if let Err(e) = self.store().put_conversation_state(conv_state) {
                eprintln!("Warning: Failed to save conversation state: {}", e);
            }
        }

        Ok(format!("{}: '{}'", npc.name, dialog))
    }

    /// Render a dialog node with choices for branching conversations
    fn render_dialog_node(
        &self,
        npc_name: &str,
        npc_id: &str,
        session: &crate::tmush::types::DialogSession,
        player: &crate::tmush::types::PlayerRecord,
    ) -> Result<String> {
        let node = session
            .get_current_node()
            .ok_or_else(|| anyhow::anyhow!("Dialog node not found"))?;

        let mut response = format!("{}: '{}'\n", npc_name, node.text);

        if !node.choices.is_empty() {
            response.push('\n');

            // Filter choices based on conditions
            let mut visible_choices = Vec::new();
            for choice in &node.choices {
                if self
                    .evaluate_conditions(&choice.conditions, player, npc_id)
                    .unwrap_or(false)
                {
                    visible_choices.push(choice);
                }
            }

            if visible_choices.is_empty() {
                response.push_str("(No available options at this time)\n");
            } else {
                for (i, choice) in visible_choices.iter().enumerate() {
                    response.push_str(&format!("{}) {}\n", i + 1, choice.label));
                }
            }

            if session.can_go_back() {
                response.push_str("\nType BACK to return, EXIT to end conversation.");
            } else {
                response.push_str("\nType EXIT to end conversation.");
            }
        }

        Ok(response)
    }

    /// Handle TALKED command - view conversation history
    async fn handle_talked(
        &mut self,
        session: &Session,
        npc_filter: Option<String>,
        _config: &Config,
    ) -> Result<String> {
        let username = session.node_id.to_string();

        // Get all conversation states for this player
        let states = self.store().get_player_conversation_states(&username)?;

        if states.is_empty() {
            return Ok("You haven't talked to anyone yet.".to_string());
        }

        // If specific NPC requested, filter to that one
        let filtered_states: Vec<_> = if let Some(npc_name) = npc_filter {
            // Find NPC by name
            let npc_ids = self.store().list_npc_ids()?;
            let mut matching_npc_id = None;

            for npc_id in npc_ids {
                if let Ok(npc) = self.store().get_npc(&npc_id) {
                    if npc.name.to_uppercase().contains(&npc_name)
                        || npc.id.to_uppercase().contains(&npc_name)
                    {
                        matching_npc_id = Some(npc.id.clone());
                        break;
                    }
                }
            }

            if let Some(npc_id) = matching_npc_id {
                states.into_iter().filter(|s| s.npc_id == npc_id).collect()
            } else {
                return Ok(format!("You haven't talked to '{}'.", npc_name));
            }
        } else {
            states
        };

        if filtered_states.is_empty() {
            return Ok("No conversation history found.".to_string());
        }

        // Build response
        let mut response = String::from("📜 Conversation History:\n\n");

        for state in filtered_states {
            // Get NPC name
            let npc_name = match self.store().get_npc(&state.npc_id) {
                Ok(npc) => npc.name,
                Err(_) => state.npc_id.clone(),
            };

            response.push_str(&format!(
                "🗣️  {} ({} conversations)\n",
                npc_name, state.total_conversations
            ));
            response.push_str(&format!(
                "   Last talked: {}\n",
                state.last_conversation_time.format("%Y-%m-%d %H:%M")
            ));

            if !state.topics_discussed.is_empty() {
                response.push_str("   Topics: ");
                response.push_str(&state.topics_discussed.join(", "));
                response.push('\n');
            }

            response.push('\n');
        }

        Ok(response)
    }

    /// Evaluate dialogue conditions for a player
    fn evaluate_conditions(
        &self,
        conditions: &[crate::tmush::types::DialogCondition],
        player: &crate::tmush::types::PlayerRecord,
        npc_id: &str,
    ) -> Result<bool> {
        use crate::tmush::types::DialogCondition;

        // Empty conditions = always show
        if conditions.is_empty() {
            return Ok(true);
        }

        // Check each condition (ALL must be true)
        for condition in conditions {
            match condition {
                DialogCondition::Always => continue,

                DialogCondition::HasDiscussed { topic } => {
                    if let Ok(Some(conv_state)) = self
                        .store()
                        .get_conversation_state(&player.username, npc_id)
                    {
                        if !conv_state.has_discussed(topic) {
                            return Ok(false);
                        }
                    } else {
                        return Ok(false);
                    }
                }

                DialogCondition::HasFlag { flag, value } => {
                    if let Ok(Some(conv_state)) = self
                        .store()
                        .get_conversation_state(&player.username, npc_id)
                    {
                        if conv_state.get_flag(flag) != *value {
                            return Ok(false);
                        }
                    } else {
                        return Ok(false);
                    }
                }

                DialogCondition::HasItem { item_id } => {
                    if !Self::player_has_item(&player, item_id) {
                        return Ok(false);
                    }
                }

                DialogCondition::HasCurrency { amount } => {
                    if player.currency.base_value() < *amount {
                        return Ok(false);
                    }
                }

                DialogCondition::MinLevel { level } => {
                    // Level is stored in stats, not directly on player
                    // For now, just pass (we can add proper level tracking later)
                    let _ = level; // Suppress unused warning
                }

                DialogCondition::QuestStatus { quest_id, status } => {
                    // Check player's quests vector
                    let has_quest = player.quests.iter().any(|q| {
                        q.quest_id == *quest_id && format!("{:?}", q.state).contains(status)
                    });
                    if !has_quest {
                        return Ok(false);
                    }
                }

                DialogCondition::HasAchievement { achievement_id } => {
                    let has_achievement = player
                        .achievements
                        .iter()
                        .any(|a| &a.achievement_id == achievement_id);
                    if !has_achievement {
                        return Ok(false);
                    }
                }
            }
        }

        Ok(true)
    }

    /// Execute dialogue actions when a node is reached
    async fn execute_actions(
        &mut self,
        actions: &[crate::tmush::types::DialogAction],
        player_name: &str,
        npc_id: &str,
    ) -> Result<Vec<String>> {
        use crate::tmush::types::DialogAction;

        let mut messages = Vec::new();

        for action in actions {
            match action {
                DialogAction::GiveItem { item_id, quantity } => {
                    // Add item to player inventory using proper inventory system
                    use crate::tmush::inventory::add_item_to_inventory;
                    use crate::tmush::types::{InventoryConfig, InventoryResult};

                    let mut player = self.store().get_player(player_name)?;

                    // Get the object to add
                    let object = match self.store().get_object(item_id) {
                        Ok(obj) => obj,
                        Err(_) => {
                            messages.push(format!("⚠️ Error: Item '{}' not found", item_id));
                            continue;
                        }
                    };

                    let config = InventoryConfig::default();
                    match add_item_to_inventory(&mut player, &object, *quantity, &config) {
                        InventoryResult::Added {
                            quantity: added, ..
                        } => {
                            self.store().put_player(player)?;
                            let qty_msg = if added > 1 {
                                format!("{} x{}", object.name, added)
                            } else {
                                object.name.clone()
                            };
                            messages.push(format!("🎁 You received: {}", qty_msg));
                        }
                        InventoryResult::Failed { reason } => {
                            messages
                                .push(format!("⚠️ Could not receive {}: {}", object.name, reason));
                        }
                        _ => {} // Other result types not applicable
                    }
                }

                DialogAction::TakeItem { item_id, quantity } => {
                    // Remove item from player inventory using proper inventory system
                    use crate::tmush::inventory::remove_item_from_inventory;
                    use crate::tmush::types::InventoryResult;

                    let mut player = self.store().get_player(player_name)?;

                    match remove_item_from_inventory(&mut player, item_id, *quantity) {
                        InventoryResult::Removed {
                            quantity: removed, ..
                        } => {
                            self.store().put_player(player)?;
                            let qty_msg = if removed > 1 {
                                format!("{} x{}", item_id, removed)
                            } else {
                                item_id.clone()
                            };
                            messages.push(format!("📤 You gave: {}", qty_msg));
                        }
                        InventoryResult::Failed { reason } => {
                            messages.push(format!("⚠️ Could not give {}: {}", item_id, reason));
                        }
                        _ => {} // Other result types not applicable
                    }
                }

                DialogAction::GiveCurrency { amount } => {
                    // Add currency to player
                    let mut player = self.store().get_player(player_name)?;

                    // Create currency amount to add
                    let currency_to_add = match &player.currency {
                        crate::tmush::types::CurrencyAmount::Decimal { .. } => {
                            crate::tmush::types::CurrencyAmount::Decimal {
                                minor_units: *amount,
                            }
                        }
                        crate::tmush::types::CurrencyAmount::MultiTier { .. } => {
                            crate::tmush::types::CurrencyAmount::MultiTier {
                                base_units: *amount,
                            }
                        }
                    };

                    player.currency = player
                        .currency
                        .add(&currency_to_add)
                        .map_err(|e| anyhow::anyhow!("Currency error: {}", e))?;
                    self.store().put_player(player)?;

                    messages.push(format!("💰 You received {} credits!", amount));
                }

                DialogAction::TakeCurrency { amount } => {
                    // Deduct currency from player
                    let mut player = self.store().get_player(player_name)?;

                    if player.currency.base_value() >= *amount {
                        // Create currency amount to subtract
                        let currency_to_sub = match &player.currency {
                            crate::tmush::types::CurrencyAmount::Decimal { .. } => {
                                crate::tmush::types::CurrencyAmount::Decimal {
                                    minor_units: *amount,
                                }
                            }
                            crate::tmush::types::CurrencyAmount::MultiTier { .. } => {
                                crate::tmush::types::CurrencyAmount::MultiTier {
                                    base_units: *amount,
                                }
                            }
                        };

                        player.currency = player
                            .currency
                            .subtract(&currency_to_sub)
                            .map_err(|e| anyhow::anyhow!("Currency error: {}", e))?;
                        self.store().put_player(player)?;
                        messages.push(format!("💸 You paid {} credits.", amount));
                    } else {
                        messages.push("❌ Insufficient funds.".to_string());
                    }
                }

                DialogAction::StartQuest { quest_id } => {
                    // Start a quest for the player
                    let mut player = self.store().get_player(player_name)?;

                    // Check if player already has this quest
                    let has_quest = player.quests.iter().any(|q| &q.quest_id == quest_id);

                    if !has_quest {
                        use crate::tmush::types::{PlayerQuest, QuestState};

                        // Create a basic quest with empty objectives
                        // In a real implementation, you'd load objectives from quest definition
                        let new_quest = PlayerQuest {
                            quest_id: quest_id.clone(),
                            state: QuestState::Active {
                                started_at: chrono::Utc::now(),
                            },
                            objectives: Vec::new(), // Would be populated from quest definition
                        };

                        player.quests.push(new_quest);
                        self.store().put_player(player)?;

                        messages.push(format!("📜 New quest started: {}", quest_id));
                    } else {
                        messages.push("ℹ️ You already have this quest.".to_string());
                    }
                }

                DialogAction::CompleteQuest { quest_id } => {
                    // Complete a quest for the player
                    let mut player = self.store().get_player(player_name)?;

                    let mut completed = false;
                    for quest in &mut player.quests {
                        if &quest.quest_id == quest_id {
                            use crate::tmush::types::QuestState;
                            quest.state = QuestState::Completed {
                                completed_at: chrono::Utc::now(),
                            };
                            completed = true;
                            break;
                        }
                    }

                    if completed {
                        self.store().put_player(player)?;
                        messages.push(format!("✅ Quest completed: {}", quest_id));
                    } else {
                        messages.push("❌ Quest not found or already complete.".to_string());
                    }
                }

                DialogAction::GrantAchievement { achievement_id } => {
                    // Grant an achievement to the player
                    let mut player = self.store().get_player(player_name)?;

                    // Check if player already has this achievement
                    let achievement_index = player
                        .achievements
                        .iter()
                        .position(|a| &a.achievement_id == achievement_id);

                    match achievement_index {
                        Some(idx) if !player.achievements[idx].earned => {
                            // Mark existing achievement as earned
                            player.achievements[idx].earned = true;
                            player.achievements[idx].earned_at = Some(chrono::Utc::now());
                            self.store().put_player(player)?;
                            messages.push(format!("🏆 Achievement unlocked: {}", achievement_id));
                        }
                        Some(_) => {
                            // Already earned, do nothing
                        }
                        None => {
                            // Create new achievement
                            use crate::tmush::types::PlayerAchievement;

                            let new_achievement = PlayerAchievement {
                                achievement_id: achievement_id.clone(),
                                progress: 0,
                                earned: true,
                                earned_at: Some(chrono::Utc::now()),
                            };

                            player.achievements.push(new_achievement);
                            self.store().put_player(player)?;
                            messages.push(format!("🏆 Achievement unlocked: {}", achievement_id));
                        }
                    }
                }

                DialogAction::SetFlag { flag, value } => {
                    // Set a conversation flag
                    let mut conv_state = self
                        .store()
                        .get_conversation_state(player_name, npc_id)?
                        .unwrap_or_else(|| {
                            use crate::tmush::types::ConversationState;
                            ConversationState::new(player_name, npc_id)
                        });

                    conv_state.set_flag(flag, *value);
                    self.store().put_conversation_state(conv_state)?;

                    // Don't show message for flag setting (internal state)
                }

                DialogAction::Teleport { room_id } => {
                    // Move player to a room
                    let mut player = self.store().get_player(player_name)?;

                    // Check if room exists (get_room returns Result<RoomRecord>)
                    match self
                        .store()
                        .resolve_destination_for_player(player_name, room_id)
                    {
                        Ok(resolved_room) => {
                            if self.store().get_room(&resolved_room).is_ok() {
                                player.current_room = resolved_room.clone();
                                self.store().put_player(player)?;
                                messages.push(format!(
                                    "🌀 You have been teleported to {}!",
                                    resolved_room
                                ));
                            } else {
                                messages.push(format!("❌ Location '{}' not found.", room_id));
                            }
                        }
                        Err(e) => {
                            messages.push(format!("❌ Unable to reach '{}': {}", room_id, e));
                        }
                    }
                }

                DialogAction::SendMessage { text } => {
                    // Send a system message
                    messages.push(format!("📢 {}", text));
                }
            }
        }

        Ok(messages)
    }

    /// Handle QUEST command - manage quests (list, accept, status)
    async fn handle_quest(
        &mut self,
        session: &Session,
        subcommand: Option<String>,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::quest::{
            accept_quest, can_accept_quest, format_quest_list, format_quest_status,
            get_active_quests, get_available_quests,
        };

        let username = session.username.as_deref().unwrap_or("guest");

        match subcommand.as_deref() {
            None => {
                // Show active quests
                let active = get_active_quests(self.store(), username)
                    .map_err(|e| anyhow::anyhow!("Failed to get active quests: {}", e))?;

                if active.is_empty() {
                    return Ok(
                        "You have no active quests.\nUse QUEST LIST to see available quests."
                            .to_string(),
                    );
                }

                let mut output = String::from("=== ACTIVE QUESTS ===\n");
                for (idx, player_quest) in active.iter().enumerate() {
                    let quest = self
                        .store()
                        .get_quest(&player_quest.quest_id)
                        .map_err(|e| anyhow::anyhow!("Failed to get quest: {}", e))?;
                    let all_done = player_quest.all_objectives_complete();
                    let status_char = if all_done { "!" } else { " " };
                    output.push_str(&format!(
                        "{}. [{}] {} - {} obj.\n",
                        idx + 1,
                        status_char,
                        quest.name,
                        player_quest.objectives.len()
                    ));
                }
                output.push_str("\nQUEST <id> - View details\nQUEST LIST - Available quests");
                Ok(output)
            }
            Some(cmd) if cmd.starts_with("LIST") => {
                // List available quests
                let available = get_available_quests(self.store(), username)
                    .map_err(|e| anyhow::anyhow!("Failed to get available quests: {}", e))?;

                let messages = format_quest_list(self.store(), &available)
                    .map_err(|e| anyhow::anyhow!("Failed to format quest list: {}", e))?;

                Ok(messages.join("\n"))
            }
            Some(cmd) if cmd.starts_with("ACCEPT ") => {
                // Accept a quest by ID
                let quest_id = cmd.strip_prefix("ACCEPT ").unwrap().trim().to_lowercase();

                if !can_accept_quest(self.store(), username, &quest_id)
                    .map_err(|e| anyhow::anyhow!("Failed to check quest: {}", e))?
                {
                    return Ok("Cannot accept that quest (already accepted/completed, or prerequisites not met).".to_string());
                }

                accept_quest(self.store(), username, &quest_id)
                    .map_err(|e| anyhow::anyhow!("Failed to accept quest: {}", e))?;

                let quest = self
                    .store()
                    .get_quest(&quest_id)
                    .map_err(|e| anyhow::anyhow!("Failed to get quest: {}", e))?;

                Ok(format!(
                    "Quest accepted: {}\n{}\nObjectives: {}\n\nUse QUEST to view progress.",
                    quest.name,
                    quest.description,
                    quest.objectives.len()
                ))
            }
            Some(cmd) if cmd.starts_with("COMPLETE") || cmd.starts_with("COMP") => {
                // Complete a quest
                Ok("Quest completion is automatic when all objectives are met.\nTalk to the quest giver to turn in.".to_string())
            }
            Some(quest_id) => {
                // Show quest details by ID
                let quest_id = quest_id.trim().to_lowercase();
                let active = get_active_quests(self.store(), username)
                    .map_err(|e| anyhow::anyhow!("Failed to get active quests: {}", e))?;

                let player_quest = active.iter().find(|pq| pq.quest_id == quest_id);

                if let Some(pq) = player_quest {
                    let status = format_quest_status(self.store(), &quest_id, pq)
                        .map_err(|e| anyhow::anyhow!("Failed to format quest status: {}", e))?;
                    Ok(status)
                } else {
                    Ok(format!(
                        "Quest '{}' not found in your active quests.",
                        quest_id
                    ))
                }
            }
        }
    }

    /// Handle ABANDON command - abandon an active quest
    async fn handle_abandon(
        &mut self,
        session: &Session,
        quest_id: String,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::quest::abandon_quest;

        let username = session.username.as_deref().unwrap_or("guest");

        match abandon_quest(self.store(), username, &quest_id) {
            Ok(_) => {
                let quest = self
                    .store()
                    .get_quest(&quest_id)
                    .map_err(|e| anyhow::anyhow!("Failed to get quest: {}", e))?;
                Ok(format!("You have abandoned the quest: {}", quest.name))
            }
            Err(e) => Ok(format!("Failed to abandon quest: {}", e)),
        }
    }

    /// Handle ACHIEVEMENTS command - view and manage achievements
    async fn handle_achievements(
        &mut self,
        session: &Session,
        subcommand: Option<String>,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::achievement::{
            get_achievements_by_category, get_available_achievements, get_earned_achievements,
        };
        use crate::tmush::types::AchievementCategory;

        let username = session.username.as_deref().unwrap_or("guest");

        match subcommand.as_deref() {
            None | Some("LIST") => {
                // Show all achievements with progress
                let achievements = get_available_achievements(self.store(), username)
                    .map_err(|e| anyhow::anyhow!("Failed to get achievements: {}", e))?;

                if achievements.is_empty() {
                    return Ok("No achievements available.".to_string());
                }

                let mut output = String::from("=== ACHIEVEMENTS ===\n");
                let mut by_category: std::collections::HashMap<String, Vec<_>> =
                    std::collections::HashMap::new();

                for (achievement, player_progress) in achievements {
                    let category = format!("{:?}", achievement.category);
                    by_category
                        .entry(category)
                        .or_default()
                        .push((achievement, player_progress));
                }

                let categories = vec![
                    "Combat",
                    "Exploration",
                    "Social",
                    "Economic",
                    "Crafting",
                    "Quest",
                    "Special",
                ];
                for cat_name in categories {
                    if let Some(achievements) = by_category.get(cat_name) {
                        output.push_str(&format!("\n--- {} ---\n", cat_name));
                        for (achievement, player_progress) in achievements {
                            let earned_marker = if let Some(pa) = player_progress {
                                if pa.earned {
                                    "[✓]"
                                } else {
                                    &format!(
                                        "[{}%]",
                                        (pa.progress * 100)
                                            / self.get_achievement_required(&achievement.trigger)
                                    )
                                }
                            } else {
                                "[ ]"
                            };

                            let title_info = if let Some(ref title) = achievement.title {
                                format!(" - Title: {}", title)
                            } else {
                                String::new()
                            };

                            output.push_str(&format!(
                                "{} {}{}\n",
                                earned_marker, achievement.name, title_info
                            ));
                        }
                    }
                }

                output.push_str("\nUse ACHIEVEMENTS EARNED for earned only\nUse ACHIEVEMENTS COMBAT|EXPLORATION|etc for category");
                Ok(output)
            }
            Some("EARNED") => {
                // Show only earned achievements
                let earned = get_earned_achievements(self.store(), username)
                    .map_err(|e| anyhow::anyhow!("Failed to get earned achievements: {}", e))?;

                if earned.is_empty() {
                    return Ok("You haven't earned any achievements yet.\nKeep exploring and trying new things!".to_string());
                }

                let mut output = String::from("=== EARNED ACHIEVEMENTS ===\n");
                for achievement in earned {
                    let title_info = if let Some(ref title) = achievement.title {
                        format!(" - {}", title)
                    } else {
                        String::new()
                    };
                    output.push_str(&format!(
                        "✓ {}{}\n  {}\n",
                        achievement.name, title_info, achievement.description
                    ));
                }
                Ok(output)
            }
            Some(cat) => {
                // Filter by category
                let category = match cat {
                    "COMBAT" => AchievementCategory::Combat,
                    "EXPLORATION" | "EXPLORE" => AchievementCategory::Exploration,
                    "SOCIAL" => AchievementCategory::Social,
                    "ECONOMIC" | "ECONOMY" => AchievementCategory::Economic,
                    "CRAFTING" | "CRAFT" => AchievementCategory::Crafting,
                    "QUEST" | "QUESTS" => AchievementCategory::Quest,
                    "SPECIAL" => AchievementCategory::Special,
                    _ => return Ok(format!("Unknown category: {}\nAvailable: COMBAT, EXPLORATION, SOCIAL, ECONOMIC, CRAFTING, QUEST, SPECIAL", cat)),
                };

                let cat_name = format!("{:?}", category);
                let achievements =
                    get_achievements_by_category(self.store(), username, category)
                        .map_err(|e| anyhow::anyhow!("Failed to get achievements: {}", e))?;

                if achievements.is_empty() {
                    return Ok(format!("No achievements found in category: {}", cat_name));
                }

                let mut output = format!("=== {} ACHIEVEMENTS ===\n", cat);
                for (achievement, player_progress) in achievements {
                    let status = if let Some(pa) = player_progress {
                        if pa.earned {
                            "[✓] EARNED".to_string()
                        } else {
                            format!(
                                "[{}/{}]",
                                pa.progress,
                                self.get_achievement_required(&achievement.trigger)
                            )
                        }
                    } else {
                        "[0]".to_string()
                    };

                    output.push_str(&format!(
                        "{} - {}\n  {}\n",
                        status, achievement.name, achievement.description
                    ));
                }
                Ok(output)
            }
        }
    }

    /// Handle TITLE command - manage player titles
    async fn handle_title(
        &mut self,
        session: &Session,
        subcommand: Option<String>,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::achievement::get_earned_achievements;

        let username = session.username.as_deref().unwrap_or("guest");

        match subcommand.as_deref() {
            None | Some("LIST") => {
                // List all available titles from earned achievements
                let earned = get_earned_achievements(self.store(), username)
                    .map_err(|e| anyhow::anyhow!("Failed to get earned achievements: {}", e))?;

                let titles: Vec<String> = earned.iter().filter_map(|a| a.title.clone()).collect();

                if titles.is_empty() {
                    return Ok(
                        "You haven't unlocked any titles yet.\nEarn achievements to unlock titles!"
                            .to_string(),
                    );
                }

                let player = self.get_or_create_player(session).await?;
                let equipped = player.equipped_title.as_deref().unwrap_or("None");

                let mut output =
                    format!("=== YOUR TITLES ===\nCurrently equipped: {}\n\n", equipped);
                for (idx, title) in titles.iter().enumerate() {
                    let marker = if Some(title.as_str()) == player.equipped_title.as_deref() {
                        "[*]"
                    } else {
                        "   "
                    };
                    output.push_str(&format!("{}  {}. {}\n", marker, idx + 1, title));
                }
                output.push_str("\nTITLE EQUIP <name> - Equip title\nTITLE UNEQUIP - Remove title");
                Ok(output)
            }
            Some(cmd) if cmd.starts_with("EQUIP ") => {
                // Equip a title
                let title = cmd.strip_prefix("EQUIP ").unwrap().trim();

                // Verify player has earned this title
                let earned = get_earned_achievements(self.store(), username)
                    .map_err(|e| anyhow::anyhow!("Failed to get earned achievements: {}", e))?;

                let has_title = earned.iter().any(|a| a.title.as_deref() == Some(title));

                if !has_title {
                    return Ok(format!("You haven't unlocked the title: {}", title));
                }

                let mut player = self.get_or_create_player(session).await?;
                player.equipped_title = Some(title.to_string());
                player.touch();
                self.store().put_player(player)?;

                Ok(format!(
                    "Title equipped: {}\nYou are now known as {} {}",
                    title, username, title
                ))
            }
            Some("UNEQUIP") => {
                // Remove equipped title
                let mut player = self.get_or_create_player(session).await?;
                if player.equipped_title.is_none() {
                    return Ok("You don't have any title equipped.".to_string());
                }

                player.equipped_title = None;
                player.touch();
                self.store().put_player(player)?;

                Ok("Title removed. You are no longer using a title.".to_string())
            }
            Some(title) if !title.starts_with("EQUIP") => {
                // Try to equip by name (shortcut)
                let earned = get_earned_achievements(self.store(), username)
                    .map_err(|e| anyhow::anyhow!("Failed to get earned achievements: {}", e))?;

                let has_title = earned.iter().any(|a| a.title.as_deref() == Some(title));

                if !has_title {
                    return Ok(format!("You haven't unlocked the title: {}", title));
                }

                let mut player = self.get_or_create_player(session).await?;
                player.equipped_title = Some(title.to_string());
                player.touch();
                self.store().put_player(player)?;

                Ok(format!("Title equipped: {}", title))
            }
            _ => Ok("Usage: TITLE [LIST|EQUIP <name>|UNEQUIP]".to_string()),
        }
    }

    async fn handle_companion(
        &mut self,
        session: &Session,
        subcommand: Option<String>,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::companion::{
            find_companion_in_room, format_companion_list, format_companion_status,
            get_player_companions, tame_companion,
        };

        let username = session.username.as_deref().unwrap_or("guest");
        let player = self.get_or_create_player(session).await?;
        let room_id = &player.current_room;

        match subcommand.as_deref() {
            None | Some("LIST") => {
                // List player's companions
                let companions = get_player_companions(self.store(), username)?;
                if companions.is_empty() {
                    return Ok("You don't have any companions.\nTAME a wild companion to add them to your party!".to_string());
                }
                Ok(format_companion_list(&companions))
            }
            Some(cmd) if cmd.starts_with("TAME ") => {
                // Tame a wild companion
                let name = cmd.strip_prefix("TAME ").unwrap().trim();

                match find_companion_in_room(self.store(), room_id, name)? {
                    Some(companion) if companion.owner.is_none() => {
                        tame_companion(self.store(), username, &companion.id)?;
                        // Fetch updated companion to show loyalty
                        let updated = self.store().get_companion(&companion.id)?;
                        Ok(format!(
                            "You've tamed {}!\nLoyalty: {}/100",
                            updated.name, updated.loyalty
                        ))
                    }
                    Some(_) => Ok(format!("{} already has an owner.", name)),
                    None => Ok(format!("There's no companion named '{}' here.", name)),
                }
            }
            Some(cmd) if cmd.starts_with("RELEASE ") => {
                // Release a companion back to wild
                use crate::tmush::companion::release_companion;
                let name = cmd.strip_prefix("RELEASE ").unwrap().trim();

                let companions = get_player_companions(self.store(), username)?;
                if let Some(comp) = companions
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(name))
                {
                    release_companion(self.store(), username, &comp.id)?;
                    Ok(format!("You've released {} back to the wild.", comp.name))
                } else {
                    Ok(format!("You don't have a companion named '{}'.", name))
                }
            }
            Some("STAY") => {
                // Leave all companions in current room (toggle auto-follow off)
                let companions = get_player_companions(self.store(), username)?;
                if companions.is_empty() {
                    return Ok("You don't have any companions.".to_string());
                }

                let mut count = 0;
                for comp in companions
                    .iter()
                    .filter(|c| c.room_id == *room_id && c.has_auto_follow())
                {
                    // Remove AutoFollow behavior
                    let mut updated = comp.clone();
                    updated.behaviors.retain(|b| {
                        !matches!(b, crate::tmush::types::CompanionBehavior::AutoFollow)
                    });
                    self.store().put_companion(updated)?;
                    count += 1;
                }

                if count > 0 {
                    Ok(format!("{} companion(s) will stay here.", count))
                } else {
                    Ok("No companions with auto-follow are here.".to_string())
                }
            }
            Some("COME") => {
                // Summon all companions to player's room
                use crate::tmush::companion::move_companion_to_room;
                let companions = get_player_companions(self.store(), username)?;
                if companions.is_empty() {
                    return Ok("You don't have any companions.".to_string());
                }

                let mut count = 0;
                for comp in companions.iter().filter(|c| c.room_id != *room_id) {
                    move_companion_to_room(self.store(), &comp.id, room_id)?;
                    count += 1;
                }

                if count > 0 {
                    Ok(format!("{} companion(s) arrive at your side.", count))
                } else {
                    Ok("All your companions are already here.".to_string())
                }
            }
            Some("INVENTORY") | Some("INV") => {
                // Show all companions and their inventory
                let companions = get_player_companions(self.store(), username)?;
                if companions.is_empty() {
                    return Ok("You don't have any companions.".to_string());
                }

                let mut output = String::from("=== COMPANION INVENTORY ===\n");
                for comp in companions.iter() {
                    output.push_str(&format!("{}: ", comp.name));
                    if comp.inventory.is_empty() {
                        output.push_str("(empty)\n");
                    } else {
                        output.push_str(&format!("{} items\n", comp.inventory.len()));
                        for item in &comp.inventory {
                            output.push_str(&format!("  - {}\n", item));
                        }
                    }
                }
                Ok(output)
            }
            Some(name) => {
                // Show companion status
                let companions = get_player_companions(self.store(), username)?;
                if let Some(comp) = companions
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(name))
                {
                    Ok(format_companion_status(comp))
                } else {
                    Ok(format!("You don't have a companion named '{}'.", name))
                }
            }
        }
    }

    async fn handle_feed(
        &mut self,
        session: &Session,
        name: String,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::companion::{feed_companion, get_player_companions};

        let username = session.username.as_deref().unwrap_or("guest");
        let companions = get_player_companions(self.store(), username)?;

        if let Some(comp) = companions
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&name))
        {
            let gain = feed_companion(self.store(), username, &comp.id)?;
            // Fetch updated companion to show current happiness
            let updated = self.store().get_companion(&comp.id)?;
            Ok(format!(
                "You feed {}. Happiness +{} ({}/100)",
                updated.name, gain, updated.happiness
            ))
        } else {
            Ok(format!("You don't have a companion named '{}'.", name))
        }
    }

    async fn handle_pet(
        &mut self,
        session: &Session,
        name: String,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::companion::{get_player_companions, pet_companion};

        let username = session.username.as_deref().unwrap_or("guest");
        let companions = get_player_companions(self.store(), username)?;

        if let Some(comp) = companions
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&name))
        {
            let gain = pet_companion(self.store(), username, &comp.id)?;
            // Fetch updated companion to show current loyalty
            let updated = self.store().get_companion(&comp.id)?;
            Ok(format!(
                "You pet {}. Loyalty +{} ({}/100)",
                updated.name, gain, updated.loyalty
            ))
        } else {
            Ok(format!("You don't have a companion named '{}'.", name))
        }
    }

    async fn handle_mount(
        &mut self,
        session: &Session,
        name: String,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::companion::{get_player_companions, mount_companion};
        use crate::tmush::types::CompanionType;

        let username = session.username.as_deref().unwrap_or("guest");
        let companions = get_player_companions(self.store(), username)?;

        if let Some(comp) = companions
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&name))
        {
            if comp.companion_type != CompanionType::Horse {
                return Ok(format!("{} is not mountable.", comp.name));
            }
            if comp.is_mounted {
                return Ok(format!("You're already mounted on {}.", comp.name));
            }

            mount_companion(self.store(), username, &comp.id)?;
            Ok(format!("You mount {}. Ready to ride!", comp.name))
        } else {
            Ok(format!("You don't have a companion named '{}'.", name))
        }
    }

    async fn handle_dismount(&mut self, session: &Session, _config: &Config) -> Result<String> {
        use crate::tmush::companion::dismount_companion;

        let username = session.username.as_deref().unwrap_or("guest");

        match dismount_companion(self.store(), username) {
            Ok(companion_name) => Ok(format!("You dismount from {}.", companion_name)),
            Err(_) => Ok("You're not currently mounted.".to_string()),
        }
    }

    async fn handle_train(
        &mut self,
        session: &Session,
        companion_name: String,
        skill: String,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::companion::get_player_companions;

        let username = session.username.as_deref().unwrap_or("guest");
        let companions = get_player_companions(self.store(), username)?;

        if let Some(comp) = companions
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&companion_name))
        {
            // For now, simple training system - could be expanded with skill trees
            let valid_skills = match comp.companion_type {
                crate::tmush::types::CompanionType::Horse => vec!["speed", "endurance", "carrying"],
                crate::tmush::types::CompanionType::Dog => vec!["tracking", "guarding", "hunting"],
                crate::tmush::types::CompanionType::Cat => vec!["stealth", "agility", "hunting"],
                crate::tmush::types::CompanionType::Familiar => {
                    vec!["magic", "wisdom", "perception"]
                }
                crate::tmush::types::CompanionType::Mercenary => {
                    vec!["combat", "tactics", "defense"]
                }
                crate::tmush::types::CompanionType::Construct => {
                    vec!["strength", "durability", "efficiency"]
                }
            };

            let skill_lower = skill.to_lowercase();
            if !valid_skills.contains(&skill_lower.as_str()) {
                return Ok(format!(
                    "{} cannot learn '{}'. Valid skills: {}",
                    comp.name,
                    skill,
                    valid_skills.join(", ")
                ));
            }

            // Check loyalty requirement
            if comp.loyalty < 50 {
                return Ok(format!(
                    "{} needs loyalty 50+ to train. Current: {}/100",
                    comp.name, comp.loyalty
                ));
            }

            // Training successful (skill progression would be tracked in companion record)
            Ok(format!(
                "You train {} in {}. They show promise!",
                comp.name, skill
            ))
        } else {
            Ok(format!(
                "You don't have a companion named '{}'.",
                companion_name
            ))
        }
    }

    /// Handle HOUSING command - manage player housing
    async fn handle_housing(
        &mut self,
        session: &Session,
        subcommand: Option<String>,
        config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;
        let store = self.store();

        // Get world config for error messages
        let world_config = store.get_world_config()?;

        // Get current room to check for HousingOffice flag
        let current_room = store.get_room_async(&player.current_room).await?;

        match subcommand.as_deref() {
            None | Some("") => {
                // Show player's housing status
                let instances = store.get_player_housing_instances(&player.username)?;

                if instances.is_empty() {
                    Ok("You don't own any housing yet.\n\n\
                        Visit a housing office to rent or purchase a place!\n\
                        Type HOUSING LIST to see available options."
                        .to_string())
                } else {
                    let mut output = "=== YOUR HOUSING ===\n\n".to_string();
                    for instance in instances {
                        let template = store.get_housing_template(&instance.template_id)?;
                        let active_status = if instance.active {
                            "✓ Active"
                        } else {
                            "✗ Inactive"
                        };
                        output.push_str(&format!(
                            "{} ({})\n  Template: {}\n  {} rooms, {} guests\n  {}\n\n",
                            template.name,
                            instance.id,
                            instance.template_id,
                            instance.room_mappings.len(),
                            instance.guests.len(),
                            active_status
                        ));
                    }
                    output.push_str("Type HOME to visit your housing.\n");
                    output.push_str("Type HOUSING INFO for more details.");
                    Ok(output)
                }
            }
            Some("LIST") => {
                // Check if player is at a housing office
                use crate::tmush::types::RoomFlag;
                if !current_room.flags.contains(&RoomFlag::HousingOffice) {
                    return Ok(world_config.err_housing_not_at_office.clone());
                }

                // Get all templates
                let all_template_ids = store.list_housing_templates_async().await?;

                if all_template_ids.is_empty() {
                    return Ok(world_config.err_housing_no_templates.clone());
                }

                // Filter templates by room's housing_filter_tags
                let mut templates = Vec::new();
                for template_id in all_template_ids {
                    if let Ok(template) = store.get_housing_template(&template_id) {
                        // Check if template matches room's filter
                        if template.matches_filter(&current_room.housing_filter_tags) {
                            templates.push(template);
                        }
                    }
                }

                if templates.is_empty() {
                    return Ok(world_config.err_housing_no_templates.clone());
                }

                // Build output
                let mut output = world_config.msg_housing_list_header.clone();
                output.push_str("\n\n");

                for (idx, template) in templates.iter().enumerate() {
                    let current_count = store.count_template_instances(&template.id)?;
                    let availability = if template.max_instances < 0 {
                        "Unlimited".to_string()
                    } else {
                        let remaining = template.max_instances - current_count as i32;
                        format!(
                            "{} of {} available",
                            remaining.max(0),
                            template.max_instances
                        )
                    };

                    let cost_str = if template.recurring_cost > 0 {
                        format!(
                            "{} credits ({} per month)",
                            template.cost, template.recurring_cost
                        )
                    } else {
                        format!("{} credits (one-time)", template.cost)
                    };

                    output.push_str(&format!(
                        "{}. {} ({})\n\
                         {}\n\
                         {} rooms | {} | Category: {}\n\
                         Availability: {}\n\n",
                        idx + 1,
                        template.name,
                        template.id,
                        template.description,
                        template.rooms.len(),
                        cost_str,
                        if template.category.is_empty() {
                            "general"
                        } else {
                            &template.category
                        },
                        availability
                    ));
                }

                output.push_str("\nType RENT <id> to acquire housing.");
                Ok(output)
            }
            Some(other) => Ok(format!(
                "Unknown HOUSING subcommand: {}\n\
                    Available: LIST, INFO",
                other
            )),
        }
    }

    /// Handle RENT command - rent/purchase housing from template
    async fn handle_rent(
        &mut self,
        session: &Session,
        template_id: String,
        config: &Config,
    ) -> Result<String> {
        let mut player = self.get_or_create_player(session).await?;
        let store = self.store();
        let world_config = store.get_world_config().unwrap_or_default();

        // Get current room to check if we're at a housing office
        let current_room = store.get_room_async(&player.current_room).await?;

        // Check if current location is a housing office
        if !current_room.flags.contains(&RoomFlag::HousingOffice) {
            return Ok(world_config.err_housing_not_at_office.clone());
        }

        // Load the housing template
        let template = store.get_housing_template(&template_id)?;

        // Check if template matches this location's filter
        if !template.matches_filter(&current_room.housing_filter_tags) {
            return Ok(world_config.err_housing_no_templates.clone());
        }

        // Check if player already owns housing
        let existing_instances = store.get_player_housing_instances(&player.username)?;
        if !existing_instances.is_empty() {
            return Ok(world_config.err_housing_already_owns.clone());
        }

        // Check if template has available instances
        if template.max_instances >= 0 {
            let current_count = store.count_template_instances(&template.id)?;
            if current_count >= template.max_instances as usize {
                return Ok(format!(
                    "Sorry, all {} housing units are currently occupied. \
                    Please check back later.",
                    template.name
                ));
            }
        }

        // Check if player has sufficient funds (currency + bank)
        let total_funds = player.currency.base_value() + player.banked_currency.base_value();
        let required = template.cost;

        if total_funds < required {
            let deficit = required - total_funds;
            return Ok(world_config
                .err_housing_insufficient_funds
                .replace("{cost}", &template.cost.to_string())
                .replace("{deficit}", &deficit.to_string()));
        }

        // Clone the template to create player's housing instance
        let instance = store.clone_housing_template(&template.id, &player.username)?;

        // Deduct cost from player (currency first, then bank if needed)
        let mut remaining_cost = required;
        let currency_value = player.currency.base_value();

        // Create amount to subtract matching player's currency type
        if currency_value >= remaining_cost {
            // Currency covers it all
            let to_subtract = match player.currency {
                CurrencyAmount::Decimal { .. } => CurrencyAmount::decimal(remaining_cost),
                CurrencyAmount::MultiTier { .. } => CurrencyAmount::multi_tier(remaining_cost),
            };
            player.currency = player
                .currency
                .subtract(&to_subtract)
                .map_err(|e| anyhow::anyhow!("Currency subtraction failed: {}", e))?;
        } else {
            // Use all currency, then bank
            if currency_value > 0 {
                let to_subtract = match player.currency {
                    CurrencyAmount::Decimal { .. } => CurrencyAmount::decimal(currency_value),
                    CurrencyAmount::MultiTier { .. } => CurrencyAmount::multi_tier(currency_value),
                };
                player.currency = player
                    .currency
                    .subtract(&to_subtract)
                    .map_err(|e| anyhow::anyhow!("Currency subtraction failed: {}", e))?;
                remaining_cost -= currency_value;
            }
            let to_subtract = match player.banked_currency {
                CurrencyAmount::Decimal { .. } => CurrencyAmount::decimal(remaining_cost),
                CurrencyAmount::MultiTier { .. } => CurrencyAmount::multi_tier(remaining_cost),
            };
            player.banked_currency = player
                .banked_currency
                .subtract(&to_subtract)
                .map_err(|e| anyhow::anyhow!("Bank currency subtraction failed: {}", e))?;
        }

        // Save updated player
        store.put_player_async(player).await?;

        // Return success message with HOME hint
        let success_msg = world_config
            .msg_housing_rented
            .replace("{name}", &template.name)
            .replace("{id}", &instance.id);

        Ok(format!(
            "{}\n\nUse the HOME command to teleport to your new housing.",
            success_msg
        ))
    }

    /// Handle HOME command - teleport to owned housing or manage homes
    async fn handle_home(
        &mut self,
        session: &Session,
        subcommand: Option<String>,
        config: &Config,
    ) -> Result<String> {
        use chrono::Utc;

        let mut player = self.get_or_create_player(session).await?;
        let store = self.store();
        let world_config = store.get_world_config().unwrap_or_default();

        match subcommand {
            Some(ref cmd) if cmd == "LIST" => {
                // HOME LIST - show all accessible housing (owned + guest access)
                let owned_instances = store.get_player_housing_instances(&player.username)?;
                let guest_instances = store.get_guest_housing_instances(&player.username)?;

                if owned_instances.is_empty() && guest_instances.is_empty() {
                    return Ok(world_config.msg_home_list_empty.clone());
                }

                let mut output = format!("{}\n\n", world_config.msg_home_list_header);

                // Show owned housing first
                for (idx, instance) in owned_instances.iter().enumerate() {
                    let template = store.get_housing_template(&instance.template_id)?;
                    let num = idx + 1;

                    // Check if this is primary
                    let is_primary = player.primary_housing_id.as_ref() == Some(&instance.id);
                    let primary_marker = if is_primary {
                        "[★ Primary] "
                    } else {
                        "           "
                    };

                    output.push_str(&format!(
                        "{}{}. {} ({})\n   Location: {} | Access: OWNED\n\n",
                        primary_marker, num, template.name, template.category, instance.id
                    ));
                }

                // Show guest housing
                let owned_count = owned_instances.len();
                for (idx, instance) in guest_instances.iter().enumerate() {
                    let template = store.get_housing_template(&instance.template_id)?;
                    let num = owned_count + idx + 1;

                    output.push_str(&format!(
                        "           {}. {} ({})\n   Owner: {} | Access: GUEST\n\n",
                        num, template.name, template.category, instance.owner
                    ));
                }

                output.push_str(&format!("{}\n", world_config.msg_home_list_footer_travel));
                output.push_str(&world_config.msg_home_list_footer_set);

                Ok(output)
            }

            Some(ref cmd) if cmd.starts_with("SET ") => {
                // HOME SET <id> - set primary housing
                let id_or_num = cmd.strip_prefix("SET ").unwrap().trim().to_string();
                let instances = store.get_player_housing_instances(&player.username)?;

                if instances.is_empty() {
                    return Ok(world_config.msg_home_list_empty.clone());
                }

                // Try to parse as number first
                let target_instance = if let Ok(num) = id_or_num.parse::<usize>() {
                    if num >= 1 && num <= instances.len() {
                        Some(&instances[num - 1])
                    } else {
                        None
                    }
                } else {
                    // Try to match by ID
                    instances.iter().find(|inst| inst.id == id_or_num)
                };

                let target_instance = match target_instance {
                    Some(inst) => inst,
                    None => {
                        return Ok(world_config.err_home_not_found.replace("{id}", &id_or_num));
                    }
                };

                // Set as primary
                player.primary_housing_id = Some(target_instance.id.clone());
                player.touch();
                store.put_player_async(player).await?;

                let template = store.get_housing_template(&target_instance.template_id)?;

                Ok(world_config
                    .msg_home_set_success
                    .replace("{name}", &template.name))
            }

            Some(id_or_num) => {
                // HOME <id> - teleport to specific housing by ID or number
                let owned_instances = store.get_player_housing_instances(&player.username)?;
                let guest_instances = store.get_guest_housing_instances(&player.username)?;

                // Combine owned + guest for unified numbering
                let mut all_instances = owned_instances.clone();
                all_instances.extend(guest_instances.clone());

                if all_instances.is_empty() {
                    return Ok(world_config.err_no_housing.clone());
                }

                // Try to parse as number first, then try as ID
                let target_instance = if let Ok(num) = id_or_num.parse::<usize>() {
                    if num >= 1 && num <= all_instances.len() {
                        Some(&all_instances[num - 1])
                    } else {
                        None
                    }
                } else {
                    all_instances.iter().find(|inst| inst.id == id_or_num)
                };

                let target_instance = match target_instance {
                    Some(inst) => inst,
                    None => {
                        return Ok(world_config.err_home_not_found.replace("{id}", &id_or_num));
                    }
                };

                // Validation checks
                if player.in_combat {
                    return Ok(world_config.err_teleport_in_combat.clone());
                }

                let current_room = store.get_room_async(&player.current_room).await?;
                if current_room.flags.contains(&RoomFlag::NoTeleportOut) {
                    return Ok(world_config.err_teleport_restricted.clone());
                }

                if let Some(last_teleport) = player.last_teleport {
                    let now = Utc::now();
                    let elapsed = (now - last_teleport).num_seconds() as u64;
                    let cooldown = world_config.home_cooldown_seconds;

                    if elapsed < cooldown {
                        let remaining = cooldown - elapsed;
                        let minutes = remaining / 60;
                        let seconds = remaining % 60;
                        let time_str = if minutes > 0 {
                            format!(
                                "{} minute{} {} second{}",
                                minutes,
                                if minutes == 1 { "" } else { "s" },
                                seconds,
                                if seconds == 1 { "" } else { "s" }
                            )
                        } else {
                            format!("{} second{}", seconds, if seconds == 1 { "" } else { "s" })
                        };

                        return Ok(world_config
                            .err_teleport_cooldown
                            .replace("{time}", &time_str));
                    }
                }

                // Teleport
                player.current_room = target_instance.entry_room_id.clone();
                player.last_teleport = Some(Utc::now());
                player.touch();
                store.put_player_async(player.clone()).await?;

                let template = store.get_housing_template(&target_instance.template_id)?;
                let success_msg = world_config
                    .msg_teleport_success
                    .replace("{name}", &template.name);

                let look_output = self.describe_current_room(&player).await?;

                Ok(format!("{}\n\n{}", success_msg, look_output))
            }

            None => {
                // HOME with no args - teleport to primary housing

                // 1. Check if player is in combat
                if player.in_combat {
                    return Ok(world_config.err_teleport_in_combat.clone());
                }

                // 2. Check if current room allows teleportation out
                let current_room = store.get_room_async(&player.current_room).await?;
                if current_room.flags.contains(&RoomFlag::NoTeleportOut) {
                    return Ok(world_config.err_teleport_restricted.clone());
                }

                // 3. Check teleport cooldown
                if let Some(last_teleport) = player.last_teleport {
                    let now = Utc::now();
                    let elapsed = (now - last_teleport).num_seconds() as u64;
                    let cooldown = world_config.home_cooldown_seconds;

                    if elapsed < cooldown {
                        let remaining = cooldown - elapsed;
                        let minutes = remaining / 60;
                        let seconds = remaining % 60;
                        let time_str = if minutes > 0 {
                            format!(
                                "{} minute{} {} second{}",
                                minutes,
                                if minutes == 1 { "" } else { "s" },
                                seconds,
                                if seconds == 1 { "" } else { "s" }
                            )
                        } else {
                            format!("{} second{}", seconds, if seconds == 1 { "" } else { "s" })
                        };

                        return Ok(world_config
                            .err_teleport_cooldown
                            .replace("{time}", &time_str));
                    }
                }

                // 4. Check if player has housing
                let instances = store.get_player_housing_instances(&player.username)?;
                if instances.is_empty() {
                    return Ok(world_config.err_no_housing.clone());
                }

                // 5. Determine target housing instance
                let target_instance = if let Some(primary_id) = &player.primary_housing_id {
                    // Try to use primary housing
                    instances
                        .iter()
                        .find(|inst| &inst.id == primary_id)
                        .or_else(|| instances.first()) // Fallback to first if primary not found
                } else {
                    // No primary set, use first instance
                    instances.first()
                };

                let target_instance = match target_instance {
                    Some(inst) => inst,
                    None => return Ok(world_config.err_no_housing.clone()),
                };

                // Verify player still has access
                if target_instance.owner != player.username {
                    return Ok(world_config.err_teleport_no_access.clone());
                }

                // 6. Teleport player to housing entry room
                player.current_room = target_instance.entry_room_id.clone();
                player.last_teleport = Some(Utc::now());
                player.touch();

                // Save player
                store.put_player_async(player.clone()).await?;

                // 7. Get the template name for success message
                let template = store.get_housing_template(&target_instance.template_id)?;
                let housing_name = template.name;

                // Build success message
                let success_msg = world_config
                    .msg_teleport_success
                    .replace("{name}", &housing_name);

                // Get room description using describe_current_room
                let look_output = self.describe_current_room(&player).await?;

                Ok(format!("{}\n\n{}", success_msg, look_output))
            }
        }
    }

    /// Handle INVITE command - add guest to housing
    async fn handle_invite(
        &mut self,
        session: &Session,
        player_name: String,
        config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;
        let store = self.store();
        let world_config = store.get_world_config().unwrap_or_default();

        // Check player owns housing
        let instances = store.get_player_housing_instances(&player.username)?;
        if instances.is_empty() {
            return Ok(world_config.err_invite_no_housing.clone());
        }

        // Check if player is currently in one of their housing rooms
        let current_instance = instances.iter().find(|inst| {
            inst.room_mappings
                .values()
                .any(|room_id| room_id == &player.current_room)
        });

        let mut current_instance = match current_instance {
            Some(inst) => inst.clone(),
            None => return Ok(world_config.err_invite_not_in_housing.clone()),
        };

        // Validate target player exists
        let target = store.get_player_async(&player_name).await;
        if target.is_err() {
            return Ok(world_config
                .err_invite_player_not_found
                .replace("{name}", &player_name));
        }

        // Check if already a guest
        if current_instance.guests.contains(&player_name) {
            return Ok(world_config
                .err_invite_already_guest
                .replace("{name}", &player_name));
        }

        // Add to guest list
        current_instance.guests.push(player_name.clone());
        store.put_housing_instance(&current_instance)?;

        Ok(world_config
            .msg_invite_success
            .replace("{name}", &player_name))
    }

    /// Handle UNINVITE command - remove guest from housing
    async fn handle_uninvite(
        &mut self,
        session: &Session,
        player_name: String,
        config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;
        let store = self.store();
        let world_config = store.get_world_config().unwrap_or_default();

        // Check player owns housing
        let instances = store.get_player_housing_instances(&player.username)?;
        if instances.is_empty() {
            return Ok(world_config.err_invite_no_housing.clone());
        }

        // Check if player is currently in one of their housing rooms
        let current_instance = instances.iter().find(|inst| {
            inst.room_mappings
                .values()
                .any(|room_id| room_id == &player.current_room)
        });

        let mut current_instance = match current_instance {
            Some(inst) => inst.clone(),
            None => return Ok(world_config.err_invite_not_in_housing.clone()),
        };

        // Check if player is on guest list
        if !current_instance.guests.contains(&player_name) {
            return Ok(world_config
                .err_uninvite_not_guest
                .replace("{name}", &player_name));
        }

        // Remove from guest list
        current_instance.guests.retain(|g| g != &player_name);
        store.put_housing_instance(&current_instance)?;

        Ok(world_config
            .msg_uninvite_success
            .replace("{name}", &player_name))
    }

    /// Handle DESCRIBE command - edit current room description (housing only)
    async fn handle_describe(
        &mut self,
        session: &Session,
        description: Option<String>,
        config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;
        let store = self.store();
        let world_config = store.get_world_config().unwrap_or_default();

        // Get all housing instances player owns or has guest access to
        let owned_instances = store.get_player_housing_instances(&player.username)?;
        let guest_instances = store.get_guest_housing_instances(&player.username)?;

        // Check if player is currently in a housing room
        let current_owned = owned_instances.iter().find(|inst| {
            inst.room_mappings
                .values()
                .any(|room_id| room_id == &player.current_room)
        });

        let current_guest = guest_instances.iter().find(|inst| {
            inst.room_mappings
                .values()
                .any(|room_id| room_id == &player.current_room)
        });

        // Determine which instance and check permissions
        let (current_instance, is_owner) = if let Some(inst) = current_owned {
            (inst, true)
        } else if let Some(inst) = current_guest {
            (inst, false)
        } else {
            return Ok(world_config.err_describe_not_in_housing.clone());
        };

        // Get the housing template to check permissions
        let template = store.get_housing_template(&current_instance.template_id)?;

        // Check if editing is allowed
        if !is_owner && !template.permissions.can_edit_description {
            return Ok(world_config.err_describe_no_permission.clone());
        }

        // If no description provided, show current description and permissions
        if description.is_none() {
            let current_room = store.get_room_async(&player.current_room).await?;
            let desc = if current_room.long_desc.is_empty() {
                "Empty room."
            } else {
                &current_room.long_desc
            };

            return Ok(world_config.msg_describe_current.replace("{desc}", desc));
        }

        // Update the room description
        let new_desc = description.unwrap();

        // Validate length (500 char max for room descriptions)
        const MAX_DESC_LENGTH: usize = 500;
        if new_desc.len() > MAX_DESC_LENGTH {
            return Ok(world_config
                .err_describe_too_long
                .replace("{max}", &MAX_DESC_LENGTH.to_string())
                .replace("{actual}", &new_desc.len().to_string()));
        }

        // Update the room
        let mut current_room = store.get_room_async(&player.current_room).await?;
        current_room.long_desc = new_desc;
        store.put_room_async(current_room).await?;

        Ok(world_config.msg_describe_success.clone())
    }

    /// Handle LOCK command - lock room or item
    async fn handle_lock(
        &mut self,
        session: &Session,
        target: Option<String>,
        config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;
        let store = self.store();

        // If target is None, lock current room
        if target.is_none() {
            // Check if player owns housing instances
            let instances = store.get_player_housing_instances(&player.username)?;
            if instances.is_empty() {
                return Ok("You don't own any housing.".to_string());
            }

            // Check if player is currently in one of their housing rooms
            let current_instance = instances.iter().find(|inst| {
                inst.room_mappings
                    .values()
                    .any(|room_id| room_id == &player.current_room)
            });

            if current_instance.is_none() {
                return Ok("You can only lock rooms in your own housing.".to_string());
            }

            // Get the current room
            let mut current_room = store.get_room_async(&player.current_room).await?;

            // Check if already locked
            if current_room.locked {
                return Ok("This room is already locked.".to_string());
            }

            // Lock the room
            current_room.locked = true;
            store.put_room_async(current_room).await?;

            return Ok("You lock the room. Only you and your guests can enter now.".to_string());
        }

        // Phase 4: Item locking
        let target_name = target.unwrap().to_uppercase();

        // Search for item in player's inventory
        for item_id in &player.inventory {
            if let Ok(mut item) = store.get_object(item_id) {
                if item.name.to_uppercase() == target_name {
                    // Check if player owns this item
                    match &item.owner {
                        crate::tmush::types::ObjectOwner::Player { username } => {
                            if username != &player.username {
                                return Ok(format!("You don't own {}.", item.name));
                            }
                        }
                        crate::tmush::types::ObjectOwner::World => {
                            return Ok(format!(
                                "{} is a world item and cannot be locked.",
                                item.name
                            ));
                        }
                    }

                    // Check if already locked
                    if item.locked {
                        return Ok(format!("{} is already locked.", item.name));
                    }

                    // Lock the item
                    item.locked = true;
                    store.put_object(item.clone())?;

                    return Ok(format!(
                        "You lock {}. It cannot be taken by others.",
                        item.name
                    ));
                }
            }
        }

        Ok(format!(
            "You don't have '{}' in your inventory.",
            target_name
        ))
    }

    /// Handle UNLOCK command - unlock room or item
    async fn handle_unlock(
        &mut self,
        session: &Session,
        target: Option<String>,
        config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;
        let store = self.store();

        // If target is None, unlock current room
        if target.is_none() {
            // Check if player owns housing instances
            let instances = store.get_player_housing_instances(&player.username)?;
            if instances.is_empty() {
                return Ok("You don't own any housing.".to_string());
            }

            // Check if player is currently in one of their housing rooms
            let current_instance = instances.iter().find(|inst| {
                inst.room_mappings
                    .values()
                    .any(|room_id| room_id == &player.current_room)
            });

            if current_instance.is_none() {
                return Ok("You can only unlock rooms in your own housing.".to_string());
            }

            // Get the current room
            let mut current_room = store.get_room_async(&player.current_room).await?;

            // Check if already unlocked
            if !current_room.locked {
                return Ok("This room is already unlocked.".to_string());
            }

            // Unlock the room
            current_room.locked = false;
            store.put_room_async(current_room).await?;

            return Ok("You unlock the room. Anyone can enter now.".to_string());
        }

        // Phase 4: Item unlocking
        let target_name = target.unwrap().to_uppercase();

        // Search for item in player's inventory
        for item_id in &player.inventory {
            if let Ok(mut item) = store.get_object(item_id) {
                if item.name.to_uppercase() == target_name {
                    // Check if player owns this item
                    match &item.owner {
                        crate::tmush::types::ObjectOwner::Player { username } => {
                            if username != &player.username {
                                return Ok(format!("You don't own {}.", item.name));
                            }
                        }
                        crate::tmush::types::ObjectOwner::World => {
                            return Ok(format!(
                                "{} is a world item and cannot be unlocked.",
                                item.name
                            ));
                        }
                    }

                    // Check if already unlocked
                    if !item.locked {
                        return Ok(format!("{} is already unlocked.", item.name));
                    }

                    // Unlock the item
                    item.locked = false;
                    store.put_object(item.clone())?;

                    return Ok(format!("You unlock {}. Others can take it now.", item.name));
                }
            }
        }

        Ok(format!(
            "You don't have '{}' in your inventory.",
            target_name
        ))
    }

    /// Handle KICK command - remove player from housing
    async fn handle_kick(
        &mut self,
        session: &Session,
        target: Option<String>,
        config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;

        let target = match target {
            Some(t) => t,
            None => return Ok("Usage: KICK <player> or KICK ALL".to_string()),
        };

        // Get instances and housing rooms first (separate scope)
        let (mut current_instance, housing_rooms) = {
            let store = self.store();

            // Check if player owns housing
            let instances = store.get_player_housing_instances(&player.username)?;
            if instances.is_empty() {
                return Ok("You don't own any housing.".to_string());
            }

            // Check if player is currently in one of their housing rooms
            let current_instance = instances.iter().find(|inst| {
                inst.room_mappings
                    .values()
                    .any(|room_id| room_id == &player.current_room)
            });

            let current_instance = match current_instance {
                Some(inst) => inst.clone(),
                None => return Ok("You can only kick players from your own housing.".to_string()),
            };

            let housing_rooms: Vec<String> =
                current_instance.room_mappings.values().cloned().collect();
            (current_instance, housing_rooms)
        };

        // Handle KICK ALL
        if target == "ALL" {
            let guest_count = current_instance.guests.len();

            // Find all guests currently in the housing
            let guests_to_kick = {
                let room_manager = self.get_room_manager().await?;
                let mut guests = Vec::new();
                for room_id in &housing_rooms {
                    let players_in_room = room_manager.get_players_in_room(room_id);
                    for guest_username in players_in_room {
                        if guest_username != player.username {
                            guests.push(guest_username);
                        }
                    }
                }
                guests
            };

            // Teleport them
            for guest_username in guests_to_kick {
                let store = self.store();
                if let Ok(mut guest_player) = store.get_player_async(&guest_username).await {
                    guest_player.current_room = "town_square".to_string();
                    let _ = store.put_player_async(guest_player).await;
                }
            }

            // Clear guest list
            current_instance.guests.clear();
            let store = self.store();
            store.put_housing_instance(&current_instance)?;

            return Ok(format!(
                "You kick all guests from your housing. {} guest(s) removed from guest list.",
                guest_count
            ));
        }

        // Handle KICK <player>
        if !current_instance.guests.contains(&target) {
            return Ok(format!("{} is not on your guest list.", target));
        }

        // Check if the player is currently in the housing
        let was_in_housing = {
            let room_manager = self.get_room_manager().await?;
            let mut found = false;
            for room_id in &housing_rooms {
                let players_in_room = room_manager.get_players_in_room(room_id);
                if players_in_room.contains(&target) {
                    found = true;
                    break;
                }
            }
            found
        };

        // Teleport if in housing
        if was_in_housing {
            let store = self.store();
            if let Ok(mut target_player) = store.get_player_async(&target).await {
                target_player.current_room = "town_square".to_string();
                store.put_player_async(target_player).await?;
            }
        }

        // Remove from guest list
        current_instance.guests.retain(|g| g != &target);
        let store = self.store();
        store.put_housing_instance(&current_instance)?;

        if was_in_housing {
            return Ok(format!(
                "You kick {} from your housing. They have been teleported to town square.",
                target
            ));
        }

        Ok(format!(
            "You remove {} from your guest list. They can no longer enter your housing.",
            target
        ))
    }

    /// Handle @SETCONFIG command - set world configuration
    async fn handle_set_config(
        &mut self,
        session: &Session,
        field: String,
        value: String,
        config: &Config,
    ) -> Result<String> {
        // Note: In future, add role-based permissions here
        // For now, allowing any authenticated user for testing
        let player = self.get_or_create_player(session).await?;

        let store = self.store();

        // Update the configuration field
        match store.update_world_config_field(&field, &value, &player.username) {
            Ok(_) => Ok(format!(
                "Configuration updated:\n{}: {}\n\nUpdated by: {}",
                field, value, player.username
            )),
            Err(e) => Ok(format!("Error updating configuration: {}", e)),
        }
    }

    /// Handle @GETCONFIG command - view world configuration
    async fn handle_get_config(
        &mut self,
        session: &Session,
        field: Option<String>,
        config: &Config,
    ) -> Result<String> {
        let _player = self.get_or_create_player(session).await?;
        let store = self.store();

        let world_config = store.get_world_config()?;

        match field {
            Some(f) => {
                let value = match f.as_str() {
                    // Branding
                    "welcome_message" => &world_config.welcome_message,
                    "motd" => &world_config.motd,
                    "world_name" => &world_config.world_name,
                    "world_description" => &world_config.world_description,
                    // Help system
                    "help_main" => &world_config.help_main,
                    "help_commands" => &world_config.help_commands,
                    "help_movement" => &world_config.help_movement,
                    "help_social" => &world_config.help_social,
                    "help_bulletin" => &world_config.help_bulletin,
                    "help_companion" => &world_config.help_companion,
                    "help_mail" => &world_config.help_mail,
                    // Error messages
                    "err_no_exit" => &world_config.err_no_exit,
                    "err_whisper_self" => &world_config.err_whisper_self,
                    "err_no_shops" => &world_config.err_no_shops,
                    "err_item_not_found" => &world_config.err_item_not_found,
                    "err_trade_self" => &world_config.err_trade_self,
                    "err_say_what" => &world_config.err_say_what,
                    "err_emote_what" => &world_config.err_emote_what,
                    "err_insufficient_funds" => &world_config.err_insufficient_funds,
                    // Success messages
                    "msg_deposit_success" => &world_config.msg_deposit_success,
                    "msg_withdraw_success" => &world_config.msg_withdraw_success,
                    "msg_buy_success" => &world_config.msg_buy_success,
                    "msg_sell_success" => &world_config.msg_sell_success,
                    "msg_trade_initiated" => &world_config.msg_trade_initiated,
                    _ => return Ok(format!(
                        "Unknown configuration field: {}\n\n\
                        Available fields:\n\
                        Branding: welcome_message, motd, world_name, world_description\n\
                        Help: help_main, help_commands, help_movement, help_social, help_bulletin, help_companion, help_mail\n\
                        Errors: err_no_exit, err_whisper_self, err_no_shops, err_item_not_found, err_trade_self, err_say_what, err_emote_what, err_insufficient_funds\n\
                        Messages: msg_deposit_success, msg_withdraw_success, msg_buy_success, msg_sell_success, msg_trade_initiated",
                        f
                    )),
                };
                Ok(format!("{}:\n{}", f, value))
            }
            None => {
                // Show summary of configuration
                Ok(format!(
                    "=== WORLD CONFIGURATION ===\n\n\
                    World Name: {}\n\
                    Description: {}\n\n\
                    Configuration Fields: 24\n\
                    - 4 branding fields\n\
                    - 7 help system templates\n\
                    - 8 error message templates\n\
                    - 5 success message templates\n\n\
                    Use @GETCONFIG <field> to view specific field.\n\
                    Use @SETCONFIG <field> <value> to update.\n\n\
                    Last Updated: {}\n\
                    Updated By: {}",
                    world_config.world_name,
                    world_config.world_description,
                    world_config.updated_at.format("%Y-%m-%d %H:%M:%S"),
                    world_config.updated_by
                ))
            }
        }
    }

    /// Handle @EDITROOM command - edit any room description (admin only)
    async fn handle_edit_room(
        &mut self,
        session: &Session,
        room_id: String,
        new_description: String,
        config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;

        // Note: Allowing any authenticated user for alpha testing
        let store = self.store();

        // Validate description length (500 char max)
        const MAX_DESC_LENGTH: usize = 500;
        if new_description.len() > MAX_DESC_LENGTH {
            return Ok(format!(
                "Description too long: {} chars (max {})",
                new_description.len(),
                MAX_DESC_LENGTH
            ));
        }

        // Try to get the room
        let mut room = match store.get_room_async(&room_id).await {
            Ok(room) => room,
            Err(_) => {
                return Ok(format!(
                    "Room '{}' not found.\n\n\
                    Common room IDs:\n\
                    - gazebo_landing (Landing Gazebo)\n\
                    - town_square (Old Towne Square)\n\
                    - city_hall_lobby (City Hall Lobby)\n\
                    - mayor_office (Mayor's Office)\n\
                    - mesh_museum (Mesh Museum)\n\
                    - north_gate (Northern Gate)\n\
                    - south_market (Southern Market)",
                    room_id
                ));
            }
        };

        // Store old description for confirmation
        let old_desc = room.long_desc.clone();

        // Update the description
        room.long_desc = new_description.clone();

        // Save to database
        store.put_room_async(room).await?;

        Ok(format!(
            "Room '{}' description updated by {}.\n\n\
            OLD:\n{}\n\n\
            NEW:\n{}",
            room_id,
            player.username,
            if old_desc.is_empty() {
                "(empty)"
            } else {
                &old_desc
            },
            new_description
        ))
    }

    /// Handle @EDITNPC command - edit NPC properties (admin only)
    async fn handle_edit_npc(
        &mut self,
        session: &Session,
        npc_id: String,
        field: String,
        value: String,
        config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;

        // TODO: Add proper role-based permissions
        // For now, allowing any authenticated user for alpha testing

        let store = self.store();

        // Get the NPC
        let mut npc = match store.get_npc(&npc_id) {
            Ok(npc) => npc,
            Err(_) => {
                return Ok(format!(
                    "NPC '{}' not found.\n\n\
                    Available NPCs:\n\
                    - mayor_thompson (Mayor's Office)\n\
                    - city_clerk (City Hall Lobby)\n\
                    - gate_guard (North Gate)\n\
                    - market_vendor (South Market)\n\
                    - museum_curator (Mesh Museum)",
                    npc_id
                ));
            }
        };

        let old_value: String;

        // Parse field and update
        if field.starts_with("dialog.") {
            // Edit dialog entry
            let dialog_key = field.strip_prefix("dialog.").unwrap();
            old_value = npc
                .dialog
                .get(dialog_key)
                .cloned()
                .unwrap_or_else(|| "(not set)".to_string());

            // Validate length
            const MAX_DIALOG_LENGTH: usize = 500;
            if value.len() > MAX_DIALOG_LENGTH {
                return Ok(format!(
                    "Dialog too long: {} chars (max {})",
                    value.len(),
                    MAX_DIALOG_LENGTH
                ));
            }

            npc.dialog.insert(dialog_key.to_string(), value.clone());
        } else if field == "description" {
            old_value = npc.description.clone();

            // Validate length
            const MAX_DESC_LENGTH: usize = 500;
            if value.len() > MAX_DESC_LENGTH {
                return Ok(format!(
                    "Description too long: {} chars (max {})",
                    value.len(),
                    MAX_DESC_LENGTH
                ));
            }

            npc.description = value.clone();
        } else if field == "room" {
            old_value = npc.room_id.clone();

            // Verify room exists
            if store.get_room(&value).is_err() {
                return Ok(format!("Room '{}' not found.", value));
            }

            npc.room_id = value.clone();
        } else {
            return Ok(format!(
                "Unknown field: {}\n\n\
                Supported fields:\n\
                - dialog.<key> - Edit dialog entry (e.g., dialog.greeting)\n\
                - description - Edit NPC description\n\
                - room - Move NPC to different room",
                field
            ));
        }

        // Save updated NPC
        store.put_npc(npc)?;

        Ok(format!(
            "NPC '{}' updated by {}.\n\
            Field: {}\n\n\
            OLD: {}\n\n\
            NEW: {}",
            npc_id, player.username, field, old_value, value
        ))
    }

    /// Handle @DIALOG command - manage NPC dialogue trees (admin only)
    async fn handle_dialog(
        &mut self,
        session: &Session,
        npc_id: String,
        subcommand: String,
        args: Option<String>,
        config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;

        // TODO: Add proper role-based permissions
        // For now, allowing any authenticated user for alpha testing

        let store = self.store();

        // Get the NPC
        let mut npc = match store.get_npc(&npc_id) {
            Ok(npc) => npc,
            Err(_) => {
                return Ok(format!(
                    "NPC '{}' not found.\n\n\
                    Available NPCs: mayor_thompson, city_clerk, gate_guard, market_vendor, museum_curator",
                    npc_id
                ));
            }
        };

        match subcommand.as_str() {
            "LIST" => {
                // List all dialogue topics for this NPC
                let mut response = format!("=== Dialogue Topics for {} ===\n\n", npc.name);

                if npc.dialog.is_empty() && npc.dialog_tree.is_empty() {
                    response.push_str("(No dialogue configured)\n");
                } else {
                    if !npc.dialog.is_empty() {
                        response.push_str("Simple Dialogue:\n");
                        let mut topics: Vec<_> = npc.dialog.keys().collect();
                        topics.sort();
                        for topic in topics {
                            let text = &npc.dialog[topic];
                            let preview = if text.len() > 50 {
                                format!("{}...", &text[..47])
                            } else {
                                text.clone()
                            };
                            response.push_str(&format!("  {} - {}\n", topic, preview));
                        }
                        response.push('\n');
                    }

                    if !npc.dialog_tree.is_empty() {
                        response.push_str("Dialogue Trees:\n");
                        let mut topics: Vec<_> = npc.dialog_tree.keys().collect();
                        topics.sort();
                        for topic in topics {
                            let node = &npc.dialog_tree[topic];
                            let choice_count = node.choices.len();
                            let action_count = node.actions.len();
                            let condition_count = node.conditions.len();
                            response.push_str(&format!(
                                "  {} - {} choices, {} actions, {} conditions\n",
                                topic, choice_count, action_count, condition_count
                            ));
                        }
                    }
                }

                response.push_str("\nCommands:\n");
                response.push_str("  @DIALOG <npc> VIEW <topic> - View dialogue details\n");
                response.push_str("  @DIALOG <npc> ADD <topic> <text> - Add simple dialogue\n");
                response.push_str("  @DIALOG <npc> EDIT <topic> <json> - Edit dialogue tree\n");
                response.push_str("  @DIALOG <npc> DELETE <topic> - Remove dialogue\n");
                response.push_str("  @DIALOG <npc> TEST <topic> - Test dialogue conditions\n");

                Ok(response)
            }

            "VIEW" => {
                // View details of a specific topic
                let topic = match args {
                    Some(t) => t,
                    None => return Ok("Usage: @DIALOG <npc> VIEW <topic>".to_string()),
                };

                let mut response = format!("=== Dialogue: {} - {} ===\n\n", npc.name, topic);

                // Check simple dialogue first
                if let Some(text) = npc.dialog.get(&topic) {
                    response.push_str("Type: Simple Text\n\n");
                    response.push_str(&format!("Text:\n{}\n", text));
                    return Ok(response);
                }

                // Check dialogue tree
                if let Some(node) = npc.dialog_tree.get(&topic) {
                    response.push_str("Type: Dialogue Tree\n\n");

                    // Show as formatted JSON
                    match serde_json::to_string_pretty(node) {
                        Ok(json) => {
                            response.push_str("JSON Definition:\n");
                            response.push_str(&json);
                            response.push('\n');
                        }
                        Err(e) => {
                            response.push_str(&format!("Error serializing: {}\n", e));
                        }
                    }

                    return Ok(response);
                }

                Ok(format!(
                    "Topic '{}' not found for NPC '{}'.",
                    topic, npc.name
                ))
            }

            "ADD" => {
                // Add simple text dialogue
                let args_str = match args {
                    Some(a) => a,
                    None => return Ok("Usage: @DIALOG <npc> ADD <topic> <text>".to_string()),
                };

                let parts: Vec<&str> = args_str.splitn(2, ' ').collect();
                if parts.len() < 2 {
                    return Ok("Usage: @DIALOG <npc> ADD <topic> <text>".to_string());
                }

                let topic = parts[0].to_string();
                let text = parts[1].to_string();

                // Validate
                const MAX_TEXT_LENGTH: usize = 500;
                if text.len() > MAX_TEXT_LENGTH {
                    return Ok(format!(
                        "Text too long: {} chars (max {})",
                        text.len(),
                        MAX_TEXT_LENGTH
                    ));
                }

                // Check if topic already exists
                if npc.dialog.contains_key(&topic) || npc.dialog_tree.contains_key(&topic) {
                    return Ok(format!(
                        "Topic '{}' already exists. Use @DIALOG {} DELETE {} first, or @DIALOG {} EDIT {}",
                        topic, npc_id, topic, npc_id, topic
                    ));
                }

                // Add to simple dialogue
                npc.dialog.insert(topic.clone(), text.clone());
                let npc_name = npc.name.clone();
                store.put_npc(npc)?;

                Ok(format!(
                    "Added simple dialogue for {} by {}.\n\
                    Topic: {}\n\
                    Text: {}",
                    npc_name, player.username, topic, text
                ))
            }

            "EDIT" => {
                // Edit dialogue tree with JSON
                let args_str = match args {
                    Some(a) => a,
                    None => return Ok("Usage: @DIALOG <npc> EDIT <topic> <json>\n\nExample JSON:\n{\n  \"text\": \"Hello!\",\n  \"choices\": [\n    {\"label\": \"Goodbye\", \"exit\": true}\n  ]\n}".to_string()),
                };

                let parts: Vec<&str> = args_str.splitn(2, ' ').collect();
                if parts.len() < 2 {
                    return Ok("Usage: @DIALOG <npc> EDIT <topic> <json>".to_string());
                }

                let topic = parts[0].to_string();
                let json_str = parts[1].to_string();

                // Parse JSON into DialogNode
                use crate::tmush::types::DialogNode;
                let node: DialogNode = match serde_json::from_str(&json_str) {
                    Ok(n) => n,
                    Err(e) => {
                        return Ok(format!(
                            "Invalid JSON: {}\n\n\
                            Expected format:\n\
                            {{\n\
                              \"text\": \"Dialogue text here\",\n\
                              \"actions\": [{{\n\
                                \"type\": \"give_item\",\n\
                                \"item_id\": \"sword\",\n\
                                \"quantity\": 1\n\
                              }}],\n\
                              \"choices\": [{{\n\
                                \"label\": \"Continue\",\n\
                                \"goto\": \"next_node\"\n\
                              }}]\n\
                            }}",
                            e
                        ));
                    }
                };

                // Remove from simple dialogue if it exists there
                npc.dialog.remove(&topic);

                // Add/update in dialogue tree
                npc.dialog_tree.insert(topic.clone(), node);
                let npc_name = npc.name.clone();
                store.put_npc(npc)?;

                Ok(format!(
                    "Updated dialogue tree for {} by {}.\n\
                    Topic: {}\n\n\
                    Use @DIALOG {} VIEW {} to see the result.",
                    npc_name, player.username, topic, npc_id, topic
                ))
            }

            "DELETE" => {
                // Delete a dialogue topic
                let topic = match args {
                    Some(t) => t,
                    None => return Ok("Usage: @DIALOG <npc> DELETE <topic>".to_string()),
                };

                let was_simple = npc.dialog.remove(&topic).is_some();
                let was_tree = npc.dialog_tree.remove(&topic).is_some();

                if !was_simple && !was_tree {
                    return Ok(format!(
                        "Topic '{}' not found for NPC '{}'.",
                        topic, npc.name
                    ));
                }

                let npc_name = npc.name.clone();
                store.put_npc(npc)?;

                let type_str = if was_tree {
                    "dialogue tree"
                } else {
                    "simple dialogue"
                };
                Ok(format!(
                    "Deleted {} '{}' from {} by {}.",
                    type_str, topic, npc_name, player.username
                ))
            }

            "TEST" => {
                // Test dialogue conditions for current player
                let topic = match args {
                    Some(t) => t,
                    None => return Ok("Usage: @DIALOG <npc> TEST <topic>".to_string()),
                };

                // Check if topic exists
                if let Some(node) = npc.dialog_tree.get(&topic) {
                    let mut response =
                        format!("=== Testing Dialogue: {} - {} ===\n\n", npc.name, topic);

                    // Test node conditions
                    response.push_str("Node Conditions:\n");
                    if node.conditions.is_empty() {
                        response.push_str("  (none - always visible)\n");
                    } else {
                        for (_i, condition) in node.conditions.iter().enumerate() {
                            let result =
                                self.evaluate_conditions(&[condition.clone()], &player, &npc.id);
                            let status = if result.unwrap_or(false) {
                                "✓ PASS"
                            } else {
                                "✗ FAIL"
                            };
                            response.push_str(&format!("  {} - {:?}\n", status, condition));
                        }
                    }

                    response.push_str("\nVisible Choices:\n");
                    let visible_choices: Vec<_> = node
                        .choices
                        .iter()
                        .filter(|c| {
                            self.evaluate_conditions(&c.conditions, &player, &npc.id)
                                .unwrap_or(false)
                        })
                        .collect();

                    if visible_choices.is_empty() {
                        response.push_str("  (none visible - player doesn't meet conditions)\n");
                    } else {
                        for (i, choice) in visible_choices.iter().enumerate() {
                            response.push_str(&format!("  {}) {}\n", i + 1, choice.label));
                            if !choice.conditions.is_empty() {
                                response.push_str("     Conditions: ");
                                for cond in &choice.conditions {
                                    response.push_str(&format!("{:?} ", cond));
                                }
                                response.push('\n');
                            }
                        }
                    }

                    response.push_str("\nActions:\n");
                    if node.actions.is_empty() {
                        response.push_str("  (none)\n");
                    } else {
                        for action in &node.actions {
                            response.push_str(&format!("  {:?}\n", action));
                        }
                    }

                    Ok(response)
                } else if npc.dialog.contains_key(&topic) {
                    Ok(format!(
                        "Topic '{}' is simple text dialogue (no conditions to test).",
                        topic
                    ))
                } else {
                    Ok(format!(
                        "Topic '{}' not found for NPC '{}'.",
                        topic, npc.name
                    ))
                }
            }

            _ => Ok(format!(
                "Unknown subcommand: {}\n\n\
                    Available subcommands:\n\
                    - LIST - Show all dialogue topics\n\
                    - VIEW <topic> - View dialogue details\n\
                    - ADD <topic> <text> - Add simple dialogue\n\
                    - EDIT <topic> <json> - Edit dialogue tree\n\
                    - DELETE <topic> - Remove dialogue\n\
                    - TEST <topic> - Test conditions for current player",
                subcommand
            )),
        }
    }

    /// Handle @RECIPE command - manage crafting recipes (admin only)
    async fn handle_recipe(
        &mut self,
        session: &Session,
        subcommand: String,
        args: Vec<String>,
        _config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;

        // Check admin level (sysop = 3, admin = 2)
        if player.admin_level.unwrap_or(0) < 2 {
            return Ok("Only admins can manage crafting recipes.".to_string());
        }

        let store = self.store();

        match subcommand.as_str() {
            "CREATE" => {
                if args.len() < 2 {
                    return Ok("Usage: @RECIPE CREATE <id> <name>\nExample: @RECIPE CREATE goat_cheese \"Goat Milk Cheese\"".to_string());
                }

                let recipe_id = args[0].to_lowercase();
                let recipe_name = args[1..].join(" ");

                // Check if recipe already exists
                if store.recipe_exists(&recipe_id)? {
                    return Ok(format!("Recipe '{}' already exists. Use @RECIPE EDIT to modify it or @RECIPE DELETE to remove it first.", recipe_id));
                }

                // Create new recipe
                let recipe = crate::tmush::types::CraftingRecipe::new(
                    &recipe_id,
                    &recipe_name,
                    &recipe_id, // Default result item ID same as recipe ID
                    &player.username,
                );

                store.put_recipe(recipe)?;

                Ok(format!(
                    "Recipe '{}' created.\n\nNext steps:\n\
                    - @RECIPE EDIT {} MATERIAL ADD <item_id> <qty>\n\
                    - @RECIPE EDIT {} RESULT <item_id> [qty]\n\
                    - @RECIPE EDIT {} DESCRIPTION <text>\n\
                    - @RECIPE EDIT {} STATION <station_id>",
                    recipe_name, recipe_id, recipe_id, recipe_id, recipe_id
                ))
            }
            "EDIT" => {
                if args.is_empty() {
                    return Ok("Usage: @RECIPE EDIT <id> <field> <value>\nFields: MATERIAL, RESULT, DESCRIPTION, STATION".to_string());
                }

                let recipe_id = args[0].to_lowercase();
                let mut recipe = match store.get_recipe(&recipe_id) {
                    Ok(r) => r,
                    Err(_) => return Ok(format!("Recipe '{}' not found.", recipe_id)),
                };

                if args.len() < 2 {
                    return Ok("Usage: @RECIPE EDIT <id> <field> <value>".to_string());
                }

                let field = args[1].to_uppercase();
                match field.as_str() {
                    "MATERIAL" => {
                        if args.len() < 3 {
                            return Ok(
                                "Usage: @RECIPE EDIT <id> MATERIAL ADD/REMOVE <item_id> [qty]"
                                    .to_string(),
                            );
                        }

                        let action = args[2].to_uppercase();
                        match action.as_str() {
                            "ADD" => {
                                if args.len() < 5 {
                                    return Ok(
                                        "Usage: @RECIPE EDIT <id> MATERIAL ADD <item_id> <qty>"
                                            .to_string(),
                                    );
                                }

                                let item_id = &args[3];
                                let quantity: u32 = args[4].parse().unwrap_or(1);

                                recipe
                                    .materials
                                    .push(crate::tmush::types::RecipeMaterial::new(
                                        item_id, quantity,
                                    ));
                                store.put_recipe(recipe)?;

                                Ok(format!("Added material: {} x{}", item_id, quantity))
                            }
                            "REMOVE" => {
                                if args.len() < 4 {
                                    return Ok(
                                        "Usage: @RECIPE EDIT <id> MATERIAL REMOVE <item_id>"
                                            .to_string(),
                                    );
                                }

                                let item_id = &args[3];
                                recipe.materials.retain(|m| m.item_id != *item_id);
                                store.put_recipe(recipe)?;

                                Ok(format!("Removed material: {}", item_id))
                            }
                            _ => Ok(
                                "Usage: @RECIPE EDIT <id> MATERIAL ADD/REMOVE <item_id> [qty]"
                                    .to_string(),
                            ),
                        }
                    }
                    "RESULT" => {
                        if args.len() < 3 {
                            return Ok(
                                "Usage: @RECIPE EDIT <id> RESULT <item_id> [qty]".to_string()
                            );
                        }

                        recipe.result_item_id = args[2].clone();
                        if args.len() >= 4 {
                            recipe.result_quantity = args[3].parse().unwrap_or(1);
                        }

                        store.put_recipe(recipe.clone())?;

                        Ok(format!(
                            "Recipe will create: {} x{}",
                            recipe.result_item_id, recipe.result_quantity
                        ))
                    }
                    "DESCRIPTION" => {
                        if args.len() < 3 {
                            return Ok("Usage: @RECIPE EDIT <id> DESCRIPTION <text>".to_string());
                        }

                        recipe.description = args[2..].join(" ");
                        store.put_recipe(recipe)?;

                        Ok("Description updated.".to_string())
                    }
                    "STATION" => {
                        if args.len() < 3 {
                            return Ok("Usage: @RECIPE EDIT <id> STATION <station_id>\nExample: @RECIPE EDIT goat_cheese STATION cheese_press".to_string());
                        }

                        recipe.requires_station = Some(args[2].clone());
                        store.put_recipe(recipe.clone())?;

                        Ok(format!("Recipe requires crafting station: {}", args[2]))
                    }
                    _ => Ok(
                        "Unknown field. Use: MATERIAL, RESULT, DESCRIPTION, or STATION".to_string(),
                    ),
                }
            }
            "DELETE" => {
                if args.is_empty() {
                    return Ok("Usage: @RECIPE DELETE <id>".to_string());
                }

                let recipe_id = args[0].to_lowercase();
                match store.get_recipe(&recipe_id) {
                    Ok(recipe) => {
                        store.delete_recipe(&recipe_id)?;
                        Ok(format!("Recipe '{}' has been deleted.", recipe.name))
                    }
                    Err(_) => Ok(format!("Recipe '{}' not found.", recipe_id)),
                }
            }
            "LIST" => {
                let station_filter = args.first().map(|s| s.as_str());
                let recipes = store.list_recipes(station_filter)?;

                if recipes.is_empty() {
                    return Ok("No recipes found.".to_string());
                }

                let mut output = if let Some(station) = station_filter {
                    format!("=== RECIPES FOR {} ===\n\n", station)
                } else {
                    "=== ALL RECIPES ===\n\n".to_string()
                };

                for (idx, recipe) in recipes.iter().enumerate() {
                    output.push_str(&format!(
                        "{}. {} (ID: {})\n",
                        idx + 1,
                        recipe.name,
                        recipe.id
                    ));

                    if let Some(station) = &recipe.requires_station {
                        output.push_str(&format!("   Station: {}\n", station));
                    }

                    output.push_str(&format!("   Materials: {}\n", recipe.materials.len()));
                    output.push_str(&format!(
                        "   Creates: {} x{}\n\n",
                        recipe.result_item_id, recipe.result_quantity
                    ));
                }

                Ok(output)
            }
            "SHOW" => {
                if args.is_empty() {
                    return Ok("Usage: @RECIPE SHOW <id>".to_string());
                }

                let recipe_id = args[0].to_lowercase();
                let recipe = match store.get_recipe(&recipe_id) {
                    Ok(r) => r,
                    Err(_) => return Ok(format!("Recipe '{}' not found.", recipe_id)),
                };

                let mut output = format!(
                    "=== RECIPE: {} ===\n\
                    ID: {}\n\
                    Description: {}\n\n\
                    MATERIALS REQUIRED:\n",
                    recipe.name,
                    recipe.id,
                    if recipe.description.is_empty() {
                        "(no description)"
                    } else {
                        &recipe.description
                    }
                );

                if recipe.materials.is_empty() {
                    output.push_str("  (no materials set - use @RECIPE EDIT to add)\n");
                } else {
                    for material in &recipe.materials {
                        let consumed = if material.consumed {
                            "consumed"
                        } else {
                            "tool"
                        };
                        output.push_str(&format!(
                            "  - {} x{} ({})\n",
                            material.item_id, material.quantity, consumed
                        ));
                    }
                }

                output.push_str(&format!(
                    "\nCREATES: {} x{}\n",
                    recipe.result_item_id, recipe.result_quantity
                ));

                if let Some(station) = &recipe.requires_station {
                    output.push_str(&format!("REQUIRES STATION: {}\n", station));
                }

                output.push_str(&format!(
                    "\nCreated by: {} on {}\n",
                    recipe.created_by,
                    recipe.created_at.format("%Y-%m-%d")
                ));

                Ok(output)
            }
            _ => Ok(format!(
                "Unknown @RECIPE subcommand: {}\n\
                Use: CREATE, EDIT, DELETE, LIST, SHOW",
                subcommand
            )),
        }
    }

    /// Handle @QUEST command - manage quests (admin only)
    async fn handle_quest_admin(
        &mut self,
        session: &Session,
        subcommand: String,
        args: Vec<String>,
        _config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;

        // Check admin level (sysop = 3, admin = 2)
        if player.admin_level.unwrap_or(0) < 2 {
            return Ok("Only admins can manage quests.".to_string());
        }

        let store = self.store();

        match subcommand.as_str() {
            "CREATE" => {
                if args.len() < 2 {
                    return Ok("Usage: @QUEST CREATE <id> <name>\nExample: @QUEST CREATE fetch_water \"Fetch Water for the Village\"".to_string());
                }

                let quest_id = args[0].to_lowercase();
                let quest_name = args[1..].join(" ");

                // Check if quest already exists
                if store.quest_exists(&quest_id)? {
                    return Ok(format!("Quest '{}' already exists. Use @QUEST EDIT to modify it or @QUEST DELETE to remove it first.", quest_id));
                }

                // Create new quest with minimal defaults
                let quest = crate::tmush::types::QuestRecord::new(
                    &quest_id,
                    &quest_name,
                    "",       // Empty description initially
                    "system", // Default NPC
                    1,        // Default level
                );

                store.put_quest(quest)?;

                Ok(format!(
                    "Quest '{}' created.\n\nNext steps:\n\
                    - @QUEST EDIT {} DESCRIPTION <text>\n\
                    - @QUEST EDIT {} GIVER <npc_id>\n\
                    - @QUEST EDIT {} LEVEL <num>\n\
                    - @QUEST EDIT {} OBJECTIVE ADD <type> <details>\n\
                    - @QUEST EDIT {} REWARD CURRENCY <amount>",
                    quest_name, quest_id, quest_id, quest_id, quest_id, quest_id
                ))
            }
            "EDIT" => {
                if args.is_empty() {
                    return Ok("Usage: @QUEST EDIT <id> <field> <value>\nFields: DESCRIPTION, GIVER, LEVEL, OBJECTIVE, REWARD, PREREQUISITE".to_string());
                }

                let quest_id = args[0].to_lowercase();
                let mut quest = match store.get_quest(&quest_id) {
                    Ok(q) => q,
                    Err(_) => return Ok(format!("Quest '{}' not found.", quest_id)),
                };

                if args.len() < 2 {
                    return Ok("Usage: @QUEST EDIT <id> <field> <value>".to_string());
                }

                let field = args[1].to_uppercase();
                match field.as_str() {
                    "DESCRIPTION" => {
                        if args.len() < 3 {
                            return Ok("Usage: @QUEST EDIT <id> DESCRIPTION <text>".to_string());
                        }

                        quest.description = args[2..].join(" ");
                        store.put_quest(quest)?;

                        Ok("Quest description updated.".to_string())
                    }
                    "GIVER" => {
                        if args.len() < 3 {
                            return Ok("Usage: @QUEST EDIT <id> GIVER <npc_id>".to_string());
                        }

                        quest.quest_giver_npc = args[2].clone();
                        store.put_quest(quest.clone())?;

                        Ok(format!("Quest giver set to: {}", args[2]))
                    }
                    "LEVEL" => {
                        if args.len() < 3 {
                            return Ok("Usage: @QUEST EDIT <id> LEVEL <num>".to_string());
                        }

                        match args[2].parse::<u8>() {
                            Ok(level) if level >= 1 && level <= 5 => {
                                quest.difficulty = level;
                                store.put_quest(quest)?;
                                Ok(format!("Quest difficulty set to: {}", level))
                            }
                            _ => Ok("Difficulty must be a number (1-5)".to_string()),
                        }
                    }
                    "OBJECTIVE" => {
                        if args.len() < 3 {
                            return Ok("Usage: @QUEST EDIT <id> OBJECTIVE ADD/REMOVE <details>".to_string());
                        }

                        let action = args[2].to_uppercase();
                        match action.as_str() {
                            "ADD" => {
                                if args.len() < 5 {
                                    return Ok("Usage: @QUEST EDIT <id> OBJECTIVE ADD <type> <target> [count]\nTypes: VISIT, TALK, KILL, COLLECT, USE".to_string());
                                }

                                let obj_type = args[3].to_uppercase();
                                let target = args[4].clone();
                                let count = if args.len() > 5 {
                                    args[5].parse().unwrap_or(1)
                                } else {
                                    1
                                };

                                use crate::tmush::types::{ObjectiveType, QuestObjective};

                                let objective = match obj_type.as_str() {
                                    "VISIT" => QuestObjective::new(
                                        &format!("Visit {}", target),
                                        ObjectiveType::VisitLocation {
                                            room_id: target,
                                        },
                                        count,
                                    ),
                                    "TALK" => QuestObjective::new(
                                        &format!("Talk to {}", target),
                                        ObjectiveType::TalkToNpc {
                                            npc_id: target,
                                        },
                                        count,
                                    ),
                                    "KILL" => QuestObjective::new(
                                        &format!("Defeat {} {}", count, target),
                                        ObjectiveType::KillEnemy {
                                            enemy_type: target,
                                            count,
                                        },
                                        count,
                                    ),
                                    "COLLECT" => QuestObjective::new(
                                        &format!("Collect {} {}", count, target),
                                        ObjectiveType::CollectItem {
                                            item_id: target,
                                            count,
                                        },
                                        count,
                                    ),
                                    "USE" => QuestObjective::new(
                                        &format!("Use {}", target),
                                        ObjectiveType::UseItem {
                                            item_id: target.clone(),
                                            target,
                                        },
                                        count,
                                    ),
                                    _ => return Ok("Unknown objective type. Use: VISIT, TALK, KILL, COLLECT, or USE".to_string()),
                                };

                                quest.objectives.push(objective);
                                store.put_quest(quest)?;

                                Ok(format!("Added objective: {} {}", obj_type, args[4]))
                            }
                            "REMOVE" => {
                                if args.len() < 4 {
                                    return Ok("Usage: @QUEST EDIT <id> OBJECTIVE REMOVE <index>".to_string());
                                }

                                match args[3].parse::<usize>() {
                                    Ok(index) if index > 0 && index <= quest.objectives.len() => {
                                        quest.objectives.remove(index - 1);
                                        store.put_quest(quest)?;
                                        Ok(format!("Removed objective #{}", index))
                                    }
                                    _ => Ok(format!("Invalid index. Quest has {} objectives.", quest.objectives.len())),
                                }
                            }
                            _ => Ok("Usage: @QUEST EDIT <id> OBJECTIVE ADD/REMOVE <details>".to_string()),
                        }
                    }
                    "REWARD" => {
                        if args.len() < 3 {
                            return Ok("Usage: @QUEST EDIT <id> REWARD CURRENCY/XP/ITEM <value>".to_string());
                        }

                        let reward_type = args[2].to_uppercase();
                        match reward_type.as_str() {
                            "CURRENCY" => {
                                if args.len() < 4 {
                                    return Ok("Usage: @QUEST EDIT <id> REWARD CURRENCY <amount>".to_string());
                                }

                                match args[3].parse::<i64>() {
                                    Ok(amount) => {
                                        use crate::tmush::types::CurrencyAmount;
                                        quest.rewards.currency = Some(CurrencyAmount::Decimal { minor_units: amount });
                                        store.put_quest(quest)?;
                                        Ok(format!("Currency reward set to: {}", amount))
                                    }
                                    Err(_) => Ok("Amount must be a number".to_string()),
                                }
                            }
                            "XP" => {
                                if args.len() < 4 {
                                    return Ok("Usage: @QUEST EDIT <id> REWARD XP <amount>".to_string());
                                }

                                match args[3].parse::<u32>() {
                                    Ok(amount) => {
                                        quest.rewards.experience = amount;
                                        store.put_quest(quest)?;
                                        Ok(format!("Experience reward set to: {} XP", amount))
                                    }
                                    Err(_) => Ok("Amount must be a number".to_string()),
                                }
                            }
                            "ITEM" => {
                                if args.len() < 4 {
                                    return Ok("Usage: @QUEST EDIT <id> REWARD ITEM <item_id>".to_string());
                                }

                                quest.rewards.items.push(args[3].clone());
                                store.put_quest(quest)?;
                                Ok(format!("Added item reward: {}", args[3]))
                            }
                            _ => Ok("Unknown reward type. Use: CURRENCY, XP, or ITEM".to_string()),
                        }
                    }
                    "PREREQUISITE" => {
                        if args.len() < 3 {
                            return Ok("Usage: @QUEST EDIT <id> PREREQUISITE <quest_id>".to_string());
                        }

                        let prereq_id = args[2].to_lowercase();
                        
                        // Verify prerequisite quest exists
                        if !store.quest_exists(&prereq_id)? {
                            return Ok(format!("Prerequisite quest '{}' does not exist.", prereq_id));
                        }

                        quest.prerequisites.push(prereq_id.clone());
                        store.put_quest(quest)?;

                        Ok(format!("Added prerequisite: {}", prereq_id))
                    }
                    _ => Ok("Unknown field. Use: DESCRIPTION, GIVER, LEVEL, OBJECTIVE, REWARD, or PREREQUISITE".to_string()),
                }
            }
            "DELETE" => {
                if args.is_empty() {
                    return Ok("Usage: @QUEST DELETE <id>".to_string());
                }

                let quest_id = args[0].to_lowercase();
                match store.get_quest(&quest_id) {
                    Ok(quest) => {
                        store.delete_quest(&quest_id)?;
                        Ok(format!("Quest '{}' has been deleted.", quest.name))
                    }
                    Err(_) => Ok(format!("Quest '{}' not found.", quest_id)),
                }
            }
            "LIST" => {
                let quest_ids = store.list_quest_ids()?;

                if quest_ids.is_empty() {
                    return Ok("No quests found.".to_string());
                }

                let mut output = "=== ALL QUESTS ===\n\n".to_string();

                for (idx, quest_id) in quest_ids.iter().enumerate() {
                    if let Ok(quest) = store.get_quest(quest_id) {
                        output.push_str(&format!(
                            "{}. {} (ID: {})\n",
                            idx + 1,
                            quest.name,
                            quest.id
                        ));
                        output.push_str(&format!(
                            "   Level: {} | Giver: {}\n",
                            quest.difficulty, quest.quest_giver_npc
                        ));
                        output.push_str(&format!(
                            "   Objectives: {} | Prerequisites: {}\n\n",
                            quest.objectives.len(),
                            quest.prerequisites.len()
                        ));
                    }
                }

                Ok(output)
            }
            "SHOW" => {
                if args.is_empty() {
                    return Ok("Usage: @QUEST SHOW <id>".to_string());
                }

                let quest_id = args[0].to_lowercase();
                let quest = match store.get_quest(&quest_id) {
                    Ok(q) => q,
                    Err(_) => return Ok(format!("Quest '{}' not found.", quest_id)),
                };

                let mut output = format!(
                    "=== QUEST: {} ===\n\
                    ID: {}\n\
                    Description: {}\n\
                    Quest Giver: {}\n\
                    Difficulty: {}\n\n",
                    quest.name,
                    quest.id,
                    if quest.description.is_empty() {
                        "(no description)"
                    } else {
                        &quest.description
                    },
                    quest.quest_giver_npc,
                    quest.difficulty
                );

                // Objectives
                output.push_str("OBJECTIVES:\n");
                if quest.objectives.is_empty() {
                    output.push_str("  (no objectives set - use @QUEST EDIT to add)\n");
                } else {
                    for (idx, obj) in quest.objectives.iter().enumerate() {
                        output.push_str(&format!(
                            "  {}. {} ({})\n",
                            idx + 1,
                            obj.description,
                            obj.required
                        ));
                    }
                }

                // Prerequisites
                if !quest.prerequisites.is_empty() {
                    output.push_str("\nPREREQUISITES:\n");
                    for prereq in &quest.prerequisites {
                        output.push_str(&format!("  - {}\n", prereq));
                    }
                }

                // Rewards
                output.push_str("\nREWARDS:\n");
                if quest.rewards.experience > 0 {
                    output.push_str(&format!("  - {} XP\n", quest.rewards.experience));
                }
                if let Some(crate::tmush::types::CurrencyAmount::Decimal { minor_units }) =
                    &quest.rewards.currency
                {
                    if *minor_units > 0 {
                        output.push_str(&format!("  - {} currency\n", minor_units));
                    }
                }
                if !quest.rewards.items.is_empty() {
                    for item in &quest.rewards.items {
                        output.push_str(&format!("  - Item: {}\n", item));
                    }
                }

                Ok(output)
            }
            _ => Ok(format!(
                "Unknown @QUEST subcommand: {}\n\
                Use: CREATE, EDIT, DELETE, LIST, SHOW",
                subcommand
            )),
        }
    }

    /// Handle the ACHIEVEMENT admin command for data-driven achievement management.
    async fn handle_achievement_admin(
        &mut self,
        session: &Session,
        subcommand: String,
        args: Vec<String>,
        _config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;
        if player.admin_level.unwrap_or(0) < 2 {
            return Ok(
                "Insufficient permission: admin level 2+ required for @ACHIEVEMENT commands."
                    .to_string(),
            );
        }

        let store = self.store();

        let subcmd = subcommand.to_uppercase();
        match subcmd.as_str() {
            "CREATE" => {
                if args.is_empty() {
                    return Ok("Usage: @ACHIEVEMENT CREATE <achievement_id> <name>\nExample: @ACHIEVEMENT CREATE combat_veteran \"Combat Veteran\"".to_string());
                }
                let achievement_id = &args[0];
                let name = args[1..].join(" ");
                if name.is_empty() {
                    return Ok("Achievement name cannot be empty".to_string());
                }

                if store.achievement_exists(achievement_id)? {
                    return Ok(format!("Achievement '{}' already exists", achievement_id));
                }

                let achievement = AchievementRecord::new(
                    achievement_id,
                    &name,
                    "No description set yet",
                    AchievementCategory::Special,
                    AchievementTrigger::KillCount { required: 1 },
                );

                store.put_achievement(achievement)?;
                Ok(format!(
                    "Created achievement '{}' with name \"{}\"",
                    achievement_id, name
                ))
            }
            "EDIT" => {
                if args.len() < 2 {
                    return Ok(
                        "Usage: @ACHIEVEMENT EDIT <achievement_id> <field> <value>\n\
                        Fields: DESCRIPTION, CATEGORY, TRIGGER, TITLE, HIDDEN\n\
                        Examples:\n\
                        @ACHIEVEMENT EDIT first_blood DESCRIPTION You defeated your first enemy\n\
                        @ACHIEVEMENT EDIT first_blood CATEGORY Combat\n\
                        @ACHIEVEMENT EDIT first_blood TRIGGER KILLCOUNT 1\n\
                        @ACHIEVEMENT EDIT first_blood TITLE \"Rookie Warrior\"\n\
                        @ACHIEVEMENT EDIT first_blood HIDDEN false"
                            .to_string(),
                    );
                }

                let achievement_id = &args[0];
                let field = args[1].to_uppercase();
                let value_args = &args[2..];

                let mut achievement = match store.get_achievement(achievement_id) {
                    Ok(ach) => ach,
                    Err(_) => {
                        return Ok(format!("Achievement '{}' does not exist", achievement_id))
                    }
                };

                match field.as_str() {
                    "DESCRIPTION" => {
                        let description = value_args.join(" ");
                        if description.is_empty() {
                            return Ok("Description cannot be empty".to_string());
                        }
                        achievement.description = description.clone();
                        store.put_achievement(achievement.clone())?;
                        Ok(format!("Updated description for '{}'", achievement_id))
                    }
                    "CATEGORY" => {
                        if value_args.is_empty() {
                            return Ok("Usage: @ACHIEVEMENT EDIT <id> CATEGORY <category>\nCategories: Combat, Exploration, Social, Economic, Quest, Special".to_string());
                        }
                        let category_str = value_args[0].to_uppercase();
                        let category = match category_str.as_str() {
                            "COMBAT" => AchievementCategory::Combat,
                            "EXPLORATION" => AchievementCategory::Exploration,
                            "SOCIAL" => AchievementCategory::Social,
                            "ECONOMIC" => AchievementCategory::Economic,
                            "QUEST" => AchievementCategory::Quest,
                            "SPECIAL" => AchievementCategory::Special,
                            _ => return Ok(format!("Invalid category: {}\nValid categories: Combat, Exploration, Social, Economic, Quest, Special", value_args[0])),
                        };
                        achievement.category = category;
                        store.put_achievement(achievement.clone())?;
                        Ok(format!("Updated category for '{}' to {:?}", achievement_id, achievement.category))
                    }
                    "TRIGGER" => {
                        if value_args.len() < 2 {
                            return Ok(
                                "Usage: @ACHIEVEMENT EDIT <id> TRIGGER <type> <params>\n\
                                Trigger types:\n\
                                  KILLCOUNT <required>\n\
                                  ROOMVISITS <required>\n\
                                  FRIENDCOUNT <required>\n\
                                  MESSAGESSENT <required>\n\
                                  TRADECOUNT <required>\n\
                                  CURRENCYEARNED <amount>\n\
                                  QUESTCOMPLETION <required>\n\
                                  VISITLOCATION <room_id>\n\
                                  COMPLETEQUEST <quest_id>".to_string()
                            );
                        }

                        let trigger_type = value_args[0].to_uppercase();
                        let trigger = match trigger_type.as_str() {
                            "KILLCOUNT" => {
                                let required = value_args.get(1)
                                    .and_then(|s| s.parse::<u32>().ok())
                                    .ok_or_else(|| anyhow::anyhow!("Invalid number for required kills"))?;
                                AchievementTrigger::KillCount { required }
                            }
                            "ROOMVISITS" => {
                                let required = value_args.get(1)
                                    .and_then(|s| s.parse::<u32>().ok())
                                    .ok_or_else(|| anyhow::anyhow!("Invalid number for required room visits"))?;
                                AchievementTrigger::RoomVisits { required }
                            }
                            "FRIENDCOUNT" => {
                                let required = value_args.get(1)
                                    .and_then(|s| s.parse::<u32>().ok())
                                    .ok_or_else(|| anyhow::anyhow!("Invalid number for required friends"))?;
                                AchievementTrigger::FriendCount { required }
                            }
                            "MESSAGESSENT" => {
                                let required = value_args.get(1)
                                    .and_then(|s| s.parse::<u32>().ok())
                                    .ok_or_else(|| anyhow::anyhow!("Invalid number for required messages"))?;
                                AchievementTrigger::MessagesSent { required }
                            }
                            "TRADECOUNT" => {
                                let required = value_args.get(1)
                                    .and_then(|s| s.parse::<u32>().ok())
                                    .ok_or_else(|| anyhow::anyhow!("Invalid number for required trades"))?;
                                AchievementTrigger::TradeCount { required }
                            }
                            "CURRENCYEARNED" => {
                                let amount = value_args.get(1)
                                    .and_then(|s| s.parse::<i64>().ok())
                                    .ok_or_else(|| anyhow::anyhow!("Invalid currency amount"))?;
                                AchievementTrigger::CurrencyEarned { amount }
                            }
                            "QUESTCOMPLETION" => {
                                let required = value_args.get(1)
                                    .and_then(|s| s.parse::<u32>().ok())
                                    .ok_or_else(|| anyhow::anyhow!("Invalid number for required quests"))?;
                                AchievementTrigger::QuestCompletion { required }
                            }
                            "VISITLOCATION" => {
                                let room_id = value_args.get(1)
                                    .ok_or_else(|| anyhow::anyhow!("Missing room_id for VISITLOCATION trigger"))?
                                    .to_string();
                                AchievementTrigger::VisitLocation { room_id }
                            }
                            "COMPLETEQUEST" => {
                                let quest_id = value_args.get(1)
                                    .ok_or_else(|| anyhow::anyhow!("Missing quest_id for COMPLETEQUEST trigger"))?
                                    .to_string();
                                AchievementTrigger::CompleteQuest { quest_id }
                            }
                            _ => return Ok(format!("Invalid trigger type: {}\nSee @ACHIEVEMENT EDIT for valid trigger types", value_args[0])),
                        };

                        achievement.trigger = trigger;
                        store.put_achievement(achievement.clone())?;
                        Ok(format!("Updated trigger for '{}' to {:?}", achievement_id, achievement.trigger))
                    }
                    "TITLE" => {
                        let title = value_args.join(" ");
                        if title.is_empty() {
                            achievement.title = None;
                            store.put_achievement(achievement.clone())?;
                            Ok(format!("Cleared title for '{}'", achievement_id))
                        } else {
                            achievement.title = Some(title.clone());
                            store.put_achievement(achievement.clone())?;
                            Ok(format!("Updated title for '{}' to \"{}\"", achievement_id, title))
                        }
                    }
                    "HIDDEN" => {
                        if value_args.is_empty() {
                            return Ok("Usage: @ACHIEVEMENT EDIT <id> HIDDEN <true|false>".to_string());
                        }
                        let hidden_str = value_args[0].to_lowercase();
                        let hidden = match hidden_str.as_str() {
                            "true" | "yes" | "1" => true,
                            "false" | "no" | "0" => false,
                            _ => return Ok(format!("Invalid hidden value: {}\nUse: true or false", value_args[0])),
                        };
                        achievement.hidden = hidden;
                        store.put_achievement(achievement.clone())?;
                        Ok(format!("Updated hidden status for '{}' to {}", achievement_id, hidden))
                    }
                    _ => Ok(format!(
                        "Unknown field: {}\nValid fields: DESCRIPTION, CATEGORY, TRIGGER, TITLE, HIDDEN",
                        field
                    )),
                }
            }
            "DELETE" => {
                if args.is_empty() {
                    return Ok("Usage: @ACHIEVEMENT DELETE <achievement_id>".to_string());
                }
                let achievement_id = &args[0];

                if !store.achievement_exists(achievement_id)? {
                    return Ok(format!("Achievement '{}' does not exist", achievement_id));
                }

                store.delete_achievement(achievement_id)?;
                Ok(format!("Deleted achievement '{}'", achievement_id))
            }
            "LIST" => {
                let category_filter = args.get(0).map(|s| s.to_uppercase());

                let achievements = if let Some(cat_str) = category_filter {
                    let category = match cat_str.as_str() {
                        "COMBAT" => AchievementCategory::Combat,
                        "EXPLORATION" => AchievementCategory::Exploration,
                        "SOCIAL" => AchievementCategory::Social,
                        "ECONOMIC" => AchievementCategory::Economic,
                        "QUEST" => AchievementCategory::Quest,
                        "SPECIAL" => AchievementCategory::Special,
                        _ => return Ok(format!("Invalid category: {}\nValid categories: Combat, Exploration, Social, Economic, Quest, Special", args[0])),
                    };
                    store.get_achievements_by_category(&category)?
                } else {
                    let ids = store.list_achievement_ids()?;
                    let mut achs = Vec::new();
                    for id in ids {
                        if let Ok(ach) = store.get_achievement(&id) {
                            achs.push(ach);
                        }
                    }
                    achs
                };

                if achievements.is_empty() {
                    return Ok("No achievements found".to_string());
                }

                let mut output = format!("Achievements ({})\n", achievements.len());
                output.push_str("─".repeat(60).as_str());
                output.push('\n');
                for ach in achievements {
                    let hidden_str = if ach.hidden { " [HIDDEN]" } else { "" };
                    output.push_str(&format!(
                        "• {} - \"{}\" [{:?}]{}\n",
                        ach.id, ach.name, ach.category, hidden_str
                    ));
                }

                Ok(output)
            }
            "SHOW" => {
                if args.is_empty() {
                    return Ok("Usage: @ACHIEVEMENT SHOW <achievement_id>".to_string());
                }
                let achievement_id = &args[0];

                let achievement = match store.get_achievement(achievement_id) {
                    Ok(ach) => ach,
                    Err(_) => {
                        return Ok(format!("Achievement '{}' does not exist", achievement_id))
                    }
                };

                let mut output = format!("Achievement: {}\n", achievement.id);
                output.push_str("─".repeat(60).as_str());
                output.push('\n');
                output.push_str(&format!("Name: {}\n", achievement.name));
                output.push_str(&format!("Description: {}\n", achievement.description));
                output.push_str(&format!("Category: {:?}\n", achievement.category));
                output.push_str(&format!("Trigger: {:?}\n", achievement.trigger));
                if let Some(title) = &achievement.title {
                    output.push_str(&format!("Title Reward: \"{}\"\n", title));
                } else {
                    output.push_str("Title Reward: None\n");
                }
                output.push_str(&format!("Hidden: {}\n", achievement.hidden));

                Ok(output)
            }
            _ => Ok(format!(
                "Unknown @ACHIEVEMENT subcommand: {}\n\
                Use: CREATE, EDIT, DELETE, LIST, SHOW",
                subcommand
            )),
        }
    }

    /// Handle the NPC admin command for data-driven NPC management.
    async fn handle_npc_admin(
        &mut self,
        session: &Session,
        subcommand: String,
        args: Vec<String>,
        _config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;

        if player.admin_level.unwrap_or(0) < 2 {
            return Ok(
                "Insufficient permission: admin level 2+ required for @NPC commands.".to_string(),
            );
        }

        let store = self.store();

        let subcmd = subcommand.to_uppercase();
        match subcmd.as_str() {
            "CREATE" => {
                if args.len() < 2 {
                    return Ok("Usage: @NPC CREATE <npc_id> <name>\nExample: @NPC CREATE blacksmith \"Forge Master Grimm\"".to_string());
                }
                let npc_id = &args[0];
                let name = args[1..].join(" ");
                if name.is_empty() {
                    return Ok("NPC name cannot be empty".to_string());
                }

                if store.npc_exists(npc_id)? {
                    return Ok(format!("NPC '{}' already exists", npc_id));
                }

                use crate::tmush::types::NpcRecord;
                let npc = NpcRecord::new(
                    npc_id,
                    &name,
                    "No title set",
                    "No description set yet",
                    "starting_room", // Default room
                );

                store.put_npc(npc)?;
                Ok(format!("Created NPC '{}' with name \"{}\"", npc_id, name))
            }
            "EDIT" => {
                if args.len() < 2 {
                    return Ok("Usage: @NPC EDIT <npc_id> <field> <value>\n\
                        Fields: NAME, TITLE, DESCRIPTION, ROOM, DIALOG, FLAG\n\
                        Examples:\n\
                        @NPC EDIT blacksmith NAME Forge Master Grimm\n\
                        @NPC EDIT blacksmith TITLE Master Blacksmith\n\
                        @NPC EDIT blacksmith DESCRIPTION A burly dwarf with...\n\
                        @NPC EDIT blacksmith ROOM town_forge\n\
                        @NPC EDIT blacksmith DIALOG greeting Welcome to my forge!\n\
                        @NPC EDIT blacksmith FLAG VENDOR"
                        .to_string());
                }

                let npc_id = &args[0];
                let field = args[1].to_uppercase();
                let value_args = &args[2..];

                let mut npc = match store.get_npc(npc_id) {
                    Ok(n) => n,
                    Err(_) => return Ok(format!("NPC '{}' does not exist", npc_id)),
                };

                match field.as_str() {
                    "NAME" => {
                        let name = value_args.join(" ");
                        if name.is_empty() {
                            return Ok("Name cannot be empty".to_string());
                        }
                        npc.name = name.clone();
                        store.put_npc(npc)?;
                        Ok(format!("Updated name for '{}'", npc_id))
                    }
                    "TITLE" => {
                        let title = value_args.join(" ");
                        if title.is_empty() {
                            return Ok("Title cannot be empty".to_string());
                        }
                        npc.title = title.clone();
                        store.put_npc(npc)?;
                        Ok(format!("Updated title for '{}'", npc_id))
                    }
                    "DESCRIPTION" | "DESC" => {
                        let description = value_args.join(" ");
                        if description.is_empty() {
                            return Ok("Description cannot be empty".to_string());
                        }
                        npc.description = description.clone();
                        store.put_npc(npc)?;
                        Ok(format!("Updated description for '{}'", npc_id))
                    }
                    "ROOM" => {
                        if value_args.is_empty() {
                            return Ok("Usage: @NPC EDIT <id> ROOM <room_id>".to_string());
                        }
                        let room_id = value_args[0].to_string();
                        npc.room_id = room_id.clone();
                        store.put_npc(npc)?;
                        Ok(format!("Moved '{}' to room '{}'", npc_id, room_id))
                    }
                    "DIALOG" | "DIALOGUE" => {
                        if value_args.len() < 2 {
                            return Ok("Usage: @NPC EDIT <id> DIALOG <key> <response>\nExample: @NPC EDIT blacksmith DIALOG greeting Welcome traveler!".to_string());
                        }
                        let key = value_args[0].to_lowercase();
                        let response = value_args[1..].join(" ");
                        npc.dialog.insert(key.clone(), response.clone());
                        store.put_npc(npc)?;
                        Ok(format!("Added dialog '{}' to '{}'", key, npc_id))
                    }
                    "FLAG" => {
                        if value_args.is_empty() {
                            return Ok("Usage: @NPC EDIT <id> FLAG <flag>\nFlags: VENDOR, GUARD, TUTORIALNPC, QUESTGIVER, IMMORTAL".to_string());
                        }
                        use crate::tmush::types::NpcFlag;
                        let flag_str = value_args[0].to_uppercase();
                        let flag = match flag_str.as_str() {
                            "VENDOR" => NpcFlag::Vendor,
                            "GUARD" => NpcFlag::Guard,
                            "TUTORIALNPC" | "TUTORIAL" => NpcFlag::TutorialNpc,
                            "QUESTGIVER" | "QUEST" => NpcFlag::QuestGiver,
                            "IMMORTAL" => NpcFlag::Immortal,
                            _ => return Ok(format!("Invalid flag: {}\nValid flags: VENDOR, GUARD, TUTORIALNPC, QUESTGIVER, IMMORTAL", value_args[0])),
                        };
                        
                        if !npc.flags.contains(&flag) {
                            npc.flags.push(flag.clone());
                            store.put_npc(npc)?;
                            Ok(format!("Added flag {:?} to '{}'", flag, npc_id))
                        } else {
                            Ok(format!("NPC '{}' already has flag {:?}", npc_id, flag))
                        }
                    }
                    _ => Ok(format!(
                        "Unknown field: {}\nValid fields: NAME, TITLE, DESCRIPTION, ROOM, DIALOG, FLAG",
                        field
                    )),
                }
            }
            "DELETE" => {
                if args.is_empty() {
                    return Ok("Usage: @NPC DELETE <npc_id>".to_string());
                }
                let npc_id = &args[0];

                if !store.npc_exists(npc_id)? {
                    return Ok(format!("NPC '{}' does not exist", npc_id));
                }

                store.delete_npc(npc_id)?;
                Ok(format!("Deleted NPC '{}'", npc_id))
            }
            "LIST" => {
                let ids = store.list_npc_ids()?;

                if ids.is_empty() {
                    return Ok("No NPCs found".to_string());
                }

                let mut output = format!("NPCs ({})\n", ids.len());
                output.push_str("─".repeat(60).as_str());
                output.push('\n');

                for id in ids {
                    if let Ok(npc) = store.get_npc(&id) {
                        let flags_str = if !npc.flags.is_empty() {
                            format!(" [{:?}]", npc.flags)
                        } else {
                            String::new()
                        };
                        output.push_str(&format!(
                            "• {} - \"{}\" ({}){}\n",
                            npc.id, npc.name, npc.room_id, flags_str
                        ));
                    }
                }

                Ok(output)
            }
            "SHOW" => {
                if args.is_empty() {
                    return Ok("Usage: @NPC SHOW <npc_id>".to_string());
                }
                let npc_id = &args[0];

                let npc = match store.get_npc(npc_id) {
                    Ok(n) => n,
                    Err(_) => return Ok(format!("NPC '{}' does not exist", npc_id)),
                };

                let mut output = format!("NPC: {}\n", npc.id);
                output.push_str("─".repeat(60).as_str());
                output.push('\n');
                output.push_str(&format!("Name: {}\n", npc.name));
                output.push_str(&format!("Title: {}\n", npc.title));
                output.push_str(&format!("Description: {}\n", npc.description));
                output.push_str(&format!("Location: {}\n", npc.room_id));

                if !npc.flags.is_empty() {
                    output.push_str(&format!("Flags: {:?}\n", npc.flags));
                }

                if !npc.dialog.is_empty() {
                    output.push_str("\nDialogue responses:\n");
                    for (key, response) in &npc.dialog {
                        output.push_str(&format!("  {}: {}\n", key, response));
                    }
                }

                Ok(output)
            }
            _ => Ok(format!(
                "Unknown @NPC subcommand: {}\n\
                Use: CREATE, EDIT, DELETE, LIST, SHOW",
                subcommand
            )),
        }
    }

    /// Handle @COMPANION command - manage companions (admin level 2+)
    async fn handle_companion_admin(
        &mut self,
        session: &Session,
        subcommand: String,
        args: Vec<String>,
        _config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;

        if player.admin_level.unwrap_or(0) < 2 {
            return Ok(
                "Insufficient permission: admin level 2+ required for @COMPANION commands."
                    .to_string(),
            );
        }

        let store = self.store();

        let subcmd = subcommand.to_uppercase();
        match subcmd.as_str() {
            "CREATE" => {
                if args.len() < 2 {
                    return Ok("Usage: @COMPANION CREATE <id> <name>\nExample: @COMPANION CREATE war_horse \"Battle Steed\"".to_string());
                }
                let companion_id = args[0].to_lowercase();
                let name = args[1..].join(" ");

                // Check if companion already exists
                if store.companion_exists(&companion_id)? {
                    return Ok(format!(
                        "Companion '{}' already exists. Use @COMPANION EDIT to modify it.",
                        companion_id
                    ));
                }

                // Create companion with default type (Dog) and default room (starting_room)
                use crate::tmush::types::{CompanionRecord, CompanionType};
                let companion =
                    CompanionRecord::new(&companion_id, &name, CompanionType::Dog, "starting_room");
                store.put_companion(companion)?;

                Ok(format!(
                    "Created companion '{}' ({}). Use @COMPANION EDIT to customize.",
                    companion_id, name
                ))
            }
            "EDIT" => {
                if args.len() < 3 {
                    return Ok("Usage: @COMPANION EDIT <id> <field> <value>\nFields: NAME, DESCRIPTION, TYPE, ROOM, BEHAVIOR\nExample: @COMPANION EDIT war_horse TYPE HORSE".to_string());
                }
                let companion_id = args[0].to_lowercase();
                let field = args[1].to_uppercase();
                let value = args[2..].join(" ");

                let mut companion = match store.get_companion(&companion_id) {
                    Ok(c) => c,
                    Err(_) => {
                        return Ok(format!(
                            "Companion '{}' not found. Use @COMPANION CREATE to create it.",
                            companion_id
                        ))
                    }
                };

                match field.as_str() {
                    "NAME" => {
                        companion.name = value.clone();
                        store.put_companion(companion)?;
                        Ok(format!(
                            "Updated companion '{}' name to '{}'",
                            companion_id, value
                        ))
                    }
                    "DESCRIPTION" => {
                        companion.description = value.clone();
                        store.put_companion(companion)?;
                        Ok(format!("Updated companion '{}' description", companion_id))
                    }
                    "TYPE" => {
                        use crate::tmush::types::CompanionType;
                        let companion_type = match value.to_uppercase().as_str() {
                            "HORSE" => CompanionType::Horse,
                            "DOG" => CompanionType::Dog,
                            "CAT" => CompanionType::Cat,
                            "FAMILIAR" => CompanionType::Familiar,
                            "MERCENARY" => CompanionType::Mercenary,
                            "CONSTRUCT" => CompanionType::Construct,
                            _ => return Ok(format!("Invalid companion type '{}'. Valid types: HORSE, DOG, CAT, FAMILIAR, MERCENARY, CONSTRUCT", value)),
                        };
                        companion.companion_type = companion_type;
                        store.put_companion(companion)?;
                        Ok(format!(
                            "Updated companion '{}' type to {}",
                            companion_id,
                            value.to_uppercase()
                        ))
                    }
                    "ROOM" => {
                        companion.room_id = value.clone();
                        store.put_companion(companion)?;
                        Ok(format!(
                            "Updated companion '{}' location to room '{}'",
                            companion_id, value
                        ))
                    }
                    "BEHAVIOR" => {
                        use crate::tmush::types::CompanionBehavior;

                        // Parse behavior from args[2..]
                        if args.len() < 3 {
                            return Ok("Usage: @COMPANION EDIT <id> BEHAVIOR <behavior> [params]\nBehaviors:\n  AutoFollow\n  AlertDanger\n  ExtraStorage <capacity>\n  CombatAssist <damage_bonus>\n  Healing <heal_amount> <cooldown_seconds>\n  SkillBoost <skill> <bonus>\n  IdleChatter <message1> [message2...]\nExample: @COMPANION EDIT war_horse BEHAVIOR ExtraStorage 30".to_string());
                        }

                        let behavior_type = args[2].to_uppercase();
                        let behavior = match behavior_type.as_str() {
                            "AUTOFOLLOW" => CompanionBehavior::AutoFollow,
                            "ALERTDANGER" => CompanionBehavior::AlertDanger,
                            "EXTRASTORAGE" => {
                                if args.len() < 4 {
                                    return Ok("Usage: @COMPANION EDIT <id> BEHAVIOR ExtraStorage <capacity>\nExample: @COMPANION EDIT war_horse BEHAVIOR ExtraStorage 30".to_string());
                                }
                                let capacity: u32 = args[3].parse().unwrap_or(20);
                                CompanionBehavior::ExtraStorage { capacity }
                            }
                            "COMBATASSIST" => {
                                if args.len() < 4 {
                                    return Ok("Usage: @COMPANION EDIT <id> BEHAVIOR CombatAssist <damage_bonus>\nExample: @COMPANION EDIT guard_dog BEHAVIOR CombatAssist 5".to_string());
                                }
                                let damage_bonus: u32 = args[3].parse().unwrap_or(5);
                                CompanionBehavior::CombatAssist { damage_bonus }
                            }
                            "HEALING" => {
                                if args.len() < 5 {
                                    return Ok("Usage: @COMPANION EDIT <id> BEHAVIOR Healing <heal_amount> <cooldown_seconds>\nExample: @COMPANION EDIT healing_cat BEHAVIOR Healing 10 300".to_string());
                                }
                                let heal_amount: u32 = args[3].parse().unwrap_or(10);
                                let cooldown_seconds: u64 = args[4].parse().unwrap_or(300);
                                CompanionBehavior::Healing { heal_amount, cooldown_seconds }
                            }
                            "SKILLBOOST" => {
                                if args.len() < 5 {
                                    return Ok("Usage: @COMPANION EDIT <id> BEHAVIOR SkillBoost <skill> <bonus>\nExample: @COMPANION EDIT magic_cat BEHAVIOR SkillBoost magic 3".to_string());
                                }
                                let skill = args[3].clone();
                                let bonus: u32 = args[4].parse().unwrap_or(2);
                                CompanionBehavior::SkillBoost { skill, bonus }
                            }
                            "IDLECHATTER" => {
                                if args.len() < 4 {
                                    return Ok("Usage: @COMPANION EDIT <id> BEHAVIOR IdleChatter <message1> [message2...]\nExample: @COMPANION EDIT friendly_dog BEHAVIOR IdleChatter \"*wags tail*\" \"*barks happily*\"".to_string());
                                }
                                let messages: Vec<String> = args[3..].iter().map(|s| s.to_string()).collect();
                                CompanionBehavior::IdleChatter { messages }
                            }
                            _ => return Ok(format!("Invalid behavior type '{}'. Valid types: AutoFollow, AlertDanger, ExtraStorage, CombatAssist, Healing, SkillBoost, IdleChatter", behavior_type)),
                        };

                        companion.behaviors.push(behavior);
                        store.put_companion(companion)?;
                        Ok(format!(
                            "Added behavior '{}' to companion '{}'",
                            behavior_type, companion_id
                        ))
                    }
                    _ => Ok(format!(
                        "Unknown field '{}'. Valid fields: NAME, DESCRIPTION, TYPE, ROOM, BEHAVIOR",
                        field
                    )),
                }
            }
            "DELETE" => {
                if args.is_empty() {
                    return Ok(
                        "Usage: @COMPANION DELETE <id>\nExample: @COMPANION DELETE war_horse"
                            .to_string(),
                    );
                }
                let companion_id = args[0].to_lowercase();

                if !store.companion_exists(&companion_id)? {
                    return Ok(format!("Companion '{}' not found.", companion_id));
                }

                store.delete_companion(&companion_id)?;
                Ok(format!("Deleted companion '{}'", companion_id))
            }
            "LIST" => {
                let companion_ids = store.list_companion_ids()?;
                if companion_ids.is_empty() {
                    return Ok("No companions found.".to_string());
                }

                let mut output = format!("Companions ({})\n", companion_ids.len());
                output.push_str("─────────────────────────────────────────────────────\n");
                for companion_id in companion_ids {
                    match store.get_companion(&companion_id) {
                        Ok(companion) => {
                            let type_str = format!("{:?}", companion.companion_type);
                            let owner_str = companion
                                .owner
                                .as_ref()
                                .map(|o| format!(" [Owner: {}]", o))
                                .unwrap_or_default();
                            output.push_str(&format!(
                                "  {} ({}): {} - {}{}\n",
                                companion.id,
                                type_str,
                                companion.name,
                                companion.room_id,
                                owner_str
                            ));
                        }
                        Err(_) => {} // Skip if companion can't be loaded
                    }
                }
                Ok(output)
            }
            "SHOW" => {
                if args.is_empty() {
                    return Ok(
                        "Usage: @COMPANION SHOW <id>\nExample: @COMPANION SHOW war_horse"
                            .to_string(),
                    );
                }
                let companion_id = args[0].to_lowercase();

                let companion = match store.get_companion(&companion_id) {
                    Ok(c) => c,
                    Err(_) => return Ok(format!("Companion '{}' not found.", companion_id)),
                };

                let mut output = format!("Companion Details: {}\n", companion.id);
                output.push_str("═════════════════════════════════════════════════════\n");
                output.push_str(&format!("Name: {}\n", companion.name));
                output.push_str(&format!("Type: {:?}\n", companion.companion_type));
                output.push_str(&format!("Description: {}\n", companion.description));
                output.push_str(&format!("Room: {}\n", companion.room_id));
                output.push_str(&format!(
                    "Owner: {}\n",
                    companion.owner.as_ref().unwrap_or(&"None".to_string())
                ));
                output.push_str(&format!(
                    "Loyalty: {} | Happiness: {}\n",
                    companion.loyalty, companion.happiness
                ));
                output.push_str(&format!(
                    "Mounted: {}\n",
                    if companion.is_mounted { "Yes" } else { "No" }
                ));

                if !companion.behaviors.is_empty() {
                    output.push_str("\nBehaviors:\n");
                    for behavior in &companion.behaviors {
                        output.push_str(&format!("  - {:?}\n", behavior));
                    }
                }

                if !companion.inventory.is_empty() {
                    output.push_str(&format!(
                        "\nInventory ({} items):\n",
                        companion.inventory.len()
                    ));
                    for item in &companion.inventory {
                        output.push_str(&format!("  - {}\n", item));
                    }
                }

                Ok(output)
            }
            _ => Ok(format!(
                "Unknown subcommand '{}'. Valid: CREATE, EDIT, DELETE, LIST, SHOW",
                subcmd
            )),
        }
    }

    /// Handle @ROOM command - manage rooms (admin level 2+)
    async fn handle_room_admin(
        &mut self,
        session: &Session,
        subcommand: String,
        args: Vec<String>,
        _config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;

        if player.admin_level.unwrap_or(0) < 2 {
            return Ok(
                "Insufficient permission: admin level 2+ required for @ROOM commands.".to_string(),
            );
        }

        let store = self.store();

        let subcmd = subcommand.to_uppercase();
        match subcmd.as_str() {
            "CREATE" => {
                if args.len() < 2 {
                    return Ok("Usage: @ROOM CREATE <id> <name>\nExample: @ROOM CREATE dark_cave \"Mysterious Cave\"".to_string());
                }
                let room_id = args[0].to_lowercase();
                let name = args[1..].join(" ");

                // Check if room already exists
                if store.room_exists(&room_id)? {
                    return Ok(format!(
                        "Room '{}' already exists. Use @ROOM EDIT to modify it.",
                        room_id
                    ));
                }

                // Create room with default settings
                use crate::tmush::types::RoomRecord;
                let room = RoomRecord::world(
                    &room_id,
                    &name,
                    "A new location.",
                    "This area has not been fully described yet.",
                );
                store.put_room(room)?;

                Ok(format!(
                    "Created room '{}' ({}). Use @ROOM EDIT to customize.",
                    room_id, name
                ))
            }
            "EDIT" => {
                if args.len() < 3 {
                    return Ok("Usage: @ROOM EDIT <id> <field> <value>\nFields: NAME, SHORTDESC, LONGDESC, EXIT, FLAG, CAPACITY, VISIBILITY, LOCKED, OWNER, HOUSING_TAGS\nExample: @ROOM EDIT dark_cave NAME \"Dark Cavern\"\nExample: @ROOM EDIT vault LOCKED true\nExample: @ROOM EDIT study VISIBILITY PRIVATE".to_string());
                }
                let room_id = args[0].to_lowercase();
                let field = args[1].to_uppercase();

                let mut room = match store.get_room(&room_id) {
                    Ok(r) => r,
                    Err(_) => {
                        return Ok(format!(
                            "Room '{}' not found. Use @ROOM CREATE to create it.",
                            room_id
                        ))
                    }
                };

                match field.as_str() {
                    "NAME" => {
                        let name = args[2..].join(" ");
                        if name.is_empty() {
                            return Ok("Name cannot be empty".to_string());
                        }
                        room.name = name.clone();
                        store.put_room(room)?;
                        Ok(format!("Updated room '{}' name to '{}'", room_id, name))
                    }
                    "SHORTDESC" | "SHORT" => {
                        let desc = args[2..].join(" ");
                        if desc.is_empty() {
                            return Ok("Short description cannot be empty".to_string());
                        }
                        room.short_desc = desc;
                        store.put_room(room)?;
                        Ok(format!("Updated room '{}' short description", room_id))
                    }
                    "LONGDESC" | "LONG" | "DESCRIPTION" => {
                        let desc = args[2..].join(" ");
                        if desc.is_empty() {
                            return Ok("Long description cannot be empty".to_string());
                        }
                        room.long_desc = desc;
                        store.put_room(room)?;
                        Ok(format!("Updated room '{}' long description", room_id))
                    }
                    "EXIT" => {
                        if args.len() < 4 {
                            return Ok("Usage: @ROOM EDIT <id> EXIT <direction> <dest_room|REMOVE>\nDirections: N, S, E, W, U, D, NE, NW, SE, SW\nExample: @ROOM EDIT tavern EXIT NORTH town_square\nExample: @ROOM EDIT tavern EXIT SOUTH REMOVE".to_string());
                        }
                        let direction_str = args[2].to_uppercase();
                        let dest_or_remove = args[3].to_uppercase();

                        // Parse direction
                        use crate::tmush::types::Direction;
                        let direction = match direction_str.as_str() {
                            "N" | "NORTH" => Direction::North,
                            "S" | "SOUTH" => Direction::South,
                            "E" | "EAST" => Direction::East,
                            "W" | "WEST" => Direction::West,
                            "U" | "UP" => Direction::Up,
                            "D" | "DOWN" => Direction::Down,
                            "NE" | "NORTHEAST" => Direction::Northeast,
                            "NW" | "NORTHWEST" => Direction::Northwest,
                            "SE" | "SOUTHEAST" => Direction::Southeast,
                            "SW" | "SOUTHWEST" => Direction::Southwest,
                            _ => return Ok(format!("Invalid direction '{}'. Valid: N, S, E, W, U, D, NE, NW, SE, SW", direction_str)),
                        };

                        // Check if removing exit
                        if dest_or_remove == "REMOVE" {
                            if room.exits.remove(&direction).is_some() {
                                store.put_room(room)?;
                                Ok(format!("Removed {} exit from room '{}'", direction_str, room_id))
                            } else {
                                Ok(format!("Room '{}' has no {} exit", room_id, direction_str))
                            }
                        } else {
                            let dest_room = dest_or_remove.to_lowercase();
                            // Check if destination room exists
                            if !store.room_exists(&dest_room)? {
                                return Ok(format!("Destination room '{}' does not exist. Create it first with @ROOM CREATE.", dest_room));
                            }

                            room.exits.insert(direction, dest_room.clone());
                            store.put_room(room)?;
                            Ok(format!("Added exit {} from '{}' to '{}'", direction_str, room_id, dest_room))
                        }
                    }
                    "FLAG" => {
                        if args.len() < 3 {
                            return Ok("Usage: @ROOM EDIT <id> FLAG <flag>\nExample: @ROOM EDIT dark_cave FLAG DARK".to_string());
                        }
                        let flag_str = args[2].to_uppercase();

                        use crate::tmush::types::RoomFlag;
                        let flag = match flag_str.as_str() {
                            "SAFE" => RoomFlag::Safe,
                            "DARK" => RoomFlag::Dark,
                            "INDOOR" => RoomFlag::Indoor,
                            "SHOP" => RoomFlag::Shop,
                            "QUESTLOCATION" => RoomFlag::QuestLocation,
                            "PVPENABLED" => RoomFlag::PvpEnabled,
                            "PLAYERCREATED" => RoomFlag::PlayerCreated,
                            "PRIVATE" => RoomFlag::Private,
                            "MODERATED" => RoomFlag::Moderated,
                            "INSTANCED" => RoomFlag::Instanced,
                            "CROWDED" => RoomFlag::Crowded,
                            "HOUSINGOFFICE" => RoomFlag::HousingOffice,
                            "NOTELEPORTOUT" => RoomFlag::NoTeleportOut,
                            _ => return Ok(format!("Invalid flag '{}'. Valid flags: SAFE, DARK, INDOOR, SHOP, QUESTLOCATION, PVPENABLED, PLAYERCREATED, PRIVATE, MODERATED, INSTANCED, CROWDED, HOUSINGOFFICE, NOTELEPORTOUT", flag_str)),
                        };

                        if !room.flags.contains(&flag) {
                            room.flags.push(flag);
                            store.put_room(room)?;
                            Ok(format!("Added flag {} to room '{}'", flag_str, room_id))
                        } else {
                            Ok(format!("Room '{}' already has flag {}", room_id, flag_str))
                        }
                    }
                    "CAPACITY" => {
                        if args.len() < 3 {
                            return Ok("Usage: @ROOM EDIT <id> CAPACITY <number>\nExample: @ROOM EDIT tavern CAPACITY 50".to_string());
                        }
                        let capacity: u16 = match args[2].parse() {
                            Ok(c) => c,
                            Err(_) => return Ok("Capacity must be a number between 1 and 65535".to_string()),
                        };
                        if capacity == 0 {
                            return Ok("Capacity must be at least 1".to_string());
                        }
                        room.max_capacity = capacity;
                        store.put_room(room)?;
                        Ok(format!("Set room '{}' capacity to {}", room_id, capacity))
                    }
                    "VISIBILITY" => {
                        if args.len() < 3 {
                            return Ok("Usage: @ROOM EDIT <id> VISIBILITY <public|private|hidden>\nExample: @ROOM EDIT study VISIBILITY PRIVATE".to_string());
                        }
                        let visibility_str = args[2].to_uppercase();
                        
                        use crate::tmush::types::RoomVisibility;
                        let visibility = match visibility_str.as_str() {
                            "PUBLIC" => RoomVisibility::Public,
                            "PRIVATE" => RoomVisibility::Private,
                            "HIDDEN" => RoomVisibility::Hidden,
                            _ => return Ok("Visibility must be PUBLIC, PRIVATE, or HIDDEN".to_string()),
                        };
                        
                        room.visibility = visibility;
                        store.put_room(room)?;
                        Ok(format!("Set room '{}' visibility to {}", room_id, visibility_str))
                    }
                    "LOCKED" => {
                        if args.len() < 3 {
                            return Ok("Usage: @ROOM EDIT <id> LOCKED <true|false>\nExample: @ROOM EDIT vault LOCKED true".to_string());
                        }
                        let value = args[2].to_lowercase();
                        match value.as_str() {
                            "true" | "yes" | "1" => {
                                room.locked = true;
                                store.put_room(room)?;
                                Ok(format!("Locked room '{}' (guests cannot enter)", room_id))
                            }
                            "false" | "no" | "0" => {
                                room.locked = false;
                                store.put_room(room)?;
                                Ok(format!("Unlocked room '{}'", room_id))
                            }
                            _ => Ok("Value must be 'true' or 'false'.".to_string()),
                        }
                    }
                    "OWNER" => {
                        if args.len() < 3 {
                            return Ok("Usage: @ROOM EDIT <id> OWNER <player|world>\nExample: @ROOM EDIT player_house OWNER alice\nExample: @ROOM EDIT tavern OWNER world".to_string());
                        }
                        let owner_str = args[2].to_lowercase();
                        
                        use crate::tmush::types::RoomOwner;
                        if owner_str == "world" {
                            room.owner = RoomOwner::World;
                            store.put_room(room)?;
                            Ok(format!("Transferred room '{}' to World ownership", room_id))
                        } else {
                            // Verify player exists
                            if store.get_player(&owner_str).is_err() {
                                return Ok(format!("Player '{}' not found", owner_str));
                            }
                            room.owner = RoomOwner::Player { username: owner_str.clone() };
                            store.put_room(room)?;
                            Ok(format!("Transferred room '{}' to player '{}'", room_id, owner_str))
                        }
                    }
                    "HOUSING_TAGS" | "HOUSINGTAGS" => {
                        if args.len() < 3 {
                            return Ok("Usage: @ROOM EDIT <id> HOUSING_TAGS <tag1,tag2,...>\nExample: @ROOM EDIT housing_office HOUSING_TAGS cozy,small,affordable\nUse empty value to clear: @ROOM EDIT housing_office HOUSING_TAGS \"\"".to_string());
                        }
                        let tags_str = args[2..].join(" ");
                        
                        if tags_str.is_empty() || tags_str == "\"\"" {
                            room.housing_filter_tags.clear();
                            store.put_room(room)?;
                            Ok(format!("Cleared housing filter tags from room '{}'", room_id))
                        } else {
                            let tags: Vec<String> = tags_str
                                .split(',')
                                .map(|s| s.trim().to_lowercase())
                                .filter(|s| !s.is_empty())
                                .collect();
                            
                            if tags.is_empty() {
                                return Ok("No valid tags provided".to_string());
                            }
                            
                            room.housing_filter_tags = tags.clone();
                            store.put_room(room)?;
                            Ok(format!("Set housing filter tags for room '{}': {}", room_id, tags.join(", ")))
                        }
                    }
                    _ => Ok(format!("Unknown field '{}'. Valid fields: NAME, SHORTDESC, LONGDESC, EXIT, FLAG, CAPACITY, VISIBILITY, LOCKED, OWNER, HOUSING_TAGS", field)),
                }
            }
            "DELETE" => {
                if args.is_empty() {
                    return Ok(
                        "Usage: @ROOM DELETE <id>\nExample: @ROOM DELETE dark_cave".to_string()
                    );
                }
                let room_id = args[0].to_lowercase();

                if !store.room_exists(&room_id)? {
                    return Ok(format!("Room '{}' not found.", room_id));
                }

                store.delete_room(&room_id)?;
                Ok(format!("Deleted room '{}'", room_id))
            }
            "LIST" => {
                let room_ids = store.list_room_ids()?;
                if room_ids.is_empty() {
                    return Ok("No rooms found.".to_string());
                }

                let mut output = format!("Rooms ({})\n", room_ids.len());
                output.push_str("─────────────────────────────────────────────────────\n");
                for room_id in room_ids {
                    match store.get_room(&room_id) {
                        Ok(room) => {
                            let flags_str = if room.flags.is_empty() {
                                String::new()
                            } else {
                                format!(
                                    " [{}]",
                                    room.flags
                                        .iter()
                                        .map(|f| format!("{:?}", f))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                )
                            };
                            let exits_count = room.exits.len();
                            output.push_str(&format!(
                                "  {}: {} ({} exits){}\n",
                                room.id, room.name, exits_count, flags_str
                            ));
                        }
                        Err(_) => {} // Skip if room can't be loaded
                    }
                }
                Ok(output)
            }
            "SEARCH" => {
                if args.is_empty() {
                    return Ok("Usage: @ROOM SEARCH <pattern>\nExample: @ROOM SEARCH square\nSearches room names and descriptions for the pattern.".to_string());
                }
                let search_pattern = args.join(" ").to_lowercase();

                let room_ids = store.list_room_ids()?;
                let mut matches = Vec::new();

                // Search through all rooms
                for room_id in room_ids {
                    if let Ok(room) = store.get_room(&room_id) {
                        let id_lower = room.id.to_lowercase();
                        let name_lower = room.name.to_lowercase();
                        let short_desc_lower = room.short_desc.to_lowercase();
                        let long_desc_lower = room.long_desc.to_lowercase();

                        // Calculate score based on where the pattern matches
                        let mut score = 0;
                        if id_lower.contains(&search_pattern) {
                            score += 100; // Exact ID match is highest priority
                        }
                        if name_lower.contains(&search_pattern) {
                            score += 50; // Name match is high priority
                        }
                        if short_desc_lower.contains(&search_pattern) {
                            score += 25; // Short description match
                        }
                        if long_desc_lower.contains(&search_pattern) {
                            score += 10; // Long description match
                        }

                        if score > 0 {
                            matches.push((room, score));
                        }
                    }
                }

                if matches.is_empty() {
                    return Ok(format!("No rooms found matching '{}'.", search_pattern));
                }

                // Sort by score (highest first)
                matches.sort_by(|a, b| b.1.cmp(&a.1));

                let mut output = format!(
                    "Rooms matching '{}' ({} found)\n",
                    search_pattern,
                    matches.len()
                );
                output.push_str("─────────────────────────────────────────────────────\n");

                for (room, score) in matches.iter().take(20) {
                    let flags_str = if room.flags.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " [{}]",
                            room.flags
                                .iter()
                                .map(|f| format!("{:?}", f))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    let exits_count = room.exits.len();
                    output.push_str(&format!(
                        "  {} ({}): {} ({} exits){}\n",
                        room.id, score, room.name, exits_count, flags_str
                    ));
                }

                if matches.len() > 20 {
                    output.push_str(&format!(
                        "\n... and {} more results. Be more specific to narrow search.\n",
                        matches.len() - 20
                    ));
                }

                Ok(output)
            }
            "SHOW" => {
                if args.is_empty() {
                    return Ok("Usage: @ROOM SHOW <id>\nExample: @ROOM SHOW dark_cave".to_string());
                }
                let room_id = args[0].to_lowercase();

                let room = match store.get_room(&room_id) {
                    Ok(r) => r,
                    Err(_) => return Ok(format!("Room '{}' not found.", room_id)),
                };

                let mut output = format!("Room Details: {}\n", room.id);
                output.push_str("═════════════════════════════════════════════════════\n");
                output.push_str(&format!("Name: {}\n", room.name));
                output.push_str(&format!("Short: {}\n", room.short_desc));
                output.push_str(&format!("Long: {}\n", room.long_desc));
                output.push_str(&format!("Owner: {:?}\n", room.owner));
                output.push_str(&format!("Capacity: {}\n", room.max_capacity));
                output.push_str(&format!("Visibility: {:?}\n", room.visibility));
                output.push_str(&format!(
                    "Locked: {}\n",
                    if room.locked { "Yes" } else { "No" }
                ));

                if !room.flags.is_empty() {
                    output.push_str("\nFlags:\n");
                    for flag in &room.flags {
                        output.push_str(&format!("  - {:?}\n", flag));
                    }
                }

                if !room.exits.is_empty() {
                    output.push_str("\nExits:\n");
                    for (direction, dest) in &room.exits {
                        output.push_str(&format!("  {:?} -> {}\n", direction, dest));
                    }
                }

                if !room.items.is_empty() {
                    output.push_str(&format!("\nItems ({}):\n", room.items.len()));
                    for item in &room.items {
                        output.push_str(&format!("  - {}\n", item));
                    }
                }

                Ok(output)
            }
            _ => Ok(format!(
                "Unknown subcommand '{}'. Valid: CREATE, EDIT, DELETE, LIST, SEARCH, SHOW",
                subcmd
            )),
        }
    }

    /// Handle @OBJECT command - manage world objects (admin level 2+)
    async fn handle_object_admin(
        &mut self,
        session: &Session,
        subcommand: String,
        args: Vec<String>,
        _config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;

        if player.admin_level.unwrap_or(0) < 2 {
            return Ok(
                "Insufficient permission: admin level 2+ required for @OBJECT commands."
                    .to_string(),
            );
        }

        let store = self.store();

        let subcmd = subcommand.to_uppercase();
        match subcmd.as_str() {
            "CREATE" => {
                if args.len() < 2 {
                    return Ok("Usage: @OBJECT CREATE <id> <name>\nExample: @OBJECT CREATE basic_torch \"Wooden Torch\"".to_string());
                }
                let object_id = args[0].to_lowercase();
                let name = args[1..].join(" ");

                // Check if object already exists
                if store.object_exists(&object_id)? {
                    return Ok(format!("Object '{}' already exists. Use @OBJECT EDIT to modify it.", object_id));
                }

                // Create object with default settings
                use crate::tmush::types::ObjectRecord;
                let object = ObjectRecord::new_world(&object_id, &name, "A new object.");
                store.put_object(object)?;

                Ok(format!("Created object '{}' ({}). Use @OBJECT EDIT to customize.", object_id, name))
            }
            "EDIT" => {
                if args.len() < 3 {
                    return Ok("Usage: @OBJECT EDIT <id> <field> <value>\nFields: NAME, DESCRIPTION, WEIGHT, VALUE, FLAG, TAKEABLE, USABLE, LOCKED, TRIGGER\nExample: @OBJECT EDIT basic_torch NAME \"Bright Torch\"\nExample: @OBJECT EDIT mushroom TRIGGER ONENTER message(\"Chimes!\")".to_string());
                }
                let object_id = args[0].to_lowercase();
                let field = args[1].to_uppercase();

                let mut object = match store.get_object(&object_id) {
                    Ok(o) => o,
                    Err(_) => return Ok(format!("Object '{}' not found. Use @OBJECT CREATE to create it.", object_id)),
                };

                // Check if this object is heavily used and warn admin
                let mut warnings = Vec::new();
                
                // Check if it's a prototype that has been cloned
                if object.clone_depth == 0 && object.clone_count > 0 {
                    warnings.push(format!("⚠️  This is a PROTOTYPE that has been cloned {} times.", object.clone_count));
                    warnings.push("   Clones are independent - editing this won't affect existing clones.".to_string());
                }

                // Count instances in rooms, players, and NPCs
                let mut instance_count = 0;
                let mut location_summary = Vec::new();

                // Check rooms
                let room_ids = store.list_room_ids()?;
                let mut room_count = 0;
                for room_id in room_ids {
                    if let Ok(room) = store.get_room(&room_id) {
                        if room.items.contains(&object_id) {
                            room_count += 1;
                            instance_count += 1;
                        }
                    }
                }
                if room_count > 0 {
                    location_summary.push(format!("{} rooms", room_count));
                }

                // Check player inventories
                let player_ids = store.list_player_ids()?;
                let mut player_count = 0;
                for player_id in player_ids {
                    if let Ok(player) = store.get_player(&player_id) {
                        if Self::player_has_item(&player, &object_id) {
                            player_count += 1;
                            instance_count += 1;
                        }
                    }
                }
                if player_count > 0 {
                    location_summary.push(format!("{} players", player_count));
                }

                // Warn if object is in multiple locations
                if instance_count > 0 {
                    warnings.push(format!("⚠️  This object appears in {} locations: {}", 
                        instance_count, 
                        location_summary.join(", ")
                    ));
                    warnings.push(format!("   Changes will affect ALL instances of '{}' in these locations.", object_id));
                    warnings.push("   Use @OBJECT INSTANCES to see exact locations.".to_string());
                }

                // Display warnings if any
                let warning_message = if !warnings.is_empty() {
                    format!("\n{}\n\n", warnings.join("\n"))
                } else {
                    String::new()
                };

                match field.as_str() {
                    "NAME" => {
                        let name = args[2..].join(" ");
                        object.name = name.clone();
                        store.put_object(object)?;
                        Ok(format!("{}Set object name to '{}'.", warning_message, name))
                    }
                    "DESCRIPTION" | "DESC" => {
                        let description = args[2..].join(" ");
                        object.description = description.clone();
                        store.put_object(object)?;
                        Ok(format!("{}Set object description to '{}'.", warning_message, description))
                    }
                    "WEIGHT" => {
                        if args.len() < 3 {
                            return Ok("Usage: @OBJECT EDIT <id> WEIGHT <number>\nExample: @OBJECT EDIT torch WEIGHT 5".to_string());
                        }
                        match args[2].parse::<u8>() {
                            Ok(weight) => {
                                object.weight = weight;
                                store.put_object(object)?;
                                Ok(format!("{}Set object weight to {}.", warning_message, weight))
                            }
                            Err(_) => Ok("Weight must be a number between 0 and 255.".to_string()),
                        }
                    }
                    "VALUE" => {
                        if args.len() < 3 {
                            return Ok("Usage: @OBJECT EDIT <id> VALUE <number>\nExample: @OBJECT EDIT sword VALUE 100\nSets the currency value in base units.".to_string());
                        }
                        let value_str = &args[2];
                        match value_str.parse::<i64>() {
                            Ok(value) => {
                                use crate::tmush::types::CurrencyAmount;
                                object.currency_value = CurrencyAmount::multi_tier(value);
                                store.put_object(object)?;
                                Ok(format!("{}Set object value to {}.", warning_message, value))
                            }
                            Err(_) => Ok("Value must be a number (base currency units).".to_string()),
                        }
                    }
                    "FLAG" => {
                        if args.len() < 3 {
                            return Ok("Usage: @OBJECT EDIT <id> FLAG <flag>\nFlags: QUESTITEM, CONSUMABLE, EQUIPMENT, KEYITEM, CONTAINER, MAGICAL, COMPANION, CLONABLE, UNIQUE, NOVALUE, NOCLONECHILDREN, LIGHTSOURCE\nExample: @OBJECT EDIT torch FLAG LIGHTSOURCE".to_string());
                        }
                        use crate::tmush::types::ObjectFlag;
                        let flag_str = args[2].to_uppercase();
                        let flag = match flag_str.as_str() {
                            "QUESTITEM" => ObjectFlag::QuestItem,
                            "CONSUMABLE" => ObjectFlag::Consumable,
                            "EQUIPMENT" => ObjectFlag::Equipment,
                            "KEYITEM" => ObjectFlag::KeyItem,
                            "CONTAINER" => ObjectFlag::Container,
                            "MAGICAL" => ObjectFlag::Magical,
                            "COMPANION" => ObjectFlag::Companion,
                            "CLONABLE" => ObjectFlag::Clonable,
                            "UNIQUE" => ObjectFlag::Unique,
                            "NOVALUE" => ObjectFlag::NoValue,
                            "NOCLONECHILDREN" => ObjectFlag::NoCloneChildren,
                            "LIGHTSOURCE" => ObjectFlag::LightSource,
                            _ => return Ok(format!("Unknown flag '{}'. Valid flags: QUESTITEM, CONSUMABLE, EQUIPMENT, KEYITEM, CONTAINER, MAGICAL, COMPANION, CLONABLE, UNIQUE, NOVALUE, NOCLONECHILDREN, LIGHTSOURCE", flag_str)),
                        };
                        
                        if !object.flags.contains(&flag) {
                            object.flags.push(flag);
                            store.put_object(object)?;
                            Ok(format!("{}Added flag {} to object.", warning_message, flag_str))
                        } else {
                            Ok(format!("Object already has flag {}.", flag_str))
                        }
                    }
                    "TAKEABLE" => {
                        if args.len() < 3 {
                            return Ok("Usage: @OBJECT EDIT <id> TAKEABLE <true|false>\nExample: @OBJECT EDIT torch TAKEABLE true".to_string());
                        }
                        let value = args[2].to_lowercase();
                        match value.as_str() {
                            "true" | "yes" | "1" => {
                                object.takeable = true;
                                store.put_object(object)?;
                                Ok(format!("{}Set object as takeable.", warning_message))
                            }
                            "false" | "no" | "0" => {
                                object.takeable = false;
                                store.put_object(object)?;
                                Ok(format!("{}Set object as not takeable.", warning_message))
                            }
                            _ => Ok("Value must be 'true' or 'false'.".to_string()),
                        }
                    }
                    "USABLE" => {
                        if args.len() < 3 {
                            return Ok("Usage: @OBJECT EDIT <id> USABLE <true|false>\nExample: @OBJECT EDIT potion USABLE true".to_string());
                        }
                        let value = args[2].to_lowercase();
                        match value.as_str() {
                            "true" | "yes" | "1" => {
                                object.usable = true;
                                store.put_object(object)?;
                                Ok(format!("{}Set object as usable.", warning_message))
                            }
                            "false" | "no" | "0" => {
                                object.usable = false;
                                store.put_object(object)?;
                                Ok(format!("{}Set object as not usable.", warning_message))
                            }
                            _ => Ok("Value must be 'true' or 'false'.".to_string()),
                        }
                    }
                    "LOCKED" => {
                        if args.len() < 3 {
                            return Ok("Usage: @OBJECT EDIT <id> LOCKED <true|false>\nExample: @OBJECT EDIT statue LOCKED true".to_string());
                        }
                        let value = args[2].to_lowercase();
                        match value.as_str() {
                            "true" | "yes" | "1" => {
                                object.locked = true;
                                store.put_object(object)?;
                                Ok(format!("{}Set object as locked (cannot be taken).", warning_message))
                            }
                            "false" | "no" | "0" => {
                                object.locked = false;
                                store.put_object(object)?;
                                Ok(format!("{}Set object as unlocked.", warning_message))
                            }
                            _ => Ok("Value must be 'true' or 'false'.".to_string()),
                        }
                    }
                    "TRIGGER" => {
                        if args.len() < 3 {
                            return Ok("Usage: @OBJECT EDIT <id> TRIGGER <type> <script>\n       @OBJECT EDIT <id> TRIGGER <type> REMOVE\n\nTrigger Types: ONENTER, ONLOOK, ONTAKE, ONDROP, ONUSE, ONPOKE, ONFOLLOW, ONIDLE, ONCOMBAT, ONHEAL\n\nExamples:\n  @OBJECT EDIT mushroom TRIGGER ONENTER message(\"🍄 The mushroom chimes!\")\n  @OBJECT EDIT potion TRIGGER ONUSE heal(50) && consume()\n  @OBJECT EDIT box TRIGGER ONPOKE random_chance(50) && message(\"Click!\")\n  @OBJECT EDIT mushroom TRIGGER ONENTER REMOVE".to_string());
                        }
                        
                        let trigger_type_str = args[2].to_uppercase();
                        
                        // Parse trigger type
                        let trigger_type = match trigger_type_str.as_str() {
                            "ONENTER" => ObjectTrigger::OnEnter,
                            "ONLOOK" => ObjectTrigger::OnLook,
                            "ONTAKE" => ObjectTrigger::OnTake,
                            "ONDROP" => ObjectTrigger::OnDrop,
                            "ONUSE" => ObjectTrigger::OnUse,
                            "ONPOKE" => ObjectTrigger::OnPoke,
                            "ONFOLLOW" => ObjectTrigger::OnFollow,
                            "ONIDLE" => ObjectTrigger::OnIdle,
                            "ONCOMBAT" => ObjectTrigger::OnCombat,
                            "ONHEAL" => ObjectTrigger::OnHeal,
                            _ => return Ok(format!("Unknown trigger type '{}'. Valid types: ONENTER, ONLOOK, ONTAKE, ONDROP, ONUSE, ONPOKE, ONFOLLOW, ONIDLE, ONCOMBAT, ONHEAL", trigger_type_str)),
                        };
                        
                        // Check if this is a REMOVE command
                        if args.len() > 3 && args[3].to_uppercase() == "REMOVE" {
                            // Remove the trigger
                            if object.actions.remove(&trigger_type).is_some() {
                                store.put_object(object)?;
                                Ok(format!("{}Removed {} trigger from object.", warning_message, trigger_type_str))
                            } else {
                                Ok(format!("Object does not have a {} trigger.", trigger_type_str))
                            }
                        } else {
                            // Set the trigger - rest of args is the script
                            if args.len() < 4 {
                                return Ok("Usage: @OBJECT EDIT <id> TRIGGER <type> <script>\nExample: @OBJECT EDIT mushroom TRIGGER ONENTER message(\"Hello!\")".to_string());
                            }
                            
                            let script = args[3..].join(" ");
                            
                            // Normalize smart/curly quotes to straight quotes
                            // macOS and some terminals auto-convert quotes which breaks DSL parsing
                            let script = script
                                .replace('\u{201C}', "\"")  // " LEFT DOUBLE QUOTATION MARK
                                .replace('\u{201D}', "\"")  // " RIGHT DOUBLE QUOTATION MARK
                                .replace('\u{2018}', "'")   // ' LEFT SINGLE QUOTATION MARK
                                .replace('\u{2019}', "'");  // ' RIGHT SINGLE QUOTATION MARK
                            
                            // Validate script is not empty
                            if script.trim().is_empty() {
                                return Ok("Trigger script cannot be empty. Use 'REMOVE' to delete a trigger.".to_string());
                            }
                            
                            // Set the trigger
                            let was_update = object.actions.contains_key(&trigger_type);
                            object.actions.insert(trigger_type, script.clone());
                            store.put_object(object)?;
                            
                            if was_update {
                                Ok(format!("{}Updated {} trigger:\n  {}", warning_message, trigger_type_str, script))
                            } else {
                                Ok(format!("{}Added {} trigger:\n  {}", warning_message, trigger_type_str, script))
                            }
                        }
                    }
                    "OWNER" => {
                        if args.len() < 3 {
                            return Ok("Usage: @OBJECT EDIT <id> OWNER <player|world>\nExample: @OBJECT EDIT magic_sword OWNER alice\nExample: @OBJECT EDIT torch OWNER world".to_string());
                        }
                        let owner_str = args[2].to_lowercase();
                        
                        use crate::tmush::types::ObjectOwner;
                        if owner_str == "world" {
                            object.owner = ObjectOwner::World;
                            store.put_object(object)?;
                            Ok(format!("{}Transferred object '{}' to World ownership", warning_message, object_id))
                        } else {
                            // Verify player exists
                            if store.get_player(&owner_str).is_err() {
                                return Ok(format!("Player '{}' not found", owner_str));
                            }
                            object.owner = ObjectOwner::Player { username: owner_str.clone() };
                            store.put_object(object)?;
                            Ok(format!("{}Transferred object '{}' to player '{}'", warning_message, object_id, owner_str))
                        }
                    }
                    _ => Ok(format!("Unknown field '{}'. Valid fields: NAME, DESCRIPTION, WEIGHT, VALUE, FLAG, TAKEABLE, USABLE, LOCKED, TRIGGER, OWNER", field)),
                }
            }
            "DELETE" => {
                if args.is_empty() {
                    return Ok("Usage: @OBJECT DELETE <id>\nExample: @OBJECT DELETE old_torch".to_string());
                }
                let object_id = args[0].to_lowercase();

                // Check if object exists
                if !store.object_exists(&object_id)? {
                    return Ok(format!("Object '{}' not found.", object_id));
                }

                // Use the storage method for deleting world objects
                store.delete_object_world(&object_id)?;

                Ok(format!("Deleted object '{}'.", object_id))
            }
            "LIST" => {
                let object_ids = store.list_object_ids()?;
                
                if object_ids.is_empty() {
                    return Ok("No world objects found. Use @OBJECT CREATE to create objects.".to_string());
                }

                // Support optional search pattern: @OBJECT LIST [pattern]
                let search_pattern = if !args.is_empty() {
                    Some(args.join(" ").to_lowercase())
                } else {
                    None
                };

                let mut filtered_objects = Vec::new();
                for id in object_ids {
                    if let Ok(object) = store.get_object(&id) {
                        // Filter by search pattern if provided
                        if let Some(ref pattern) = search_pattern {
                            let matches = object.id.to_lowercase().contains(pattern)
                                || object.name.to_lowercase().contains(pattern)
                                || object.description.to_lowercase().contains(pattern);
                            if !matches {
                                continue;
                            }
                        }
                        filtered_objects.push(object);
                    }
                }

                if filtered_objects.is_empty() {
                    if search_pattern.is_some() {
                        return Ok(format!("No objects found matching '{}'. Try @OBJECT SEARCH for fuzzy matching.", search_pattern.unwrap()));
                    } else {
                        return Ok("No world objects found. Use @OBJECT CREATE to create objects.".to_string());
                    }
                }

                let mut output = if let Some(ref pattern) = search_pattern {
                    format!("World Objects matching '{}' ({} of {} total):\n", pattern, filtered_objects.len(), filtered_objects.len())
                } else {
                    format!("World Objects ({} total):\n", filtered_objects.len())
                };

                for object in filtered_objects {
                    let flags_str = if object.flags.is_empty() {
                        "none".to_string()
                    } else {
                        object.flags.iter()
                            .map(|f| format!("{:?}", f))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    let takeable = if object.takeable { "takeable" } else { "fixed" };
                    let usable = if object.usable { "usable" } else { "not-usable" };
                    output.push_str(&format!(
                        "  {} - {} (weight: {}, value: {:?}, {}, {}, flags: {})\n",
                        object.id, object.name, object.weight, object.currency_value, takeable, usable, flags_str
                    ));
                }
                Ok(output)
            }
            "SEARCH" => {
                if args.is_empty() {
                    return Ok("Usage: @OBJECT SEARCH <pattern>\nExample: @OBJECT SEARCH torch\nSearches object ID, name, and description.".to_string());
                }

                let pattern = args.join(" ").to_lowercase();
                let object_ids = store.list_object_ids()?;
                
                if object_ids.is_empty() {
                    return Ok("No world objects found. Use @OBJECT CREATE to create objects.".to_string());
                }

                // Fuzzy search with scoring
                let mut matches: Vec<(i32, crate::tmush::types::ObjectRecord)> = Vec::new();
                
                for id in object_ids {
                    if let Ok(object) = store.get_object(&id) {
                        let mut score = 0;
                        
                        // Exact ID match (highest priority)
                        if object.id.to_lowercase() == pattern {
                            score += 100;
                        } else if object.id.to_lowercase().contains(&pattern) {
                            score += 50;
                        }
                        
                        // Name matching (high priority)
                        if object.name.to_lowercase() == pattern {
                            score += 80;
                        } else if object.name.to_lowercase().contains(&pattern) {
                            score += 40;
                        }
                        
                        // Description matching (lower priority)
                        if object.description.to_lowercase().contains(&pattern) {
                            score += 20;
                        }
                        
                        // Add bonus for word boundary matches
                        let words: Vec<&str> = pattern.split_whitespace().collect();
                        for word in words {
                            if object.name.to_lowercase().split_whitespace().any(|w| w == word) {
                                score += 10;
                            }
                        }
                        
                        if score > 0 {
                            matches.push((score, object));
                        }
                    }
                }

                if matches.is_empty() {
                    return Ok(format!("No objects found matching '{}'. Try a different search term.", pattern));
                }

                // Sort by score (descending)
                matches.sort_by(|a, b| b.0.cmp(&a.0));

                let mut output = format!("Search results for '{}' ({} matches):\n\n", pattern, matches.len());
                
                for (score, object) in matches.iter().take(20) {  // Limit to top 20 results
                    let flags_str = if object.flags.is_empty() {
                        "none".to_string()
                    } else {
                        object.flags.iter()
                            .map(|f| format!("{:?}", f))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    
                    output.push_str(&format!(
                        "  [Score: {}] {} - {}\n",
                        score, object.id, object.name
                    ));
                    output.push_str(&format!(
                        "    Description: {}\n",
                        if object.description.len() > 60 {
                            format!("{}...", &object.description[..60])
                        } else {
                            object.description.clone()
                        }
                    ));
                    output.push_str(&format!(
                        "    Value: {:?}, Flags: {}\n\n",
                        object.currency_value, flags_str
                    ));
                }

                if matches.len() > 20 {
                    output.push_str(&format!("Showing top 20 of {} matches. Refine your search for better results.\n", matches.len()));
                }

                Ok(output)
            }
            "INSTANCES" => {
                if args.is_empty() {
                    return Ok("Usage: @OBJECT INSTANCES <id>\nExample: @OBJECT INSTANCES basic_torch\nShows where an object appears in the world.".to_string());
                }
                let object_id = args[0].to_lowercase();

                // Verify object exists
                let object = match store.get_object(&object_id) {
                    Ok(o) => o,
                    Err(_) => return Ok(format!("Object '{}' not found.", object_id)),
                };

                let mut output = format!("Instances of '{}' ({}):\n\n", object.id, object.name);
                let mut total_count = 0;

                // Check all rooms for this object
                let room_ids = store.list_room_ids()?;
                let mut rooms_with_object = Vec::new();
                
                for room_id in room_ids {
                    if let Ok(room) = store.get_room(&room_id) {
                        if room.items.contains(&object_id) {
                            rooms_with_object.push(room);
                        }
                    }
                }

                if !rooms_with_object.is_empty() {
                    output.push_str(&format!("📍 Rooms ({}):\n", rooms_with_object.len()));
                    for room in rooms_with_object {
                        total_count += 1;
                        output.push_str(&format!("  • {} - {}\n", room.id, room.name));
                    }
                    output.push_str("\n");
                }

                // Check all player inventories
                let player_ids = store.list_player_ids()?;
                let mut players_with_object = Vec::new();
                
                for player_id in player_ids {
                    if let Ok(player) = store.get_player(&player_id) {
                        if Self::player_has_item(&player, &object_id) {
                            players_with_object.push(player);
                        }
                    }
                }

                if !players_with_object.is_empty() {
                    output.push_str(&format!("🎒 Player Inventories ({}):\n", players_with_object.len()));
                    for player in players_with_object {
                        total_count += 1;
                        output.push_str(&format!("  • {} ({})\n", player.username, player.current_room));
                    }
                    output.push_str("\n");
                }

                // Show clone information if this is a prototype
                if object.clone_depth == 0 && object.clone_count > 0 {
                    output.push_str(&format!("🔄 Clone Information:\n"));
                    output.push_str(&format!("  This is a prototype that has been cloned {} times.\n", object.clone_count));
                    output.push_str(&format!("  Note: Clones are independent objects with unique IDs.\n\n"));
                }

                // Summary
                if total_count == 0 {
                    output.push_str("⚠️  This object doesn't appear in any rooms, player inventories, or NPC inventories.\n");
                    output.push_str("   It may have been created but not yet placed in the world.\n");
                } else {
                    output.push_str(&format!("📊 Total instances found: {}\n", total_count));
                }

                Ok(output)
            }
            "SHOW" => {
                if args.is_empty() {
                    return Ok("Usage: @OBJECT SHOW <id or name>\nExample: @OBJECT SHOW basic_torch\nExample: @OBJECT SHOW wooden torch".to_string());
                }
                let search_term = args.join(" ").to_lowercase();

                // Try exact ID match first
                let object = if let Ok(obj) = store.get_object(&search_term) {
                    obj
                } else {
                    // Try fuzzy matching by name across all objects
                    let all_objects = store.list_object_ids()?;
                    let matches = self.find_objects_by_partial_name(&search_term, &all_objects);
                    
                    if matches.is_empty() {
                        return Ok(format!("Object '{}' not found. Use exact ID or object name.", search_term));
                    } else if matches.len() > 1 {
                        let names: Vec<String> = matches.iter()
                            .map(|o| format!("{} ({})", o.name, o.id))
                            .collect();
                        return Ok(format!("Multiple objects match '{}'. Please be more specific:\n{}", 
                            search_term, 
                            names.join("\n")));
                    }
                    
                    matches.into_iter().next().unwrap()
                };

                let flags_str = if object.flags.is_empty() {
                    "none".to_string()
                } else {
                    object.flags.iter()
                        .map(|f| format!("{:?}", f))
                        .collect::<Vec<_>>()
                        .join(", ")
                };

                let owner_str = match &object.owner {
                    crate::tmush::types::ObjectOwner::World => "World".to_string(),
                    crate::tmush::types::ObjectOwner::Player { username } => format!("Player: {}", username),
                };

                let mut output = format!("Object: {} ({})\n", object.id, object.name);
                output.push_str(&format!("Description: {}\n", object.description));
                output.push_str(&format!("Owner: {}\n", owner_str));
                output.push_str(&format!("Weight: {}\n", object.weight));
                output.push_str(&format!("Value: {:?}\n", object.currency_value));
                output.push_str(&format!("Takeable: {}\n", if object.takeable { "yes" } else { "no" }));
                output.push_str(&format!("Usable: {}\n", if object.usable { "yes" } else { "no" }));
                output.push_str(&format!("Locked: {}\n", if object.locked { "yes" } else { "no" }));
                output.push_str(&format!("Flags: {}\n", flags_str));
                
                // Display triggers
                if object.actions.is_empty() {
                    output.push_str("Triggers: none\n");
                } else {
                    output.push_str(&format!("Triggers: ({})\n", object.actions.len()));
                    // Sort triggers for consistent display
                    let mut trigger_list: Vec<_> = object.actions.iter().collect();
                    trigger_list.sort_by_key(|(trigger, _)| format!("{:?}", trigger));
                    for (trigger, script) in trigger_list {
                        output.push_str(&format!("  {:?}: {}\n", trigger, script));
                    }
                }
                
                output.push_str(&format!("Created: {}\n", object.created_at.format("%Y-%m-%d %H:%M:%S UTC")));
                output.push_str(&format!("Created by: {}\n", object.created_by));
                
                if let Some(ref source) = object.clone_source_id {
                    output.push_str(&format!("Clone source: {} (depth: {})\n", source, object.clone_depth));
                }
                output.push_str(&format!("Times cloned: {}\n", object.clone_count));

                Ok(output)
            }
            _ => Ok(format!("Unknown subcommand '{}'. Valid: CREATE, EDIT, DELETE, LIST, SEARCH, INSTANCES, SHOW", subcmd)),
        }
    }

    /// Handle @LISTABANDONED command - view abandoned/at-risk housing (admin)
    async fn handle_list_abandoned(
        &mut self,
        _session: &Session,
        storage: &Storage,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::housing_cleanup::list_abandoned_housing;

        // Get abandoned housing list
        let abandoned = list_abandoned_housing(&self.store, storage)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list abandoned housing: {}", e))?;

        if abandoned.is_empty() {
            return Ok("� ABANDONED HOUSING REPORT\n\n\
                      No housing instances are currently at risk or abandoned.\n\
                      All players are actively maintaining their housing!"
                .to_string());
        }

        let mut output = format!(
            "🏚️  ABANDONED HOUSING REPORT\n\n\
                                  Found {} housing instances at risk:\n\n",
            abandoned.len()
        );

        for (idx, info) in abandoned.iter().enumerate() {
            output.push_str(&format!(
                "{}. {} (Owner: {})\n\
                 Template: {}\n\
                 {} (Inactive for {} days)\n\
                 Reclaim Box: {} items\n\n",
                idx + 1,
                info.instance_id,
                info.owner_username,
                info.template_id,
                info.status_message(),
                info.days_inactive,
                info.reclaim_box_items
            ));
        }

        output.push_str(
            "\nTimeline:\n\
                        30 days: Items moved to reclaim box\n\
                        60 days: Housing marked for reclamation\n\
                        80 days: Final warning issued\n\
                        90 days: Reclaim box permanently deleted\n",
        );

        Ok(output)
    }

    /// Handle `@ADMIN` command - show admin status and available commands
    ///
    /// Displays the player's admin status, level, and available admin commands based on their
    /// permission level. This command is available to all players but shows different information
    /// based on admin status.
    ///
    /// # Admin Levels
    /// - Level 0: Not an admin (default)
    /// - Level 1: Moderator (can view admin list)
    /// - Level 2: Admin (can grant/revoke moderator and admin)
    /// - Level 3: Sysop (full admin commands, can grant any level)
    ///
    /// # Example Output (Admin)
    /// ```text
    /// 🛡️  ADMIN STATUS
    ///
    /// Player: Alice
    /// Username: alice
    ///
    /// ✅ Admin Status: ACTIVE
    /// 🔐 Admin Level: 2 (Admin)
    ///
    /// Available Admin Commands:
    ///   @ADMINS - List all administrators
    ///   @SETADMIN <player> <level> - Grant admin privileges
    ///   @REMOVEADMIN <player> - Revoke admin privileges
    ///
    /// Total Administrators: 3
    /// ```
    ///
    /// # Example Output (Non-Admin)
    /// ```text
    /// 🛡️  ADMIN STATUS
    ///
    /// Player: Bob
    /// Username: bob
    ///
    /// ❌ Admin Status: NOT ADMIN
    ///
    /// You do not have administrative privileges.
    /// Contact a system administrator if you need admin access.
    /// ```
    async fn handle_admin(&mut self, session: &Session, config: &Config) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        let store = self.store();

        let mut response = String::from("🛡️  ADMIN STATUS\n\n");
        response.push_str(&format!("Player: {}\n", player.display_name));
        response.push_str(&format!("Username: {}\n\n", player.username));

        if player.is_admin() {
            response.push_str(&format!("✅ Admin Status: ACTIVE\n"));
            response.push_str(&format!("🔐 Admin Level: {} ", player.admin_level()));

            let level_name = match player.admin_level() {
                1 => "(Moderator)",
                2 => "(Admin)",
                3 => "(Sysop)",
                _ => "(Unknown)",
            };
            response.push_str(level_name);
            response.push_str("\n\n");

            response.push_str("Available Admin Commands:\n");
            response.push_str("  @ADMINS - List all administrators\n");
            if player.admin_level() >= 1 {
                response.push_str("  @PLAYERS - List all players with location\n");
                response.push_str("  @GOTO <player|room> - Teleport to player or room\n");
            }
            if player.admin_level() >= 2 {
                response.push_str("  @SETADMIN <player> <level> - Grant admin privileges\n");
                response.push_str("  @REMOVEADMIN <player> - Revoke admin privileges\n");
                response.push_str("  @GIVE <player> <object_id> [qty] - Grant items to player\n");
                response.push_str("  @GIVE <player> CURRENCY <amt> - Grant currency to player\n");
                response.push_str("  @TELEPORT <player> <room> - Teleport player to room\n");
                response.push_str("  @STATS <player> EDIT <stat> <value> - Edit player stats\n");
            }
            if player.admin_level() >= 3 {
                response.push_str("  @EDITROOM <room> <desc> - Edit room descriptions\n");
                response.push_str("  @EDITNPC <npc> <field> <value> - Edit NPC properties\n");
                response.push_str("  @DIALOG <npc> <cmd> [args] - Manage NPC dialogues\n");
            }

            // Show total admin count
            match store.list_admins() {
                Ok(admins) => {
                    response.push_str(&format!("\nTotal Administrators: {}\n", admins.len()));
                }
                Err(e) => {
                    response.push_str(&format!("\n⚠️  Error listing admins: {}\n", e));
                }
            }
        } else {
            response.push_str("❌ Admin Status: NOT ADMIN\n\n");
            response.push_str("You do not have administrative privileges.\n");
            response.push_str("Contact a system administrator if you need admin access.\n");
        }

        Ok(response)
    }

    /// Handle `@SETADMIN` command - grant admin privileges to a player
    ///
    /// Grants administrative privileges to the specified player with the given level.
    /// This command requires the caller to be an administrator with level 2 or higher.
    ///
    /// # Permission Requirements
    /// - Caller must be admin (level 2+)
    /// - Cannot grant a level higher than caller's own level
    /// - Target player must exist in the database
    ///
    /// # Arguments
    /// - `target_username`: The lowercase username of the player to grant admin to
    /// - `level`: Admin level to grant (0-3)
    ///   - 0: Revoke admin (use @REMOVEADMIN instead)
    ///   - 1: Moderator
    ///   - 2: Admin
    ///   - 3: Sysop
    ///
    /// # Example Usage
    /// ```text
    /// @SETADMIN alice 2
    /// ```
    ///
    /// # Example Output (Success)
    /// ```text
    /// ✅ SUCCESS
    ///
    /// Granted Admin admin privileges to 'alice'.
    ///
    /// Admin Level: 2 (Admin)
    ///
    /// The change is effective immediately.
    /// ```
    ///
    /// # Example Output (Insufficient Level)
    /// ```text
    /// ⛔ Permission denied: Cannot grant level 3 admin.
    ///
    /// Your admin level is 2. You can only grant levels up to your own level.
    /// ```
    async fn handle_set_admin(
        &mut self,
        session: &Session,
        target_username: String,
        level: u8,
        config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        let store = self.store();

        // Check if caller is admin
        if let Err(e) = store.require_admin(&player.username) {
            return Ok(format!(
                "⛔ Permission denied: {}\n\nYou must be an administrator to use this command.",
                e
            ));
        }

        // Check if caller has sufficient level
        if player.admin_level() < 2 {
            return Ok("⛔ Permission denied: Insufficient admin level.\n\nOnly level 2+ administrators can grant admin privileges.".to_string());
        }

        // Cannot grant higher level than you have
        if level > player.admin_level() {
            return Ok(format!(
                "⛔ Permission denied: Cannot grant level {} admin.\n\nYour admin level is {}. You can only grant levels up to your own level.",
                level, player.admin_level()
            ));
        }

        // Grant admin
        match store.grant_admin(&player.username, &target_username, level) {
            Ok(()) => {
                let level_name = match level {
                    0 => "None (revoked)",
                    1 => "Moderator",
                    2 => "Admin",
                    3 => "Sysop",
                    _ => "Unknown",
                };

                Ok(format!(
                    "✅ SUCCESS\n\n\
                    Granted {} admin privileges to '{}'.\n\n\
                    Admin Level: {} ({})\n\n\
                    The change is effective immediately.",
                    level_name, target_username, level, level_name
                ))
            }
            Err(e) => Ok(format!("❌ Failed to grant admin: {}", e)),
        }
    }

    /// Handle `@REMOVEADMIN` / `@REVOKEADMIN` command - revoke admin privileges from a player
    ///
    /// Revokes administrative privileges from the specified player, demoting them to a regular
    /// user. This command requires the caller to be an administrator with level 2 or higher.
    ///
    /// # Permission Requirements
    /// - Caller must be admin (level 2+)
    /// - Cannot revoke your own admin privileges (self-protection)
    /// - Target player must exist in the database
    ///
    /// # Arguments
    /// - `target_username`: The lowercase username of the player to revoke admin from
    ///
    /// # Example Usage
    /// ```text
    /// @REMOVEADMIN alice
    /// @REVOKEADMIN bob
    /// ```
    ///
    /// # Example Output (Success)
    /// ```text
    /// ✅ SUCCESS
    ///
    /// Revoked admin privileges from 'alice'.
    ///
    /// They are now a regular user.
    ///
    /// The change is effective immediately.
    /// ```
    ///
    /// # Example Output (Self-Revocation Attempt)
    /// ```text
    /// ⛔ Cannot revoke your own admin privileges.
    ///
    /// Have another administrator revoke your access if needed.
    /// ```
    async fn handle_remove_admin(
        &mut self,
        session: &Session,
        target_username: String,
        config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        let store = self.store();

        // Check if caller is admin
        if let Err(e) = store.require_admin(&player.username) {
            return Ok(format!(
                "⛔ Permission denied: {}\n\nYou must be an administrator to use this command.",
                e
            ));
        }

        // Check if caller has sufficient level
        if player.admin_level() < 2 {
            return Ok("⛔ Permission denied: Insufficient admin level.\n\nOnly level 2+ administrators can revoke admin privileges.".to_string());
        }

        // Revoke admin
        match store.revoke_admin(&player.username, &target_username) {
            Ok(()) => Ok(format!(
                "✅ SUCCESS\n\n\
                    Revoked admin privileges from '{}'.\n\n\
                    They are now a regular user.\n\n\
                    The change is effective immediately.",
                target_username
            )),
            Err(e) => {
                if e.to_string().contains("Cannot revoke your own") {
                    Ok("⛔ Cannot revoke your own admin privileges.\n\nHave another administrator revoke your access if needed.".to_string())
                } else {
                    Ok(format!("❌ Failed to revoke admin: {}", e))
                }
            }
        }
    }

    /// Handle `@ADMINS` / `@ADMINLIST` command - list all administrators
    ///
    /// Lists all players with administrative privileges, sorted by admin level (descending)
    /// and then by username. This is a public command - any player can view the admin list.
    ///
    /// # Permission Requirements
    /// None - this command is available to all players.
    ///
    /// # Output Format
    /// - Sorted by level (highest first), then by username (alphabetical)
    /// - Shows admin level, role name, and display name
    /// - Marks the current player with "(you)" indicator
    /// - Includes legend explaining admin levels
    ///
    /// # Example Usage
    /// ```text
    /// @ADMINS
    /// @ADMINLIST
    /// ```
    ///
    /// # Example Output
    /// ```text
    /// 🛡️  SYSTEM ADMINISTRATORS
    ///
    /// Total: 3
    ///
    ///   [3] Sysop     - Admin (Admin) (you)
    ///   [2] Admin     - Alice
    ///   [1] Moderator - Bob
    ///
    /// Levels: 1=Moderator, 2=Admin, 3=Sysop
    /// ```
    ///
    /// # Notes
    /// - Useful for players to know who can help with admin requests
    /// - Transparent governance - everyone can see the admin team
    /// - Sorted display helps identify senior administrators quickly
    async fn handle_admins(&mut self, session: &Session, config: &Config) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        let store = self.store();

        // Anyone can view admin list
        match store.list_admins() {
            Ok(admins) => {
                if admins.is_empty() {
                    return Ok(
                        "⚠️  No administrators found.\n\nThis is unusual - contact support."
                            .to_string(),
                    );
                }

                let mut response = String::from("🛡️  SYSTEM ADMINISTRATORS\n\n");
                response.push_str(&format!("Total: {}\n\n", admins.len()));

                // Sort by level (descending) then username
                let mut sorted_admins = admins;
                sorted_admins.sort_by(|a, b| {
                    b.admin_level()
                        .cmp(&a.admin_level())
                        .then_with(|| a.username.cmp(&b.username))
                });

                for admin in sorted_admins {
                    let level_name = match admin.admin_level() {
                        1 => "Moderator",
                        2 => "Admin    ",
                        3 => "Sysop    ",
                        _ => "Unknown  ",
                    };

                    let indicator = if admin.username == player.username {
                        " (you)"
                    } else {
                        ""
                    };

                    response.push_str(&format!(
                        "  [{}] {} - {}{}\n",
                        admin.admin_level(),
                        level_name,
                        admin.display_name,
                        indicator
                    ));
                }

                response.push_str("\nLevels: 1=Moderator, 2=Admin, 3=Sysop\n");

                Ok(response)
            }
            Err(e) => Ok(format!("❌ Error listing administrators: {}", e)),
        }
    }

    /// Handle `@GIVE` command - grant items or currency to a player
    ///
    /// Grants items or currency to a player's inventory. Admin level 2+ required.
    ///
    /// # Usage
    /// - `@GIVE <player> <object_id> [quantity]`: Grant items to player
    /// - `@GIVE <player> CURRENCY <amount>`: Grant currency to player
    ///
    /// # Permission Requirements
    /// - Caller must be admin level 2+ (Admin or higher)
    ///
    /// # Examples
    /// ```text
    /// @GIVE alice basic_torch
    /// @GIVE alice basic_torch 5
    /// @GIVE alice CURRENCY 1000
    /// ```
    async fn handle_give(
        &mut self,
        session: &Session,
        target_username: String,
        args: Vec<String>,
        config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Check admin permissions (level 2+ required)
        if player.admin_level() < 2 {
            return Ok(
                "❌ Permission denied. Requires admin level 2+ (Admin or higher).".to_string(),
            );
        }

        let store = self.store();

        // Load target player
        let mut target = match store.get_player(&target_username) {
            Ok(p) => p,
            Err(_) => {
                return Ok(format!("❌ Player '{}' not found.", target_username));
            }
        };

        if args.is_empty() {
            return Ok("❌ Usage: @GIVE <player> <object_id> [quantity]\n       @GIVE <player> CURRENCY <amount>".to_string());
        }

        // Check if giving currency
        if args[0].eq_ignore_ascii_case("CURRENCY") {
            if args.len() < 2 {
                return Ok("❌ Usage: @GIVE <player> CURRENCY <amount>".to_string());
            }

            let amount: i64 = match args[1].parse() {
                Ok(amt) => amt,
                Err(_) => {
                    return Ok(format!("❌ Invalid amount: {}", args[1]));
                }
            };

            // Add currency matching the player's current currency type
            let to_add = match target.currency {
                CurrencyAmount::Decimal { .. } => CurrencyAmount::decimal(amount),
                CurrencyAmount::MultiTier { .. } => CurrencyAmount::multi_tier(amount),
            };

            target.currency = match target.currency.add(&to_add) {
                Ok(new_amount) => new_amount,
                Err(e) => {
                    return Ok(format!("❌ Failed to add currency: {}", e));
                }
            };

            let display_name = target.display_name.clone();
            let currency_value = target.currency.base_value();
            store.put_player_async(target).await?;

            return Ok(format!(
                "✅ Granted {} currency to {}.\nNew balance: {}",
                amount, display_name, currency_value
            ));
        }

        // Giving an object
        let object_id = &args[0];
        let quantity: u32 = if args.len() > 1 {
            match args[1].parse() {
                Ok(qty) => qty,
                Err(_) => {
                    return Ok(format!("❌ Invalid quantity: {}", args[1]));
                }
            }
        } else {
            1
        };

        // Verify object exists
        let obj = match store.get_object(object_id) {
            Ok(o) => o,
            Err(_) => {
                return Ok(format!("❌ Object '{}' not found.", object_id));
            }
        };

        // Check if object already exists in inventory (for stacking)
        if let Some(stack) = target
            .inventory_stacks
            .iter_mut()
            .find(|s| s.object_id == *object_id)
        {
            stack.quantity += quantity;
        } else {
            // Create new stack
            target
                .inventory_stacks
                .push(ItemStack::new(object_id.clone(), quantity));
        }

        let display_name = target.display_name.clone();
        store.put_player_async(target).await?;

        Ok(format!(
            "✅ Granted {}x {} to {}.",
            quantity, obj.name, display_name
        ))
    }

    /// Handle `@TELEPORT` command - teleport a player to a room (admin only)
    ///
    /// Teleports another player to a specified room. Admin level 2+ required.
    ///
    /// # Usage
    /// - `@TELEPORT <player> <room>`: Teleport player to room
    ///
    /// # Permission Requirements
    /// - Caller must be admin level 2+ (Admin or higher)
    ///
    /// # Example
    /// ```text
    /// @TELEPORT alice town_square
    /// @TELEPORT bob tavern
    /// ```
    async fn handle_teleport(
        &mut self,
        session: &Session,
        target_username: String,
        room_id: String,
        config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Check admin permissions (level 2+ required)
        if player.admin_level() < 2 {
            return Ok(
                "❌ Permission denied. Requires admin level 2+ (Admin or higher).".to_string(),
            );
        }

        let store = self.store();

        // Load target player
        let mut target = match store.get_player(&target_username) {
            Ok(p) => p,
            Err(_) => {
                return Ok(format!("❌ Player '{}' not found.", target_username));
            }
        };

        // Verify room exists
        if store.get_room(&room_id).is_err() {
            return Ok(format!("❌ Room '{}' not found.", room_id));
        }

        let display_name = target.display_name.clone();
        target.current_room = room_id.clone();
        store.put_player_async(target).await?;

        Ok(format!(
            "✅ Teleported {} to room '{}'.",
            display_name, room_id
        ))
    }

    /// Handle `@STATS` command - view or edit player statistics
    ///
    /// View or modify player statistics like HP, max HP, etc. Admin level 2+ required for editing.
    ///
    /// # Usage
    /// - `@STATS <player> EDIT <stat> <value>`: Modify player stat
    ///
    /// # Permission Requirements
    /// - Caller must be admin level 2+ (Admin or higher)
    ///
    /// # Examples
    /// ```text
    /// @STATS alice EDIT HP 100
    /// @STATS bob EDIT HP 50
    /// ```
    async fn handle_stats(
        &mut self,
        session: &Session,
        target_username: String,
        args: Vec<String>,
        config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Check admin permissions (level 2+ required)
        if player.admin_level() < 2 {
            return Ok(
                "❌ Permission denied. Requires admin level 2+ (Admin or higher).".to_string(),
            );
        }

        let store = self.store();

        // Load target player
        let mut target = match store.get_player(&target_username) {
            Ok(p) => p,
            Err(_) => {
                return Ok(format!("❌ Player '{}' not found.", target_username));
            }
        };

        if args.is_empty() {
            return Ok(
                "❌ Usage: @STATS <player> EDIT <stat> <value>\nExample: @STATS alice EDIT HP 100"
                    .to_string(),
            );
        }

        // Handle EDIT subcommand
        if args[0].eq_ignore_ascii_case("EDIT") {
            if args.len() < 3 {
                return Ok("❌ Usage: @STATS <player> EDIT <stat> <value>".to_string());
            }

            let stat_name = args[1].to_uppercase();
            let value: u32 = match args[2].parse() {
                Ok(v) => v,
                Err(_) => {
                    return Ok(format!("❌ Invalid value: {}", args[2]));
                }
            };

            match stat_name.as_str() {
                "HP" => {
                    target.stats.hp = value;
                    let display_name = target.display_name.clone();
                    store.put_player_async(target).await?;
                    Ok(format!("✅ Set {}'s HP to {}.", display_name, value))
                }
                _ => Ok(format!("❌ Unknown stat: {}\nAvailable: HP", stat_name)),
            }
        } else {
            Ok("❌ Usage: @STATS <player> EDIT <stat> <value>".to_string())
        }
    }

    /// Handle `/PLAYERS` / `/WHO` command - list all players with status and location
    ///
    /// Lists all players in the TinyMUSH world, showing their current status (online/offline)
    /// and current location. This is an admin-only command requiring level 1+ (Moderator).
    ///
    /// # Permission Requirements
    /// - Caller must be admin (level 1+)
    ///
    /// # Example Output
    /// ```text
    /// 👥 PLAYERS IN TINYMUSH
    ///
    /// Total: 5 players
    ///
    /// Online Players (3):
    ///   [3] Alice (Sysop) - town_square
    ///   [2] Bob (Admin) - market
    ///   [1] Charlie (Moderator) - tavern
    ///
    /// Registered Players (2):
    ///   [0] Dave - town_square (last seen: 2h ago)
    ///   [0] Eve - library (last seen: 1d ago)
    ///
    /// Note: Online status is approximate. Use /WHERE for precise location.
    /// ```
    ///
    /// # Notes
    /// - Shows admin level for administrators
    /// - Groups online and offline players
    /// - Displays current room location
    /// - Useful for monitoring world activity
    async fn handle_players(&mut self, session: &Session, config: &Config) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        let store = self.store();

        // Check if caller is admin
        if !player.is_admin() {
            return Ok("⛔ Permission denied: Not an administrator\n\nThis command is for administrators only.".to_string());
        }

        // Get all players from the store
        let player_ids = match store.list_player_ids() {
            Ok(ids) => ids,
            Err(e) => return Ok(format!("❌ Error listing players: {}", e)),
        };

        if player_ids.is_empty() {
            return Ok("📭 No players found in TinyMUSH.\n\nThe world is empty!".to_string());
        }

        let mut response = String::from("👥 PLAYERS IN TINYMUSH\n\n");
        response.push_str(&format!("Total: {} players\n\n", player_ids.len()));

        // For now, we'll list all players as "registered" since we don't have online tracking yet
        // In a future update, we can integrate with session tracking to show who's actually online
        response.push_str("Registered Players:\n");

        for username in player_ids.iter().take(50) {
            if let Ok(p) = store.get_player(username) {
                let level_indicator = if p.is_admin() {
                    let level_name = match p.admin_level() {
                        1 => "Moderator",
                        2 => "Admin",
                        3 => "Sysop",
                        _ => "Admin",
                    };
                    format!("[{}] ", level_name)
                } else {
                    String::new()
                };

                response.push_str(&format!(
                    "  {}{} - {}\n",
                    level_indicator, p.display_name, p.current_room
                ));
            }
        }

        if player_ids.len() > 50 {
            response.push_str(&format!(
                "\n... and {} more players\n",
                player_ids.len() - 50
            ));
        }

        response.push_str("\nNote: Use /WHERE <player> for detailed location info.\n");

        Ok(response)
    }

    /// Handle `/GOTO` command - teleport admin to player or room
    ///
    /// Teleports the administrator to the specified player's location or directly to a room.
    /// This is an admin-only command requiring level 1+ (Moderator).
    ///
    /// # Permission Requirements
    /// - Caller must be admin (level 1+)
    ///
    /// # Arguments
    /// - `target`: Player username or room ID to teleport to
    ///
    /// # Example Usage
    /// ```text
    /// /GOTO alice
    /// /GOTO town_square
    /// /GOTO market
    /// ```
    ///
    /// # Example Output (Player Target)
    /// ```text
    /// ✈️  TELEPORTING...
    ///
    /// Teleporting to alice's location: market
    ///
    /// === Market ===
    ///
    /// A bustling marketplace filled with merchants and shoppers.
    /// Colorful stalls line the street, selling goods from across the land.
    ///
    /// Exits: north, south, east, west
    /// Players here: alice, bob
    /// ```
    ///
    /// # Example Output (Room Target)
    /// ```text
    /// ✈️  TELEPORTING...
    ///
    /// Teleporting to: tavern
    ///
    /// === Tavern ===
    ///
    /// A cozy tavern with a crackling fireplace.
    /// The smell of ale and roasted meat fills the air.
    ///
    /// Exits: north, east
    /// Players here: charlie
    /// ```
    ///
    /// # Notes
    /// - Can target players (by username) or rooms (by room ID)
    /// - Shows destination room details after teleport
    /// - Action is logged for accountability
    /// - Bypasses normal movement restrictions
    async fn handle_goto(
        &mut self,
        session: &Session,
        target: String,
        config: &Config,
    ) -> Result<String> {
        let mut player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Check if caller is admin
        if !player.is_admin() {
            return Ok("⛔ Permission denied: Not an administrator\n\nThis command is for administrators only.".to_string());
        }

        let target_lower = target.to_lowercase();
        let mut destination: String;
        let mut destination_name: String;
        let mut found_player = false;

        // Try to interpret target as a player first
        {
            let store = self.store();
            if let Ok(target_player) = store.get_player(&target_lower) {
                destination = target_player.current_room.clone();
                destination_name =
                    format!("{}'s location: {}", target_player.display_name, destination);
                found_player = true;
            } else {
                destination = String::new(); // Will be set below
                destination_name = String::new();
            }
        }

        // If not a player, treat as room ID
        if !found_player {
            let room_mgr = self.get_room_manager().await?;
            if room_mgr.get_room(&target_lower).is_ok() {
                destination = target_lower.clone();
                destination_name = destination.clone();
            } else {
                return Ok(format!(
                    "❌ Target not found: {}\n\nCould not find player or room with that name.",
                    target
                ));
            }
        }

        // Teleport the admin
        {
            let store = self.store();
            player.current_room = destination.clone();
            store.put_player(player.clone())?;
        }

        // Show the destination
        let mut response = String::from("✈️  TELEPORTING...\n\n");
        response.push_str(&format!("Teleporting to {}\n\n", destination_name));

        // Show room description - get fresh borrows
        let room_mgr = self.get_room_manager().await?;
        if let Ok(room) = room_mgr.get_room(&destination) {
            response.push_str(&format!("=== {} ===\n\n", room.name));
            response.push_str(&format!("{}\n\n", room.long_desc));

            // Show exits
            if !room.exits.is_empty() {
                let exit_names: Vec<String> =
                    room.exits.keys().map(|k| format!("{:?}", k)).collect();
                response.push_str(&format!("Exits: {}\n", exit_names.join(", ")));
            } else {
                response.push_str("No obvious exits.\n");
            }

            // Show other players in the room - need store again
            let store = self.store();
            let players_here: Vec<String> = store
                .list_player_ids()?
                .iter()
                .filter_map(|username| {
                    if let Ok(p) = store.get_player(username) {
                        if p.current_room == destination && p.username != player.username {
                            Some(p.display_name.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();

            if !players_here.is_empty() {
                response.push_str(&format!("\nPlayers here: {}\n", players_here.join(", ")));
            }
        }

        Ok(response)
    }

    /// Handle `/CONVERT_CURRENCY` command - migrate all currency in the world
    ///
    /// This is a powerful sysop-only command that converts all currency in the world between
    /// Decimal and MultiTier systems. It affects:
    /// - All player wallets
    /// - All player bank accounts
    /// - All item currency values
    /// - All shop inventory items
    ///
    /// The conversion uses a standard ratio: 100 copper = 1 decimal unit (e.g., $1.00 = 100cp)
    ///
    /// Use --dry-run flag to preview changes without applying them.
    ///
    /// # Arguments
    /// - `session`: Current player session
    /// - `currency_type`: Target currency type ("decimal" or "multitier")
    /// - `dry_run`: If true, preview changes without applying them
    /// - `config`: Server configuration
    ///
    /// # Returns
    /// Detailed conversion report with success/failure counts and errors
    ///
    /// # Example
    /// ```text
    /// /CONVERT_CURRENCY multitier --dry-run  # Preview conversion
    /// /CONVERT_CURRENCY multitier             # Actually perform conversion
    /// ```
    async fn handle_convert_currency(
        &mut self,
        session: &Session,
        currency_type: String,
        dry_run: bool,
        _config: &Config,
    ) -> Result<String> {
        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check admin privileges - requires sysop level (3)
        if !store.is_admin(&username.to_lowercase())? {
            return Ok("⛔ Permission denied. This command requires sysop privileges.".to_string());
        }

        let player = store.get_player(&username.to_lowercase())?;

        if player.admin_level.unwrap_or(0) < 3 {
            return Ok(format!(
                "⛔ Permission denied. This command requires sysop level (3).\nYour level: {}\n\nThis is a world-altering command that affects all players.",
                player.admin_level.unwrap_or(0)
            ));
        }

        let to_multitier = currency_type == "multitier";
        let target_type = if to_multitier { "MultiTier" } else { "Decimal" };

        let mut response = String::new();
        response.push_str("═══════════════════════════════════════\n");
        response.push_str(&format!(
            "🔄 CURRENCY MIGRATION TO {}\n",
            target_type.to_uppercase()
        ));
        response.push_str("═══════════════════════════════════════\n\n");

        if dry_run {
            response.push_str("⚠️  DRY RUN MODE - No changes will be made\n\n");
        } else {
            response.push_str("⚠️  WARNING: This will convert all currency in the world!\n\n");
        }

        // Perform the conversion
        let result =
            crate::tmush::currency_migration::migrate_all_currency(&store, to_multitier, dry_run)
                .map_err(|e| anyhow::anyhow!(e))?;

        // Report results
        response.push_str("Results:\n");
        response.push_str(&format!(
            "✅ Successful conversions: {}\n",
            result.success_count
        ));
        response.push_str(&format!(
            "❌ Failed conversions: {}\n",
            result.failure_count
        ));
        response.push_str(&format!(
            "💰 Total amount converted: {} base units\n\n",
            result.total_converted
        ));

        if !result.errors.is_empty() {
            response.push_str("Errors:\n");
            for (i, error) in result.errors.iter().take(10).enumerate() {
                response.push_str(&format!("  {}. {}\n", i + 1, error));
            }
            if result.errors.len() > 10 {
                response.push_str(&format!(
                    "  ... and {} more errors\n",
                    result.errors.len() - 10
                ));
            }
            response.push_str("\n");
        }

        if dry_run {
            response.push_str("💡 Run without --dry-run to apply these changes.\n");
        } else {
            response.push_str("✅ Currency migration complete!\n");
            response.push_str("📝 All conversions have been logged for audit.\n");
        }

        Ok(response)
    }

    // ============================================================================
    // Builder Permission Commands (Phase 7)
    // ============================================================================

    /// Handle `/BUILDER` command - show builder status and available commands
    ///
    /// Displays the player's builder level and lists builder commands they have access to.
    ///
    /// # Builder Levels
    /// - Level 0: No builder permissions
    /// - Level 1: Apprentice - create objects, basic room editing
    /// - Level 2: Builder - create rooms, link exits, modify flags
    /// - Level 3: Architect - full world editing, deletion powers
    ///
    /// # Example
    /// ```text
    /// > /BUILDER
    /// ═══════════════════════════════════════
    /// 🔨 BUILDER STATUS
    /// ═══════════════════════════════════════
    /// Username: alice
    /// Builder Level: 2 (Builder)
    ///
    /// Available Commands:
    ///   /CREATE <name> - Create new objects
    ///   /DESCRIBE <target> <text> - Set descriptions
    ///   /DIG <direction> <room_name> - Create rooms
    ///   /LINK <direction> <destination> - Create exits
    ///   /UNLINK <direction> - Remove exits
    ///   /SETFLAG <target> <flag> - Modify flags
    ///
    /// Total Builders: 5
    /// ```
    async fn handle_builder(&mut self, session: &Session, _config: &Config) -> Result<String> {
        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        let player = store.get_player(&username.to_lowercase())?;
        let builder_level = player.builder_level();

        let mut response = String::new();
        response.push_str("═══════════════════════════════════════\n");
        response.push_str("🔨 BUILDER STATUS\n");
        response.push_str("═══════════════════════════════════════\n");
        response.push_str(&format!("Username: {}\n", username));

        if builder_level == 0 {
            response.push_str("Builder Level: 0 (None)\n\n");
            response.push_str("❌ You do not have builder privileges.\n");
            response.push_str("Contact an admin to request builder access.\n");
            return Ok(response);
        }

        let level_name = match builder_level {
            1 => "Apprentice",
            2 => "Builder",
            3 => "Architect",
            _ => "Unknown",
        };

        response.push_str(&format!(
            "Builder Level: {} ({})\n\n",
            builder_level, level_name
        ));
        response.push_str("Available Commands:\n");

        // Level 1+ commands
        if builder_level >= 1 {
            response.push_str("  /CREATE <name> - Create new objects\n");
            response.push_str("  /DESCRIBE <target> <text> - Set descriptions\n");
        }

        // Level 2+ commands
        if builder_level >= 2 {
            response.push_str("  /DIG <direction> <room_name> - Create rooms\n");
            response.push_str("  /LINK <direction> <destination> - Create exits\n");
            response.push_str("  /UNLINK <direction> - Remove exits\n");
            response.push_str("  /SETFLAG <target> <flag> - Modify flags\n");
        }

        // Level 3 commands
        if builder_level >= 3 {
            response.push_str("  /DESTROY <object> - Delete objects\n");
        }

        // Count total builders
        let all_player_ids = store.list_player_ids()?;
        let builder_count = all_player_ids
            .iter()
            .filter_map(|id| store.get_player(id).ok())
            .filter(|p| p.is_builder())
            .count();

        response.push_str(&format!("\n🛠️ Total Builders: {}\n", builder_count));

        Ok(response)
    }

    /// Handle `/SETBUILDER` command - grant builder privileges to a player
    ///
    /// Grants builder privileges at the specified level (0-3). Only sysop-level admins
    /// (level 3) can grant builder privileges, as builders can modify world structure.
    ///
    /// # Builder Levels
    /// - Level 0: Revoke builder privileges
    /// - Level 1: Apprentice - create objects, basic editing
    /// - Level 2: Builder - create rooms, link exits, modify flags
    /// - Level 3: Architect - full world editing, deletion powers
    ///
    /// # Permission Requirements
    /// - Requires admin level 3 (Sysop)
    /// - Grants significant world-editing capabilities
    ///
    /// # Example
    /// ```text
    /// > /SETBUILDER alice 2
    /// ✅ Granted builder level 2 (Builder) to alice
    /// ```
    async fn handle_set_builder(
        &mut self,
        session: &Session,
        target_username: String,
        level: u8,
        _config: &Config,
    ) -> Result<String> {
        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check if requester is admin with level 3 (sysop)
        if !store.is_admin(&username.to_lowercase())? {
            return Ok(
                "⛔ Permission denied. Only sysops can grant builder privileges.".to_string(),
            );
        }

        let requester = store.get_player(&username.to_lowercase())?;
        if requester.admin_level() < 3 {
            return Ok(format!(
                "⛔ Permission denied. Only sysops (level 3) can grant builder privileges.\nYour admin level: {}",
                requester.admin_level()
            ));
        }

        // Validate level
        if level > 3 {
            return Ok("❌ Invalid builder level. Must be 0-3.".to_string());
        }

        // Get target player
        let mut target = match store.get_player(&target_username.to_lowercase()) {
            Ok(player) => player,
            Err(_) => return Ok(format!("❌ Player '{}' not found.", target_username)),
        };

        // Grant or revoke builder privileges
        if level == 0 {
            target.revoke_builder();
            store.put_player(target)?;
            Ok(format!(
                "✅ Revoked builder privileges from {}",
                target_username
            ))
        } else {
            target.grant_builder(level);
            store.put_player(target)?;

            let level_name = match level {
                1 => "Apprentice",
                2 => "Builder",
                3 => "Architect",
                _ => "Unknown",
            };

            Ok(format!(
                "✅ Granted builder level {} ({}) to {}",
                level, level_name, target_username
            ))
        }
    }

    /// Handle `/REMOVEBUILDER` command - revoke builder privileges from a player
    ///
    /// Removes all builder privileges from the specified player. Only sysop-level admins
    /// can revoke builder privileges.
    ///
    /// # Permission Requirements
    /// - Requires admin level 3 (Sysop)
    ///
    /// # Example
    /// ```text
    /// > /REMOVEBUILDER alice
    /// ✅ Revoked builder privileges from alice
    /// ```
    async fn handle_remove_builder(
        &mut self,
        session: &Session,
        target_username: String,
        _config: &Config,
    ) -> Result<String> {
        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check if requester is admin with level 3 (sysop)
        if !store.is_admin(&username.to_lowercase())? {
            return Ok(
                "⛔ Permission denied. Only sysops can revoke builder privileges.".to_string(),
            );
        }

        let requester = store.get_player(&username.to_lowercase())?;
        if requester.admin_level() < 3 {
            return Ok(format!(
                "⛔ Permission denied. Only sysops (level 3) can revoke builder privileges.\nYour admin level: {}",
                requester.admin_level()
            ));
        }

        // Get target player
        let mut target = match store.get_player(&target_username.to_lowercase()) {
            Ok(player) => player,
            Err(_) => return Ok(format!("❌ Player '{}' not found.", target_username)),
        };

        if !target.is_builder() {
            return Ok(format!(
                "❌ {} does not have builder privileges.",
                target_username
            ));
        }

        target.revoke_builder();
        store.put_player(target)?;

        Ok(format!(
            "✅ Revoked builder privileges from {}",
            target_username
        ))
    }

    /// Handle `/BUILDERS` command - list all builders
    ///
    /// Shows all players with builder privileges and their builder levels.
    /// This is a public command available to all players.
    ///
    /// # Example
    /// ```text
    /// > /BUILDERS
    /// ═══════════════════════════════════════
    /// 🔨 ACTIVE BUILDERS
    /// ═══════════════════════════════════════
    /// alice - Level 2 (Builder)
    /// bob - Level 3 (Architect)
    /// carol - Level 1 (Apprentice)
    ///
    /// Total: 3 builders
    /// ```
    async fn handle_builders(&mut self, _session: &Session, _config: &Config) -> Result<String> {
        let store = self.store();
        let all_player_ids = store.list_player_ids()?;

        let mut builders: Vec<_> = all_player_ids
            .iter()
            .filter_map(|id| store.get_player(id).ok())
            .filter(|p| p.is_builder())
            .collect();

        if builders.is_empty() {
            return Ok("No builders currently registered.".to_string());
        }

        // Sort by builder level (descending) then username
        builders.sort_by(|a, b| {
            b.builder_level()
                .cmp(&a.builder_level())
                .then_with(|| a.username.cmp(&b.username))
        });

        let mut response = String::new();
        response.push_str("═══════════════════════════════════════\n");
        response.push_str("🔨 ACTIVE BUILDERS\n");
        response.push_str("═══════════════════════════════════════\n");

        for builder in &builders {
            let level_name = match builder.builder_level() {
                1 => "Apprentice",
                2 => "Builder",
                3 => "Architect",
                _ => "Unknown",
            };
            response.push_str(&format!(
                "{} - Level {} ({})\n",
                builder.username,
                builder.builder_level(),
                level_name
            ));
        }

        response.push_str(&format!("\nTotal: {} builders\n", builders.len()));

        Ok(response)
    }

    // ============================================================================
    // Backup & Recovery Commands (Phase 9.5)
    // ============================================================================

    /// Handle `/BACKUP` command - create a manual backup
    async fn handle_backup(
        &mut self,
        session: &Session,
        name: Option<String>,
        _config: &Config,
    ) -> Result<String> {
        use crate::storage::backup::{BackupManager, BackupType, RetentionPolicy};

        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check admin permissions (level 2+ required)
        if !store.is_admin(&username.to_lowercase())? {
            return Ok("⛔ Permission denied. Backup commands require admin level 2+.".to_string());
        }

        let player = store.get_player(&username.to_lowercase())?;
        if player.admin_level() < 2 {
            return Ok(format!(
                "⛔ Permission denied. Backup commands require admin level 2+.\nYour admin level: {}",
                player.admin_level()
            ));
        }

        // Create backup manager
        let db_path = std::path::PathBuf::from("data/tinymush");
        let backup_path = std::path::PathBuf::from("data/backups");

        let mut manager = BackupManager::new(db_path, backup_path, RetentionPolicy::default())?;

        // Create backup
        let metadata = manager.create_backup(name, BackupType::Manual)?;

        Ok(format!(
            "✅ Backup created successfully\n\n\
            ID: {}\n\
            Name: {}\n\
            Size: {} bytes\n\
            Checksum: {}\n\n\
            Use /RESTORE {} to restore this backup.",
            metadata.id,
            metadata.name.as_deref().unwrap_or("(no name)"),
            metadata.size_bytes,
            &metadata.checksum[..16],
            metadata.id
        ))
    }

    /// Handle `/LISTBACKUPS` command - list all backups
    async fn handle_list_backups(&mut self, session: &Session, _config: &Config) -> Result<String> {
        use crate::storage::backup::{BackupManager, RetentionPolicy};

        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check admin permissions
        if !store.is_admin(&username.to_lowercase())? {
            return Ok("⛔ Permission denied. Backup commands require admin level 2+.".to_string());
        }

        let player = store.get_player(&username.to_lowercase())?;
        if player.admin_level() < 2 {
            return Ok(format!(
                "⛔ Permission denied. Backup commands require admin level 2+.\nYour admin level: {}",
                player.admin_level()
            ));
        }

        // Create backup manager
        let db_path = std::path::PathBuf::from("data/tinymush");
        let backup_path = std::path::PathBuf::from("data/backups");

        let manager = BackupManager::new(db_path, backup_path, RetentionPolicy::default())?;

        let backups = manager.list_backups();

        if backups.is_empty() {
            return Ok("No backups available.".to_string());
        }

        let mut response = String::from("═══════════════════════════════════════\n");
        response.push_str("💾 AVAILABLE BACKUPS\n");
        response.push_str("═══════════════════════════════════════\n\n");

        for backup in &backups {
            let verified = if backup.verified { "✓" } else { "?" };
            let size_mb = backup.size_bytes as f64 / 1_048_576.0;
            response.push_str(&format!(
                "[{}] {} {}\n  Type: {:?} | Size: {:.2} MB\n  Created: {}\n\n",
                verified,
                backup.id,
                backup.name.as_deref().unwrap_or("(no name)"),
                backup.backup_type,
                size_mb,
                backup.created_at.format("%Y-%m-%d %H:%M:%S")
            ));
        }

        response.push_str(&format!("Total: {} backups\n", backups.len()));

        Ok(response)
    }

    /// Handle `/VERIFYBACKUP` command - verify backup integrity
    async fn handle_verify_backup(
        &mut self,
        session: &Session,
        backup_id: String,
        _config: &Config,
    ) -> Result<String> {
        use crate::storage::backup::{BackupManager, RetentionPolicy};

        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check admin permissions
        if !store.is_admin(&username.to_lowercase())? {
            return Ok("⛔ Permission denied. Backup commands require admin level 2+.".to_string());
        }

        let player = store.get_player(&username.to_lowercase())?;
        if player.admin_level() < 2 {
            return Ok(format!(
                "⛔ Permission denied. Backup commands require admin level 2+.\nYour admin level: {}",
                player.admin_level()
            ));
        }

        // Create backup manager
        let db_path = std::path::PathBuf::from("data/tinymush");
        let backup_path = std::path::PathBuf::from("data/backups");

        let mut manager = BackupManager::new(db_path, backup_path, RetentionPolicy::default())?;

        // Verify backup
        let is_valid = manager.verify_backup(&backup_id)?;

        if is_valid {
            Ok(format!(
                "✅ Backup {} verified successfully - integrity intact",
                backup_id
            ))
        } else {
            Ok(format!(
                "❌ Backup {} FAILED verification - checksum mismatch!",
                backup_id
            ))
        }
    }

    /// Handle `/DELETEBACKUP` command - delete a backup
    async fn handle_delete_backup(
        &mut self,
        session: &Session,
        backup_id: String,
        _config: &Config,
    ) -> Result<String> {
        use crate::storage::backup::{BackupManager, RetentionPolicy};

        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check admin permissions
        if !store.is_admin(&username.to_lowercase())? {
            return Ok("⛔ Permission denied. Backup commands require admin level 2+.".to_string());
        }

        let player = store.get_player(&username.to_lowercase())?;
        if player.admin_level() < 2 {
            return Ok(format!(
                "⛔ Permission denied. Backup commands require admin level 2+.\nYour admin level: {}",
                player.admin_level()
            ));
        }

        // Create backup manager
        let db_path = std::path::PathBuf::from("data/tinymush");
        let backup_path = std::path::PathBuf::from("data/backups");

        let mut manager = BackupManager::new(db_path, backup_path, RetentionPolicy::default())?;

        // Delete backup
        manager.delete_backup(&backup_id)?;

        Ok(format!("✅ Backup {} deleted successfully", backup_id))
    }

    /// Handle `/RESTORE` command - restore from backup (sysop only)
    async fn handle_restore_backup(
        &mut self,
        session: &Session,
        backup_id: String,
        _config: &Config,
    ) -> Result<String> {
        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check sysop permissions (level 3 required for restore)
        if !store.is_admin(&username.to_lowercase())? {
            return Ok(
                "⛔ Permission denied. Restore requires sysop privileges (admin level 3)."
                    .to_string(),
            );
        }

        let player = store.get_player(&username.to_lowercase())?;
        if player.admin_level() < 3 {
            return Ok(format!(
                "⛔ Permission denied. Restore requires sysop privileges (admin level 3).\nYour admin level: {}",
                player.admin_level()
            ));
        }

        // For now, just show instructions
        Ok(format!(
            "🚨 RESTORE OPERATION\n\n\
            To restore backup {}:\n\n\
            1. Stop the server\n\
            2. Run: meshbbs restore {}\n\
            3. Restart the server\n\n\
            ⚠️  WARNING: This will overwrite the current database!",
            backup_id, backup_id
        ))
    }

    /// Handle `/BACKUPCONFIG` command - configure automatic backups
    async fn handle_backup_config(
        &mut self,
        session: &Session,
        args: Vec<String>,
        _config: &Config,
    ) -> Result<String> {
        use crate::storage::backup_scheduler::BackupFrequency;

        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check admin permissions (level 2+ required)
        if !store.is_admin(&username.to_lowercase())? {
            return Ok(
                "⛔ Permission denied. Backup configuration requires admin level 2+.".to_string(),
            );
        }

        let player = store.get_player(&username.to_lowercase())?;
        if player.admin_level() < 2 {
            return Ok(format!(
                "⛔ Permission denied. Backup configuration requires admin level 2+.\nYour admin level: {}",
                player.admin_level()
            ));
        }

        // Parse subcommand
        if args.is_empty() {
            // Show usage
            return Ok("Usage: /BACKUPCONFIG <subcommand>\n\n\
                Subcommands:\n\
                  status            - Show current backup configuration\n\
                  enable            - Enable automatic backups\n\
                  disable           - Disable automatic backups\n\
                  frequency <freq>  - Set backup frequency\n\n\
                Available frequencies:\n\
                  hourly, 2h, 4h, 6h, 12h, daily\n\n\
                Examples:\n\
                  /BACKUPCONFIG status\n\
                  /BACKUPCONFIG enable\n\
                  /BACKUPCONFIG frequency 6h"
                .to_string());
        }

        let subcommand = args[0].to_lowercase();

        match subcommand.as_str() {
            "status" => {
                // Load current configuration
                use crate::storage::backup_scheduler::BackupSchedulerConfig;
                let config = BackupSchedulerConfig::load().unwrap_or_default();

                let status_text = if config.enabled {
                    "✅ Enabled"
                } else {
                    "❌ Disabled"
                };

                Ok(format!(
                    "═══════════════════════════════════════\n\
                    ⚙️  AUTOMATIC BACKUP CONFIGURATION\n\
                    ═══════════════════════════════════════\n\n\
                    Status: {}\n\
                    Frequency: {}\n\
                    Database: {}\n\
                    Backup Path: {}\n\n\
                    Retention Policy:\n\
                    - Daily backups: Keep last {}\n\
                    - Weekly backups: Keep last {}\n\
                    - Monthly backups: Keep last {}\n\n\
                    ℹ️  Changes take effect on next server restart or\n\
                    when the scheduler next checks (every minute).",
                    status_text,
                    config.frequency.description(),
                    config.db_path.display(),
                    config.backup_path.display(),
                    config.retention.daily_count,
                    config.retention.weekly_count,
                    config.retention.monthly_count,
                ))
            }
            "enable" => {
                use crate::storage::backup_scheduler::BackupSchedulerConfig;
                let mut config = BackupSchedulerConfig::load().unwrap_or_default();
                config.enabled = true;
                config.save()?;

                Ok(format!(
                    "✅ Automatic backups enabled\n\n\
                    Frequency: {}\n\
                    Backups will be created {}",
                    config.frequency.description(),
                    config.frequency.description().to_lowercase()
                ))
            }
            "disable" => {
                use crate::storage::backup_scheduler::BackupSchedulerConfig;
                let mut config = BackupSchedulerConfig::load().unwrap_or_default();
                config.enabled = false;
                config.save()?;

                Ok("✅ Automatic backups disabled\n\nManual backups can still be created with /BACKUP.".to_string())
            }
            "frequency" | "freq" => {
                if args.len() < 2 {
                    return Ok("❌ Missing frequency argument\n\n\
                        Usage: /BACKUPCONFIG frequency <freq>\n\n\
                        Available frequencies:\n\
                        - hourly (every hour)\n\
                        - 2h (every 2 hours)\n\
                        - 4h (every 4 hours)\n\
                        - 6h (every 6 hours)\n\
                        - 12h (every 12 hours)\n\
                        - daily (once per day at midnight UTC)\n\n\
                        Example: /BACKUPCONFIG frequency 6h"
                        .to_string());
                }

                let freq_str = &args[1];
                match BackupFrequency::from_str(freq_str) {
                    Some(BackupFrequency::Disabled) => Ok(
                        "❌ Use '/BACKUPCONFIG disable' to disable automatic backups".to_string(),
                    ),
                    Some(freq) => {
                        use crate::storage::backup_scheduler::BackupSchedulerConfig;
                        let mut config = BackupSchedulerConfig::load().unwrap_or_default();
                        config.frequency = freq;
                        config.save()?;

                        Ok(format!(
                            "✅ Backup frequency set to: {}\n\n\
                            Backups will be created {}",
                            freq.description(),
                            freq.description().to_lowercase()
                        ))
                    }
                    None => Ok(format!(
                        "❌ Invalid frequency: {}\n\n\
                            Available frequencies:\n\
                            - hourly, 2h, 4h, 6h, 12h, daily",
                        freq_str
                    )),
                }
            }
            _ => Ok(format!(
                "❌ Unknown subcommand: {}\n\n\
                    Use /BACKUPCONFIG without arguments to see usage.",
                subcommand
            )),
        }
    }

    // ============================================================================
    // Builder World Manipulation Commands (Phase 7)
    // ============================================================================

    /// Handle `/DIG` command - create a new room and link it from current location
    ///
    /// Creates a new room with the specified name and automatically creates a bidirectional
    /// exit connecting the current room to the new room in the specified direction.
    ///
    /// # Permission Requirements
    /// - Requires builder level 2+ (Builder or Architect)
    ///
    /// # Example
    /// ```text
    /// > /DIG north Mysterious Cave
    /// ✅ Created room 'mysterious_cave_1234' (Mysterious Cave)
    /// ✅ Linked north → mysterious_cave_1234
    /// ✅ Linked mysterious_cave_1234 south → town_square
    /// ```
    async fn handle_dig(
        &mut self,
        session: &Session,
        direction_str: String,
        room_name: String,
        _config: &Config,
    ) -> Result<String> {
        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check builder permissions (level 2+ required)
        let player = store.get_player(&username.to_lowercase())?;
        if !player.has_builder_level(2) {
            return Ok(
                "⛔ Permission denied. Creating rooms requires builder level 2 (Builder)."
                    .to_string(),
            );
        }

        // Parse direction
        let direction = match parse_direction_string(&direction_str) {
            Some(dir) => dir,
            None => return Ok(format!("❌ Invalid direction: {}", direction_str)),
        };

        // Get current room
        let mut current_room = store.get_room(&player.current_room)?;

        // Check if exit already exists
        if current_room.exits.contains_key(&direction) {
            return Ok(format!(
                "❌ An exit already exists to the {}.",
                direction_str
            ));
        }

        // Generate unique room ID
        use chrono::Utc;
        let timestamp = Utc::now().timestamp_millis();
        let room_id = format!(
            "{}_{}",
            room_name.to_lowercase().replace(" ", "_"),
            timestamp
        );

        // Create the new room
        use crate::tmush::types::{RoomOwner, RoomRecord};
        let new_room = RoomRecord {
            id: room_id.clone(),
            name: room_name.clone(),
            short_desc: format!("A newly created room: {}", room_name),
            long_desc: format!(
                "This is {}. It needs a description.\nUse /DESCRIBE here <text> to customize it.",
                room_name
            ),
            owner: RoomOwner::Player {
                username: username.to_string(),
            },
            created_at: Utc::now(),
            visibility: crate::tmush::types::RoomVisibility::Public,
            exits: std::collections::HashMap::new(),
            items: Vec::new(),
            flags: vec![crate::tmush::types::RoomFlag::PlayerCreated],
            max_capacity: 15,
            housing_filter_tags: Vec::new(),
            locked: false,
            schema_version: crate::tmush::types::ROOM_SCHEMA_VERSION,
        };

        // Save new room
        store.put_room(new_room.clone())?;

        // Link current room to new room
        current_room.exits.insert(direction, room_id.clone());
        store.put_room(current_room.clone())?;

        // Create reverse exit
        let mut new_room_with_exit = new_room;
        let reverse_direction = get_reverse_direction(&direction);
        new_room_with_exit
            .exits
            .insert(reverse_direction, current_room.id.clone());
        store.put_room(new_room_with_exit)?;

        Ok(format!(
            "✅ Created room '{}' ({})\n✅ Linked {} → {}\n✅ Linked {} {} → {}",
            room_id,
            room_name,
            direction_str,
            room_id,
            room_id,
            format_direction(&reverse_direction),
            current_room.id
        ))
    }

    /// Handle `/DESCRIBE` command for targeting specific rooms or objects
    ///
    /// Sets the description of a specified target. Use "here" to describe the current room.
    ///
    /// # Permission Requirements
    /// - Requires builder level 1+ (Apprentice)
    /// - Can only edit rooms you own or have permission to edit
    ///
    /// # Example
    /// ```text
    /// > /DESCRIBE here A dark and mysterious cave filled with ancient crystals
    /// ✅ Updated description for 'Mysterious Cave'
    /// ```
    /// Handle `/DESCRIBE` command for targeting specific rooms or objects
    ///
    /// Sets the description of a specified target. Use "here" to describe the current room.
    /// Use an object name to describe objects in the current room.
    ///
    /// # Permission Requirements
    /// - Requires builder level 1+ (Apprentice)
    /// - Can only edit rooms you own or have permission to edit
    /// - Can only edit objects you own or have permission to edit
    ///
    /// # Example
    /// ```text
    /// > /DESCRIBE here A dark and mysterious cave filled with ancient crystals
    /// ✅ Updated description for 'Mysterious Cave'
    ///
    /// > /DESCRIBE sword An ancient blade forged in dragon fire
    /// ✅ Updated description for 'Ancient Sword'
    /// ```
    async fn handle_describe_target(
        &mut self,
        session: &Session,
        target: String,
        description: String,
        _config: &Config,
    ) -> Result<String> {
        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check builder permissions (level 1+ required)
        let player = store.get_player(&username.to_lowercase())?;
        if !player.has_builder_level(1) {
            return Ok(
                "⛔ Permission denied. Setting descriptions requires builder level 1 (Apprentice)."
                    .to_string(),
            );
        }

        // Handle "here" as current room
        if target.to_lowercase() == "here" {
            let mut room = store.get_room(&player.current_room)?;

            // Check if player owns the room or is high-level builder
            let can_edit = match &room.owner {
                crate::tmush::types::RoomOwner::Player { username: owner } => {
                    owner == &username || player.has_builder_level(3)
                }
                crate::tmush::types::RoomOwner::World => player.has_builder_level(3),
            };

            if !can_edit {
                return Ok("⛔ You don't have permission to edit this room.\nOnly the room owner or an Architect can edit it.".to_string());
            }

            room.long_desc = description;
            store.put_room(room.clone())?;

            return Ok(format!("✅ Updated description for '{}'", room.name));
        }

        // Handle objects - search in current room
        let room = store.get_room(&player.current_room)?;

        // Find object in room (case-insensitive search by ID or name)
        let object_result = room.items.iter().find_map(|id| {
            // Try exact ID match first
            if id.to_lowercase() == target.to_lowercase() {
                return store.get_object(id).ok();
            }
            // Try matching by object name
            if let Ok(obj) = store.get_object(id) {
                if obj.name.to_lowercase() == target.to_lowercase() {
                    return Some(obj);
                }
            }
            None
        });

        match object_result {
            Some(mut object) => {
                // Check if player owns the object or is architect
                let can_edit = match &object.owner {
                    crate::tmush::types::ObjectOwner::Player { username: owner } => {
                        owner == &username || player.has_builder_level(3)
                    },
                    crate::tmush::types::ObjectOwner::World => player.has_builder_level(3),
                };

                if !can_edit {
                    return Ok("⛔ You don't have permission to edit this object.\nOnly the object owner or an Architect can edit it.".to_string());
                }

                // Update description
                object.description = description;
                store.put_object(object.clone())?;

                Ok(format!("✅ Updated description for '{}'", object.name))
            },
            None => Ok(format!(
                "❌ Target '{}' not found.\nUse 'here' to describe the current room, or specify an object name.",
                target
            ))
        }
    }

    /// Handle `/LINK` command - create an exit from current room to destination
    ///
    /// Creates a one-way exit in the specified direction to the destination room.
    /// Does not create a reverse exit automatically (use /DIG for bidirectional).
    ///
    /// # Permission Requirements
    /// - Requires builder level 2+ (Builder)
    ///
    /// # Example
    /// ```text
    /// > /LINK north cave_entrance
    /// ✅ Created exit north → cave_entrance
    /// ```
    async fn handle_link(
        &mut self,
        session: &Session,
        direction_str: String,
        destination: String,
        _config: &Config,
    ) -> Result<String> {
        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check builder permissions (level 2+ required)
        let player = store.get_player(&username.to_lowercase())?;
        if !player.has_builder_level(2) {
            return Ok(
                "⛔ Permission denied. Creating exits requires builder level 2 (Builder)."
                    .to_string(),
            );
        }

        // Parse direction
        let direction = match parse_direction_string(&direction_str) {
            Some(dir) => dir,
            None => return Ok(format!("❌ Invalid direction: {}", direction_str)),
        };

        // Verify destination room exists
        if store.get_room(&destination).is_err() {
            return Ok(format!(
                "❌ Destination room '{}' does not exist.",
                destination
            ));
        }

        // Get current room
        let mut current_room = store.get_room(&player.current_room)?;

        // Check if exit already exists
        if current_room.exits.contains_key(&direction) {
            return Ok(format!(
                "❌ An exit already exists to the {}.",
                direction_str
            ));
        }

        // Check permissions
        let can_edit = match &current_room.owner {
            crate::tmush::types::RoomOwner::Player { username: owner } => {
                owner == &username || player.has_builder_level(3)
            }
            crate::tmush::types::RoomOwner::World => player.has_builder_level(3),
        };

        if !can_edit {
            return Ok("⛔ You don't have permission to edit this room.".to_string());
        }

        // Create the exit
        current_room.exits.insert(direction, destination.clone());
        store.put_room(current_room)?;

        Ok(format!(
            "✅ Created exit {} → {}",
            direction_str, destination
        ))
    }

    /// Handle `/UNLINK` command - remove an exit from current room
    ///
    /// Removes the exit in the specified direction from the current room.
    /// Does not affect reverse exits.
    ///
    /// # Permission Requirements
    /// - Requires builder level 2+ (Builder)
    ///
    /// # Example
    /// ```text
    /// > /UNLINK north
    /// ✅ Removed exit to the north
    /// ```
    async fn handle_unlink(
        &mut self,
        session: &Session,
        direction_str: String,
        _config: &Config,
    ) -> Result<String> {
        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check builder permissions (level 2+ required)
        let player = store.get_player(&username.to_lowercase())?;
        if !player.has_builder_level(2) {
            return Ok(
                "⛔ Permission denied. Removing exits requires builder level 2 (Builder)."
                    .to_string(),
            );
        }

        // Parse direction
        let direction = match parse_direction_string(&direction_str) {
            Some(dir) => dir,
            None => return Ok(format!("❌ Invalid direction: {}", direction_str)),
        };

        // Get current room
        let mut current_room = store.get_room(&player.current_room)?;

        // Check if exit exists
        if !current_room.exits.contains_key(&direction) {
            return Ok(format!("❌ No exit exists to the {}.", direction_str));
        }

        // Check permissions
        let can_edit = match &current_room.owner {
            crate::tmush::types::RoomOwner::Player { username: owner } => {
                owner == &username || player.has_builder_level(3)
            }
            crate::tmush::types::RoomOwner::World => player.has_builder_level(3),
        };

        if !can_edit {
            return Ok("⛔ You don't have permission to edit this room.".to_string());
        }

        // Remove the exit
        current_room.exits.remove(&direction);
        store.put_room(current_room)?;

        Ok(format!("✅ Removed exit to the {}", direction_str))
    }

    /// Handle `/SETFLAG` command - modify flags on rooms or objects
    ///
    /// Adds or removes flags from rooms or objects. Prefix flag with - to remove it.
    ///
    /// # Permission Requirements
    /// - Requires builder level 2+ (Builder)
    ///
    /// # Example
    /// ```text
    /// > /SETFLAG here safe
    /// ✅ Added flag 'safe' to 'Mysterious Cave'
    /// > /SETFLAG here -dark
    /// ✅ Removed flag 'dark' from 'Mysterious Cave'
    /// ```
    /// Handle `/SETFLAG` command - modify flags on rooms or objects
    ///
    /// Adds or removes flags from rooms or objects. Prefix flag with - to remove it.
    ///
    /// # Permission Requirements
    /// - Requires builder level 2+ (Builder)
    ///
    /// # Room Flags
    /// safe, dark, indoor, shop, questlocation, pvpenabled, playercreated, private,
    /// moderated, instanced, crowded, housingoffice, noteleportout
    ///
    /// # Object Flags
    /// questitem, consumable, equipment, keyitem, container, magical, companion
    ///
    /// # Example
    /// ```text
    /// > /SETFLAG here safe
    /// ✅ Added flag 'safe' to 'Mysterious Cave'
    /// > /SETFLAG here -dark
    /// ✅ Removed flag 'dark' from 'Mysterious Cave'
    /// > /SETFLAG sword magical
    /// ✅ Added flag 'magical' to 'Ancient Sword'
    /// ```
    async fn handle_set_flag(
        &mut self,
        session: &Session,
        target: String,
        flag_str: String,
        _config: &Config,
    ) -> Result<String> {
        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check builder permissions (level 2+ required)
        let player = store.get_player(&username.to_lowercase())?;
        if !player.has_builder_level(2) {
            return Ok(
                "⛔ Permission denied. Modifying flags requires builder level 2 (Builder)."
                    .to_string(),
            );
        }

        // Determine if we're adding or removing the flag
        let (remove, flag_name) = if flag_str.starts_with('-') {
            (true, &flag_str[1..])
        } else {
            (false, flag_str.as_str())
        };

        // Handle "here" as current room
        if target.to_lowercase() == "here" {
            let mut room = store.get_room(&player.current_room)?;

            // Check permissions
            let can_edit = match &room.owner {
                crate::tmush::types::RoomOwner::Player { username: owner } => {
                    owner == &username || player.has_builder_level(3)
                }
                crate::tmush::types::RoomOwner::World => player.has_builder_level(3),
            };

            if !can_edit {
                return Ok("⛔ You don't have permission to edit this room.".to_string());
            }

            // Parse room flag
            let flag = match parse_room_flag(flag_name) {
                Some(f) => f,
                None => return Ok(format!("❌ Unknown room flag: {}\nValid flags: safe, dark, indoor, shop, pvpenabled, private, moderated, noteleportout", flag_name)),
            };

            if remove {
                room.flags.retain(|f| f != &flag);
                store.put_room(room.clone())?;
                Ok(format!(
                    "✅ Removed flag '{}' from '{}'",
                    flag_name, room.name
                ))
            } else {
                if !room.flags.contains(&flag) {
                    room.flags.push(flag);
                }
                store.put_room(room.clone())?;
                Ok(format!("✅ Added flag '{}' to '{}'", flag_name, room.name))
            }
        } else {
            // Handle objects - search in current room
            let room = store.get_room(&player.current_room)?;

            // Find object in room (case-insensitive search by ID or name)
            let object_result = room.items.iter().find_map(|id| {
                // Try exact ID match first
                if id.to_lowercase() == target.to_lowercase() {
                    return store.get_object(id).ok();
                }
                // Try matching by object name
                if let Ok(obj) = store.get_object(id) {
                    if obj.name.to_lowercase() == target.to_lowercase() {
                        return Some(obj);
                    }
                }
                None
            });

            match object_result {
                Some(mut object) => {
                    // Check permissions
                    let can_edit = match &object.owner {
                        crate::tmush::types::ObjectOwner::Player { username: owner } => {
                            owner == &username || player.has_builder_level(3)
                        },
                        crate::tmush::types::ObjectOwner::World => player.has_builder_level(3),
                    };

                    if !can_edit {
                        return Ok("⛔ You don't have permission to edit this object.".to_string());
                    }

                    // Parse object flag
                    let flag = match parse_object_flag(flag_name) {
                        Some(f) => f,
                        None => return Ok(format!("❌ Unknown object flag: {}\nValid flags: questitem, consumable, equipment, keyitem, container, magical, companion", flag_name)),
                    };

                    if remove {
                        object.flags.retain(|f| f != &flag);
                        store.put_object(object.clone())?;
                        Ok(format!("✅ Removed flag '{}' from '{}'", flag_name, object.name))
                    } else {
                        if !object.flags.contains(&flag) {
                            object.flags.push(flag);
                        }
                        store.put_object(object.clone())?;
                        Ok(format!("✅ Added flag '{}' to '{}'", flag_name, object.name))
                    }
                },
                None => Ok(format!(
                    "❌ Target '{}' not found.\nUse 'here' to modify the current room, or specify an object name.",
                    target
                ))
            }
        }
    }

    /// Handle `/CREATE` command - create a new object in current room
    ///
    /// Creates a new takeable object with the specified name in the current room.
    ///
    /// # Permission Requirements
    /// - Requires builder level 1+ (Apprentice)
    ///
    /// # Example
    /// ```text
    /// > /CREATE Ancient Sword
    /// ✅ Created object 'ancient_sword_1234' (Ancient Sword)
    /// ```
    async fn handle_create(
        &mut self,
        session: &Session,
        object_name: String,
        _config: &Config,
    ) -> Result<String> {
        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check builder permissions (level 1+ required)
        let player = store.get_player(&username.to_lowercase())?;
        if !player.has_builder_level(1) {
            return Ok(
                "⛔ Permission denied. Creating objects requires builder level 1 (Apprentice)."
                    .to_string(),
            );
        }

        // Generate unique object ID
        use chrono::Utc;
        let timestamp = Utc::now().timestamp_millis();
        let object_id = format!(
            "{}_{}",
            object_name.to_lowercase().replace(" ", "_"),
            timestamp
        );

        // Create the object
        use crate::tmush::types::{ObjectRecord, OwnershipReason};
        let object = ObjectRecord::new_player_owned(
            &object_id,
            &object_name,
            &format!(
                "A newly created object: {}. Use /DESCRIBE {} <text> to set a description.",
                object_name, object_name
            ),
            username,
            OwnershipReason::Created,
        );

        // Save object
        store.put_object(object)?;

        // Add to current room
        let mut room = store.get_room(&player.current_room)?;
        room.items.push(object_id.clone());
        store.put_room(room)?;

        Ok(format!(
            "✅ Created object '{}' ({})",
            object_id, object_name
        ))
    }

    /// Handle `/DESTROY` command - permanently delete an object
    ///
    /// Permanently deletes the specified object from the world. This action cannot be undone.
    ///
    /// # Permission Requirements
    /// - Requires builder level 3 (Architect)
    /// - Destructive action with no undo
    ///
    /// # Example
    /// ```text
    /// > /DESTROY ancient_sword_1234
    /// ⚠️  WARNING: This will permanently delete the object!
    /// ✅ Deleted object 'ancient_sword_1234'
    /// ```
    /// Handle `/DESTROY` command - permanently delete an object
    ///
    /// Permanently deletes the specified object from the world. This action cannot be undone.
    ///
    /// # Container Safety
    /// - If object is an empty container: deleted immediately
    /// - If container has items (non-containers): contents moved to room, then deleted
    /// - If container has nested containers: ERROR - must empty nested containers first
    ///
    /// # Permission Requirements
    /// - Requires builder level 3 (Architect)
    /// - Destructive action with no undo
    ///
    /// # Example
    /// ```text
    /// > /DESTROY ancient_sword_1234
    /// ✅ Deleted object 'ancient_sword_1234'
    ///
    /// > /DESTROY treasure_chest
    /// ✅ Deleted container 'treasure_chest' (3 items moved to room)
    ///
    /// > /DESTROY nested_box
    /// ❌ Container 'Nested Box' contains other containers. Empty it first.
    /// ```
    async fn handle_destroy(
        &mut self,
        session: &Session,
        object_name: String,
        _config: &Config,
    ) -> Result<String> {
        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check builder permissions (level 3 required - destructive action)
        let player = store.get_player(&username.to_lowercase())?;
        if !player.has_builder_level(3) {
            return Ok(
                "⛔ Permission denied. Deleting objects requires builder level 3 (Architect)."
                    .to_string(),
            );
        }

        // Get current room
        let room = store.get_room(&player.current_room)?;

        // Find object in room (case-insensitive search)
        let object_id = room
            .items
            .iter()
            .find(|id| {
                // Try exact match first
                if id.to_lowercase() == object_name.to_lowercase() {
                    return true;
                }
                // Try matching by object name
                if let Ok(obj) = store.get_object(id) {
                    if obj.name.to_lowercase() == object_name.to_lowercase() {
                        return true;
                    }
                }
                false
            })
            .cloned();

        match object_id {
            Some(id) => {
                // Get the object to show its name in response
                let object = store.get_object(&id)?;

                // Attempt to delete the object (handles container safety)
                match store.delete_object(&id, &player.current_room) {
                    Ok(relocated_items) => {
                        if relocated_items.is_empty() {
                            Ok(format!("✅ Deleted object '{}'", object.name))
                        } else {
                            Ok(format!(
                                "✅ Deleted container '{}' ({} items moved to this room)",
                                object.name,
                                relocated_items.len()
                            ))
                        }
                    }
                    Err(crate::tmush::errors::TinyMushError::ContainerNotEmpty(msg)) => {
                        Ok(format!("❌ {}", msg))
                    }
                    Err(e) => Ok(format!("❌ Failed to delete object: {}", e)),
                }
            }
            None => Ok(format!(
                "❌ Object '{}' not found in current room.",
                object_name
            )),
        }
    }

    /// Handle `/CLONE` command - create a copy of an owned clonable object
    ///
    /// Security features:
    /// - Player must own the source object
    /// - Object must have Clonable flag set
    /// - Respects clone depth limits (max 3 generations)
    /// - Enforces per-player quotas (20 clones/hour)
    /// - Requires cooldown between clones (60 seconds)
    /// - Prevents cloning high-value objects (>100 gold)
    /// - Strips currency value from clones
    /// - Prevents cloning unique/quest items
    ///
    /// ## Usage
    /// ```text
    /// > /CLONE sword
    /// ✨ Cloned 'Iron Sword'!
    /// Clone ID: #obj_alic...
    /// Clone Depth: 1/3
    /// Clone Quota: 19/20 remaining this hour
    /// ```
    ///
    /// See docs/development/CLONING_SECURITY.md for full threat model.
    fn clone_object_internal(
        &self,
        source_id: &str,
        buyer_username: &str,
        source_object: &crate::tmush::types::ObjectRecord,
    ) -> Result<crate::tmush::types::ObjectRecord> {
        use uuid::Uuid;

        // Create a simple clone for shop purchases (no security checks needed)
        let mut cloned = source_object.clone();

        // Generate new unique ID
        cloned.id = format!(
            "obj_{}",
            Uuid::new_v4().to_string().replace("-", "")[..12].to_string()
        );

        // Set new owner
        cloned.owner = crate::tmush::types::ObjectOwner::Player {
            username: buyer_username.to_string(),
        };

        // Preserve clone depth (vending machines keep originals, so depth stays 0)
        // But track in clone_source_id for genealogy
        cloned.clone_source_id = Some(source_id.to_string());

        // Clear any flags that shouldn't transfer
        cloned.flags.retain(|f| {
            !matches!(
                f,
                crate::tmush::types::ObjectFlag::Unique
                    | crate::tmush::types::ObjectFlag::QuestItem
                    | crate::tmush::types::ObjectFlag::Companion
            )
        });

        // Record ownership
        Self::record_ownership_transfer(
            &mut cloned,
            None,
            buyer_username.to_string(),
            crate::tmush::types::OwnershipReason::Purchased,
        );

        // Save the cloned object to database
        self.store().put_object(cloned.clone())?;

        Ok(cloned)
    }

    async fn handle_clone(
        &mut self,
        session: &Session,
        object_name: String,
        _config: &Config,
    ) -> Result<String> {
        use crate::tmush::{clone::handle_clone_command, resolver::ResolutionContext};

        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Get player state
        let player = store.get_player(&username.to_lowercase())?;

        // Build resolution context
        let context = ResolutionContext::new(
            username.to_lowercase(),
            player.current_room.clone(),
            None, // last_examined not needed for cloning
        );

        // Call the clone handler from the clone module
        let result = handle_clone_command(&object_name, &context, &store);

        match result {
            Ok(message) => Ok(message),
            Err(e) => Ok(format!("❌ Clone failed: {}", e)),
        }
    }

    /// Handle `/LISTCLONES` command - list all clones owned by a player
    ///
    /// Shows clone genealogy with depth, source, and creation details.
    /// Useful for detecting clone abuse and quota violations.
    ///
    /// ## Usage
    /// ```text
    /// > /LISTCLONES alice
    /// 🔍 Clones owned by alice (5 total):
    ///
    /// 1. 'Iron Sword' (depth 1, cloned from 'Original Sword')
    /// 2. 'Magic Wand' (depth 2, cloned from 'Wand Copy')
    /// ...
    /// ```
    ///
    /// Permission: Admin level 1+ (Moderator)
    async fn handle_list_clones(
        &mut self,
        session: &Session,
        target_username: Option<String>,
        _config: &Config,
    ) -> Result<String> {
        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check admin permissions (level 1+)
        let admin = store.get_player(&username.to_lowercase())?;
        if admin.admin_level() < 1 {
            return Ok(
                "⛔ Permission denied. This command requires moderator privileges.".to_string(),
            );
        }

        // If no username specified, list current player's clones
        let target = target_username.as_deref().unwrap_or(username);

        // Get player
        let player = match store.get_player(&target.to_lowercase()) {
            Ok(p) => p,
            Err(_) => return Ok(format!("❌ Player '{}' not found.", target)),
        };

        // Find all objects in player's inventory that are clones (clone_depth > 0)
        let mut clones = Vec::new();

        for obj_id in &player.inventory {
            if let Ok(obj) = store.get_object(obj_id) {
                if obj.clone_depth > 0 {
                    clones.push(obj);
                }
            }
        }

        if clones.is_empty() {
            return Ok(format!("🔍 No clones found in {}'s inventory.", target));
        }

        // Sort by clone depth, then by name
        clones.sort_by(|a, b| a.clone_depth.cmp(&b.clone_depth).then(a.name.cmp(&b.name)));

        let mut response = format!(
            "🔍 Clones owned by {} ({} total):\n\n",
            target,
            clones.len()
        );

        for (idx, clone) in clones.iter().enumerate() {
            let source_info = if let Some(source_id) = &clone.clone_source_id {
                if let Ok(source) = store.get_object(source_id) {
                    format!("cloned from '{}'", source.name)
                } else {
                    format!("source: {}", &source_id[..8])
                }
            } else {
                "no source".to_string()
            };

            response.push_str(&format!(
                "{}. '{}' (depth {}/{}, {})\n   ID: {} | Created: {}\n",
                idx + 1,
                clone.name,
                clone.clone_depth,
                crate::tmush::clone::MAX_CLONE_DEPTH,
                source_info,
                &clone.id[..12],
                clone.created_at.format("%Y-%m-%d %H:%M")
            ));
        }

        response.push_str(&format!(
            "\n📊 Player Clone Quota: {}/{} remaining | Total Objects: {}/{}\n",
            player.clone_quota,
            crate::tmush::clone::CLONES_PER_HOUR,
            player.total_objects_owned,
            crate::tmush::clone::MAX_OBJECTS_PER_PLAYER
        ));

        Ok(response)
    }

    /// Handle `/CLONESTATS` command - server-wide clone statistics
    ///
    /// Shows aggregate cloning metrics for detecting abuse patterns.
    ///
    /// ## Usage
    /// ```text
    /// > /CLONESTATS
    /// 📊 Server-Wide Clone Statistics
    ///
    /// Total Clones: 1,234
    /// Total Players with Clones: 56
    /// Average Clones per Player: 22.0
    ///
    /// Top 5 Cloners:
    /// 1. alice: 145 clones (4 at max depth)
    /// 2. bob: 98 clones (12 at max depth)
    /// ...
    /// ```
    ///
    /// Permission: Admin level 1+ (Moderator)
    async fn handle_clone_stats(&mut self, session: &Session, _config: &Config) -> Result<String> {
        let username = session.username.as_deref().unwrap_or("unknown");
        let store = self.store();

        // Check admin permissions (level 1+)
        let admin = store.get_player(&username.to_lowercase())?;
        if admin.admin_level() < 1 {
            return Ok(
                "⛔ Permission denied. This command requires moderator privileges.".to_string(),
            );
        }

        // Note: This is a simplified implementation that only scans active players' inventories.
        // A production version would have an indexed query for all clones in the system.

        let mut response = String::from("📊 Clone Statistics (Simplified)\n\n");
        response.push_str("Note: This shows only clones in active players' inventories.\n");
        response.push_str("For complete statistics, a database index is needed.\n\n");

        response.push_str(&format!(
            "Security Limits:\n\
            - Max Clone Depth: {}\n\
            - Clones Per Hour: {}\n\
            - Max Objects Per Player: {}\n\
            - Max Clonable Value: {} gold\n\
            - Cooldown: {} seconds\n\n",
            crate::tmush::clone::MAX_CLONE_DEPTH,
            crate::tmush::clone::CLONES_PER_HOUR,
            crate::tmush::clone::MAX_OBJECTS_PER_PLAYER,
            crate::tmush::clone::MAX_CLONABLE_VALUE,
            crate::tmush::clone::CLONE_COOLDOWN
        ));

        response.push_str("Use /LISTCLONES <player> to view a specific player's clones.\n");

        Ok(response)
    }

    /// Helper to get required progress for achievement
    fn get_achievement_required(&self, trigger: &crate::tmush::types::AchievementTrigger) -> u32 {
        use crate::tmush::types::AchievementTrigger::*;
        match trigger {
            KillCount { required }
            | RoomVisits { required }
            | FriendCount { required }
            | QuestCompletion { required }
            | CraftCount { required }
            | TradeCount { required }
            | MessagesSent { required } => *required,
            CurrencyEarned { amount } => (*amount).max(0) as u32,
            VisitLocation { .. } | CompleteQuest { .. } => 1,
        }
    }

    /// Handle HELP command
    async fn handle_help(
        &mut self,
        _session: &Session,
        topic: Option<String>,
        config: &Config,
    ) -> Result<String> {
        // Load world config for help text
        let store = self.store();
        let world_config = store.get_world_config()?;

        match topic.as_deref() {
            Some("commands") | Some("COMMANDS") => Ok(world_config.help_commands),
            Some("movement") | Some("MOVEMENT") => Ok(world_config.help_movement),
            Some("social") | Some("SOCIAL") => Ok(world_config.help_social),
            Some("board") | Some("BOARD") | Some("bulletin") | Some("BULLETIN") => {
                Ok(world_config.help_bulletin)
            }
            Some("mail") | Some("MAIL") => Ok(world_config.help_mail),
            Some("companion") | Some("COMPANION") | Some("companions") | Some("COMPANIONS") => {
                Ok(world_config.help_companion)
            }
            None => Ok(world_config.help_main),
            Some(topic) => Ok(format!(
                "No help available for: {}\nTry: HELP COMMANDS",
                topic
            )),
        }
    }

    /// Handle QUIT command - exit TinyMUSH
    async fn handle_quit(&mut self, session: &mut Session, _config: &Config) -> Result<String> {
        // Record game exit metrics
        if let Some(slug) = session.current_game_slug.take() {
            let counters = metrics::record_game_exit(&slug);
            info!(
                target: "meshbbs::games",
                "game.exit slug={} session={} user={} node={} reason=command command=QUIT active={} exits={} entries={} peak={}",
                slug,
                escape_log(&session.id),
                escape_log(&session.display_name()),
                escape_log(&session.node_id),
                counters.currently_active,
                counters.exits,
                counters.entries,
                counters.concurrent_peak
            );
        }

        if let Ok(player) = self.get_or_create_player(session).await {
            if crate::tmush::state::is_any_landing_room(&player.current_room) {
                self.store()
                    .clear_landing_instance_for_player(&player.username);
            }
        }

        // Return to main menu
        session.state = SessionState::MainMenu;
        Ok("Leaving TinyMUSH...\n\nReturning to main menu.".to_string())
    }

    /// Handle SAVE command - force save player state
    async fn handle_save(&mut self, session: &Session, _config: &Config) -> Result<String> {
        match self.get_or_create_player(session).await {
            Ok(player) => match self.store().put_player(player) {
                Ok(()) => Ok("Player state saved.".to_string()),
                Err(e) => Ok(format!("Save failed: {}", e)),
            },
            Err(e) => Ok(format!("Error saving: {}", e)),
        }
    }

    /// Handle BOARD command - view bulletin board
    async fn handle_board(
        &mut self,
        session: &Session,
        board_id: Option<String>,
        _config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // For now, only support the "stump" board in the town square
        let board_id = board_id.unwrap_or_else(|| "stump".to_string());

        if board_id != "stump" {
            return Ok(
                "Only the 'stump' bulletin board is available.\nUsage: BOARD or BOARD stump"
                    .to_string(),
            );
        }

        // Check if player is in the town square
        if player.current_room != "town_square" {
            return Ok("You must be at the Town Square to access the Town Stump bulletin board.\nHead to the town square and try again.".to_string());
        }

        // Initialize stump board if it doesn't exist
        let board = match self.store().get_bulletin_board(&board_id) {
            Ok(board) => board,
            Err(TinyMushError::NotFound(_)) => {
                // Create the stump board
                let board = BulletinBoard::new(
                    "stump",
                    "Town Stump",
                    "A weathered stump with notices posted by travelers",
                    "town_square",
                );
                self.store().put_bulletin_board(board.clone())?;
                board
            }
            Err(e) => return Ok(format!("Bulletin board error: {}", e)),
        };

        // Get recent messages (last 10)
        let messages = match self.store().list_bulletins(&board_id, 0, 10) {
            Ok(messages) => messages,
            Err(e) => return Ok(format!("Error reading bulletins: {}", e)),
        };

        let mut response = format!("=== {} ===\n{}\n\n", board.name, board.description);

        if messages.is_empty() {
            response.push_str("No messages posted.\n");
        } else {
            response.push_str("Recent messages:\n");
            for (i, msg) in messages.iter().enumerate() {
                // Format: [ID] Subject - by Author (date)
                let date = msg.posted_at.format("%m/%d");
                response.push_str(&format!(
                    "[{}] {} - by {} ({})\n",
                    msg.id, msg.subject, msg.author, date
                ));

                // Limit to fit in 200 bytes
                if response.len() > 150 {
                    let remaining = messages.len() - i - 1;
                    if remaining > 0 {
                        response.push_str(&format!("... and {} more.\n", remaining));
                    }
                    break;
                }
            }
        }

        response.push_str("\nPOST <subject> <message> to add\nREAD <id> to read a message");
        Ok(response)
    }

    /// Handle POST command - post message to bulletin board
    async fn handle_post(
        &mut self,
        session: &Session,
        subject: String,
        message: String,
        _config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Check if player is in the town square
        if player.current_room != "town_square" {
            return Ok("You must be at the Town Square to post to the bulletin board.".to_string());
        }

        // Validate input
        if subject.trim().is_empty() {
            return Ok("Subject cannot be empty.\nUsage: POST <subject> <message>".to_string());
        }

        if message.trim().is_empty() {
            return Ok("Message cannot be empty.\nUsage: POST <subject> <message>".to_string());
        }

        if subject.len() > 50 {
            return Ok("Subject too long (max 50 characters).".to_string());
        }

        if message.len() > 300 {
            return Ok("Message too long (max 300 characters).".to_string());
        }

        // Create the bulletin message
        let bulletin = BulletinMessage::new(&session.display_name(), &subject, &message, "stump");

        // Post the message
        match self.store().post_bulletin_async(bulletin).await {
            Ok(message_id) => {
                // Clean up old messages if needed
                let _ = self.store().cleanup_bulletins("stump", 50);

                Ok(format!(
                    "Message posted to Town Stump bulletin board.\nMessage ID: {} - '{}'\nOthers can read it with: READ {}",
                    message_id, subject, message_id
                ))
            }
            Err(e) => Ok(format!("Failed to post message: {}", e)),
        }
    }

    /// Handle READ command - read specific bulletin message
    async fn handle_read(
        &mut self,
        session: &Session,
        message_id: u64,
        _config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Check if player is in the town square
        if player.current_room != "town_square" {
            return Ok(
                "You must be at the Town Square to read bulletin board messages.".to_string(),
            );
        }

        // Get the message
        match self.store().get_bulletin("stump", message_id) {
            Ok(message) => {
                let date = message.posted_at.format("%b %d, %Y at %H:%M");
                let mut response = format!(
                    "=== Message {} ===\n\
                    From: {}\n\
                    Subject: {}\n\
                    Date: {}\n\n\
                    {}",
                    message.id, message.author, message.subject, date, message.body
                );

                // Ensure we stay under 200 bytes
                if response.len() > 200 {
                    response.truncate(197);
                    response.push_str("...");
                }

                Ok(response)
            }
            Err(TinyMushError::NotFound(_)) => Ok(format!(
                "No bulletin message with ID {}.\nUse BOARD to see available messages.",
                message_id
            )),
            Err(e) => Ok(format!("Error reading message: {}", e)),
        }
    }

    /// Get or create player record for this session
    async fn get_or_create_player(&self, session: &Session) -> Result<PlayerRecord, TinyMushError> {
        let username = session.display_name();

        match self.store().get_player(&username) {
            Ok(player) => Ok(player),
            Err(TinyMushError::NotFound(_)) => {
                // Create new player at gazebo for tutorial
                let display_name = if session.is_logged_in() {
                    session.display_name()
                } else {
                    format!("Guest_{}", &session.id[..8])
                };

                let landing_id = self.store().ensure_personal_landing_room(&username)?;
                let player = PlayerRecord::new(&username, &display_name, &landing_id);
                self.store().put_player(player.clone())?;
                Ok(player)
            }
            Err(e) => Err(e),
        }
    }

    /// Helper: Find object by name in a list of object IDs
    ///
    /// Searches through object IDs, loads each object, and returns the first match
    /// where the object name matches (case-insensitive).
    fn find_object_by_name(&self, name: &str, object_ids: &[String]) -> Option<ObjectRecord> {
        let name_upper = name.to_uppercase();

        for object_id in object_ids {
            if let Ok(object) = self.store().get_object(object_id) {
                if object.name.to_uppercase() == name_upper {
                    return Some(object);
                }
            }
        }

        None
    }

    /// Find objects by partial name match (fuzzy matching)
    /// Returns all objects whose display names contain the search term (case-insensitive)
    fn find_objects_by_partial_name(
        &self,
        search_term: &str,
        object_ids: &[String],
    ) -> Vec<ObjectRecord> {
        let search_upper = search_term.to_uppercase();
        let mut matches = Vec::new();

        for object_id in object_ids {
            if let Ok(object) = self.store().get_object(object_id) {
                let name_upper = object.name.to_uppercase();

                // Check if the object name contains the search term
                if name_upper.contains(&search_upper) {
                    matches.push(object);
                }
            }
        }

        matches
    }

    /// Helper: Check if player has an item in their inventory_stacks
    fn player_has_item(player: &PlayerRecord, object_id: &str) -> bool {
        player
            .inventory_stacks
            .iter()
            .any(|stack| stack.object_id == object_id)
    }

    /// Helper: Remove item from player's inventory_stacks
    fn remove_item_from_player(player: &mut PlayerRecord, object_id: &str, quantity: u32) -> bool {
        use crate::tmush::inventory::remove_item_from_inventory;
        use crate::tmush::types::InventoryResult;

        match remove_item_from_inventory(player, object_id, quantity) {
            InventoryResult::Removed { .. } => true,
            _ => false,
        }
    }

    /// Helper: Record ownership transfer in item's history (Phase 5)
    fn record_ownership_transfer(
        item: &mut ObjectRecord,
        from_owner: Option<String>,
        to_owner: String,
        reason: crate::tmush::types::OwnershipReason,
    ) {
        use crate::tmush::types::OwnershipTransfer;
        use chrono::Utc;

        let transfer = OwnershipTransfer {
            from_owner,
            to_owner,
            timestamp: Utc::now(),
            reason,
        };

        item.ownership_history.push(transfer);
    }

    /// Helper: Move housing items to reclaim box on deletion (Phase 6)
    /// TODO: Call this when implementing housing deletion command
    #[allow(dead_code)]
    async fn move_housing_to_reclaim_box(
        &mut self,
        instance: &mut crate::tmush::types::HousingInstance,
        config: &Config,
    ) -> Result<String> {
        // 1. Collect all items from all housing rooms
        let mut items_moved = 0;
        {
            let store = self.store();
            for room_id in instance.room_mappings.values() {
                if let Ok(mut room) = store.get_room_async(room_id).await {
                    // Move all items to reclaim box
                    for item_id in &room.items {
                        instance.reclaim_box.push(item_id.clone());
                        items_moved += 1;
                    }
                    room.items.clear();
                    store.put_room_async(room).await?;
                }
            }
        }

        // 2. Teleport all occupants to town square
        let housing_rooms: Vec<String> = instance.room_mappings.values().cloned().collect();
        let mut players_teleported = 0;

        // Collect all players to teleport
        let mut players_to_teleport = Vec::new();
        {
            let room_manager = self.get_room_manager().await?;
            for room_id in &housing_rooms {
                let players_in_room = room_manager.get_players_in_room(room_id);
                players_to_teleport.extend(players_in_room);
            }
        }

        // Now teleport them
        for username in players_to_teleport {
            let store = self.store();
            if let Ok(mut player) = store.get_player_async(&username).await {
                player.current_room = "town_square".to_string();
                store.put_player_async(player).await?;
                players_teleported += 1;
            }
        }

        // 3. TODO: Return companions to owner's companion list
        // This will be implemented when companion system is fully integrated

        // 4. Mark instance as inactive for Phase 7 cleanup
        instance.inactive_since = Some(chrono::Utc::now());

        Ok(format!(
            "Housing contents moved to reclaim box: {} item(s). {} player(s) teleported to town square.",
            items_moved, players_teleported
        ))
    }

    /// Handle HISTORY command - view item ownership audit trail (Phase 5)
    async fn handle_history(
        &mut self,
        session: &Session,
        item_name: String,
        config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;
        let store = self.store();

        let item_name_upper = item_name.to_uppercase();

        // Search for item in player's inventory
        for item_id in &player.inventory {
            if let Ok(item) = store.get_object(item_id) {
                if item.name.to_uppercase() == item_name_upper {
                    // Check if player owns this item
                    match &item.owner {
                        crate::tmush::types::ObjectOwner::Player { username } => {
                            if username != &player.username {
                                return Ok(format!(
                                    "You don't own {}. Only the owner can view ownership history.",
                                    item.name
                                ));
                            }
                        }
                        crate::tmush::types::ObjectOwner::World => {
                            return Ok(format!(
                                "{} is a world item with no ownership history.",
                                item.name
                            ));
                        }
                    }

                    // Display ownership history
                    let mut response = String::new();
                    response.push_str(&format!("=== Ownership History: {} ===\n\n", item.name));

                    if item.ownership_history.is_empty() {
                        response.push_str("No ownership transfers recorded.\n");
                        response
                            .push_str("(This item may predate the ownership tracking system)\n");
                    } else {
                        for (idx, transfer) in item.ownership_history.iter().enumerate() {
                            let from = transfer.from_owner.as_deref().unwrap_or("WORLD");
                            let to = &transfer.to_owner;
                            let reason = format!("{:?}", transfer.reason);
                            let timestamp = transfer.timestamp.format("%Y-%m-%d %H:%M:%S");

                            response.push_str(&format!(
                                "{}. {} → {} | {} | {}\n",
                                idx + 1,
                                from,
                                to,
                                reason,
                                timestamp
                            ));
                        }

                        response.push_str(&format!(
                            "\nTotal transfers: {}\n",
                            item.ownership_history.len()
                        ));
                    }

                    return Ok(response);
                }
            }
        }

        Ok(format!("You don't have '{}' in your inventory.", item_name))
    }

    /// Handle RECLAIM command - retrieve items from reclaim box (Phase 6)
    async fn handle_reclaim(
        &mut self,
        session: &Session,
        item_name: Option<String>,
        config: &Config,
    ) -> Result<String> {
        let player = self.get_or_create_player(session).await?;
        let store = self.store();

        // Get all player's housing instances (including those with reclaim boxes)
        let instances = store.get_player_housing_instances(&player.username)?;

        // Find instances with non-empty reclaim boxes
        let reclaim_instances: Vec<_> = instances
            .into_iter()
            .filter(|inst| !inst.reclaim_box.is_empty())
            .collect();

        if reclaim_instances.is_empty() {
            return Ok("Your reclaim box is empty.".to_string());
        }

        // If no item specified, list reclaim box contents
        if item_name.is_none() {
            let mut response = String::new();
            response.push_str("=== Reclaim Box ===\n\n");
            response.push_str("Items available for recovery:\n\n");

            let mut item_count = 0;
            for instance in &reclaim_instances {
                for item_id in &instance.reclaim_box {
                    if let Ok(item) = store.get_object(item_id) {
                        item_count += 1;
                        response.push_str(&format!(
                            "{}. {} - {}\n",
                            item_count, item.name, item.description
                        ));
                    }
                }
            }

            if item_count == 0 {
                response.push_str("(No valid items found)\n");
            } else {
                response.push_str(&format!("\nTotal: {} item(s)\n", item_count));
                response.push_str("Use RECLAIM <item> to retrieve an item.\n");
            }

            return Ok(response);
        }

        // Retrieve specific item from reclaim box
        let item_name_upper = item_name.unwrap().to_uppercase();

        for mut instance in reclaim_instances {
            // Find matching item and get its index
            let mut found_idx = None;
            let mut found_item_id = None;

            for (idx, item_id) in instance.reclaim_box.iter().enumerate() {
                if let Ok(item) = store.get_object(item_id) {
                    if item.name.to_uppercase() == item_name_upper {
                        found_idx = Some(idx);
                        found_item_id = Some(item_id.clone());
                        break;
                    }
                }
            }

            if let (Some(idx), Some(item_id)) = (found_idx, found_item_id) {
                // Found the item! Get it and update
                let mut item = store.get_object(&item_id)?;

                // Remove from reclaim box
                instance.reclaim_box.remove(idx);

                // Record ownership transfer (Phase 5)
                Self::record_ownership_transfer(
                    &mut item,
                    Some("RECLAIM_BOX".to_string()),
                    player.username.clone(),
                    crate::tmush::types::OwnershipReason::Reclaimed,
                );

                // Update ownership
                item.owner = crate::tmush::types::ObjectOwner::Player {
                    username: player.username.clone(),
                };

                // Save updated item
                store.put_object(item.clone())?;

                // Add to player inventory using proper inventory system
                use crate::tmush::inventory::add_item_to_inventory;
                use crate::tmush::types::{InventoryConfig, InventoryResult};

                let mut updated_player = player.clone();
                let config = InventoryConfig::default();

                match add_item_to_inventory(&mut updated_player, &item, 1, &config) {
                    InventoryResult::Added { .. } => {
                        store.put_player_async(updated_player).await?;
                    }
                    InventoryResult::Failed { reason } => {
                        return Ok(format!(
                            "Could not add {} to inventory: {}",
                            item.name, reason
                        ));
                    }
                    _ => {} // Other result types not applicable
                }

                // Save updated housing instance
                store.put_housing_instance(&instance)?;

                return Ok(format!(
                    "You reclaim {} from the reclaim box. It's now in your inventory.",
                    item.name
                ));
            }
        }

        Ok(format!(
            "'{}' not found in your reclaim box.",
            item_name_upper
        ))
    }

    /// Describe the current room (placeholder for Phase 3)
    async fn describe_current_room(&self, player: &PlayerRecord) -> Result<String> {
        match self.store().get_room(&player.current_room) {
            Ok(room) => {
                // Check if room is dark and player has no light source (Phase 4.3)
                let is_dark = room.flags.contains(&crate::tmush::types::RoomFlag::Dark);
                let has_light = self.player_has_light_source(player);

                if is_dark && !has_light {
                    // Dark room without light source - minimal description
                    return Ok("=== Darkness ===\n\
You are in pitch darkness. You can't see anything!\n\
You might need a light source to explore safely here.\n\
You can still navigate carefully, but you might miss important details.\n\n\
Obvious exits: (too dark to see clearly)"
                        .to_string());
                }

                let mut response = String::new();

                // Room name
                response.push_str(&format!("=== {} ===\n", room.name));

                // Room description
                response.push_str(&format!("{}\n\n", room.long_desc));

                // Show exits if any
                if !room.exits.is_empty() {
                    response.push_str("Obvious exits: ");
                    let mut exit_names: Vec<String> = room
                        .exits
                        .keys()
                        .map(|dir| format!("{:?}", dir).to_lowercase())
                        .collect();
                    exit_names.sort(); // Consistent ordering
                    response.push_str(&exit_names.join(", "));
                    response.push('\n');
                }

                // Show NPCs in room
                if let Ok(npcs) = self.store().get_npcs_in_room(&player.current_room) {
                    if !npcs.is_empty() {
                        response.push('\n');
                        for npc in &npcs {
                            response.push_str(&format!("🧑 {} is here.\n", npc.name));
                        }
                    }
                }

                // Show objects in room
                if !room.items.is_empty() {
                    response.push('\n');
                    response.push_str("Objects here:\n");
                    for object_id in &room.items {
                        if let Ok(object) = self.store().get_object(object_id) {
                            response.push_str(&format!("  📦 {}\n", object.name));
                        }
                    }
                }

                // Note: Tutorial hints are managed by handle_move() to avoid duplication
                // Tutorial progress is checked during movement and hint shown at end of response

                // Show other players (Phase 4 feature - placeholder for now)
                // response.push_str("Players here: (none visible)\n");

                Ok(response)
            }
            Err(_) => Ok(format!(
                "You are in a mysterious void (room '{}' not found).\nType WHERE for help.",
                player.current_room
            )),
        }
    }

    /// Main help text
    pub fn help_main(&self) -> String {
        "=TINYMUSH HELP=\n".to_string()
            + "Move: N/S/E/W/U/D + diagonals\n"
            + "Look: L | I (inv) | WHO | SCORE\n"
            + "Talk: SAY/EMOTE\n"
            + "Board: BOARD/POST/READ\n"
            + "Mail: MAIL/SEND\n"
            + "More: HELP <topic>\n"
            + "Topics: COMMANDS MOVEMENT SOCIAL BOARD MAIL"
    }

    /// Commands help
    pub fn help_commands(&self) -> String {
        "=COMMANDS=\n".to_string()
            + "L - look | I - inventory\n"
            + "WHO - players | SCORE - stats\n"
            + "SAY/EMOTE - talk\n"
            + "BOARD/POST/READ - bulletin\n"
            + "MAIL/SEND/RMAIL - messages\n"
            + "SAVE | QUIT"
    }
    /// Movement help
    pub fn help_movement(&self) -> String {
        "=MOVEMENT=\n".to_string()
            + "N/S/E/W - cardinal\n"
            + "U/D - up/down\n"
            + "NE/NW/SE/SW - diagonals\n"
            + "L - look around"
    }

    /// Social commands help  
    pub fn help_social(&self) -> String {
        "=SOCIAL=\n".to_string()
            + "SAY <txt> - speak aloud\n"
            + "WHISPER <plr> <txt> - private\n"
            + "EMOTE/: <act> - action\n"
            + "POSE/; <pose> - describe\n"
            + "OOC <txt> - out of char\n"
            + "WHO - list players"
    }

    /// Bulletin board help
    pub fn help_bulletin(&self) -> String {
        "=BULLETIN BOARD=\n".to_string()
            + "Town Stump message board\n"
            + "BOARD - view messages\n"
            + "POST <subj> <msg> - post\n"
            + "READ <id> - read\n"
            + "Use at Town Square\n"
            + "Max: 50 char subj, 300 msg"
    }

    /// Companion commands help
    pub fn help_companion(&self) -> String {
        "=COMPANIONS=\n".to_string()
            + "COMP [LIST] - your pets\n"
            + "COMP TAME <name> - claim\n"
            + "COMP <name> - status\n"
            + "COMP RELEASE <name> - free\n"
            + "COMP STAY/COME - control\n"
            + "COMP INV - storage\n"
            + "FEED/PET <name> - care\n"
            + "MOUNT/DISMOUNT - riding\n"
            + "TRAIN <name> <skill> - teach"
    }

    /// Handle MAIL command - view mail folders
    async fn handle_mail(
        &mut self,
        session: &Session,
        folder: Option<String>,
        _config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        let folder = folder.unwrap_or_else(|| "inbox".to_string());

        // Only support inbox and sent folders for now
        if folder != "inbox" && folder != "sent" {
            return Ok("Available folders: MAIL inbox, MAIL sent".to_string());
        }

        // Get mail messages
        let messages = match self.store().list_mail(&folder, &player.username, 0, 10) {
            Ok(messages) => messages,
            Err(e) => return Ok(format!("Error reading mail: {}", e)),
        };

        let mut response = format!("=== {} MAIL ===\n", folder.to_uppercase());

        if messages.is_empty() {
            response.push_str("No mail messages.\n");
        } else {
            for (i, msg) in messages.iter().enumerate() {
                let status = if msg.status == crate::tmush::types::MailStatus::Unread {
                    "*"
                } else {
                    " "
                };
                let date = msg.sent_at.format("%m/%d");
                let sender_recipient = if folder == "inbox" {
                    &msg.sender
                } else {
                    &msg.recipient
                };

                response.push_str(&format!(
                    "{} [{}] {} - {} ({})\n",
                    status, msg.id, msg.subject, sender_recipient, date
                ));

                // Limit to fit in 200 bytes
                if response.len() > 150 {
                    let remaining = messages.len() - i - 1;
                    if remaining > 0 {
                        response.push_str(&format!("... and {} more.\n", remaining));
                    }
                    break;
                }
            }
        }

        response.push_str(
            "\nRMAIL <id> to read, DMAIL <id> to delete\nSEND <player> <subject> <message> to send",
        );
        Ok(response)
    }

    /// Handle SEND command - send mail to another player
    async fn handle_send(
        &mut self,
        session: &Session,
        recipient: String,
        subject: String,
        message: String,
        _config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Validate input
        if recipient.trim().is_empty() {
            return Ok(
                "Recipient cannot be empty.\nUsage: SEND <player> <subject> <message>".to_string(),
            );
        }

        if subject.trim().is_empty() {
            return Ok(
                "Subject cannot be empty.\nUsage: SEND <player> <subject> <message>".to_string(),
            );
        }

        if message.trim().is_empty() {
            return Ok(
                "Message cannot be empty.\nUsage: SEND <player> <subject> <message>".to_string(),
            );
        }

        if subject.len() > 50 {
            return Ok("Subject too long (max 50 characters).".to_string());
        }

        if message.len() > 200 {
            return Ok("Message too long (max 200 characters).".to_string());
        }

        // Check if recipient exists (basic validation)
        let recipient_lower = recipient.to_ascii_lowercase();
        match self.store().get_player(&recipient_lower) {
            Ok(_) => {} // Player exists
            Err(TinyMushError::NotFound(_)) => {
                return Ok(format!(
                    "Player '{}' not found.\nMake sure they have logged in at least once.",
                    recipient
                ));
            }
            Err(e) => return Ok(format!("Error checking recipient: {}", e)),
        }

        // Create the mail message
        let mail = crate::tmush::types::MailMessage::new(
            &player.username,
            &recipient_lower,
            &subject,
            &message,
        );

        // Send the message
        match self.store().send_mail_async(mail).await {
            Ok(message_id) => {
                // Enforce mail quota for recipient
                let _ = self.store().enforce_mail_quota(&recipient_lower, 100);

                Ok(format!(
                    "Mail sent to {}.\nMessage ID: {} - '{}'\nThey can read it with: RMAIL {}",
                    recipient, message_id, subject, message_id
                ))
            }
            Err(e) => Ok(format!("Failed to send mail: {}", e)),
        }
    }

    /// Handle RMAIL command - read specific mail message
    async fn handle_read_mail(
        &mut self,
        session: &Session,
        message_id: u64,
        _config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Try to find the message in inbox first, then sent
        let message = match self
            .store()
            .get_mail_async("inbox", &player.username, message_id)
            .await
        {
            Ok(msg) => {
                // Mark as read if it's in the inbox
                let _ = self
                    .store()
                    .mark_mail_read_async("inbox", &player.username, message_id)
                    .await;
                msg
            }
            Err(TinyMushError::NotFound(_)) => {
                // Try sent folder
                match self
                    .store()
                    .get_mail_async("sent", &player.username, message_id)
                    .await
                {
                    Ok(msg) => msg,
                    Err(TinyMushError::NotFound(_)) => {
                        return Ok(format!(
                            "No mail message with ID {}.\nUse MAIL to see available messages.",
                            message_id
                        ));
                    }
                    Err(e) => return Ok(format!("Error reading mail: {}", e)),
                }
            }
            Err(e) => return Ok(format!("Error reading mail: {}", e)),
        };

        let date = message.sent_at.format("%b %d, %Y at %H:%M");
        let mut response = format!(
            "=== Mail {} ===\n\
            From: {}\n\
            To: {}\n\
            Subject: {}\n\
            Date: {}\n\n\
            {}",
            message.id, message.sender, message.recipient, message.subject, date, message.body
        );

        // Ensure we stay under 200 bytes
        if response.len() > 200 {
            response.truncate(197);
            response.push_str("...");
        }

        Ok(response)
    }

    /// Handle DMAIL command - delete mail message
    async fn handle_delete_mail(
        &mut self,
        session: &Session,
        message_id: u64,
        _config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Try to delete from inbox first, then sent
        match self
            .store()
            .delete_mail_async("inbox", &player.username, message_id)
            .await
        {
            Ok(()) => Ok(format!("Mail message {} deleted from inbox.", message_id)),
            Err(TinyMushError::NotFound(_)) => {
                // Try sent folder
                match self
                    .store()
                    .delete_mail_async("sent", &player.username, message_id)
                    .await
                {
                    Ok(()) => Ok(format!(
                        "Mail message {} deleted from sent folder.",
                        message_id
                    )),
                    Err(TinyMushError::NotFound(_)) => Ok(format!(
                        "No mail message with ID {}.\nUse MAIL to see available messages.",
                        message_id
                    )),
                    Err(e) => Ok(format!("Error deleting mail: {}", e)),
                }
            }
            Err(e) => Ok(format!("Error deleting mail: {}", e)),
        }
    }

    // ===== BANKING COMMANDS (Phase 5 Week 4) =====

    /// Handle BALANCE command - show pocket and bank balance
    async fn handle_balance(&mut self, session: &Session, _config: &Config) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        let pocket_display = format!("{:?}", player.currency);
        let bank_display = format!("{:?}", player.banked_currency);

        Ok(format!(
            "=ACCOUNT BALANCE=\nPocket: {}\nBank: {}",
            pocket_display, bank_display
        ))
    }

    /// Handle DEPOSIT command - deposit currency to bank
    async fn handle_deposit(
        &mut self,
        session: &Session,
        amount_str: String,
        _config: &Config,
    ) -> Result<String> {
        let world_config = self.get_world_config().await?;

        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Parse amount as integer (base units)
        let base_units: i64 = match amount_str.parse() {
            Ok(units) => units,
            Err(_) => return Ok("Invalid amount format.\nExample: DEPOSIT 100".to_string()),
        };

        if base_units <= 0 {
            return Ok("Amount must be positive.".to_string());
        }

        // Create CurrencyAmount matching player's currency type
        use crate::tmush::types::CurrencyAmount;
        let amount = match player.currency {
            CurrencyAmount::Decimal { .. } => CurrencyAmount::Decimal {
                minor_units: base_units,
            },
            CurrencyAmount::MultiTier { .. } => CurrencyAmount::MultiTier { base_units },
        };

        // Perform deposit via storage
        match self.store().bank_deposit(&player.username, &amount) {
            Ok(_) => Ok(world_config
                .msg_deposit_success
                .replace("{amount}", &format!("{:?}", amount))),
            Err(e) => Ok(format!("Deposit failed: {}", e)),
        }
    }

    /// Handle WITHDRAW command - withdraw currency from bank
    async fn handle_withdraw(
        &mut self,
        session: &Session,
        amount_str: String,
        _config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Parse amount as integer (base units)
        let base_units: i64 = match amount_str.parse() {
            Ok(units) => units,
            Err(_) => return Ok("Invalid amount format.\nExample: WITHDRAW 100".to_string()),
        };

        if base_units <= 0 {
            return Ok("Amount must be positive.".to_string());
        }

        // Create CurrencyAmount matching player's currency type
        use crate::tmush::types::CurrencyAmount;
        let amount = match player.currency {
            CurrencyAmount::Decimal { .. } => CurrencyAmount::Decimal {
                minor_units: base_units,
            },
            CurrencyAmount::MultiTier { .. } => CurrencyAmount::MultiTier { base_units },
        };

        // Perform withdrawal via storage
        match self.store().bank_withdraw(&player.username, &amount) {
            Ok(_) => Ok(format!(
                "Withdrew {:?} from bank.\nUse BALANCE to check your account.",
                amount
            )),
            Err(e) => Ok(format!("Withdrawal failed: {}", e)),
        }
    }

    /// Handle BTRANSFER command - transfer currency between players via bank
    async fn handle_bank_transfer(
        &mut self,
        session: &Session,
        recipient: String,
        amount_str: String,
        _config: &Config,
    ) -> Result<String> {
        let player = match self.get_or_create_player(session).await {
            Ok(player) => player,
            Err(e) => return Ok(format!("Error loading player: {}", e)),
        };

        // Can't transfer to self
        if recipient.to_lowercase() == player.username.to_lowercase() {
            return Ok("You can't transfer to yourself!".to_string());
        }

        // Check recipient exists
        let recipient_player = match self.store().get_player(&recipient) {
            Ok(p) => p,
            Err(TinyMushError::NotFound(_)) => {
                return Ok(format!("Player '{}' not found.", recipient))
            }
            Err(e) => return Ok(format!("Error loading recipient: {}", e)),
        };

        // Parse amount as integer (base units)
        let base_units: i64 = match amount_str.parse() {
            Ok(units) => units,
            Err(_) => return Ok("Invalid amount format.\nExample: BTRANSFER alice 100".to_string()),
        };

        if base_units <= 0 {
            return Ok("Amount must be positive.".to_string());
        }

        // Create CurrencyAmount matching player's currency type
        use crate::tmush::types::CurrencyAmount;
        let amount = match player.currency {
            CurrencyAmount::Decimal { .. } => CurrencyAmount::Decimal {
                minor_units: base_units,
            },
            CurrencyAmount::MultiTier { .. } => CurrencyAmount::MultiTier { base_units },
        };

        // Check sender has enough in bank
        if !player.banked_currency.can_afford(&amount) {
            return Ok(format!(
                "Insufficient bank funds.\nYou have: {:?}",
                player.banked_currency
            ));
        }

        // Perform bank-to-bank transfer by manually handling both players
        // First, withdraw from sender's bank
        let mut sender = self.store().get_player(&player.username)?;
        sender.banked_currency = match sender.banked_currency.subtract(&amount) {
            Ok(new_balance) => new_balance,
            Err(e) => return Ok(format!("Transfer failed: {}", e)),
        };

        // Then deposit to recipient's bank
        let mut recipient = self.store().get_player(&recipient_player.username)?;
        recipient.banked_currency = match recipient.banked_currency.add(&amount) {
            Ok(new_balance) => new_balance,
            Err(e) => return Ok(format!("Transfer failed: {}", e)),
        };

        // Save both players
        self.store().put_player(sender)?;
        self.store().put_player(recipient)?;

        // Log the transaction
        use crate::tmush::types::{CurrencyTransaction, TransactionReason};
        use chrono::Utc;
        let _transaction = CurrencyTransaction {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: Some(format!("{}:bank", player.username)),
            to: Some(format!("{}:bank", recipient_player.username)),
            amount: amount.clone(),
            reason: TransactionReason::Trade,
            rolled_back: false,
        };
        // Note: log_transaction is private, so we'll just skip logging for now
        // TODO: Make log_transaction public or provide a public logging method

        Ok(format!(
            "Transferred {:?} to {}'s bank account.",
            amount, recipient_player.username
        ))
    }

    // ===== TRADING COMMANDS (Phase 5 Week 4) =====

    /// Handle TRADE command - initiate trade with another player
    async fn handle_trade(
        &mut self,
        session: &Session,
        target: String,
        _config: &Config,
    ) -> Result<String> {
        let world_config = self.get_world_config().await?;
        let username = session.node_id.to_string();
        let target_lower = target.to_ascii_lowercase();

        // Can't trade with yourself
        if target_lower == username.to_ascii_lowercase() {
            return Ok("You can't trade with yourself!".to_string());
        }

        // Check if initiator already has an active trade
        if let Some(existing) = self.store().get_player_active_trade(&username)? {
            let other = if existing.player1.eq_ignore_ascii_case(&username) {
                &existing.player2
            } else {
                &existing.player1
            };
            return Ok(format!(
                "You're already trading with {}!\nType REJECT to cancel.",
                other
            ));
        }

        // Check if target player has an active trade
        if let Some(_existing) = self.store().get_player_active_trade(&target)? {
            return Ok(format!("{} is already in a trade.", target));
        }

        // Get both players to verify they exist
        let player1 = self.store().get_player(&username)?;
        let player2 = self.store().get_player(&target)?;

        // Verify both players are in the same room
        if player1.current_room != player2.current_room {
            return Ok(format!("{} is not here!", target));
        }

        // Create new trade session
        let trade_session = crate::tmush::types::TradeSession::new(&username, &target);
        self.store().put_trade_session(&trade_session)?;

        Ok(world_config
            .msg_trade_initiated
            .replace("{target}", &target))
    }

    /// Handle OFFER command - offer item or currency in active trade
    async fn handle_offer(
        &mut self,
        session: &Session,
        offer_text: String,
        _config: &Config,
    ) -> Result<String> {
        let username = session.node_id.to_string();

        // Get active trade session
        let mut trade = match self.store().get_player_active_trade(&username)? {
            Some(t) => t,
            None => {
                return Ok(
                    "You have no active trade.\nUse TRADE <player> to start one.".to_string(),
                )
            }
        };

        // Check if trade expired
        if trade.is_expired() {
            self.store().delete_trade_session(&trade.id)?;
            return Ok("Trade expired!".to_string());
        }

        let offer_trimmed = offer_text.trim();

        // Try to parse as currency amount (simple integer for base units)
        if let Ok(amount) = offer_trimmed.parse::<i64>() {
            if amount <= 0 {
                return Ok("Amount must be positive!".to_string());
            }

            // Get player's currency to match type
            let player = self.store().get_player(&username)?;

            // Create currency amount of same type as player has
            let currency_offer = match player.currency {
                crate::tmush::types::CurrencyAmount::Decimal { .. } => {
                    crate::tmush::types::CurrencyAmount::Decimal {
                        minor_units: amount,
                    }
                }
                crate::tmush::types::CurrencyAmount::MultiTier { .. } => {
                    crate::tmush::types::CurrencyAmount::MultiTier { base_units: amount }
                }
            };

            // Verify player can afford this
            if !player.currency.can_afford(&currency_offer) {
                return Ok("You don't have that much!".to_string());
            }

            // Add to trade
            trade.add_currency_offer(&username, currency_offer.clone());
            self.store().put_trade_session(&trade)?;

            return Ok(format!(
                "Added {:?} to trade.\nType ACCEPT when ready.",
                currency_offer
            ));
        }

        // Otherwise treat as item name
        let player = self.store().get_player(&username)?;
        let offer_lower = offer_trimmed.to_ascii_lowercase();

        // Check if player has this item
        let has_item = player
            .inventory_stacks
            .iter()
            .any(|stack| stack.object_id.to_ascii_lowercase() == offer_lower);

        if !has_item {
            return Ok(format!("You don't have '{}'!", offer_text));
        }

        // Add item to trade
        trade.add_item_offer(&username, offer_trimmed.to_string());
        self.store().put_trade_session(&trade)?;

        Ok(format!(
            "Added '{}' to trade.\nType ACCEPT when ready.",
            offer_trimmed
        ))
    }

    /// Handle ACCEPT command - accept trade
    async fn handle_accept(&mut self, session: &Session, _config: &Config) -> Result<String> {
        let username = session.node_id.to_string();

        // Get active trade session
        let mut trade = match self.store().get_player_active_trade(&username)? {
            Some(t) => t,
            None => return Ok("You have no active trade.".to_string()),
        };

        // Check if trade expired
        if trade.is_expired() {
            self.store().delete_trade_session(&trade.id)?;
            return Ok("Trade expired!".to_string());
        }

        // Mark this player as accepted
        trade.accept(&username);

        // If both players have accepted, execute the trade
        if trade.is_ready() {
            // Execute the atomic swap
            match self.execute_trade(&trade).await {
                Ok(()) => {
                    // Trade successful - delete session and notify
                    self.store().delete_trade_session(&trade.id)?;

                    let summary = trade.get_summary();
                    Ok(format!("Trade complete!\n{}", summary))
                }
                Err(e) => {
                    // Trade failed - delete session and return error
                    self.store().delete_trade_session(&trade.id)?;
                    Ok(format!("Trade failed: {}", e))
                }
            }
        } else {
            // Not both accepted yet - save updated session and wait
            self.store().put_trade_session(&trade)?;

            Ok("You accepted the trade.\nWaiting for other player...".to_string())
        }
    }

    /// Handle REJECT command - reject/cancel trade
    async fn handle_reject(&mut self, session: &Session, _config: &Config) -> Result<String> {
        let username = session.node_id.to_string();

        // Get active trade session
        let trade = match self.store().get_player_active_trade(&username)? {
            Some(t) => t,
            None => return Ok("You have no active trade.".to_string()),
        };

        // Get the other player's name
        let other_player = if trade.player1.eq_ignore_ascii_case(&username) {
            &trade.player2
        } else {
            &trade.player1
        };

        // Delete the trade session
        self.store().delete_trade_session(&trade.id)?;

        Ok(format!("Trade with {} cancelled.", other_player))
    }

    /// Handle THISTORY command - view trade history
    async fn handle_trade_history(
        &mut self,
        session: &Session,
        _config: &Config,
    ) -> Result<String> {
        let username = session.node_id.to_string();

        // Get last 20 transactions for this player
        let transactions = self.store().get_player_transactions(&username, 20)?;

        // Filter for trades only
        let trade_txns: Vec<_> = transactions
            .iter()
            .filter(|tx| matches!(tx.reason, crate::tmush::types::TransactionReason::Trade))
            .take(10)
            .collect();

        if trade_txns.is_empty() {
            return Ok("No trade history.".to_string());
        }

        let mut output = "=TRADE HISTORY=\n".to_string();
        for tx in trade_txns {
            let timestamp = tx.timestamp.format("%m/%d %H:%M");

            // Determine direction and other party
            let (direction, other_party) = match (&tx.from, &tx.to) {
                (Some(from), Some(to)) => {
                    if from.eq_ignore_ascii_case(&username) {
                        ("->", to.as_str())
                    } else {
                        ("<-", from.as_str())
                    }
                }
                _ => ("??", "?"),
            };

            output.push_str(&format!(
                "{} {} {} {:?}\n",
                timestamp, direction, other_party, tx.amount
            ));
        }

        Ok(output)
    }

    /// Execute a two-phase commit trade between players (atomic swap)
    async fn execute_trade(&mut self, trade: &crate::tmush::types::TradeSession) -> Result<()> {
        // Phase 1: Validate both players can complete the trade
        let mut player1 = self.store().get_player(&trade.player1)?;
        let mut player2 = self.store().get_player(&trade.player2)?;

        // Validate player1 can afford their currency offer
        if !player1.currency.can_afford(&trade.player1_currency) {
            return Err(TinyMushError::InsufficientFunds.into());
        }

        // Validate player2 can afford their currency offer
        if !player2.currency.can_afford(&trade.player2_currency) {
            return Err(TinyMushError::InsufficientFunds.into());
        }

        // Validate player1 has all offered items
        for item_id in &trade.player1_items {
            if !player1
                .inventory_stacks
                .iter()
                .any(|s| &s.object_id == item_id)
            {
                return Err(TinyMushError::NotFound(format!(
                    "{} no longer has {}",
                    trade.player1, item_id
                ))
                .into());
            }
        }

        // Validate player2 has all offered items
        for item_id in &trade.player2_items {
            if !player2
                .inventory_stacks
                .iter()
                .any(|s| &s.object_id == item_id)
            {
                return Err(TinyMushError::NotFound(format!(
                    "{} no longer has {}",
                    trade.player2, item_id
                ))
                .into());
            }
        }

        // Phase 2: Execute atomic swap

        // Swap currency (if any)
        if !trade.player1_currency.is_zero_or_negative()
            || !trade.player2_currency.is_zero_or_negative()
        {
            // Player1 gives currency, receives currency
            player1.currency = player1
                .currency
                .subtract(&trade.player1_currency)
                .map_err(|e| {
                    TinyMushError::InvalidCurrency(format!("P1 currency subtract failed: {}", e))
                })?;
            player1.currency = player1.currency.add(&trade.player2_currency).map_err(|e| {
                TinyMushError::InvalidCurrency(format!("P1 currency add failed: {}", e))
            })?;

            // Player2 gives currency, receives currency
            player2.currency = player2
                .currency
                .subtract(&trade.player2_currency)
                .map_err(|e| {
                    TinyMushError::InvalidCurrency(format!("P2 currency subtract failed: {}", e))
                })?;
            player2.currency = player2.currency.add(&trade.player1_currency).map_err(|e| {
                TinyMushError::InvalidCurrency(format!("P2 currency add failed: {}", e))
            })?;
        }

        // Swap items
        // Player1 gives items to Player2
        for item_id in &trade.player1_items {
            player1.inventory_stacks.retain(|s| &s.object_id != item_id);
            player2
                .inventory_stacks
                .push(crate::tmush::types::ItemStack {
                    object_id: item_id.clone(),
                    quantity: 1,
                    added_at: chrono::Utc::now(),
                });
        }

        // Player2 gives items to Player1
        for item_id in &trade.player2_items {
            player2.inventory_stacks.retain(|s| &s.object_id != item_id);
            player1
                .inventory_stacks
                .push(crate::tmush::types::ItemStack {
                    object_id: item_id.clone(),
                    quantity: 1,
                    added_at: chrono::Utc::now(),
                });
        }

        // Save both players (atomic commit point)
        self.store().put_player(player1)?;
        self.store().put_player(player2)?;

        // Transaction logging is skipped for P2P trades since we manually updated both players
        // The currency swap is complete and atomic at this point

        Ok(())
    }

    /// Mail system help
    pub fn help_mail(&self) -> String {
        "=MAIL SYSTEM=\n".to_string()
            + "MAIL [folder] - inbox/sent\n"
            + "SEND <plr> <subj> <msg>\n"
            + "RMAIL <id> - read\n"
            + "DMAIL <id> - delete\n"
            + "* = unread\n"
            + "Max: 50 subj, 200 msg"
    }
}

/// Integration with main command processor
pub async fn handle_tinymush_command(
    session: &mut Session,
    command: &str,
    storage: &mut Storage,
    config: &Config,
) -> Result<String> {
    // For direct routing, open a temporary store
    // In production, this function should receive the shared store from the server
    let db_path = config
        .games
        .tinymush_db_path
        .as_deref()
        .unwrap_or("data/tinymush");
    let store = TinyMushStore::open(db_path)
        .map_err(|e| anyhow::anyhow!("Failed to open TinyMUSH store: {}", e))?;

    let mut processor = TinyMushProcessor::new(store);
    processor
        .process_command(session, command, storage, config)
        .await
}

/// Check if we should route to TinyMUSH based on session state
pub fn should_route_to_tinymush(session: &Session) -> bool {
    session.current_game_slug.as_deref() == Some("tinymush")
}

// ============================================================================
// Helper Functions for Builder Commands
// ============================================================================

/// Parse a direction string into a Direction enum
fn parse_direction_string(s: &str) -> Option<crate::tmush::types::Direction> {
    use crate::tmush::types::Direction::*;
    match s.to_lowercase().as_str() {
        "n" | "north" => Some(North),
        "s" | "south" => Some(South),
        "e" | "east" => Some(East),
        "w" | "west" => Some(West),
        "u" | "up" => Some(Up),
        "d" | "down" => Some(Down),
        "ne" | "northeast" => Some(Northeast),
        "nw" | "northwest" => Some(Northwest),
        "se" | "southeast" => Some(Southeast),
        "sw" | "southwest" => Some(Southwest),
        _ => None,
    }
}

/// Get the reverse of a direction (for bidirectional exits)
fn get_reverse_direction(dir: &crate::tmush::types::Direction) -> crate::tmush::types::Direction {
    use crate::tmush::types::Direction::*;
    match dir {
        North => South,
        South => North,
        East => West,
        West => East,
        Up => Down,
        Down => Up,
        Northeast => Southwest,
        Northwest => Southeast,
        Southeast => Northwest,
        Southwest => Northeast,
    }
}

/// Format a direction for display
fn format_direction(dir: &crate::tmush::types::Direction) -> String {
    use crate::tmush::types::Direction::*;
    match dir {
        North => "north",
        South => "south",
        East => "east",
        West => "west",
        Up => "up",
        Down => "down",
        Northeast => "northeast",
        Northwest => "northwest",
        Southeast => "southeast",
        Southwest => "southwest",
    }
    .to_string()
}

/// Parse a room flag string into a RoomFlag enum
fn parse_room_flag(s: &str) -> Option<crate::tmush::types::RoomFlag> {
    use crate::tmush::types::RoomFlag::*;
    match s.to_lowercase().as_str() {
        "safe" => Some(Safe),
        "dark" => Some(Dark),
        "indoor" => Some(Indoor),
        "shop" => Some(Shop),
        "questlocation" => Some(QuestLocation),
        "pvpenabled" => Some(PvpEnabled),
        "playercreated" => Some(PlayerCreated),
        "private" => Some(Private),
        "moderated" => Some(Moderated),
        "instanced" => Some(Instanced),
        "crowded" => Some(Crowded),
        "housingoffice" => Some(HousingOffice),
        "noteleportout" => Some(NoTeleportOut),
        _ => None,
    }
}

/// Parse an object flag string into an ObjectFlag enum
fn parse_object_flag(s: &str) -> Option<crate::tmush::types::ObjectFlag> {
    use crate::tmush::types::ObjectFlag::*;
    match s.to_lowercase().as_str() {
        "questitem" => Some(QuestItem),
        "consumable" => Some(Consumable),
        "equipment" => Some(Equipment),
        "keyitem" => Some(KeyItem),
        "container" => Some(Container),
        "magical" => Some(Magical),
        "companion" => Some(Companion),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmush::storage::TinyMushStore;

    fn create_test_store(test_name: &str) -> TinyMushStore {
        // Create a unique temporary store for each test
        let temp_dir = std::env::temp_dir().join(format!(
            "tinymush_test_{}_{}",
            std::process::id(),
            test_name
        ));
        // Clean up if it exists
        let _ = std::fs::remove_dir_all(&temp_dir);
        TinyMushStore::open(&temp_dir).expect("Failed to create test store")
    }

    #[test]
    fn test_command_parsing() {
        let store = create_test_store("command_parsing");
        let processor = TinyMushProcessor::new(store);

        // Movement commands
        assert_eq!(
            processor.parse_command("n"),
            TinyMushCommand::Move(Direction::North)
        );
        assert_eq!(
            processor.parse_command("NORTH"),
            TinyMushCommand::Move(Direction::North)
        );
        assert_eq!(
            processor.parse_command("ne"),
            TinyMushCommand::Move(Direction::Northeast)
        );

        // Look commands
        assert_eq!(processor.parse_command("l"), TinyMushCommand::Look(None));
        assert_eq!(
            processor.parse_command("look sword"),
            TinyMushCommand::Look(Some("SWORD".to_string()))
        );

        // Social commands - now preserve case
        assert_eq!(
            processor.parse_command("say hello"),
            TinyMushCommand::Say("hello".to_string())
        );
        assert_eq!(
            processor.parse_command("' hello world"),
            TinyMushCommand::Say("hello world".to_string())
        );

        // System commands
        assert_eq!(processor.parse_command("help"), TinyMushCommand::Help(None));
        assert_eq!(
            processor.parse_command("help commands"),
            TinyMushCommand::Help(Some("COMMANDS".to_string()))
        );
        assert_eq!(processor.parse_command("quit"), TinyMushCommand::Quit);

        // Unknown commands
        assert_eq!(
            processor.parse_command("frobozz"),
            TinyMushCommand::Unknown("FROBOZZ".to_string())
        );
    }

    #[test]
    fn test_direction_parsing() {
        let store = create_test_store("direction_parsing");
        let processor = TinyMushProcessor::new(store);

        assert_eq!(
            processor.parse_command("n"),
            TinyMushCommand::Move(Direction::North)
        );
        assert_eq!(
            processor.parse_command("s"),
            TinyMushCommand::Move(Direction::South)
        );
        assert_eq!(
            processor.parse_command("e"),
            TinyMushCommand::Move(Direction::East)
        );
        assert_eq!(
            processor.parse_command("w"),
            TinyMushCommand::Move(Direction::West)
        );
        assert_eq!(
            processor.parse_command("u"),
            TinyMushCommand::Move(Direction::Up)
        );
        assert_eq!(
            processor.parse_command("d"),
            TinyMushCommand::Move(Direction::Down)
        );
        assert_eq!(
            processor.parse_command("ne"),
            TinyMushCommand::Move(Direction::Northeast)
        );
        assert_eq!(
            processor.parse_command("nw"),
            TinyMushCommand::Move(Direction::Northwest)
        );
        assert_eq!(
            processor.parse_command("se"),
            TinyMushCommand::Move(Direction::Southeast)
        );
        assert_eq!(
            processor.parse_command("sw"),
            TinyMushCommand::Move(Direction::Southwest)
        );
    }

    #[test]
    fn test_empty_input() {
        let store = create_test_store("empty_input");
        let processor = TinyMushProcessor::new(store);
        assert_eq!(
            processor.parse_command(""),
            TinyMushCommand::Unknown("".to_string())
        );
        assert_eq!(
            processor.parse_command("   "),
            TinyMushCommand::Unknown("".to_string())
        );
    }
}
