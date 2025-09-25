use meshbbs::bbs::BbsServer;
use meshbbs::config::Config;
use password_hash::{PasswordHasher, SaltString};
use argon2::Argon2;

fn last_for_node<'a>(msgs: &'a Vec<(String,String)>, node: &str) -> Option<&'a String> {
    for (to, m) in msgs.iter().rev() { if to == node { return Some(m); } }
    None
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moderator_can_lock_unlock_with_k_in_threads() {
    let mut cfg = Config::default();
    cfg.storage.data_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
    let mut server = BbsServer::new(cfg).await.expect("server");
    server.test_register("mod", "Password123").await.unwrap();
    server.test_update_level("mod", 5).await.unwrap();
    let node = "n1";
    server.route_test_text_direct(node, "LOGIN mod").await.unwrap();
    // Enter default first topic (created via TOML merge) or create one
    // Ensure at least one topic exists
    if server.test_get_messages("general", 1).await.is_err() {
        let _ = server.test_create_topic("general", "General", "", 0, 0, "sysop").await;
    }
    server.route_test_text_direct(node, "M").await.unwrap();
    server.route_test_text_direct(node, "1").await.unwrap();
    // Toggle lock
    server.route_test_text_direct(node, "K").await.unwrap();
    let m = last_for_node(server.test_messages(), node).unwrap();
    assert!(m.contains("Threads"), "threads after lock toggle");
    // Toggle again
    server.route_test_text_direct(node, "K").await.unwrap();
    let m = last_for_node(server.test_messages(), node).unwrap();
    assert!(m.contains("Threads"), "threads after second toggle");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sysop_grant_g_changes_user_level() {
    let mut cfg = Config::default();
    cfg.storage.data_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
    // Seed sysop with a password hash so level is 10
    let salt = SaltString::generate(&mut rand::thread_rng());
    let hash = Argon2::default().hash_password("SecretP@ss1".as_bytes(), &salt).unwrap().to_string();
    cfg.bbs.sysop_password_hash = Some(hash);
    let mut server = BbsServer::new(cfg).await.expect("server");
    server.seed_sysop().await.unwrap();
    let node = "n1";
    // Sysop is auto-seeded on first DM if not present; login as sysop
    server.route_test_text_direct(node, "LOGIN sysop").await.unwrap();
    // Create a user and grant level
    server.test_register("bob", "Password123").await.unwrap();
    server.route_test_text_direct(node, "G @bob=5").await.unwrap();
    let m = last_for_node(server.test_messages(), node).unwrap();
    assert!(m.contains("level 5"), "grant feedback: {}", m);
}
