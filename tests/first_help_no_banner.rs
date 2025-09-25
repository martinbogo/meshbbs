#[cfg(feature = "meshtastic-proto")]
#[tokio::test]
async fn first_help_no_banner() {
    use meshbbs::bbs::BbsServer;
    use meshbbs::config::Config;
    use meshbbs::meshtastic::TextEvent;

    let mut cfg = Config::default();
    cfg.storage.data_dir = tempfile::tempdir().unwrap().path().to_str().unwrap().into();
    let mut server = BbsServer::new(cfg).await.unwrap();

    let node: u32 = 0x12345678;
    let help_event = TextEvent { source: node, dest: None, is_direct: true, channel: None, content: "HELP".into() };
    server.route_text_event(help_event).await.unwrap();

    // Collect messages for this node
    let node_key = node.to_string();
    let collected: Vec<String> = server.test_messages().iter()
        .filter(|(to,_m)| to == &node_key)
        .map(|(_,m)| m.clone()).collect();

    assert_eq!(collected.len(), 1, "Expected exactly one initial HELP response, got {}: {:?}", collected.len(), collected);
    let body = &collected[0];
    assert!(body.contains("Auth: REGISTER <user> <pass> or LOGIN <user> [pass]"), "Missing menu block: {}", body);
    assert!(!body.contains("Use REGISTER <name> <pass>"), "Legacy banner text should be absent: {}", body);
}
