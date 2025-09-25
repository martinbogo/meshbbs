#[cfg(feature = "meshtastic-proto")]
#[tokio::test]
async fn compact_registration_message_fits() {
    use meshbbs::bbs::BbsServer;
    use meshbbs::config::Config;
    use meshbbs::meshtastic::TextEvent;

    // 30-char username sample
    let long_user = "abcdefghijklmnopqrstuvwxzy1234"; // adjust to exactly 30 if off
    let long_user = &long_user[..30];

    let mut cfg = Config::default();
    cfg.storage.data_dir = tempfile::tempdir().unwrap().path().to_str().unwrap().into();
    let max = cfg.storage.max_message_size;

    let mut server = BbsServer::new(cfg).await.unwrap();
    let node: u32 = 0xABCDEF01;
    let cmd = format!("REGISTER {} passw0rd8", long_user);
    let ev = TextEvent { source: node, dest: None, is_direct: true, channel: None, content: cmd };
    server.route_text_event(ev).await.unwrap();

    let key = node.to_string();
    let mut combined = String::new();
    for (to,msg) in server.test_messages() { if to == &key { combined.push_str(msg); } }

    assert!(combined.contains("Registered as"), "Registration phrase missing: {combined}");
    assert!(!combined.contains("HELP+"), "Should not reference HELP+: {combined}");
    assert!(combined.len() <= max, "Registration message length {} exceeds max {}: '{}'", combined.len(), max, combined);
}
