#![cfg(feature = "meshtastic-proto")]
use meshbbs::bbs::BbsServer;
mod common;
use meshbbs::config::Config;

#[tokio::test]
async fn fortune_broadcasts_and_not_dm() {
    let mut config = Config::default();
    config.storage.data_dir = crate::common::fixture_root().to_string_lossy().to_string();
    // Keep public cooldowns small for tests (defaults are already small)
    let mut server = BbsServer::new(config).await.expect("server");

    use meshbbs::meshtastic::TextEvent;
    let node_id = 4242u32;
    let public_evt = TextEvent { source: node_id, dest: None, is_direct: false, channel: None, content: "^FORTUNE".into() };
    server.route_text_event(public_evt).await.expect("route public fortune");

    // Inspect recorded outbound messages
    let msgs = server.test_messages();
    assert!(msgs.iter().any(|(to, body)| to == "BCAST" && body.starts_with("^FORTUNE ⟶ ")), "expected ^FORTUNE broadcast in test_messages, got {:?}", msgs);
    // Ensure no DM was sent to the origin node
    assert!(!msgs.iter().any(|(to, _)| to == &node_id.to_string()), "did not expect DM to {}", node_id);
}

#[tokio::test]
async fn fortune_respects_cooldown() {
    let mut config = Config::default();
    config.storage.data_dir = crate::common::fixture_root().to_string_lossy().to_string();
    let mut server = BbsServer::new(config).await.expect("server");

    use meshbbs::meshtastic::TextEvent;
    let node_id = 1234567890u32;
    let evt1 = TextEvent { source: node_id, dest: None, is_direct: false, channel: None, content: "^FORTUNE".into() };
    let evt2 = TextEvent { source: node_id, dest: None, is_direct: false, channel: None, content: "^FORTUNE".into() };

    server.route_text_event(evt1).await.expect("first fortune");
    server.route_text_event(evt2).await.expect("second fortune"); // Second call should be rate limited

    let bcasts: Vec<_> = server.test_messages().iter().filter(|(to, body)| to == "BCAST" && body.starts_with("^FORTUNE ⟶ ")).collect();
    assert_eq!(bcasts.len(), 1, "expected exactly one broadcast due to rate limiting, got {} broadcasts", bcasts.len());
}