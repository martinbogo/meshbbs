//! Test Unicode support in trigger scripts
use meshbbs::tmush::trigger::parser::Tokenizer;

#[test]
fn test_unicode_emoji_in_string() {
    let script = r#"message("✨ The stone flashes!")"#;
    let mut tokenizer = Tokenizer::new(script);

    match tokenizer.tokenize() {
        Ok(tokens) => {
            println!("Tokens: {:?}", tokens);
            // Should successfully tokenize
        }
        Err(e) => {
            panic!("Failed to parse Unicode emoji in string: {}", e);
        }
    }
}

#[test]
fn test_unicode_in_compound_expression() {
    let script = r#"message("✨ Flash!") && teleport("room")"#;
    let mut tokenizer = Tokenizer::new(script);

    match tokenizer.tokenize() {
        Ok(tokens) => {
            println!("Tokens: {:?}", tokens);
            // Should have message function, string with emoji, &&, teleport function, string
        }
        Err(e) => {
            panic!("Failed to parse compound expression with Unicode: {}", e);
        }
    }
}

#[test]
fn test_various_unicode_characters() {
    let test_cases = vec![
        r#"message("🎲 Roll!")"#,
        r#"message("⚔️ Attack!")"#,
        r#"message("🔮 Magic!")"#,
        r#"message("Hello 世界")"#,
        r#"message("Привет мир")"#,
    ];

    for script in test_cases {
        let mut tokenizer = Tokenizer::new(script);
        match tokenizer.tokenize() {
            Ok(_) => { /* Good */ }
            Err(e) => {
                panic!("Failed to parse '{}': {}", script, e);
            }
        }
    }
}
