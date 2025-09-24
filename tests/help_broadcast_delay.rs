use meshbbs::config::Config;
use meshbbs::bbs::BbsServer;
#[cfg(feature = "meshtastic-proto")]
use meshbbs::meshtastic::TextEvent;
use tokio::time::{sleep, Duration};

// This test validates that the HELP public broadcast is not queued immediately, but instead
// scheduled with a delay after sending the DM help message.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg(feature = "meshtastic-proto")]
async fn help_broadcast_is_delayed() {
    let mut cfg = Config::default();
    // Force a short delay to keep test fast while still exercising scheduling path
    // Configure small gaps so the required composite delay remains low for fast test
    cfg.meshtastic.help_broadcast_delay_ms = Some(200); // target delay
    cfg.meshtastic.min_send_gap_ms = Some(10); // influences required composite delay calculation in scheduling
    cfg.meshtastic.post_dm_broadcast_gap_ms = Some(10);

    let mut server = BbsServer::new(cfg.clone()).await.expect("server new");

    // We won't actually connect a device; outgoing_tx remains None so scheduling will warn and skip.
    // To exercise the scheduling code path we need an outgoing channel. We'll simulate by creating one.
    #[cfg(feature = "meshtastic-proto")]
    {
    use meshbbs::meshtastic::OutgoingMessage;
        use tokio::sync::mpsc;
        let (tx, mut rx) = mpsc::unbounded_channel::<OutgoingMessage>();
    server.test_set_outgoing(tx);

        // Spawn a task to drain queued messages into server.test_messages for inspection
        let test_msgs = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let test_msgs_clone = test_msgs.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await { test_msgs_clone.lock().await.push(msg); }
        });

        // Craft a HELP public TextEvent from node id 1234
    let ev = TextEvent { source: 1234, dest: None, is_direct: false, channel: Some(0), content: "^HELP".to_string() };
        server.route_text_event(ev).await.expect("route help");

        // Allow a brief time for immediate DM queue (writer queue entry)
        sleep(Duration::from_millis(50)).await;
        let immediate = test_msgs.lock().await.clone();
        // Expect exactly one message (DM) immediately; broadcast delayed
        assert_eq!(immediate.len(), 1, "Expected only DM queued immediately, got {:?}", immediate.len());
        assert!(immediate[0].to_node.is_some(), "First queued must be DM");

        // Wait past the delay to allow broadcast scheduling
    sleep(Duration::from_millis(400)).await; // > configured help_broadcast_delay_ms (after required adjustments)
        let final_msgs = test_msgs.lock().await.clone();
    assert_eq!(final_msgs.len(), 2, "Expected DM + delayed broadcast, got {}", final_msgs.len());
    assert!(final_msgs[1].to_node.is_none(), "Second queued must be broadcast");
    }
}
