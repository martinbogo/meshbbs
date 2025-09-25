use meshbbs::bbs::BbsServer;
use meshbbs::config::Config;
use meshbbs::meshtastic::TextEvent;

#[cfg(feature = "meshtastic-proto")]
#[tokio::test]
async fn register_no_invalid_command_echo() {
    // Use a temp directory for storage to isolate test
    let mut cfg = Config::default();
    cfg.storage.data_dir = tempfile::tempdir().unwrap().path().to_str().unwrap().into();
    cfg.bbs.sysop = "sysop".into();

    let mut server = BbsServer::new(cfg).await.expect("server init");

    let node_id: u32 = 0xBEEF;
    let register_event = TextEvent {
        source: node_id,
        dest: None,
        is_direct: true,
        channel: None,
        content: "REGISTER bob password123".into(),
    };
    server.route_text_event(register_event).await.expect("register event");

    // Gather all messages sent to this node
    let node_key = format!("{node_id}");
    let mut combined = String::new();
    for (to, msg) in server.test_messages() {
        if to == &node_key { combined.push_str(msg); combined.push('\n'); }
    }

    assert!(combined.contains("Registered as bob."), "Missing compact registration confirmation: {combined}");
    assert!(!combined.contains("Invalid command \"REGISTER"), "Unexpected invalid command after registration: {combined}");
}
// (Removed earlier experimental test harness; final test above is authoritative.)
