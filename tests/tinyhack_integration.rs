use meshbbs::bbs::server::BbsServer;
use meshbbs::config::Config;
use std::fs;
use std::path::Path;

// Basic TinyHack integration: enter game, take turns, and verify save persistence.
#[tokio::test]
async fn tinyhack_enter_play_persist() {
    // Setup config with TinyHack enabled and use a temp data dir
    let mut cfg = Config::default();
    cfg.apps.tinyhack.enabled = true;
    // Use a temp data dir and keep it alive for the duration of the test
    let _td = tempfile::tempdir().unwrap();
    cfg.storage.data_dir = _td.path().to_string_lossy().to_string();
    cfg.bbs.max_users = 10;

    let mut server = BbsServer::new(cfg.clone()).await.unwrap();

    // Create a session and login as 'alice'
    let mut session = meshbbs::bbs::session::Session::new("nodeTH1".into(), "nodeTH1".into());
    session.login("alice".into(), 1).await.unwrap();
    let node_key = session.node_id.clone();
    server.test_insert_session(session);

    // Enter TinyHack via main menu
    let before = server.test_messages().len();
    server
        .route_test_text_direct(&node_key, "G1")
        .await
        .unwrap();
    // Collect all chunks produced by this command for our node and join them
    let first_chunks: Vec<String> = server.test_messages()[before..]
        .iter()
        .filter(|(k, _)| k == &node_key)
        .map(|(_, m)| m.clone())
        .collect();
    // First chunk should contain the welcome text, second should begin with status line (L..)
    assert!(
        first_chunks
            .iter()
            .any(|m| m.contains("Welcome to TinyHack: a compact, turn-based dungeon crawl.")),
        "expected a separate welcome message chunk; got: {:?}",
        first_chunks
    );
    assert!(
        first_chunks.iter().any(|m| m.starts_with("L")),
        "expected at least one non-welcome chunk to start with 'L'; got: {:?}",
        first_chunks
    );

    // Issue a couple of turns: move east, rest
    server.route_test_text_direct(&node_key, "E").await.unwrap();
    let after_e = server.test_messages().last().unwrap().1.clone();
    assert!(
        !after_e.contains("alice (lvl1)>"),
        "TinyHack chunk should not include session prompt: {}",
        after_e
    );

    server.route_test_text_direct(&node_key, "R").await.unwrap();
    let after_r = server.test_messages().last().unwrap().1.clone();
    assert!(
        !after_r.contains("alice (lvl1)>"),
        "TinyHack chunk should not include session prompt: {}",
        after_r
    );

    // Validate save file written and contains JSON with gid
    // Since server.config is private, reconstruct expected path: cfg.storage.data_dir + "/tinyhack/alice.json"
    let save_path = Path::new(&cfg.storage.data_dir)
        .join("tinyhack")
        .join("alice.json");
    let content = fs::read_to_string(&save_path).expect("save exists");
    assert!(
        content.contains("\"gid\":"),
        "save should contain gid: {}",
        content
    );
    // Parse JSON gid and turn
    #[derive(serde::Deserialize)]
    struct SaveHead {
        gid: u32,
        turn: u32,
    }
    let head: SaveHead = serde_json::from_str(&content).expect("valid json");

    // Leave game then re-enter; gid should persist and saved turn should match re-entry state
    server.route_test_text_direct(&node_key, "B").await.unwrap();
    let back_to_menu = server.test_messages().last().unwrap().1.clone();
    assert!(back_to_menu.contains("Main Menu:"));
    assert!(back_to_menu.contains("[G]ames"));
    assert!(
        back_to_menu.ends_with("alice (lvl1)>"),
        "prompt should return after leaving TinyHack: {}",
        back_to_menu
    );
    let before2 = server.test_messages().len();
    server
        .route_test_text_direct(&node_key, "G1")
        .await
        .unwrap();
    let re_chunks: Vec<String> = server.test_messages()[before2..]
        .iter()
        .filter(|(k, _)| k == &node_key)
        .map(|(_, m)| m.clone())
        .collect();
    assert!(
        re_chunks.iter().any(|m| m.starts_with("L")),
        "expected at least one re-entry chunk to start with 'L'; got: {:?}",
        re_chunks
    );
    assert!(
        re_chunks
            .iter()
            .any(|m| m.contains("Welcome to TinyHack: a compact, turn-based dungeon crawl.")),
        "re-entry should include the welcome message"
    );
    // Re-read save file and ensure gid remains and turn didn’t regress
    let content2 = fs::read_to_string(&save_path).expect("save exists");
    let head2: SaveHead = serde_json::from_str(&content2).expect("valid json");
    assert_eq!(head2.gid, head.gid, "gid should persist across re-entry");
    assert!(
        head2.turn >= head.turn,
        "turn should not regress: {} -> {}",
        head.turn,
        head2.turn
    );
}
