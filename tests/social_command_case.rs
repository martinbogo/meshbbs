//! Test that social/text commands preserve case
//! Bug: SAY, EMOTE, POSE, WHISPER were uppercasing user text

use meshbbs::tmush::commands::TinyMushProcessor;
use meshbbs::tmush::TinyMushStore;
use tempfile::TempDir;

#[test]
fn test_command_parsing_preserves_case_for_say() {
    let temp = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp.path()).unwrap();
    let processor = TinyMushProcessor::new(store);

    // Parse SAY command with mixed case
    let cmd = processor.parse_command("SAY Hello World! This is a Test.");

    // Check that it's parsed as a Say command with preserved case
    if let meshbbs::tmush::commands::TinyMushCommand::Say(text) = cmd {
        assert_eq!(
            text, "Hello World! This is a Test.",
            "SAY text should preserve case"
        );
    } else {
        panic!("Expected Say command, got: {:?}", cmd);
    }
}

#[test]
fn test_command_parsing_preserves_case_for_emote() {
    let temp = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp.path()).unwrap();
    let processor = TinyMushProcessor::new(store);

    // Parse EMOTE command with mixed case
    let cmd = processor.parse_command("EMOTE waves Happily!");

    if let meshbbs::tmush::commands::TinyMushCommand::Emote(text) = cmd {
        assert_eq!(text, "waves Happily!", "EMOTE text should preserve case");
    } else {
        panic!("Expected Emote command, got: {:?}", cmd);
    }
}

#[test]
fn test_command_parsing_preserves_case_for_emote_shortcut() {
    let temp = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp.path()).unwrap();
    let processor = TinyMushProcessor::new(store);

    // Parse : shortcut with mixed case (note: space after : is required)
    let cmd = processor.parse_command(": laughs Merrily");

    if let meshbbs::tmush::commands::TinyMushCommand::Emote(text) = cmd {
        assert_eq!(text, "laughs Merrily", ": shortcut should preserve case");
    } else {
        panic!("Expected Emote command, got: {:?}", cmd);
    }
}

#[test]
fn test_command_parsing_preserves_case_for_pose() {
    let temp = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp.path()).unwrap();
    let processor = TinyMushProcessor::new(store);

    // Parse POSE command with mixed case
    let cmd = processor.parse_command("POSE sits Down Quietly.");

    if let meshbbs::tmush::commands::TinyMushCommand::Pose(text) = cmd {
        assert_eq!(text, "sits Down Quietly.", "POSE text should preserve case");
    } else {
        panic!("Expected Pose command, got: {:?}", cmd);
    }
}

#[test]
fn test_command_parsing_preserves_case_for_pose_shortcut() {
    let temp = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp.path()).unwrap();
    let processor = TinyMushProcessor::new(store);

    // Parse ; shortcut with mixed case (note: space after ; is required)
    let cmd = processor.parse_command("; stands Tall");

    if let meshbbs::tmush::commands::TinyMushCommand::Pose(text) = cmd {
        assert_eq!(text, "stands Tall", "; shortcut should preserve case");
    } else {
        panic!("Expected Pose command, got: {:?}", cmd);
    }
}

#[test]
fn test_command_parsing_preserves_case_for_ooc() {
    let temp = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp.path()).unwrap();
    let processor = TinyMushProcessor::new(store);

    // Parse OOC command with mixed case
    let cmd = processor.parse_command("OOC This is Out Of Character text!");

    if let meshbbs::tmush::commands::TinyMushCommand::Ooc(text) = cmd {
        assert_eq!(
            text, "This is Out Of Character text!",
            "OOC text should preserve case"
        );
    } else {
        panic!("Expected Ooc command, got: {:?}", cmd);
    }
}

#[test]
fn test_command_parsing_preserves_case_for_whisper() {
    let temp = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp.path()).unwrap();
    let processor = TinyMushProcessor::new(store);

    // Parse WHISPER command with mixed case
    let cmd = processor.parse_command("WHISPER Alice Secret Message Here!");

    if let meshbbs::tmush::commands::TinyMushCommand::Whisper(target, message) = cmd {
        // Target name should preserve case
        assert_eq!(target, "Alice", "WHISPER target should preserve case");
        assert_eq!(
            message, "Secret Message Here!",
            "WHISPER message should preserve case"
        );
    } else {
        panic!("Expected Whisper command, got: {:?}", cmd);
    }
}

#[test]
fn test_regular_commands_still_case_insensitive() {
    let temp = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp.path()).unwrap();
    let processor = TinyMushProcessor::new(store);

    // Regular commands should still be case-insensitive
    let cmd1 = processor.parse_command("inventory");
    let cmd2 = processor.parse_command("INVENTORY");
    let cmd3 = processor.parse_command("InVeNtOrY");

    // All should parse to the same command type
    assert!(matches!(
        cmd1,
        meshbbs::tmush::commands::TinyMushCommand::Inventory
    ));
    assert!(matches!(
        cmd2,
        meshbbs::tmush::commands::TinyMushCommand::Inventory
    ));
    assert!(matches!(
        cmd3,
        meshbbs::tmush::commands::TinyMushCommand::Inventory
    ));
}
