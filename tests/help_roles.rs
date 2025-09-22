use meshbbs::config::Config;
use meshbbs::storage::Storage;

// These tests focus on the CommandProcessor HELP output based on session.user_level & login state.
// We bypass full Meshtastic text routing and directly exercise the session/command path.

#[tokio::test]
async fn help_guest_vs_user() {
    let cfg = Config::default(); // config only used for data_dir path
    // Create a session manually (simulate DM) & process HELP while not logged in.
    let mut session = meshbbs::bbs::session::Session::new("s1".into(), "node1".into());
    let mut storage = Storage::new(&cfg.storage.data_dir).await.unwrap();
    let guest = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "help", &mut storage).await.unwrap();
    assert!(guest.contains("REGISTER"), "guest help should mention REGISTER");
    assert!(!guest.contains("DELETE <area>"), "guest help must not show moderator cmds");

    // Simulate login with level 1
    session.login("alice".into(), 1).await.unwrap();
    let user_help = meshbbs::bbs::commands::CommandProcessor::new().process(&mut session, "help", &mut storage).await.unwrap();
    assert!(user_help.contains("ACCT:"), "user help should show ACCT section");
    assert!(!user_help.contains("PROMOTE <user>"), "user help must not show sysop cmds");
}

#[tokio::test]
async fn help_moderator_and_sysop() {
    let cfg = Config::default();
    let mut storage = Storage::new(&cfg.storage.data_dir).await.unwrap();

    // Moderator session
    let mut mod_session = meshbbs::bbs::session::Session::new("s2".into(), "node2".into());
    mod_session.login("mod".into(), 5).await.unwrap();
    let mod_help = meshbbs::bbs::commands::CommandProcessor::new().process(&mut mod_session, "HELP", &mut storage).await.unwrap();
    assert!(mod_help.contains("MOD:"), "moderator help missing MOD section");
    assert!(!mod_help.contains("ADM:"), "moderator help should not show ADM section");

    // Sysop session
    let mut sys_session = meshbbs::bbs::session::Session::new("s3".into(), "node3".into());
    sys_session.login("root".into(), 10).await.unwrap();
    let sys_help = meshbbs::bbs::commands::CommandProcessor::new().process(&mut sys_session, "?", &mut storage).await.unwrap();
    assert!(sys_help.contains("ADM:"), "sysop help missing ADM section");
    assert!(sys_help.contains("PROMOTE"), "sysop help should list PROMOTE");
}
