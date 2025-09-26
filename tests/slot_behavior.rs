#![cfg(feature = "meshtastic-proto")]
use meshbbs::bbs::BbsServer;
mod common;
use meshbbs::config::Config;

#[tokio::test]
async fn slot_broadcasts_and_not_dm() {
    let mut config = Config::default();
    config.storage.data_dir = crate::common::fixture_root().to_string_lossy().to_string();
    let mut server = BbsServer::new(config).await.expect("server");

    use meshbbs::meshtastic::TextEvent;
    let node_id = 5150u32;
    let public_evt = TextEvent { source: node_id, dest: None, is_direct: false, channel: None, content: "^SLOT".into() };
    server.route_text_event(public_evt).await.expect("route public slot");

    // Inspect recorded outbound messages
    let msgs = server.test_messages();
    assert!(msgs.iter().any(|(to, body)| to == "BCAST" && body.starts_with("^SLOT ⟶ ")), "expected ^SLOT broadcast in test_messages, got {:?}", msgs);
    // Ensure no DM was sent to the origin node
    assert!(!msgs.iter().any(|(to, _)| to == &node_id.to_string()), "did not expect DM to {}", node_id);
}

#[cfg(not(feature = "meshtastic-proto"))]
#[test]
fn slot_behavior_skipped() {
    eprintln!("slot_behavior skipped: meshtastic-proto feature disabled");
}
