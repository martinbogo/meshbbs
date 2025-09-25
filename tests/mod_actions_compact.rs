use meshbbs::bbs::BbsServer;
use meshbbs::config::Config;

fn last_for_node<'a>(msgs: &'a [(String, String)], node: &str) -> Option<&'a String> {
    for (to, m) in msgs.iter().rev() { if to == node { return Some(m); } }
    None
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moderator_pin_rename_delete_in_compact_flow() {
    // Fresh server
    let mut cfg = Config::default();
    cfg.storage.data_dir = tempfile::tempdir().unwrap().path().to_string_lossy().to_string();
    let mut server = BbsServer::new(cfg).await.expect("server");

    // Register moderator and login
    server.test_register("mod", "Password123").await.unwrap();
    server.test_update_level("mod", 5).await.unwrap();
    let node = "n1";
    server.route_test_text_direct(node, "LOGIN mod").await.unwrap();

    // Enter topics and topic
    server.route_test_text_direct(node, "M").await.unwrap();
    server.route_test_text_direct(node, "1").await.unwrap();

    // Create two threads for ordering checks
    server.route_test_text_direct(node, "N").await.unwrap();
    server.route_test_text_direct(node, "First").await.unwrap();
    server.route_test_text_direct(node, "Body one").await.unwrap();
    server.route_test_text_direct(node, "N").await.unwrap();
    server.route_test_text_direct(node, "Second").await.unwrap();
    server.route_test_text_direct(node, "Body two").await.unwrap();

    // Pin second (index 2) and ensure 📌 indicator appears
    server.route_test_text_direct(node, "P2").await.unwrap();
    let m = last_for_node(server.test_messages(), node).unwrap();
    assert!(m.contains("\u{1F4CC}"), "threads list should show pin indicator: {}", m);

    // Rename second via R2 NewTitle
    server.route_test_text_direct(node, "R2 Renamed").await.unwrap();
    let m = last_for_node(server.test_messages(), node).unwrap();
    assert!(m.contains("Renamed"), "threads list should reflect renamed title: {}", m);

    // Delete first via D1 with confirm flow
    server.route_test_text_direct(node, "D1").await.unwrap();
    let prompt = last_for_node(server.test_messages(), node).unwrap();
    assert!(prompt.contains("Confirm delete"), "should prompt for delete: {}", prompt);
    server.route_test_text_direct(node, "Y").await.unwrap();
    let after = last_for_node(server.test_messages(), node).unwrap();
    assert!(after.contains("Deleted."), "should confirm deletion: {}", after);
}
