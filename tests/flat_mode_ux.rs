use meshbbs::bbs::BbsServer;
use meshbbs::config::Config;

fn last_for_node<'a>(msgs: &'a [(String, String)], node: &str) -> Option<&'a String> {
    for (to, m) in msgs.iter().rev() { if to == node { return Some(m); } }
    None
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compact_flow_topics_threads_read_compose_reply() {
    // Arrange: fresh server in temp storage; create one topic
    let mut cfg = Config::default();
    cfg.storage.data_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
    let mut server = BbsServer::new(cfg).await.expect("server");
    server
        .test_create_topic(
            "hello",
            "Hello Topic",
            "Test topic for compact flow",
            0,
            0,
            "sysop",
        )
        .await
        .expect("create topic");

    let node = "n1";

    // Login
    server
        .route_test_text_direct(node, "LOGIN alice")
        .await
        .expect("login");
    let m = last_for_node(server.test_messages(), node).expect("welcome msg");
    assert!(m.contains("Welcome, alice"), "welcome: {}", m);

    // M → Topics
    server
        .route_test_text_direct(node, "M")
        .await
        .expect("topics");
    let m = last_for_node(server.test_messages(), node).expect("topics msg");
    assert!(m.contains("Topics") && m.contains("Type number to select topic"), "topics list: {}", m);

    // 1 → Threads in hello
    server
        .route_test_text_direct(node, "1")
        .await
        .expect("threads");
    let m = last_for_node(server.test_messages(), node).expect("threads msg");
    assert!(m.contains("Messages in"), "threads header: {}", m);

    // N → title prompt
    server
        .route_test_text_direct(node, "N")
        .await
        .expect("compose title");
    let m = last_for_node(server.test_messages(), node).expect("title prompt");
    assert!(m.contains("New thread title"), "title prompt: {}", m);

    // Provide title
    server
        .route_test_text_direct(node, "Test Title")
        .await
        .expect("title input");
    let m = last_for_node(server.test_messages(), node).expect("body prompt");
    assert!(m.contains("Body:"), "body prompt: {}", m);

    // Provide body
    server
        .route_test_text_direct(node, "This is the body of the test thread.")
        .await
        .expect("body input");
    let m = last_for_node(server.test_messages(), node).expect("threads after post");
    assert!(m.contains("Messages in") && m.contains("Test Title"), "threads list after post: {}", m);

    // Read the thread
    server
        .route_test_text_direct(node, "1")
        .await
        .expect("read thread");
    let m = last_for_node(server.test_messages(), node).expect("read view");
    assert!(m.contains("Reply:") && m.contains("This is the body"), "read view: {}", m);

    // Reply flow
    server
        .route_test_text_direct(node, "Y")
        .await
        .expect("reply prompt");
    let m = last_for_node(server.test_messages(), node).expect("reply prompt msg");
    assert!(m.contains("Reply text"), "reply prompt: {}", m);

    server
        .route_test_text_direct(node, "Thanks!")
        .await
        .expect("post reply");
    let m = last_for_node(server.test_messages(), node).expect("read after reply");
    assert!(m.contains("— ") && m.contains("Thanks!"), "read shows reply: {}", m);

    // Budget guard: All emitted messages for this node must be <= 230 bytes
    for (_to, msg) in server.test_messages().iter() {
        if _to == node {
            assert!(msg.len() <= 230, "message exceeds 230 bytes ({}): {}", msg.len(), msg);
        }
    }
}
