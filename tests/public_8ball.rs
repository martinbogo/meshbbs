use meshbbs::bbs::public::{PublicCommandParser, PublicCommand};

#[test]
fn parse_8ball_command() {
    let parser = PublicCommandParser::new();
    match parser.parse("^8ball") { PublicCommand::EightBall => {}, other => panic!("Expected EightBall, got {:?}", other) }
    match parser.parse("^8BALL") { PublicCommand::EightBall => {}, other => panic!("Expected EightBall uppercase, got {:?}", other) }
}
