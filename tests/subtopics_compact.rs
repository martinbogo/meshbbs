use meshbbs::bbs::BbsServer;
use meshbbs::config::Config;

fn last_for_node<'a>(msgs: &'a [(String, String)], node: &str) -> Option<&'a String> {
    for (to, m) in msgs.iter().rev() { if to == node { return Some(m); } }
    None
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_navigate_and_post_in_subtopic() {
    // Arrange
    let mut cfg = Config::default();
    cfg.storage.data_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
    let mut server = BbsServer::new(cfg).await.expect("server");
    // Create root and subtopic
    server.test_create_topic("community", "Community", "Root", 0, 0, "sysop").await.unwrap();
    server.test_create_subtopic("community_news", "community", "News", "Subtopic", 0, 0, "sysop").await.unwrap();

    let node = "n1";
    server.route_test_text_direct(node, "LOGIN alice").await.unwrap();
    let _ = last_for_node(server.test_messages(), node).unwrap();

    // M -> Topics should show community with subtopic marker
    server.route_test_text_direct(node, "M").await.unwrap();
    let m = last_for_node(server.test_messages(), node).unwrap();
    assert!(m.contains("Topics") && m.contains("community") && m.contains("\u{203A}"), "topics with subtopic marker: {}", m);

    // 1 -> Subtopics view for community
    server.route_test_text_direct(node, "1").await.unwrap();
    let m = last_for_node(server.test_messages(), node).unwrap();
    assert!(m.contains("Subtopics") && m.contains("community_news"), "subtopics list: {}", m);

    // 1 -> Threads in community_news
    server.route_test_text_direct(node, "1").await.unwrap();
    let m = last_for_node(server.test_messages(), node).unwrap();
    assert!(m.contains("Threads"), "threads header in subtopic: {}", m);

    // N -> create thread
    server.route_test_text_direct(node, "N").await.unwrap();
    server.route_test_text_direct(node, "Update").await.unwrap();
    server.route_test_text_direct(node, "Today we have news.").await.unwrap();
    let m = last_for_node(server.test_messages(), node).unwrap();
    assert!(m.contains("Update"), "threads list shows new thread: {}", m);

    // Read and up-nav
    server.route_test_text_direct(node, "1").await.unwrap();
    let m = last_for_node(server.test_messages(), node).unwrap();
    assert!(m.contains("Reply:"), "read view in subtopic: {}", m);
    // Back to threads
    server.route_test_text_direct(node, "B").await.unwrap();
    let m = last_for_node(server.test_messages(), node).unwrap();
    assert!(m.contains("Threads"), "back to threads: {}", m);
    // Up to subtopics via Threads 'B'
    server.route_test_text_direct(node, "B").await.unwrap();
    let m = last_for_node(server.test_messages(), node).unwrap();
    assert!(m.contains("Subtopics"), "back to subtopics: {}", m);
}
