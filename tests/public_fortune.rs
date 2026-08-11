use meshbbs::bbs::public::{PublicCommand, PublicCommandParser};
use std::sync::OnceLock;

#[test]
fn parse_fortune_command() {
    let parser = PublicCommandParser::new();
    match parser.parse("^fortune") {
        PublicCommand::Fortune => {}
        other => panic!("Expected Fortune, got {:?}", other),
    }
    match parser.parse("^FORTUNE") {
        PublicCommand::Fortune => {}
        other => panic!("Expected Fortune uppercase, got {:?}", other),
    }
    match parser.parse("^Fortune") {
        PublicCommand::Fortune => {}
        other => panic!("Expected Fortune mixed case, got {:?}", other),
    }
}

/// Fortunes are loaded once per process from `fortunes.json`, so every test in
/// this binary shares one fixture directory that outlives the first load.
static FIXTURE: OnceLock<tempfile::TempDir> = OnceLock::new();

fn init_fortunes() {
    let dir = FIXTURE.get_or_init(|| {
        let dir = tempfile::tempdir().expect("create fixture dir");
        let entries: Vec<String> = (0..25)
            .map(|i| format!("\"Test fortune number {i}\""))
            .collect();
        std::fs::write(
            dir.path().join("fortunes.json"),
            format!("{{\"fortunes\":[{}]}}", entries.join(",")),
        )
        .expect("write fortunes.json");
        dir
    });
    meshbbs::bbs::fortune::initialize(dir.path());
}

#[test]
fn fortune_basic_functionality() {
    init_fortunes();
    // Test that fortune returns a valid string
    let fortune = meshbbs::bbs::fortune::random_fortune().expect("fortunes loaded");
    assert!(!fortune.is_empty());
    assert!(fortune.len() <= 200); // All fortunes should be under 200 chars
}

#[test]
fn fortune_returns_different_values() {
    init_fortunes();
    // Test randomness by collecting multiple fortunes
    let mut fortunes = std::collections::HashSet::new();
    for _ in 0..20 {
        fortunes.insert(meshbbs::bbs::fortune::random_fortune().expect("fortunes loaded"));
    }
    // Should get at least a few different fortunes
    assert!(
        fortunes.len() >= 5,
        "Expected variety in fortune responses, got only {} unique",
        fortunes.len()
    );
}
