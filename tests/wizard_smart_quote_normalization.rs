//! Test smart quote normalization in wizard/builder commands
use chrono::Utc;
use meshbbs::tmush::builder_commands::{handle_wizard_step, WizardSession, WizardState};
use meshbbs::tmush::types::CurrencyAmount;
use meshbbs::tmush::TinyMushStore;
use meshbbs::tmush::{ObjectOwner, ObjectRecord, ObjectTrigger};
use std::collections::HashMap;
use tempfile::TempDir;

#[test]
fn test_wizard_normalizes_smart_quotes_in_custom_script() {
    let temp = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp.path()).unwrap();

    // Create a test object
    let obj = ObjectRecord {
        id: "test_obj".to_string(),
        name: "Test Object".to_string(),
        description: "A test object".to_string(),
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
    store.put_object(obj).unwrap();

    // Create wizard session in CustomizeAction state with custom template
    let mut session = WizardSession {
        state: WizardState::CustomizeAction {
            object_id: "test_obj".to_string(),
            object_name: "Test Object".to_string(),
            trigger_type: ObjectTrigger::OnUse,
            action_template: "custom".to_string(),
        },
        available_objects: vec![],
    };

    // Simulate user input with smart quotes (using Unicode escapes)
    let input_with_smart_quotes = "teleport(\u{201C}town_square\u{201D})";

    println!("Input with smart quotes: {}", input_with_smart_quotes);
    println!("Input bytes: {:?}", input_with_smart_quotes.as_bytes());

    // Handle the wizard step - this should normalize quotes and save the trigger
    let result = handle_wizard_step(&mut session, input_with_smart_quotes, &store);

    match result {
        Ok(msg) => {
            println!("✓ Wizard succeeded: {}", msg);

            // Verify the trigger was saved with normalized quotes
            let saved_obj = store.get_object("test_obj").unwrap();
            if let Some(script) = saved_obj.actions.get(&ObjectTrigger::OnUse) {
                println!("Saved script: {}", script);
                println!("Saved script bytes: {:?}", script.as_bytes());

                // Should have straight quotes, not smart quotes
                assert_eq!(
                    script, r#"teleport("town_square")"#,
                    "Script should have normalized straight quotes"
                );
                assert!(
                    !script.contains('\u{201C}'),
                    "Should not contain left smart quote"
                );
                assert!(
                    !script.contains('\u{201D}'),
                    "Should not contain right smart quote"
                );

                // Verify the normalized script can be parsed
                use meshbbs::tmush::trigger::parser::Tokenizer;
                let mut tokenizer = Tokenizer::new(script);
                match tokenizer.tokenize() {
                    Ok(_) => println!("✓ Saved script tokenizes successfully"),
                    Err(e) => panic!("✗ Saved script should tokenize: {}", e),
                }
            } else {
                panic!("Trigger was not saved");
            }
        }
        Err(e) => {
            panic!("✗ Wizard should succeed with normalized quotes: {:?}", e);
        }
    }
}

#[test]
fn test_wizard_normalizes_smart_quotes_in_message() {
    let temp = TempDir::new().unwrap();
    let store = TinyMushStore::open(temp.path()).unwrap();

    // Create a test object
    let obj = ObjectRecord {
        id: "test_obj2".to_string(),
        name: "Test Object 2".to_string(),
        description: "Another test object".to_string(),
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
    store.put_object(obj).unwrap();

    // Create wizard session in CustomizeAction state with message template
    let mut session = WizardSession {
        state: WizardState::CustomizeAction {
            object_id: "test_obj2".to_string(),
            object_name: "Test Object 2".to_string(),
            trigger_type: ObjectTrigger::OnLook,
            action_template: "message".to_string(),
        },
        available_objects: vec![],
    };

    // Simulate user input with smart quotes - simple message without internal quotes
    let input_with_smart_quotes = "It\u{2019}s magical!";

    println!(
        "Message input with smart quotes: {}",
        input_with_smart_quotes
    );

    // Handle the wizard step
    let result = handle_wizard_step(&mut session, input_with_smart_quotes, &store);

    match result {
        Ok(msg) => {
            println!("✓ Wizard succeeded: {}", msg);

            // Verify the trigger was saved with normalized quotes
            let saved_obj = store.get_object("test_obj2").unwrap();
            if let Some(script) = saved_obj.actions.get(&ObjectTrigger::OnLook) {
                println!("Saved message script: {}", script);

                // Should have straight apostrophe
                assert!(
                    script.contains("It's magical!"),
                    "Message should have normalized straight quotes: {}",
                    script
                );
                assert!(
                    !script.contains('\u{2019}'),
                    "Should not contain right smart quote"
                );
            } else {
                panic!("Trigger was not saved");
            }
        }
        Err(e) => {
            panic!("✗ Wizard should succeed: {:?}", e);
        }
    }
}
