use meshbbs::config::Config;
use meshbbs::storage::Storage;

#[tokio::test]
async fn main_menu_single_letter_aliases() {
    let cfg = Config::default();
    let mut storage = Storage::new(&cfg.storage.data_dir).await.unwrap();
    let mut session = meshbbs::bbs::session::Session::new("s_mm".into(), "node_mm".into());
    // Transition to MainMenu
    let _banner = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "anything", &mut storage).await.unwrap();

    let help_full = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "HELP", &mut storage).await.unwrap();
    let help_single = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "h", &mut storage).await.unwrap();
    assert_eq!(help_full, help_single, "H/h should equal HELP output (guest)");

    // M vs MESSAGES
    let m_full = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "MESSAGES", &mut storage).await.unwrap();
    let mut session2 = meshbbs::bbs::session::Session::new("s_mm2".into(), "node_mm2".into());
    let _banner2 = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session2, "ignored", &mut storage).await.unwrap();
    let m_short = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session2, "m", &mut storage).await.unwrap();
    assert_eq!(m_full, m_short, "M should equal MESSAGES");
}
