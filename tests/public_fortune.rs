use meshbbs::bbs::public::{PublicCommandParser, PublicCommand};

#[test]
fn parse_fortune_command() {
    let parser = PublicCommandParser::new();
    match parser.parse("^fortune") { PublicCommand::Fortune => {}, other => panic!("Expected Fortune, got {:?}", other) }
    match parser.parse("^FORTUNE") { PublicCommand::Fortune => {}, other => panic!("Expected Fortune uppercase, got {:?}", other) }
    match parser.parse("^Fortune") { PublicCommand::Fortune => {}, other => panic!("Expected Fortune mixed case, got {:?}", other) }
}

#[test]
fn fortune_basic_functionality() {
    // Test that fortune returns a valid string
    let fortune = meshbbs::bbs::fortune::get_fortune();
    assert!(!fortune.is_empty());
    assert!(fortune.len() <= 200); // All fortunes should be under 200 chars
}

#[test]
fn fortune_returns_different_values() {
    // Test randomness by collecting multiple fortunes
    let mut fortunes = std::collections::HashSet::new();
    for _ in 0..20 {
        fortunes.insert(meshbbs::bbs::fortune::get_fortune());
    }
    // Should get at least a few different fortunes
    assert!(fortunes.len() >= 5, "Expected variety in fortune responses, got only {} unique", fortunes.len());
}