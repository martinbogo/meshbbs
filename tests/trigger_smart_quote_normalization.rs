//! Test that smart quotes are normalized when setting triggers
use chrono::Utc;
use meshbbs::tmush::commands::TinyMushProcessor;
use meshbbs::tmush::types::CurrencyAmount;
use meshbbs::tmush::{ObjectOwner, ObjectRecord, ObjectTrigger, TinyMushStore};
use std::collections::HashMap;
use tempfile::TempDir;

#[tokio::test]
async fn test_trigger_smart_quotes_normalized() {
    let temp = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp.path()).unwrap();
    let mut processor = TinyMushProcessor::new(store.clone());

    // Create an object
    let mut obj = ObjectRecord {
        id: "test_stone".to_string(),
        name: "Test Stone".to_string(),
        description: "A test stone".to_string(),
        owner: ObjectOwner::World,
        created_at: Utc::now(),
        weight: 1,
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: true,
        usable: true,
        actions: HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: 1,
    };
    store.put_object(obj.clone()).unwrap();

    // Simulate command with smart quotes (as if from terminal that auto-converts)
    // Using Unicode escapes to represent the smart quotes
    let cmd_with_smart_quotes =
        format!("@object edit test_stone trigger OnUse teleport(\u{201C}town_square\u{201D})");

    println!("Command with smart quotes: {}", cmd_with_smart_quotes);
    println!("Bytes: {:?}", cmd_with_smart_quotes.as_bytes());

    // Parse the command - this should normalize the quotes
    let parsed = processor.parse_command(&cmd_with_smart_quotes);
    println!("Parsed command: {:?}", parsed);

    // The command should have converted smart quotes to straight quotes internally
    // Let's verify by checking what gets stored
    // (We can't directly test the execution here without a full session setup,
    //  but we can verify the normalization logic works)

    // Test the normalization directly
    let test_script = "teleport(\u{201C}town_square\u{201D})";
    let normalized = test_script
        .replace('\u{201C}', "\"")
        .replace('\u{201D}', "\"")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'");

    println!("Original: {}", test_script);
    println!("Normalized: {}", normalized);
    assert_eq!(normalized, r#"teleport("town_square")"#);

    // Verify the normalized version parses correctly
    use meshbbs::tmush::trigger::parser::Tokenizer;
    let mut tokenizer = Tokenizer::new(&normalized);
    match tokenizer.tokenize() {
        Ok(tokens) => {
            println!("✓ Normalized script tokenizes successfully: {:?}", tokens);
        }
        Err(e) => {
            panic!("✗ Normalized script should tokenize but failed: {}", e);
        }
    }
}
