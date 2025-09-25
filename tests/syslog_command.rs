use meshbbs::config::Config;
use meshbbs::storage::Storage;
use meshbbs::bbs::session::Session;
use meshbbs::bbs::commands::CommandProcessor;

// Basic test: sysop can invoke SYSLOG, regular user cannot.
#[tokio::test]
async fn sysop_syslog_and_user_denied() {
    let cfg = Config::default();
    let mut storage = Storage::new(&cfg.storage.data_dir).await.unwrap();
    let proc = CommandProcessor::new();

    // Regular user session
    let mut user_session = Session::new("s1".into(), "node1".into());
    user_session.login("alice".into(), 1).await.unwrap();
    let denied = proc.process(&mut user_session, "SYSLOG INFO test message", &mut storage, &cfg).await.unwrap();
    assert!(denied.to_lowercase().contains("permission denied"));

    // Sysop session
    let mut sys_session = Session::new("s2".into(), "node2".into());
    sys_session.login("root".into(), 10).await.unwrap();
    let logged = proc.process(&mut sys_session, "SYSLOG WARN something happened", &mut storage, &cfg).await.unwrap();
    assert!(logged.contains("Logged WARN"));

    // Bad usage
    let usage = proc.process(&mut sys_session, "SYSLOG", &mut storage, &cfg).await.unwrap();
    assert!(usage.starts_with("Usage: SYSLOG"));
    let usage2 = proc.process(&mut sys_session, "SYSLOG INFO", &mut storage, &cfg).await.unwrap();
    assert!(usage2.starts_with("Usage: SYSLOG"));
    let usage3 = proc.process(&mut sys_session, "SYSLOG BAD level", &mut storage, &cfg).await.unwrap();
    assert!(usage3.starts_with("Usage: SYSLOG"));
}
