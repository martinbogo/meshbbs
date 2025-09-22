use meshbbs::bbs::public::{PublicCommandParser, PublicCommand};

#[test]
fn public_help_does_not_accept_single_letter() {
    let parser = PublicCommandParser::new();
    assert_eq!(parser.parse("^h"), PublicCommand::Unknown, "^h should not be parsed as Help");
    assert_eq!(parser.parse("^H"), PublicCommand::Unknown, "^H should not be parsed as Help");
    assert_eq!(parser.parse("^help"), PublicCommand::Help, "^help should be parsed as Help");
    assert_eq!(parser.parse("^HELP"), PublicCommand::Help, "^HELP should be parsed as Help");
}
