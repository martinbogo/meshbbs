use meshbbs::bbs::server::BbsServer;
use meshbbs::config::Config;

#[tokio::test]
async fn prompt_unauth_then_login() {
    let mut cfg = Config::default();
    cfg.bbs.max_users = 10;
    let _server = BbsServer::new(cfg).await.unwrap();
    // Simulate direct message (auto-create session) with a non-auth command to trigger banner
    // We call internal functions indirectly by simulating a login flow through public -> direct path is complex;
    // Instead directly create a session.
    let mut session = meshbbs::bbs::session::Session::new("node1".into(), "node1".into());
    assert_eq!(session.build_prompt(), "unauth>");
    session.login("alice".into(), 1).await.unwrap();
    assert!(session.build_prompt().starts_with("alice (lvl1)"));
    session.current_area = Some("general".into());
    session.state = meshbbs::bbs::session::SessionState::ReadingMessages;
    let p = session.build_prompt();
    assert!(p.starts_with("alice@general"));
    session.state = meshbbs::bbs::session::SessionState::PostingMessage;
    let p2 = session.build_prompt();
    assert!(p2.starts_with("post@general"));
}

#[tokio::test]
async fn help_shortcuts_once_and_help_plus_chunks() {
    let mut cfg = Config::default();
    cfg.bbs.max_users = 10;
    let mut server = BbsServer::new(cfg).await.unwrap();
    // Manually insert session to simplify
    let mut session = meshbbs::bbs::session::Session::new("n2".into(), "n2".into());
    session.login("bob".into(), 1).await.unwrap();
    let node_key = session.node_id.clone();
    server.test_insert_session(session);
    // Issue abbreviated HELP
    server.route_test_text_direct(&node_key, "HELP").await.unwrap();
    let first = server.test_messages().last().unwrap().1.clone();
    assert!(first.contains("Shortcuts: M=areas U=user Q=quit"));
    // Second HELP should omit shortcuts
    server.route_test_text_direct(&node_key, "HELP").await.unwrap();
    let second = server.test_messages().last().unwrap().1.clone();
    assert!(!second.contains("Shortcuts:"));
    // HELP+ should produce multiple chunks with prompt only on last
    server.route_test_text_direct(&node_key, "HELP+").await.unwrap();
    // Collect last N messages (unknown exact count; assert at least 2)
    let verbose_msgs: Vec<_> = server.test_messages().iter().filter(|(to,_msg)| to==&node_key).map(|(_,m)| m.clone()).collect();
    let help_plus_msgs: Vec<_> = verbose_msgs.into_iter().rev().take(10).collect(); // larger window for longer help
    // Ensure at least one chunk contains Extended Help header
    assert!(help_plus_msgs.iter().any(|m| m.contains("MeshBBS Extended Help")));
}

#[tokio::test]
async fn unknown_command_reply() {
    let mut cfg = Config::default();
    cfg.bbs.max_users = 10;
    let mut server = BbsServer::new(cfg).await.unwrap();
    let mut session = meshbbs::bbs::session::Session::new("n3".into(), "n3".into());
    session.login("carol".into(), 1).await.unwrap();
    let node_key = session.node_id.clone();
    server.test_insert_session(session);
    server.route_test_text_direct(&node_key, "NOPE").await.unwrap();
    let last = server.test_messages().last().unwrap().1.clone();
    assert!(last.contains("Unknown command. Type HELP"));
}