use meshbbs::config::Config;
use meshbbs::bbs::BbsServer;
#[cfg(feature = "meshtastic-proto")]
use meshbbs::meshtastic::TextEvent;

// This test requires meshtastic-proto because route_text_event is behind that feature.
// If feature not enabled, compile will skip.
#[cfg(feature = "meshtastic-proto")]
#[tokio::test]
async fn welcome_message_sent_on_login() {
    let mut cfg = Config::default();
    cfg.bbs.welcome_message = "Custom Banner Line".to_string();
    cfg.storage.data_dir = "./test-data-int".into();
    let mut server = BbsServer::new(cfg).await.expect("server");

    // Simulate public login then DM to finalize
    let public = TextEvent { source: 42, dest: None, is_direct: false, channel: None, content: "^LOGIN alice".into() };
    server.route_text_event(public).await.expect("public");
    let dm_login = TextEvent { source: 42, dest: Some(99), is_direct: true, channel: None, content: "LOGIN alice".into() };
    server.route_text_event(dm_login).await.expect("dm login");

    // We currently have no direct capture of outbound messages in tests; to validate integration we'd need
    // to expose a hook or log. For now this test ensures no panic and path executes. Future enhancement
    // could wrap send_message to record last banner for assertion.
}
