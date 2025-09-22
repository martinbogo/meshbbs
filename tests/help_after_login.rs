use meshbbs::config::Config;
use meshbbs::bbs::BbsServer;
#[cfg(feature = "meshtastic-proto")]
use meshbbs::meshtastic::TextEvent;

// Ensure HELP responds after successful login (regression test for empty response issue)
#[cfg(feature = "meshtastic-proto")]
#[tokio::test]
async fn help_after_login() {
    let mut cfg = Config::default();
    cfg.storage.data_dir = "./test-data-int".into();
    let mut server = BbsServer::new(cfg).await.expect("server");

    // Use a unique username each run to avoid collision with existing test data
    let uname = format!("testuser_help_{}", uuid::Uuid::new_v4().simple());
    let dm_register = TextEvent { source: 77, dest: Some(1), is_direct: true, channel: None, content: format!("REGISTER {} testpw", uname) };
    server.route_text_event(dm_register).await.expect("register");
    // Issue HELP
    let dm_help = TextEvent { source: 77, dest: Some(1), is_direct: true, channel: None, content: "HELP".into() };
    server.route_text_event(dm_help).await.expect("help");

    // Find last message containing Commands:
    let mut found = false;
    #[allow(clippy::redundant_clone)]
    let msgs = server.test_messages().clone();
    let mut collected = String::new();
    let mut help_len_ok = false;
    for (_to, msg) in msgs {
        collected.push_str(&format!("MSG:[[{}]]\n", msg));
        if msg.contains("ACCT:") {
            found = true;
            if msg.as_bytes().len() <= 230 { help_len_ok = true; }
        }
    }
    assert!(found, "Expected abbreviated HELP output containing ACCT: section. Collected messages:\n{}", collected);
    assert!(help_len_ok, "HELP output exceeded 230 bytes limit");
}
