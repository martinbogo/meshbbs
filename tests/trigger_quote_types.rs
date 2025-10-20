//! Test to reproduce the quote parsing issue
use meshbbs::tmush::trigger::parser::{detect_syntax_type, Tokenizer};

#[test]
fn test_teleport_simple() {
    let script = r#"teleport("town_square")"#;
    println!("Script: {}", script);
    println!("Script bytes: {:?}", script.as_bytes());
    println!("Syntax type: {:?}", detect_syntax_type(script));

    let mut tokenizer = Tokenizer::new(script);
    match tokenizer.tokenize() {
        Ok(tokens) => {
            println!("✓ Tokens: {:?}", tokens);
        }
        Err(e) => {
            panic!("✗ Failed to tokenize: {}", e);
        }
    }
}

#[test]
fn test_smart_quotes() {
    // Test with smart/curly quotes (U+201C and U+201D)
    // Using string literals with hex escapes to avoid source file issues
    let script_smart = "teleport(\u{201C}town_square\u{201D})";
    println!("Smart quotes script: {}", script_smart);
    println!("Smart quotes bytes: {:?}", script_smart.as_bytes());

    let mut tokenizer = Tokenizer::new(script_smart);
    match tokenizer.tokenize() {
        Ok(tokens) => {
            println!("✓ Unexpectedly succeeded with smart quotes: {:?}", tokens);
        }
        Err(e) => {
            println!("✗ Expected error with smart quotes: {}", e);
            assert!(
                e.contains("Unexpected character"),
                "Should fail on smart quotes"
            );
        }
    }
}

#[test]
fn test_straight_quotes() {
    // Test with straight ASCII quotes (U+0022)
    let script_straight = r#"teleport("town_square")"#;
    println!("Straight quotes script: {}", script_straight);
    println!("Straight quotes bytes: {:?}", script_straight.as_bytes());

    let mut tokenizer = Tokenizer::new(script_straight);
    match tokenizer.tokenize() {
        Ok(tokens) => {
            println!("✓ Correctly tokenized with straight quotes: {:?}", tokens);
        }
        Err(e) => {
            panic!("✗ Should not fail with straight quotes: {}", e);
        }
    }
}

#[test]
fn test_char_codes() {
    // ASCII straight quote
    let straight = '"';
    println!(
        "Straight quote: '{}' = U+{:04X} = {}",
        straight, straight as u32, straight as u32
    );

    // Unicode smart quotes using hex escapes
    let left_smart = '\u{201C}'; // U+201C LEFT DOUBLE QUOTATION MARK
    let right_smart = '\u{201D}'; // U+201D RIGHT DOUBLE QUOTATION MARK
    println!(
        "Left smart quote: '{}' = U+{:04X} = {}",
        left_smart, left_smart as u32, left_smart as u32
    );
    println!(
        "Right smart quote: '{}' = U+{:04X} = {}",
        right_smart, right_smart as u32, right_smart as u32
    );
}
