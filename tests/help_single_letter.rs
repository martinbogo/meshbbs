use meshbbs::config::Config;
use meshbbs::storage::Storage;

// Validate that 'h' and 'H' and other case variants are accepted like HELP and produce same output as full HELP
#[tokio::test]
async fn help_single_letter_alias() {
    let cfg = Config::default();
    let mut session = meshbbs::bbs::session::Session::new("s_h".into(), "node_h".into());
    let mut storage = Storage::new(&cfg.storage.data_dir).await.unwrap();
    // First command transitions from Connected -> MainMenu regardless of content, returning banner
    let _banner = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "ignored", &mut storage).await.unwrap();
    // Now in MainMenu: capture baseline help output
    let base = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "HELP", &mut storage).await.unwrap();
    for variant in ["help", "HeLp", "H", "h", "?"] {
        let out = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, variant, &mut storage).await.unwrap();
        assert_eq!(base, out, "Variant '{variant}' should produce same guest help output");
    }

    // Login and compare again (different content set)
    session.login("tester".into(), 1).await.unwrap();
    let user_base = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "HELP", &mut storage).await.unwrap();
    for variant in ["help", "HELP", "H", "h", "?"] {
        let out = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, variant, &mut storage).await.unwrap();
        assert_eq!(user_base, out, "Variant '{variant}' should produce same logged-in help output");
    }
}
