use meshbbs::config::Config;
use meshbbs::bbs::server::BbsServer;

#[tokio::test]
async fn reject_oversize_message() {
    let mut cfg = Config::default();
    // Intentionally set a larger value to verify clamp
    cfg.storage.max_message_size = 500;
    let mut server = BbsServer::new(cfg).await.expect("server");
    // Seed a simple user message: create user directly via test helper
    server.test_register("alice", "pass").await.expect("register");
    // Promote to ensure posting is allowed by default levels
    let ok = server.test_store_message("general", "alice", &"a".repeat(230)).await;
    assert!(ok.is_ok(), "230 bytes should be accepted: {ok:?}");
    let too_big = server.test_store_message("general", "alice", &"a".repeat(231)).await;
    assert!(too_big.is_err(), "231 bytes should be rejected");
}
