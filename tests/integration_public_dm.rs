use meshbbs::bbs::{BbsServer};
use meshbbs::config::Config;
use meshbbs::storage::Storage; // for type references (ensuring feature compile)

// NOTE: This is a high-level logical integration skeleton. In the absence of a real MeshtasticDevice
// mock layer in the codebase, we simulate the public -> DM flow by directly invoking internal
// methods where possible. If deeper mocking is needed, future refactor should abstract device IO.

#[tokio::test]
async fn public_login_then_dm_session_inline_commands() {
    // Build a default config (assuming Config::default or similar). If not available, construct manually.
    // For now we assume a basic constructor exists; adapt if necessary.
    let mut config = Config::default();
    config.storage.data_dir = "./test-data-int".into();

    // Initialize server (without actual device)
    let mut server = BbsServer::new(config).await.expect("server");

    // Simulate a public LOGIN (would normally arrive via TextEvent)
    use meshbbs::meshtastic::TextEvent; // re-export not present, path adjust if needed
    let public_event = TextEvent { source: 123, dest: None, is_direct: false, channel: None, content: "^LOGIN alice".into() };
    server.route_text_event(public_event).await.expect("public login");

    // Now simulate DM message to trigger session creation and finalize login
    let dm_event = TextEvent { source: 123, dest: Some(999), is_direct: true, channel: None, content: "READ".into() };
    server.route_text_event(dm_event).await.expect("dm read");

    // Post a message inline
    let dm_post = TextEvent { source: 123, dest: Some(999), is_direct: true, channel: None, content: "POST Hello world from inline".into() };
    server.route_text_event(dm_post).await.expect("dm post");

    // Read again to confirm (basic success path; deeper assertions would require exposing responses)
    let dm_read2 = TextEvent { source: 123, dest: Some(999), is_direct: true, channel: None, content: "READ".into() };
    server.route_text_event(dm_read2).await.expect("dm read2");

    // At this stage we at least validated no panics and state transitions executed.
    // Future improvement: Capture outbound messages by injecting a mock device.
}
