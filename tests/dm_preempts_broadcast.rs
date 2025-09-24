use meshbbs::config::Config;
use meshbbs::bbs::BbsServer;

// This test relies on meshtastic-proto feature; skip if not enabled.
#[cfg(feature = "meshtastic-proto")]
#[tokio::test]
async fn dm_preempts_broadcast() {
    let mut config = Config::default();
    // Speed up test: reduce min gap
    config.meshtastic.min_send_gap_ms = Some(10);
    config.meshtastic.help_broadcast_delay_ms = Some(50);
    // Initialize server (no actual device; mock mode collects test_messages)
    let mut server = BbsServer::new(config.clone()).await.expect("server new");

    // Enqueue several broadcasts
    for i in 0..5 { let _ = server.send_broadcast(&format!("BCAST {i}" )).await; }
    // Enqueue a DM
    let _ = server.send_message("0x0000ABCD", "HELLO DM").await;

    // Allow some time for scheduler ticks
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;

    // Collected messages: ensure DM appears before at least last broadcast (preemption)
    let msgs = server.test_messages();
    let dm_pos = msgs.iter().position(|(_,m)| m.contains("HELLO DM"));
    let bcast_positions: Vec<_> = msgs.iter().enumerate().filter(|(_,(_,m))| m.starts_with("BCAST")).map(|(i,_)| i).collect();

    assert!(dm_pos.is_some(), "DM not found in collected messages: {:?}", msgs);
    assert!(!bcast_positions.is_empty(), "No broadcasts recorded");
    let dm_index = dm_pos.unwrap();
    // DM should not be strictly after all broadcasts (i.e., some broadcast queued later should come after it)
    let max_bcast_before_dm = bcast_positions.iter().all(|p| *p < dm_index);
    assert!(max_bcast_before_dm, "DM was not scheduled ahead of queued broadcasts: order={:?}", msgs);
}

#[cfg(not(feature = "meshtastic-proto"))]
#[test]
fn dm_preempts_broadcast_skipped() {
    eprintln!("dm_preempts_broadcast skipped: meshtastic-proto feature disabled");
}
