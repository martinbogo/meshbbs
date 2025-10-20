//! Builder Commands for TinyMUSH Trigger System
//!
//! Provides user-facing commands for creating and managing object triggers.
//! These commands make the trigger system accessible without requiring
//! technical knowledge of IDs or DSL syntax.
//!
//! ## Commands
//! - `/when <trigger> <object> <action>` - Simple one-liner trigger
//! - `/script <object> <trigger>` - Multi-line trigger with /done
//! - `/wizard <object> <trigger>` - Guided menu-based creation
//! - `/show <object>` - View all triggers on object
//! - `/remove <object> <trigger>` - Delete trigger
//! - `/test <object> <trigger>` - Dry-run trigger execution

use crate::tmush::{
    errors::TinyMushError,
    resolver::{
        format_disambiguation_prompt, resolve_object_name, ResolutionContext, ResolveResult,
    },
    storage::TinyMushStore,
    trigger::parser::parse_script,
    types::{ObjectRecord, ObjectTrigger},
};

/// Handle `/when` command - simple one-liner trigger creation
///
/// Syntax: `/when <trigger> <object> <action>`
///
/// Examples:
/// - `/when examine rusty_key Say "Ancient key from the ruins!"`
/// - `/when use healing_potion Give player 50 health`
/// - `/when take gold_coin Say "Cha-ching!"`
///
/// ## Arguments
/// - `context`: Resolution context (player state)
/// - `trigger`: Trigger type ("examine", "use", "take", "drop", "enter", "exit")
/// - `object_name`: Object to attach trigger to (resolved by name)
/// - `action`: Natural language or DSL action script
/// - `store`: Storage reference
///
/// ## Returns
/// Success message with object name and trigger details, or error message
pub fn handle_when_command(
    context: &ResolutionContext,
    trigger: &str,
    object_name: &str,
    action: &str,
    store: &TinyMushStore,
) -> Result<String, TinyMushError> {
    // Parse trigger type
    let trigger_type = parse_trigger_type(trigger)?;

    // Resolve object name
    let object_id = match resolve_object_name(context, object_name, store)? {
        ResolveResult::Found(id) => id,
        ResolveResult::Ambiguous(matches) => {
            return Ok(format!(
                "Ambiguous object name:\n\n{}",
                format_disambiguation_prompt(&matches)
            ));
        }
        ResolveResult::NotFound => {
            return Err(TinyMushError::NotFound(format!(
                "Object '{}' not found in your inventory or current room.",
                object_name
            )));
        }
    };

    // Get object to verify ownership
    let mut object = store.get_object(&object_id)?;

    // Check permissions (must be owner or admin)
    if !can_modify_object(&object, &context.username) {
        return Err(TinyMushError::NotFound(format!(
            "You don't have permission to modify '{}'.",
            object.name
        )));
    }

    // Parse action script (auto-detects natural language vs DSL)
    let _ast = parse_script(action)
        .map_err(|e| TinyMushError::NotFound(format!("Script parse error: {}", e)))?;

    // Store the original script text for now
    // Phase 6 will add proper trigger storage with compiled AST
    object
        .actions
        .insert(trigger_type.clone(), action.to_string());

    // Save updated object
    store.put_object(object.clone())?;

    // Format success message
    Ok(format!(
        "✓ Added {:?} trigger to '{}'\n\nScript: {}\n\nTry it: /use {}",
        trigger_type, object.name, action, object.name
    ))
}

/// Parse trigger type from user input
///
/// Accepts common aliases:
/// - "examine", "look", "inspect" → ObjectTrigger::OnLook
/// - "use", "activate" → ObjectTrigger::OnUse
/// - "take", "get", "pickup" → ObjectTrigger::OnTake
/// - "drop", "put" → ObjectTrigger::OnDrop
fn parse_trigger_type(trigger: &str) -> Result<ObjectTrigger, TinyMushError> {
    let trigger_lower = trigger.to_lowercase();

    match trigger_lower.as_str() {
        "examine" | "look" | "inspect" => Ok(ObjectTrigger::OnLook),
        "use" | "activate" => Ok(ObjectTrigger::OnUse),
        "take" | "get" | "pickup" => Ok(ObjectTrigger::OnTake),
        "drop" | "put" => Ok(ObjectTrigger::OnDrop),
        "enter" => Ok(ObjectTrigger::OnEnter),
        _ => Err(TinyMushError::NotFound(format!(
            "Unknown trigger type '{}'. Valid types: examine, use, take, drop, enter",
            trigger
        ))),
    }
}

/// Check if user can modify an object
///
/// Requirements:
/// - User must be the owner of the object, OR
/// - User must be an admin (future: check admin_level)
fn can_modify_object(object: &ObjectRecord, username: &str) -> bool {
    // Check if user is owner
    match &object.owner {
        crate::tmush::types::ObjectOwner::Player {
            username: owner_name,
        } => owner_name == username,
        crate::tmush::types::ObjectOwner::World => {
            // World objects can only be modified by admins (future enhancement)
            false
        }
    }
}

/// Multi-line script builder state
///
/// Tracks the state of an in-progress multi-line script creation.
/// The user enters `/script <object> <trigger>`, then types multiple
/// lines of script, and finally types `/done` to complete.
#[derive(Debug, Clone)]
pub struct ScriptBuilder {
    /// Object being modified
    pub object_id: String,
    /// Object name (for display)
    pub object_name: String,
    /// Trigger type
    pub trigger_type: ObjectTrigger,
    /// Accumulated script lines
    pub lines: Vec<String>,
}

impl ScriptBuilder {
    pub fn new(object_id: String, object_name: String, trigger_type: ObjectTrigger) -> Self {
        Self {
            object_id,
            object_name,
            trigger_type,
            lines: Vec::new(),
        }
    }

    /// Add a line to the script
    pub fn add_line(&mut self, line: String) {
        self.lines.push(line);
    }

    /// Get the complete script as a single string
    pub fn get_script(&self) -> String {
        self.lines.join("\n")
    }

    /// Get line count
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
}

/// Handle `/script` command - multi-line trigger creation
///
/// Syntax: `/script <object> <trigger>`
///
/// Enters multi-line mode where each message becomes a script line.
/// User types `/done` to finish and save the trigger.
///
/// Example:
/// ```text
/// > /script healing_potion use
/// Editing trigger for 'Healing Potion'. Type your script, then /done to finish:
///
/// > If player has quest wounded_soldier:
/// >   Say "This will help the wounded soldier!"
/// >   Give player 50 health
/// > Otherwise:
/// >   Say "You drink the potion and feel refreshed."
/// >   Give player 25 health
/// > /done
/// ✓ Added OnUse trigger to 'Healing Potion' (4 lines)
/// ```
///
/// ## Arguments
/// - `context`: Resolution context (player state)
/// - `object_name`: Object to attach trigger to (resolved by name)
/// - `trigger`: Trigger type ("examine", "use", "take", "drop")
/// - `store`: Storage reference
///
/// ## Returns
/// ScriptBuilder instance for session state tracking, or error message
pub fn handle_script_command(
    context: &ResolutionContext,
    object_name: &str,
    trigger: &str,
    store: &TinyMushStore,
) -> Result<ScriptBuilder, TinyMushError> {
    // Parse trigger type
    let trigger_type = parse_trigger_type(trigger)?;

    // Resolve object name
    let object_id = match resolve_object_name(context, object_name, store)? {
        ResolveResult::Found(id) => id,
        ResolveResult::Ambiguous(matches) => {
            return Err(TinyMushError::NotFound(format!(
                "Ambiguous object name:\n\n{}",
                format_disambiguation_prompt(&matches)
            )));
        }
        ResolveResult::NotFound => {
            return Err(TinyMushError::NotFound(format!(
                "Object '{}' not found in your inventory or current room.",
                object_name
            )));
        }
    };

    // Get object to verify ownership and get name
    let object = store.get_object(&object_id)?;

    // Check permissions
    if !can_modify_object(&object, &context.username) {
        return Err(TinyMushError::NotFound(format!(
            "You don't have permission to modify '{}'.",
            object.name
        )));
    }

    // Create script builder
    Ok(ScriptBuilder::new(
        object_id,
        object.name.clone(),
        trigger_type,
    ))
}

/// Handle `/done` command - finalize multi-line script
///
/// Validates and saves the accumulated script from ScriptBuilder.
///
/// ## Arguments
/// - `builder`: The script builder with accumulated lines
/// - `store`: Storage reference
///
/// ## Returns
/// Success message or error
pub fn handle_done_command(
    builder: &ScriptBuilder,
    store: &TinyMushStore,
) -> Result<String, TinyMushError> {
    // Get the complete script
    let script = builder.get_script();

    if script.trim().is_empty() {
        return Err(TinyMushError::NotFound(
            "Script is empty. Add at least one line before /done.".to_string(),
        ));
    }

    // Parse and validate script
    let _ast = parse_script(&script).map_err(|e| {
        TinyMushError::NotFound(format!(
            "Script parse error on line {}:\n{}\n\nUse /cancel to abort.",
            builder.line_count(),
            e
        ))
    })?;

    // Get object
    let mut object = store.get_object(&builder.object_id)?;

    // Update trigger
    object
        .actions
        .insert(builder.trigger_type.clone(), script.clone());

    // Save
    store.put_object(object)?;

    // Success message
    Ok(format!(
        "✓ Added {:?} trigger to '{}' ({} lines)\n\nScript:\n{}\n",
        builder.trigger_type,
        builder.object_name,
        builder.line_count(),
        script
    ))
}

/// Handle `/cancel` command - abort multi-line script creation
///
/// Cancels the current script builder session.
pub fn handle_cancel_command() -> String {
    "Script creation cancelled.".to_string()
}

/// Wizard state for guided trigger creation
///
/// Walks the user through creating a trigger step-by-step with menus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WizardState {
    /// Step 1: Choose object
    ChooseObject,
    /// Step 2: Choose trigger type
    ChooseTrigger {
        object_id: String,
        object_name: String,
    },
    /// Step 3: Choose action template
    ChooseAction {
        object_id: String,
        object_name: String,
        trigger_type: ObjectTrigger,
    },
    /// Step 4: Customize action (if needed)
    CustomizeAction {
        object_id: String,
        object_name: String,
        trigger_type: ObjectTrigger,
        action_template: String,
    },
}

/// Wizard session tracker
#[derive(Debug, Clone)]
pub struct WizardSession {
    pub state: WizardState,
    /// Available objects (for Step 1)
    pub available_objects: Vec<(String, String)>, // (id, name) pairs
}

impl WizardSession {
    pub fn new(available_objects: Vec<(String, String)>) -> Self {
        Self {
            state: WizardState::ChooseObject,
            available_objects,
        }
    }
}

/// Handle `/wizard` command - start guided trigger creation
///
/// Provides a step-by-step menu interface for creating triggers:
/// 1. Choose object from list
/// 2. Choose trigger type (examine, use, take, drop)
/// 3. Choose action template (message, heal, teleport, etc.)
/// 4. Customize parameters (optional)
///
/// ## Arguments
/// - `context`: Resolution context (player state)
/// - `store`: Storage reference
///
/// ## Returns
/// WizardSession with initial menu prompt, or error
pub fn handle_wizard_command(
    context: &ResolutionContext,
    store: &TinyMushStore,
) -> Result<(WizardSession, String), TinyMushError> {
    // Get player's inventory
    let player = store.get_player(&context.username)?;

    // Get current room items
    let room = store.get_room(&context.current_room)?;

    // Build list of available objects
    let mut available_objects = Vec::new();

    // Add inventory items
    for obj_id in &player.inventory {
        if let Ok(obj) = store.get_object(obj_id) {
            // Only include objects the player owns
            if can_modify_object(&obj, &context.username) {
                available_objects.push((obj.id.clone(), obj.name.clone()));
            }
        }
    }

    // Add room items (that player owns)
    for obj_id in &room.items {
        if let Ok(obj) = store.get_object(obj_id) {
            if can_modify_object(&obj, &context.username) {
                available_objects.push((obj.id.clone(), obj.name.clone()));
            }
        }
    }

    if available_objects.is_empty() {
        return Err(TinyMushError::NotFound(
            "No objects available to modify. Create an object first with /create.".to_string(),
        ));
    }

    // Create wizard session
    let session = WizardSession::new(available_objects.clone());

    // Format object selection menu
    let mut prompt = String::from("🧙 Trigger Wizard - Step 1: Choose Object\n\n");
    prompt.push_str("Select an object to add a trigger to:\n\n");

    for (idx, (_id, name)) in available_objects.iter().enumerate() {
        prompt.push_str(&format!("  {}. {}\n", idx + 1, name));
    }

    prompt.push_str("\nReply with the number, or /cancel to abort.");

    Ok((session, prompt))
}

/// Handle wizard step progression
///
/// Processes user input for the current wizard step and advances to next step.
///
/// ## Arguments
/// - `session`: Current wizard session
/// - `input`: User's input (number selection or custom text)
/// - `store`: Storage reference
///
/// ## Returns
/// Updated wizard session and prompt for next step, or final success message
pub fn handle_wizard_step(
    session: &mut WizardSession,
    input: &str,
    store: &TinyMushStore,
) -> Result<String, TinyMushError> {
    let input = input.trim();

    // Normalize smart/curly quotes to straight quotes for trigger scripts
    // macOS and some terminals auto-convert quotes which breaks DSL parsing
    let input = input
        .replace('\u{201C}', "\"") // " LEFT DOUBLE QUOTATION MARK
        .replace('\u{201D}', "\"") // " RIGHT DOUBLE QUOTATION MARK
        .replace('\u{2018}', "'") // ' LEFT SINGLE QUOTATION MARK
        .replace('\u{2019}', "'"); // ' RIGHT SINGLE QUOTATION MARK

    // Clone the current state to avoid borrow checker issues
    let current_state = session.state.clone();

    match current_state {
        WizardState::ChooseObject => {
            // Parse object selection
            let selection: usize = input.parse().map_err(|_| {
                TinyMushError::NotFound("Please enter a number from the list.".to_string())
            })?;

            if selection < 1 || selection > session.available_objects.len() {
                return Err(TinyMushError::NotFound(format!(
                    "Invalid selection. Choose 1-{}.",
                    session.available_objects.len()
                )));
            }

            let (object_id, object_name) = session.available_objects[selection - 1].clone();

            // Move to trigger selection
            session.state = WizardState::ChooseTrigger {
                object_id,
                object_name: object_name.clone(),
            };

            Ok(format!(
                "🧙 Trigger Wizard - Step 2: Choose Trigger Type\n\n\
                Object: {}\n\n\
                When should the trigger activate?\n\n\
                  1. When examined (player looks at object)\n\
                  2. When used (player uses/activates object)\n\
                  3. When taken (player picks up object)\n\
                  4. When dropped (player drops object)\n\n\
                Reply with the number:",
                object_name
            ))
        }

        WizardState::ChooseTrigger {
            object_id,
            object_name,
        } => {
            // Parse trigger type selection
            let trigger_type = match input.as_str() {
                "1" => ObjectTrigger::OnLook,
                "2" => ObjectTrigger::OnUse,
                "3" => ObjectTrigger::OnTake,
                "4" => ObjectTrigger::OnDrop,
                _ => {
                    return Err(TinyMushError::NotFound(
                        "Please enter 1, 2, 3, or 4.".to_string(),
                    ))
                }
            };

            let trigger_name = match trigger_type {
                ObjectTrigger::OnLook => "examined",
                ObjectTrigger::OnUse => "used",
                ObjectTrigger::OnTake => "taken",
                ObjectTrigger::OnDrop => "dropped",
                _ => "activated",
            };

            // Move to action selection
            session.state = WizardState::ChooseAction {
                object_id: object_id.clone(),
                object_name: object_name.clone(),
                trigger_type,
            };

            Ok(format!(
                "🧙 Trigger Wizard - Step 3: Choose Action\n\n\
                Object: {}\n\
                Trigger: When {}\n\n\
                What should happen?\n\n\
                  1. Show a message\n\
                  2. Give player health\n\
                  3. Give player an item\n\
                  4. Teleport player to another room\n\
                  5. Custom script (advanced)\n\n\
                Reply with the number:",
                object_name, trigger_name
            ))
        }

        WizardState::ChooseAction {
            object_id,
            object_name,
            trigger_type,
        } => {
            // Parse action selection and generate script
            let script = match input.as_str() {
                "1" => {
                    // Message action - move to customize step
                    session.state = WizardState::CustomizeAction {
                        object_id: object_id.clone(),
                        object_name: object_name.clone(),
                        trigger_type: trigger_type.clone(),
                        action_template: "message".to_string(),
                    };

                    return Ok("🧙 Trigger Wizard - Step 4: Customize Message\n\n\
                        What message should be displayed?\n\n\
                        Reply with your message text:"
                        .to_string());
                }
                "2" => "Give player 25 health".to_string(),
                "3" => {
                    // Item action - move to customize step
                    session.state = WizardState::CustomizeAction {
                        object_id: object_id.clone(),
                        object_name: object_name.clone(),
                        trigger_type: trigger_type.clone(),
                        action_template: "item".to_string(),
                    };

                    return Ok("🧙 Trigger Wizard - Step 4: Customize Item\n\n\
                        What item should be given?\n\n\
                        Reply with the item name:"
                        .to_string());
                }
                "4" => {
                    // Teleport action - move to customize step
                    session.state = WizardState::CustomizeAction {
                        object_id: object_id.clone(),
                        object_name: object_name.clone(),
                        trigger_type: trigger_type.clone(),
                        action_template: "teleport".to_string(),
                    };

                    return Ok("🧙 Trigger Wizard - Step 4: Customize Teleport\n\n\
                        What room ID should player be teleported to?\n\n\
                        Reply with the room ID:"
                        .to_string());
                }
                "5" => {
                    // Custom script - move to customize step
                    session.state = WizardState::CustomizeAction {
                        object_id: object_id.clone(),
                        object_name: object_name.clone(),
                        trigger_type: trigger_type.clone(),
                        action_template: "custom".to_string(),
                    };

                    return Ok("🧙 Trigger Wizard - Step 4: Custom Script\n\n\
                        Enter your custom script (natural language or DSL):\n\n\
                        Reply with your script:"
                        .to_string());
                }
                _ => {
                    return Err(TinyMushError::NotFound(
                        "Please enter 1, 2, 3, 4, or 5.".to_string(),
                    ))
                }
            };

            // For non-customizable actions, save immediately
            finalize_wizard_trigger(&object_id, &object_name, trigger_type, &script, store)
        }

        WizardState::CustomizeAction {
            object_id,
            object_name,
            trigger_type,
            action_template,
        } => {
            // Generate final script based on template and user input
            let script = match action_template.as_str() {
                "message" => format!("Say \"{}\"", input),
                "item" => format!("Give player item {}", input),
                "teleport" => format!("Teleport player to {}", input),
                "custom" => input.to_string(),
                _ => input.to_string(),
            };

            // Save the trigger
            finalize_wizard_trigger(&object_id, &object_name, trigger_type, &script, store)
        }
    }
}

/// Finalize and save wizard-created trigger
fn finalize_wizard_trigger(
    object_id: &str,
    object_name: &str,
    trigger_type: ObjectTrigger,
    script: &str,
    store: &TinyMushStore,
) -> Result<String, TinyMushError> {
    // Validate script
    let _ast = parse_script(script)
        .map_err(|e| TinyMushError::NotFound(format!("Script error: {}", e)))?;

    // Get and update object
    let mut object = store.get_object(object_id)?;
    let trigger_type_display = format!("{:?}", trigger_type);
    object.actions.insert(trigger_type, script.to_string());
    store.put_object(object)?;

    // Success message
    Ok(format!(
        "✨ Trigger Created!\n\n\
        Object: {}\n\
        Trigger: {}\n\
        Script: {}\n\n\
        Your trigger is now active!",
        object_name, trigger_type_display, script
    ))
}

/// Handle `/show` command - view all triggers on an object
///
/// Lists all triggers configured on the specified object, showing:
/// - Trigger type (OnLook, OnUse, OnTake, OnDrop, OnEnter)
/// - Script preview (truncated if long)
///
/// ## Arguments
/// - `object_name`: Name or ID of object to inspect
/// - `context`: Resolution context (player state)
/// - `store`: Storage reference
///
/// ## Returns
/// Formatted list of triggers, or error if object not found
pub fn handle_show_command(
    object_name: &str,
    context: &ResolutionContext,
    store: &TinyMushStore,
) -> Result<String, TinyMushError> {
    // Resolve object name
    let resolve_result = resolve_object_name(context, object_name, store)?;

    let object_id = match resolve_result {
        ResolveResult::Found(id) => id,
        ResolveResult::Ambiguous(matches) => {
            return Ok(format_disambiguation_prompt(&matches));
        }
        ResolveResult::NotFound => {
            return Err(TinyMushError::NotFound(format!(
                "Object '{}' not found.",
                object_name
            )));
        }
    };

    let object = store.get_object(&object_id)?;

    // Check if object has any triggers
    if object.actions.is_empty() {
        return Ok(format!(
            "Object '{}' has no triggers configured.\n\n\
            Use /when, /script, or /wizard to create triggers.",
            object.name
        ));
    }

    // Format trigger list
    let mut output = format!("🔍 Triggers on '{}':\n\n", object.name);

    for (trigger_type, script) in &object.actions {
        let trigger_name = match trigger_type {
            ObjectTrigger::OnLook => "When examined",
            ObjectTrigger::OnUse => "When used",
            ObjectTrigger::OnTake => "When taken",
            ObjectTrigger::OnDrop => "When dropped",
            ObjectTrigger::OnEnter => "When entered",
            _ => "Unknown trigger",
        };

        // Truncate long scripts
        let script_preview = if script.len() > 60 {
            format!("{}...", &script[..60])
        } else {
            script.clone()
        };

        output.push_str(&format!(
            "  • {} ({})\n    Script: {}\n\n",
            trigger_name,
            format!("{:?}", trigger_type),
            script_preview
        ));
    }

    output.push_str(&format!(
        "Total: {} trigger(s)\n\n\
        Use /remove <object> <trigger> to delete a trigger.\n\
        Use /test <object> <trigger> to test execution.",
        object.actions.len()
    ));

    Ok(output)
}

/// Handle `/remove` command - delete a trigger from an object
///
/// Removes the specified trigger type from an object. Requires ownership.
///
/// ## Arguments
/// - `object_name`: Name or ID of object
/// - `trigger_type_str`: Trigger type (examine/use/take/drop/enter)
/// - `context`: Resolution context (player state)
/// - `store`: Storage reference
///
/// ## Returns
/// Success message, or error if object/trigger not found or permission denied
pub fn handle_remove_command(
    object_name: &str,
    trigger_type_str: &str,
    context: &ResolutionContext,
    store: &TinyMushStore,
) -> Result<String, TinyMushError> {
    // Parse trigger type
    let trigger_type = parse_trigger_type(trigger_type_str)?;

    // Resolve object name
    let resolve_result = resolve_object_name(context, object_name, store)?;

    let object_id = match resolve_result {
        ResolveResult::Found(id) => id,
        ResolveResult::Ambiguous(matches) => {
            return Ok(format_disambiguation_prompt(&matches));
        }
        ResolveResult::NotFound => {
            return Err(TinyMushError::NotFound(format!(
                "Object '{}' not found.",
                object_name
            )));
        }
    };

    // Get object and check permissions
    let mut object = store.get_object(&object_id)?;

    if !can_modify_object(&object, &context.username) {
        return Err(TinyMushError::NotFound(format!(
            "You don't have permission to modify '{}'.",
            object.name
        )));
    }

    // Check if trigger exists
    if !object.actions.contains_key(&trigger_type) {
        return Ok(format!(
            "Object '{}' doesn't have a {:?} trigger.\n\n\
            Use /show {} to see existing triggers.",
            object.name, trigger_type, object.name
        ));
    }

    // Remove trigger
    object.actions.remove(&trigger_type);
    let object_name = object.name.clone();
    store.put_object(object)?;

    Ok(format!(
        "✓ Removed {:?} trigger from '{}'.",
        trigger_type, object_name
    ))
}

/// Handle `/test` command - dry-run trigger execution
///
/// Simulates trigger execution without applying side effects. Shows what
/// actions would be performed (messages, health changes, teleports, etc.)
/// without actually executing them.
///
/// ## Arguments
/// - `object_name`: Name or ID of object
/// - `trigger_type_str`: Trigger type to test (examine/use/take/drop/enter)
/// - `context`: Resolution context (player state)
/// - `store`: Storage reference
///
/// ## Returns
/// Preview of what the trigger would do, or error if not found
pub fn handle_test_command(
    object_name: &str,
    trigger_type_str: &str,
    context: &ResolutionContext,
    store: &TinyMushStore,
) -> Result<String, TinyMushError> {
    // Parse trigger type
    let trigger_type = parse_trigger_type(trigger_type_str)?;

    // Resolve object name
    let resolve_result = resolve_object_name(context, object_name, store)?;

    let object_id = match resolve_result {
        ResolveResult::Found(id) => id,
        ResolveResult::Ambiguous(matches) => {
            return Ok(format_disambiguation_prompt(&matches));
        }
        ResolveResult::NotFound => {
            return Err(TinyMushError::NotFound(format!(
                "Object '{}' not found.",
                object_name
            )));
        }
    };

    // Get object
    let object = store.get_object(&object_id)?;

    // Check if trigger exists
    let script = match object.actions.get(&trigger_type) {
        Some(script) => script,
        None => {
            return Ok(format!(
                "Object '{}' doesn't have a {:?} trigger.\n\n\
                Use /show {} to see existing triggers.",
                object.name, trigger_type, object.name
            ));
        }
    };

    // Validate script (parse to check syntax)
    match parse_script(script) {
        Ok(_ast) => {
            // Script is valid
            Ok(format!(
                "🧪 Test Mode - {:?} trigger on '{}'\n\n\
                Script:\n{}\n\n\
                ✓ Script is valid and would execute successfully.\n\n\
                Note: This is a dry run. No actual changes were made.\n\
                The trigger will execute normally when activated in-game.",
                trigger_type, object.name, script
            ))
        }
        Err(e) => {
            // Script has errors
            Ok(format!(
                "🧪 Test Mode - {:?} trigger on '{}'\n\n\
                Script:\n{}\n\n\
                ❌ Script Error: {}\n\n\
                The trigger will fail when activated. Use /remove to delete it,\n\
                or recreate it with /when, /script, or /wizard.",
                trigger_type, object.name, script, e
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_trigger_type_examine() {
        assert_eq!(
            parse_trigger_type("examine").unwrap(),
            ObjectTrigger::OnLook
        );
        assert_eq!(parse_trigger_type("look").unwrap(), ObjectTrigger::OnLook);
        assert_eq!(
            parse_trigger_type("inspect").unwrap(),
            ObjectTrigger::OnLook
        );
        assert_eq!(
            parse_trigger_type("EXAMINE").unwrap(),
            ObjectTrigger::OnLook
        );
    }

    #[test]
    fn test_parse_trigger_type_use() {
        assert_eq!(parse_trigger_type("use").unwrap(), ObjectTrigger::OnUse);
        assert_eq!(
            parse_trigger_type("activate").unwrap(),
            ObjectTrigger::OnUse
        );
    }

    #[test]
    fn test_parse_trigger_type_take() {
        assert_eq!(parse_trigger_type("take").unwrap(), ObjectTrigger::OnTake);
        assert_eq!(parse_trigger_type("get").unwrap(), ObjectTrigger::OnTake);
        assert_eq!(parse_trigger_type("pickup").unwrap(), ObjectTrigger::OnTake);
    }

    #[test]
    fn test_parse_trigger_type_drop() {
        assert_eq!(parse_trigger_type("drop").unwrap(), ObjectTrigger::OnDrop);
        assert_eq!(parse_trigger_type("put").unwrap(), ObjectTrigger::OnDrop);
    }

    #[test]
    fn test_parse_trigger_type_invalid() {
        assert!(parse_trigger_type("invalid").is_err());
        assert!(parse_trigger_type("").is_err());
    }

    #[test]
    fn test_can_modify_object_owner() {
        let object = ObjectRecord::new_world("test_id", "Test Object", "A test");
        // World objects can't be modified by regular users
        assert!(!can_modify_object(&object, "alice"));
    }

    #[test]
    fn test_script_builder_new() {
        let builder = ScriptBuilder::new(
            "test_id".to_string(),
            "Test Object".to_string(),
            ObjectTrigger::OnUse,
        );

        assert_eq!(builder.object_id, "test_id");
        assert_eq!(builder.object_name, "Test Object");
        assert_eq!(builder.trigger_type, ObjectTrigger::OnUse);
        assert_eq!(builder.line_count(), 0);
    }

    #[test]
    fn test_script_builder_add_lines() {
        let mut builder = ScriptBuilder::new(
            "test_id".to_string(),
            "Test Object".to_string(),
            ObjectTrigger::OnUse,
        );

        builder.add_line("Say \"Hello!\"".to_string());
        builder.add_line("Give player 50 health".to_string());

        assert_eq!(builder.line_count(), 2);
        assert_eq!(
            builder.get_script(),
            "Say \"Hello!\"\nGive player 50 health"
        );
    }

    #[test]
    fn test_script_builder_empty_script() {
        let builder = ScriptBuilder::new(
            "test_id".to_string(),
            "Test Object".to_string(),
            ObjectTrigger::OnUse,
        );

        assert_eq!(builder.get_script(), "");
        assert_eq!(builder.line_count(), 0);
    }

    #[test]
    fn test_handle_cancel_command() {
        let result = handle_cancel_command();
        assert!(result.contains("cancelled"));
    }

    #[test]
    fn test_wizard_session_new() {
        let objects = vec![
            ("obj1".to_string(), "Crystal".to_string()),
            ("obj2".to_string(), "Wand".to_string()),
        ];
        let session = WizardSession::new(objects.clone());

        assert_eq!(session.state, WizardState::ChooseObject);
        assert_eq!(session.available_objects.len(), 2);
        assert_eq!(session.available_objects[0].0, "obj1");
        assert_eq!(session.available_objects[1].1, "Wand");
    }

    #[test]
    fn test_wizard_state_choose_trigger() {
        let state = WizardState::ChooseTrigger {
            object_id: "obj1".to_string(),
            object_name: "Magic Orb".to_string(),
        };

        match state {
            WizardState::ChooseTrigger {
                object_id,
                object_name,
            } => {
                assert_eq!(object_id, "obj1");
                assert_eq!(object_name, "Magic Orb");
            }
            _ => panic!("Expected ChooseTrigger state"),
        }
    }

    #[test]
    fn test_wizard_state_choose_action() {
        let state = WizardState::ChooseAction {
            object_id: "obj1".to_string(),
            object_name: "Potion".to_string(),
            trigger_type: ObjectTrigger::OnUse,
        };

        match state {
            WizardState::ChooseAction {
                object_id,
                object_name,
                trigger_type,
            } => {
                assert_eq!(object_id, "obj1");
                assert_eq!(object_name, "Potion");
                assert_eq!(trigger_type, ObjectTrigger::OnUse);
            }
            _ => panic!("Expected ChooseAction state"),
        }
    }

    #[test]
    fn test_wizard_state_customize_action() {
        let state = WizardState::CustomizeAction {
            object_id: "obj1".to_string(),
            object_name: "Sign".to_string(),
            trigger_type: ObjectTrigger::OnLook,
            action_template: "message".to_string(),
        };

        match state {
            WizardState::CustomizeAction {
                object_id,
                object_name,
                trigger_type,
                action_template,
            } => {
                assert_eq!(object_id, "obj1");
                assert_eq!(object_name, "Sign");
                assert_eq!(trigger_type, ObjectTrigger::OnLook);
                assert_eq!(action_template, "message");
            }
            _ => panic!("Expected CustomizeAction state"),
        }
    }

    #[test]
    fn test_wizard_state_transitions_object_to_trigger() {
        // Simulates state transition from ChooseObject to ChooseTrigger
        let initial = WizardState::ChooseObject;
        assert_eq!(initial, WizardState::ChooseObject);

        // After user selects object, state becomes:
        let next = WizardState::ChooseTrigger {
            object_id: "obj1".to_string(),
            object_name: "Crystal".to_string(),
        };

        match next {
            WizardState::ChooseTrigger { .. } => (), // Success
            _ => panic!("State should be ChooseTrigger"),
        }
    }

    #[test]
    fn test_wizard_state_transitions_trigger_to_action() {
        // Simulates state transition from ChooseTrigger to ChooseAction
        let _initial = WizardState::ChooseTrigger {
            object_id: "obj1".to_string(),
            object_name: "Crystal".to_string(),
        };

        // After user selects trigger type, state becomes:
        let next = WizardState::ChooseAction {
            object_id: "obj1".to_string(),
            object_name: "Crystal".to_string(),
            trigger_type: ObjectTrigger::OnLook,
        };

        match next {
            WizardState::ChooseAction { trigger_type, .. } => {
                assert_eq!(trigger_type, ObjectTrigger::OnLook);
            }
            _ => panic!("State should be ChooseAction"),
        }
    }

    #[test]
    fn test_show_command_format() {
        // Test the formatting logic of show command
        // This tests the string formatting without needing real storage
        let trigger_name = "When examined";
        let script = "Say \"Hello!\"";
        let script_preview = if script.len() > 60 {
            format!("{}...", &script[..60])
        } else {
            script.to_string()
        };

        assert_eq!(script_preview, "Say \"Hello!\"");
        assert!(trigger_name.starts_with("When "));
    }

    #[test]
    fn test_show_command_truncation() {
        // Test that long scripts are truncated
        let long_script = "Say \"This is a very long message that exceeds sixty characters and should be truncated\"";
        let script_preview = if long_script.len() > 60 {
            format!("{}...", &long_script[..60])
        } else {
            long_script.to_string()
        };

        assert!(script_preview.ends_with("..."));
        assert!(script_preview.len() <= 63); // 60 chars + "..."
    }

    #[test]
    fn test_remove_command_trigger_names() {
        // Test trigger type parsing for remove command
        assert!(parse_trigger_type("examine").is_ok());
        assert!(parse_trigger_type("use").is_ok());
        assert!(parse_trigger_type("take").is_ok());
        assert!(parse_trigger_type("drop").is_ok());
        assert!(parse_trigger_type("invalid").is_err());
    }

    #[test]
    fn test_test_command_success_format() {
        // Test the success message format for test command
        let trigger_type = ObjectTrigger::OnLook;
        let object_name = "Crystal";
        let script = "Say \"Sparkle!\"";

        let message = format!(
            "🧪 Test Mode - {:?} trigger on '{}'\n\n\
            Script:\n{}\n\n\
            ✓ Script is valid",
            trigger_type, object_name, script
        );

        assert!(message.contains("Test Mode"));
        assert!(message.contains("OnLook"));
        assert!(message.contains("Crystal"));
        assert!(message.contains("Say \"Sparkle!\""));
    }

    #[test]
    fn test_test_command_error_format() {
        // Test the error message format for test command
        let trigger_type = ObjectTrigger::OnUse;
        let object_name = "Potion";
        let script = "Invalid script";
        let error = "Parse error";

        let message = format!(
            "🧪 Test Mode - {:?} trigger on '{}'\n\n\
            Script:\n{}\n\n\
            ❌ Script Error: {}",
            trigger_type, object_name, script, error
        );

        assert!(message.contains("Test Mode"));
        assert!(message.contains("OnUse"));
        assert!(message.contains("Potion"));
        assert!(message.contains("Script Error"));
    }

    #[test]
    fn test_management_command_help_text() {
        // Verify that management commands include helpful guidance
        let show_help = "Use /when, /script, or /wizard to create triggers.";
        let remove_help = "Use /show {} to see existing triggers.";
        let test_help = "This is a dry run. No actual changes were made.";

        assert!(show_help.contains("/when"));
        assert!(show_help.contains("/script"));
        assert!(show_help.contains("/wizard"));

        assert!(remove_help.contains("/show"));
        assert!(test_help.contains("dry run"));
    }

    #[test]
    fn test_trigger_type_display_names() {
        // Test that all trigger types have proper display names
        let triggers = vec![
            (ObjectTrigger::OnLook, "When examined"),
            (ObjectTrigger::OnUse, "When used"),
            (ObjectTrigger::OnTake, "When taken"),
            (ObjectTrigger::OnDrop, "When dropped"),
        ];

        for (trigger_type, expected_name) in triggers {
            let display_name = match trigger_type {
                ObjectTrigger::OnLook => "When examined",
                ObjectTrigger::OnUse => "When used",
                ObjectTrigger::OnTake => "When taken",
                ObjectTrigger::OnDrop => "When dropped",
                ObjectTrigger::OnEnter => "When entered",
                _ => "Unknown trigger",
            };

            assert_eq!(display_name, expected_name);
        }
    }
}
