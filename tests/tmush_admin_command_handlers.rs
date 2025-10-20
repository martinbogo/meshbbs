use meshbbs::bbs::session::{Session, SessionState};
use meshbbs::config::{
    AdminDashboardConfig, BbsConfig, Config, LoggingConfig, MeshtasticConfig, StorageConfig,
};
use meshbbs::tmush::commands::TinyMushProcessor;
/// Integration tests for TinyMUSH admin command handlers
/// Tests: @ADMIN, @SETADMIN, @REMOVEADMIN, @ADMINS commands
use meshbbs::tmush::{PlayerRecord, TinyMushStoreBuilder};
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

fn tinymush_path(data_dir: &str) -> PathBuf {
    PathBuf::from(data_dir).join("tinymush")
}

fn test_config(data_dir: String) -> Config {
    let tinymush_path_str = tinymush_path(&data_dir).to_str().unwrap().to_string();
    Config {
        bbs: BbsConfig {
            name: "Test".into(),
            sysop: "sysop".into(),
            location: "Test Location".into(),
            description: "Test BBS".into(),
            max_users: 10,
            session_timeout: 10,
            welcome_message: "Welcome".into(),
            sysop_password_hash: None,
            public_command_prefix: None,
            allow_public_login: true,
            help_command: "HELP".to_string(),
        },
        meshtastic: MeshtasticConfig {
            port: "".into(),
            baud_rate: 115200,
            node_id: "".into(),
            channel: 0,
            min_send_gap_ms: None,
            dm_resend_backoff_seconds: None,
            post_dm_broadcast_gap_ms: None,
            dm_to_dm_gap_ms: None,
            help_broadcast_delay_ms: None,
            scheduler_max_queue: None,
            scheduler_aging_threshold_ms: None,
            scheduler_stats_interval_ms: None,
            require_device_at_startup: false,
        },
        storage: StorageConfig {
            data_dir,
            max_message_size: 1024,
            show_chunk_markers: false,
        },
        message_topics: HashMap::new(),
        logging: LoggingConfig {
            level: "error".into(),
            file: None,
            security_file: None,
        },
        security: None,
        ident_beacon: Default::default(),
        weather: Default::default(),
        games: meshbbs::config::GamesConfig {
            tinyhack_enabled: false,
            tinymush_enabled: true,
            tinymush_db_path: Some(tinymush_path_str),
        },
        welcome: Default::default(),
        admin_dashboard: AdminDashboardConfig::default(),
    }
}

async fn test_session(username: &str) -> Session {
    let mut session = Session::new(
        format!("{}_session", username),
        format!("{}_node", username),
    );
    session.login(username.to_string(), 1).await.unwrap();
    session.state = SessionState::TinyMush;
    session
}

#[tokio::test]
async fn test_admin_command_shows_status() {
    let dir = TempDir::new().expect("tempdir");
    let config = test_config(dir.path().to_str().unwrap().to_string());

    // Store will auto-create admin account
    let store = TinyMushStoreBuilder::new(&tinymush_path(dir.path().to_str().unwrap()))
        .open()
        .expect("store");

    // Verify admin exists
    let admin = store.get_player("admin").expect("admin exists");
    assert!(admin.is_admin());

    // Test @ADMIN command
    let mut session = test_session("admin").await;
    let mut processor = TinyMushProcessor::new(store.clone());
    let result = processor
        .process_command(
            &mut session,
            "@ADMIN",
            &mut meshbbs::storage::Storage::new(&config.storage.data_dir)
                .await
                .unwrap(),
            &config,
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();

    assert!(output.contains("ADMIN STATUS"));
    assert!(output.contains("Admin Status: ACTIVE"));
    assert!(output.contains("Admin Level: 3"));
    assert!(output.contains("Sysop"));
}

#[tokio::test]
async fn test_admin_command_non_admin() {
    let dir = TempDir::new().expect("tempdir");
    let config = test_config(dir.path().to_str().unwrap().to_string());

    let store = TinyMushStoreBuilder::new(&tinymush_path(dir.path().to_str().unwrap()))
        .open()
        .expect("store");

    // Create regular user
    let user = PlayerRecord::new("alice", "Alice", "town_square");
    store.put_player(user).expect("save user");

    // Test
    let mut session = test_session("alice").await;
    let mut processor = TinyMushProcessor::new(store.clone());
    let result = processor
        .process_command(
            &mut session,
            "@ADMIN",
            &mut meshbbs::storage::Storage::new(&config.storage.data_dir)
                .await
                .unwrap(),
            &config,
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("NOT ADMIN"));
    assert!(output.contains("do not have administrative privileges"));
}

#[tokio::test]
async fn test_setadmin_grants_privileges() {
    let dir = TempDir::new().expect("tempdir");
    let config = test_config(dir.path().to_str().unwrap().to_string());

    let store = TinyMushStoreBuilder::new(&tinymush_path(dir.path().to_str().unwrap()))
        .open()
        .expect("store");

    // Create regular user
    let user = PlayerRecord::new("bob", "Bob", "town_square");
    store.put_player(user).expect("save user");

    // Verify bob is not admin
    assert!(!store.is_admin("bob").unwrap());

    // Admin grants privileges to bob
    let mut session = test_session("admin").await;
    let mut processor = TinyMushProcessor::new(store.clone());
    let result = processor
        .process_command(
            &mut session,
            "@SETADMIN bob 2",
            &mut meshbbs::storage::Storage::new(&config.storage.data_dir)
                .await
                .unwrap(),
            &config,
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("SUCCESS"));
    assert!(output.contains("bob"));
    assert!(output.contains("Admin"));

    // Verify bob is now admin (reuse existing store)
    assert!(store.is_admin("bob").unwrap());
    let bob = store.get_player("bob").unwrap();
    assert_eq!(bob.admin_level(), 2);
}

#[tokio::test]
async fn test_setadmin_requires_admin() {
    let dir = TempDir::new().expect("tempdir");
    let config = test_config(dir.path().to_str().unwrap().to_string());

    let store = TinyMushStoreBuilder::new(&tinymush_path(dir.path().to_str().unwrap()))
        .open()
        .expect("store");

    // Create two regular users
    let alice = PlayerRecord::new("alice", "Alice", "town_square");
    let bob = PlayerRecord::new("bob", "Bob", "town_square");
    store.put_player(alice).expect("save alice");
    store.put_player(bob).expect("save bob");

    // Alice (non-admin) tries to grant admin to bob
    let mut session = test_session("alice").await;
    let mut processor = TinyMushProcessor::new(store.clone());
    let result = processor
        .process_command(
            &mut session,
            "@SETADMIN bob 1",
            &mut meshbbs::storage::Storage::new(&config.storage.data_dir)
                .await
                .unwrap(),
            &config,
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("Permission denied"));

    // Verify bob is still not admin (reuse existing store)
    assert!(!store.is_admin("bob").unwrap());
}

#[tokio::test]
async fn test_removeadmin_revokes_privileges() {
    let dir = TempDir::new().expect("tempdir");
    let config = test_config(dir.path().to_str().unwrap().to_string());

    let store = TinyMushStoreBuilder::new(&tinymush_path(dir.path().to_str().unwrap()))
        .open()
        .expect("store");

    // Create admin user
    let mut charlie = PlayerRecord::new("charlie", "Charlie", "town_square");
    charlie.grant_admin(2);
    store.put_player(charlie).expect("save charlie");

    // Verify charlie is admin
    assert!(store.is_admin("charlie").unwrap());

    // Sysop revokes charlie's admin
    let mut session = test_session("admin").await;
    let mut processor = TinyMushProcessor::new(store.clone());
    let result = processor
        .process_command(
            &mut session,
            "@REMOVEADMIN charlie",
            &mut meshbbs::storage::Storage::new(&config.storage.data_dir)
                .await
                .unwrap(),
            &config,
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("SUCCESS"));
    assert!(output.contains("Revoked admin privileges"));
    assert!(output.contains("charlie"));

    // Verify charlie is no longer admin (reuse existing store)
    assert!(!store.is_admin("charlie").unwrap());
}

#[tokio::test]
async fn test_removeadmin_cannot_revoke_self() {
    let dir = TempDir::new().expect("tempdir");
    let config = test_config(dir.path().to_str().unwrap().to_string());

    let store = TinyMushStoreBuilder::new(&tinymush_path(dir.path().to_str().unwrap()))
        .open()
        .expect("store");

    // Admin tries to revoke own privileges
    let mut session = test_session("admin").await;
    let mut processor = TinyMushProcessor::new(store.clone());
    let result = processor
        .process_command(
            &mut session,
            "@REMOVEADMIN admin",
            &mut meshbbs::storage::Storage::new(&config.storage.data_dir)
                .await
                .unwrap(),
            &config,
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("Cannot revoke your own"));
}

#[tokio::test]
async fn test_admins_lists_all_administrators() {
    let dir = TempDir::new().expect("tempdir");
    let config = test_config(dir.path().to_str().unwrap().to_string());

    let store = TinyMushStoreBuilder::new(&tinymush_path(dir.path().to_str().unwrap()))
        .open()
        .expect("store");

    // Create additional admins
    let mut mod1 = PlayerRecord::new("moderator", "Moderator", "town_square");
    mod1.grant_admin(1);
    store.put_player(mod1).expect("save mod");

    let mut admin2 = PlayerRecord::new("admin2", "Admin2", "town_square");
    admin2.grant_admin(2);
    store.put_player(admin2).expect("save admin2");

    // List admins
    let mut session = test_session("admin").await;
    let mut processor = TinyMushProcessor::new(store.clone());
    let result = processor
        .process_command(
            &mut session,
            "@ADMINS",
            &mut meshbbs::storage::Storage::new(&config.storage.data_dir)
                .await
                .unwrap(),
            &config,
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("SYSTEM ADMINISTRATORS"));
    assert!(output.contains("Total: 3"));
    assert!(output.contains("admin"));
    assert!(output.contains("Moderator")); // Display name is capitalized
    assert!(output.contains("Admin2")); // Display name is capitalized
    assert!(output.contains("Sysop"));
    assert!(output.contains("Admin"));
    assert!(output.contains("Moderator"));
}

#[tokio::test]
async fn test_admins_command_anyone_can_view() {
    let dir = TempDir::new().expect("tempdir");
    let config = test_config(dir.path().to_str().unwrap().to_string());

    let store = TinyMushStoreBuilder::new(&tinymush_path(dir.path().to_str().unwrap()))
        .open()
        .expect("store");

    // Create regular user
    let user = PlayerRecord::new("viewer", "Viewer", "town_square");
    store.put_player(user).expect("save user");

    // Regular user can view admin list
    let mut session = test_session("viewer").await;
    let mut processor = TinyMushProcessor::new(store.clone());
    let result = processor
        .process_command(
            &mut session,
            "@ADMINS",
            &mut meshbbs::storage::Storage::new(&config.storage.data_dir)
                .await
                .unwrap(),
            &config,
        )
        .await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("SYSTEM ADMINISTRATORS"));
    assert!(output.contains("admin")); // Auto-seeded admin
}
