use meshbbs::bbs::public::{PublicCommandParser, PublicCommand};

#[test]
fn test_help_command() {
    let parser = PublicCommandParser::new();
    match parser.parse("help") { PublicCommand::Help => {}, other => panic!("Expected Help, got {:?}", other) }
}

#[test]
fn test_login_command() {
    let parser = PublicCommandParser::new();
    match parser.parse("login Alice") { PublicCommand::Login(u) => assert_eq!(u, "Alice"), other => panic!("Expected Login, got {:?}", other) }
}

#[test]
fn test_invalid_login_no_name() {
    let parser = PublicCommandParser::new();
    match parser.parse("login") { PublicCommand::Invalid(_) => {}, other => panic!("Expected Invalid, got {:?}", other) }
}

#[test]
fn test_unknown() {
    let parser = PublicCommandParser::new();
    match parser.parse("garbage") { PublicCommand::Unknown => {}, other => panic!("Expected Unknown, got {:?}", other) }
}