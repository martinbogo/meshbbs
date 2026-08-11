pub mod admin;
pub mod evaluator;
pub mod integration;
/// Trigger execution engine for TinyMUSH Phase 7
///
/// This module provides a safe, sandboxed scripting system for interactive objects.
/// Triggers fire on player actions (OnLook, OnUse, OnTake, etc.) and execute
/// simple DSL scripts with conditions and actions.
///
/// **Security Features:**
/// - Script length limit: 512 characters
/// - Execution timeout: 100ms
/// - Max actions per trigger: 10
/// - Max messages per trigger: 3
/// - Sandboxed execution (no file/network access)
/// - Rate limiting (100 executions/minute per object)
///
/// **DSL Syntax:**
/// ```text
/// message("text") && condition ? action : fallback
/// ```
pub mod parser;
pub mod rate_limit;

use crate::tmush::errors::TinyMushError;
use crate::tmush::storage::TinyMushStore;
use crate::tmush::types::{ObjectRecord, ObjectTrigger, PlayerRecord, RoomRecord};
use std::time::{Duration, Instant};

pub use admin::{
    check_trigger_allowed, disable_object_trigger, enable_object_trigger, format_trigger_stats,
    get_disabled_objects, get_trigger_stats, is_object_trigger_disabled, is_trigger_system_enabled,
    record_trigger_execution, set_trigger_system_enabled,
};
pub use evaluator::{Evaluator, Value};
pub use integration::{
    execute_on_drop, execute_on_look, execute_on_poke, execute_on_take, execute_on_use,
    execute_room_on_enter,
};
pub use parser::{parse_script, AstNode, BinaryOperator};
pub use rate_limit::{RateLimitReason, RateLimitStats, TriggerRateLimiter};

/// Maximum script length in characters
pub const MAX_SCRIPT_LENGTH: usize = 512;

/// Maximum execution time per trigger
pub const MAX_EXECUTION_TIME: Duration = Duration::from_millis(100);

/// Maximum actions that can execute in a single trigger
pub const MAX_ACTIONS_PER_TRIGGER: u8 = 10;

/// Maximum messages that can be sent in a single trigger
pub const MAX_MESSAGES_PER_TRIGGER: u8 = 3;

/// Maximum nesting depth for expressions
pub const MAX_NESTED_DEPTH: u8 = 5;

/// Maximum trigger executions per minute per object
pub const MAX_EXECUTIONS_PER_MINUTE: u16 = 100;

/// Execution context for trigger scripts
///
/// Contains all the state needed to execute a trigger safely:
/// - Player state (who triggered it)
/// - Object state (what was triggered)
/// - Room state (where it happened)
/// - Execution metadata (limits, counters)
#[derive(Debug, Clone)]
pub struct TriggerContext {
    /// Username of the player who triggered this
    pub player_username: String,

    /// Player's display name
    pub player_name: String,

    /// ID of the object being triggered
    pub object_id: String,

    /// Name of the object
    pub object_name: String,

    /// Current room ID
    pub room_id: String,

    /// Current room name
    pub room_name: String,

    /// When execution started (for timeout checking)
    pub started_at: Instant,

    /// How many actions have executed so far
    pub action_count: u8,

    /// How many messages have been sent so far
    pub message_count: u8,

    /// Current nesting depth (for recursion prevention)
    pub depth: u8,
}

impl TriggerContext {
    /// Create a new trigger execution context
    pub fn new(player: &PlayerRecord, object: &ObjectRecord, room: &RoomRecord) -> Self {
        Self {
            player_username: player.username.clone(),
            player_name: player.display_name.clone(),
            object_id: object.id.clone(),
            object_name: object.name.clone(),
            room_id: room.id.clone(),
            room_name: room.name.clone(),
            started_at: Instant::now(),
            action_count: 0,
            message_count: 0,
            depth: 0,
        }
    }

    /// (Re)start the execution budget.
    ///
    /// `started_at` is set at construction, but a caller may build a context and
    /// then do unrelated work - loading records, or assembling fixtures in tests -
    /// before evaluating anything. `MAX_EXECUTION_TIME` is meant to bound script
    /// execution, so the clock restarts when evaluation actually begins. In the
    /// production path the context is built immediately before evaluation, so this
    /// is a no-op there.
    pub fn start_execution(&mut self) {
        self.started_at = Instant::now();
    }

    /// Check if execution has timed out
    pub fn is_timed_out(&self) -> bool {
        self.started_at.elapsed() > MAX_EXECUTION_TIME
    }

    /// Check if action limit has been reached
    pub fn can_execute_action(&self) -> bool {
        self.action_count < MAX_ACTIONS_PER_TRIGGER
    }

    /// Check if message limit has been reached
    pub fn can_send_message(&self) -> bool {
        self.message_count < MAX_MESSAGES_PER_TRIGGER
    }

    /// Check if nesting depth is within limits
    pub fn can_nest_deeper(&self) -> bool {
        self.depth < MAX_NESTED_DEPTH
    }

    /// Increment action counter
    pub fn increment_action(&mut self) {
        self.action_count += 1;
    }

    /// Increment message counter
    pub fn increment_message(&mut self) {
        self.message_count += 1;
    }

    /// Increment depth counter
    pub fn increment_depth(&mut self) {
        self.depth += 1;
    }

    /// Decrement depth counter
    pub fn decrement_depth(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
}

/// Result of trigger execution
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerResult {
    /// Trigger executed successfully, returned messages for player
    Success(Vec<String>),

    /// Trigger had no script or was empty
    NoScript,

    /// Trigger execution was skipped (conditions not met)
    Skipped,

    /// Trigger execution failed
    Failed(String),

    /// Trigger execution timed out
    TimedOut,

    /// Trigger hit rate limit
    RateLimited,
}

/// Execute a trigger script in sandboxed environment
///
/// # Arguments
/// * `trigger` - Type of trigger (OnLook, OnUse, etc.)
/// * `script` - DSL script to execute
/// * `context` - Execution context with player/object/room state
/// * `store` - Storage for reading/writing game state
///
/// # Returns
/// Result of trigger execution with messages to send to player
pub fn execute_trigger(
    _trigger: ObjectTrigger,
    script: &str,
    context: &mut TriggerContext,
    store: &TinyMushStore,
) -> Result<TriggerResult, TinyMushError> {
    // Validate script length
    if script.is_empty() {
        return Ok(TriggerResult::NoScript);
    }

    if script.len() > MAX_SCRIPT_LENGTH {
        return Ok(TriggerResult::Failed(format!(
            "Script too long ({} > {} chars)",
            script.len(),
            MAX_SCRIPT_LENGTH
        )));
    }

    // Check for timeout before starting
    if context.is_timed_out() {
        return Ok(TriggerResult::TimedOut);
    }

    // Parse the script
    let ast = match parse_script(script) {
        Ok(ast) => ast,
        Err(e) => return Ok(TriggerResult::Failed(format!("Parse error: {}", e))),
    };

    // Evaluate the script
    let mut evaluator = Evaluator::new(context, store);
    match evaluator.evaluate(&ast) {
        Ok(_) => {
            let messages = evaluator.messages().to_vec();
            if messages.is_empty() {
                Ok(TriggerResult::Skipped)
            } else {
                Ok(TriggerResult::Success(messages))
            }
        }
        Err(e) => Ok(TriggerResult::Failed(format!("Execution error: {}", e))),
    }
}

/// Validate a trigger script for syntax errors
///
/// # Arguments
/// * `script` - DSL script to validate
///
/// # Returns
/// Ok if script is valid, Err with detailed error message if invalid
pub fn validate_script(script: &str) -> Result<(), String> {
    // Check length
    if script.is_empty() {
        return Err("Script cannot be empty".to_string());
    }

    if script.len() > MAX_SCRIPT_LENGTH {
        return Err(format!(
            "Script too long: {} characters (max {})",
            script.len(),
            MAX_SCRIPT_LENGTH
        ));
    }

    // Parse the script to validate syntax
    parse_script(script)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmush::types::{ObjectOwner, RoomOwner, RoomVisibility};
    use chrono::Utc;

    fn create_test_context() -> TriggerContext {
        let player = PlayerRecord::new("test_player", "Test Player", "test_room");

        let object = ObjectRecord {
            id: "test_object".to_string(),
            name: "Test Object".to_string(),
            description: "A test object".to_string(),
            owner: ObjectOwner::World,
            created_at: Utc::now(),
            weight: 1,
            currency_value: Default::default(),
            value: 0,
            takeable: true,
            usable: true,
            actions: Default::default(),
            flags: vec![],
            locked: false,
            ownership_history: vec![],
            schema_version: 1,
            clone_depth: 0,
            clone_source_id: None,
            clone_count: 0,
            created_by: "world".to_string(),
        };

        let room = RoomRecord {
            id: "test_room".to_string(),
            name: "Test Room".to_string(),
            short_desc: "A test room".to_string(),
            long_desc: "This is a test room for triggers".to_string(),
            owner: RoomOwner::World,
            created_at: Utc::now(),
            visibility: RoomVisibility::Public,
            exits: Default::default(),
            items: vec![],
            flags: vec![],
            max_capacity: 10,
            housing_filter_tags: vec![],
            locked: false,
            schema_version: 1,
        };

        TriggerContext::new(&player, &object, &room)
    }

    #[test]
    fn start_execution_resets_the_budget_but_timeout_still_fires() {
        let mut ctx = create_test_context();

        // Work done between constructing a context and evaluating it must not
        // consume the script budget.
        ctx.started_at = std::time::Instant::now()
            .checked_sub(MAX_EXECUTION_TIME * 2)
            .expect("instant in range");
        assert!(ctx.is_timed_out(), "precondition: budget already exceeded");
        ctx.start_execution();
        assert!(
            !ctx.is_timed_out(),
            "start_execution should reset the clock"
        );

        // The limit itself must still be enforced once evaluation is under way.
        ctx.started_at = std::time::Instant::now()
            .checked_sub(MAX_EXECUTION_TIME + Duration::from_millis(1))
            .expect("instant in range");
        assert!(
            ctx.is_timed_out(),
            "exceeding MAX_EXECUTION_TIME must still time out"
        );
    }

    #[test]
    fn test_context_creation() {
        let ctx = create_test_context();
        assert_eq!(ctx.player_username, "test_player");
        assert_eq!(ctx.object_name, "Test Object");
        assert_eq!(ctx.room_name, "Test Room");
        assert_eq!(ctx.action_count, 0);
        assert_eq!(ctx.message_count, 0);
        assert_eq!(ctx.depth, 0);
    }

    #[test]
    fn test_context_limits() {
        let mut ctx = create_test_context();

        // Test action limit
        assert!(ctx.can_execute_action());
        for _ in 0..MAX_ACTIONS_PER_TRIGGER {
            ctx.increment_action();
        }
        assert!(!ctx.can_execute_action());

        // Test message limit
        let mut ctx = create_test_context();
        assert!(ctx.can_send_message());
        for _ in 0..MAX_MESSAGES_PER_TRIGGER {
            ctx.increment_message();
        }
        assert!(!ctx.can_send_message());

        // Test depth limit
        let mut ctx = create_test_context();
        assert!(ctx.can_nest_deeper());
        for _ in 0..MAX_NESTED_DEPTH {
            ctx.increment_depth();
        }
        assert!(!ctx.can_nest_deeper());
    }

    #[test]
    fn test_validate_script_length() {
        assert!(validate_script("message(\"hello\")").is_ok());

        let too_long = "x".repeat(MAX_SCRIPT_LENGTH + 1);
        assert!(validate_script(&too_long).is_err());

        assert!(validate_script("").is_err());
    }

    #[test]
    fn test_validate_script_parentheses() {
        assert!(validate_script("message(\"test\")").is_ok());
        assert!(validate_script("func(a, func(b))").is_ok());

        assert!(validate_script("message(\"test\"").is_err());
        assert!(validate_script("message\"test\")").is_err());
        assert!(validate_script("((())").is_err());
    }
}
