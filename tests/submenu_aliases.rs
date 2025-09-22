use meshbbs::config::Config;
use meshbbs::storage::Storage;

#[tokio::test]
async fn message_area_aliases() {
    let cfg = Config::default();
    let mut storage = Storage::new(&cfg.storage.data_dir).await.unwrap();
    let mut session = meshbbs::bbs::session::Session::new("s_ma".into(), "node_ma".into());
    // Enter main menu
    let _ = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "init", &mut storage).await.unwrap();
    // Enter message areas via full and short forms
    let _areas_full = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "MESSAGES", &mut storage).await.unwrap();
    // Back to main menu to test short form
    session.state = meshbbs::bbs::session::SessionState::MainMenu;
    let _areas_short = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "m", &mut storage).await.unwrap();
    // In MessageAreas state now. R vs READ path handled by handle_message_areas only for R/READ without area argument; we only check they produce same transition output.
    let r_full = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "READ", &mut storage).await.unwrap();
    assert!(r_full.contains("Messages in") || r_full.contains("Recent messages"), "READ output should list messages");
    // Reset state to MessageAreas to compare short form again
    session.state = meshbbs::bbs::session::SessionState::MessageAreas;
    let r_short = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "R", &mut storage).await.unwrap();
    assert!(r_short.contains("Messages in") || r_short.contains("Recent messages"), "R output should list messages");
}

#[tokio::test]
async fn user_menu_aliases() {
    let cfg = Config::default();
    let mut storage = Storage::new(&cfg.storage.data_dir).await.unwrap();
    let mut session = meshbbs::bbs::session::Session::new("s_um".into(), "node_um".into());
    // Enter main menu
    let _ = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "go", &mut storage).await.unwrap();
    // Enter user menu via full and short forms
    let full = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "USER", &mut storage).await.unwrap();
    // Back to main menu
    session.state = meshbbs::bbs::session::SessionState::MainMenu;
    let short = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "u", &mut storage).await.unwrap();
    assert_eq!(full, short, "U should equal USER");
}
