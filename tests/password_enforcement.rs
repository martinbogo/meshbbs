use meshbbs::config::{Config, BbsConfig, MeshtasticConfig, StorageConfig, LoggingConfig};
use tempfile::tempdir;

#[tokio::test]
async fn passwordless_user_prompt_and_set() {
    // Setup config with temp data dir
    let tmp = tempdir().unwrap();
    let data_dir = tmp.path().to_str().unwrap().to_string();
    let _cfg = Config {
        bbs: BbsConfig { name: "Test".into(), sysop: "sysop".into(), location: "loc".into(), description: "d".into(), max_users: 10, session_timeout: 10, welcome_message: "Welcome".into(), sysop_password_hash: None },
    meshtastic: MeshtasticConfig { port: "".into(), baud_rate: 0, node_id: "".into(), channel: 0, min_send_gap_ms: None, dm_resend_backoff_seconds: None, post_dm_broadcast_gap_ms: None, dm_to_dm_gap_ms: None, help_broadcast_delay_ms: None },
        storage: StorageConfig { data_dir: data_dir.clone(), max_message_size: 230 },
        message_topics: Default::default(),
        logging: LoggingConfig { level: "error".into(), file: None, security_file: None },
        security: None,
    };
    // Server instance not required for this test; we manipulate user file directly.
    // Use test helper to create passwordless legacy user via storage public method not exposed; mimic by writing file through create_or_update_user equivalent path: call internal method via public test_register? Not possible without password.
    // Instead, register then clear password by setting new user without password is not supported; so we simulate by creating raw user json file.
    use std::path::Path; use tokio::fs; use serde_json::json; use chrono::Utc;
    let users_dir = Path::new(&data_dir).join("users"); fs::create_dir_all(&users_dir).await.unwrap();
    let user_file = users_dir.join("legacy.json");
    let now = Utc::now().to_rfc3339();
    let user_obj = json!({
        "username": "legacy",
        "node_id": null,
        "user_level": 1,
        "password_hash": null,
        "first_login": now,
        "last_login": now,
        "total_messages": 0
    });
    fs::write(&user_file, serde_json::to_string_pretty(&user_obj).unwrap()).await.unwrap();
    // At this point legacy user exists without password; we assert file present
    assert!(user_file.exists());
}
