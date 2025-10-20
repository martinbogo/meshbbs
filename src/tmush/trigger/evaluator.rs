/// Evaluator for trigger script AST
///
/// Evaluates parsed trigger scripts in a sandboxed environment.
/// Executes actions and evaluates conditions against game state.
use super::{AstNode, BinaryOperator, TriggerContext};
use super::{MAX_ACTIONS_PER_TRIGGER, MAX_MESSAGES_PER_TRIGGER};
use crate::tmush::storage::TinyMushStore;

/// Value type for evaluation results
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Number(i64),
    Boolean(bool),
    Null,
}

impl Value {
    /// Convert value to boolean for condition evaluation
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Boolean(b) => *b,
            Value::Number(n) => *n != 0,
            Value::String(s) => !s.is_empty(),
            Value::Null => false,
        }
    }

    /// Get string representation
    pub fn as_string(&self) -> String {
        match self {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Null => "null".to_string(),
        }
    }
}

/// Evaluator state and execution
pub struct Evaluator<'a> {
    context: &'a mut TriggerContext,
    store: &'a TinyMushStore,
    messages: Vec<String>,
}

impl<'a> Evaluator<'a> {
    pub fn new(context: &'a mut TriggerContext, store: &'a TinyMushStore) -> Self {
        Self {
            context,
            store,
            messages: Vec::new(),
        }
    }

    /// Evaluate an AST node
    pub fn evaluate(&mut self, node: &AstNode) -> Result<Value, String> {
        // Check timeout
        if self.context.is_timed_out() {
            return Err("Execution timeout".to_string());
        }

        match node {
            AstNode::StringLiteral(s) => Ok(Value::String(s.clone())),

            AstNode::NumberLiteral(n) => Ok(Value::Number(*n)),

            AstNode::Variable(name) => self.evaluate_variable(name),

            AstNode::Action { name, args } => self.evaluate_action(name, args),

            AstNode::BinaryOp { op, left, right } => self.evaluate_binary_op(*op, left, right),

            AstNode::Ternary {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond_value = self.evaluate(condition)?;
                if cond_value.is_truthy() {
                    self.evaluate(then_branch)
                } else {
                    self.evaluate(else_branch)
                }
            }

            AstNode::Sequence(nodes) => {
                let mut last_value = Value::Null;
                for node in nodes {
                    last_value = self.evaluate(node)?;
                }
                Ok(last_value)
            }
        }
    }

    /// Evaluate a variable reference ($player, $object, etc.)
    fn evaluate_variable(&self, name: &str) -> Result<Value, String> {
        match name {
            "player" => Ok(Value::String(self.context.player_username.clone())),
            "player_name" => Ok(Value::String(self.context.player_name.clone())),
            "object" => Ok(Value::String(self.context.object_name.clone())),
            "object_id" => Ok(Value::String(self.context.object_id.clone())),
            "room" => Ok(Value::String(self.context.room_name.clone())),
            "room_id" => Ok(Value::String(self.context.room_id.clone())),
            _ => Err(format!("Unknown variable: ${}", name)),
        }
    }

    /// Evaluate a binary operator
    fn evaluate_binary_op(
        &mut self,
        op: BinaryOperator,
        left: &AstNode,
        right: &AstNode,
    ) -> Result<Value, String> {
        match op {
            BinaryOperator::And => {
                let left_val = self.evaluate(left)?;
                if !left_val.is_truthy() {
                    return Ok(Value::Boolean(false));
                }
                let right_val = self.evaluate(right)?;
                Ok(Value::Boolean(right_val.is_truthy()))
            }

            BinaryOperator::Or => {
                let left_val = self.evaluate(left)?;
                if left_val.is_truthy() {
                    return Ok(Value::Boolean(true));
                }
                let right_val = self.evaluate(right)?;
                Ok(Value::Boolean(right_val.is_truthy()))
            }

            BinaryOperator::Equal => {
                let left_val = self.evaluate(left)?;
                let right_val = self.evaluate(right)?;
                Ok(Value::Boolean(left_val == right_val))
            }

            BinaryOperator::NotEqual => {
                let left_val = self.evaluate(left)?;
                let right_val = self.evaluate(right)?;
                Ok(Value::Boolean(left_val != right_val))
            }

            BinaryOperator::Greater => {
                let left_val = self.evaluate(left)?;
                let right_val = self.evaluate(right)?;
                match (left_val, right_val) {
                    (Value::Number(l), Value::Number(r)) => Ok(Value::Boolean(l > r)),
                    _ => Err("Greater than comparison requires numbers".to_string()),
                }
            }

            BinaryOperator::Less => {
                let left_val = self.evaluate(left)?;
                let right_val = self.evaluate(right)?;
                match (left_val, right_val) {
                    (Value::Number(l), Value::Number(r)) => Ok(Value::Boolean(l < r)),
                    _ => Err("Less than comparison requires numbers".to_string()),
                }
            }

            BinaryOperator::GreaterEqual => {
                let left_val = self.evaluate(left)?;
                let right_val = self.evaluate(right)?;
                match (left_val, right_val) {
                    (Value::Number(l), Value::Number(r)) => Ok(Value::Boolean(l >= r)),
                    _ => Err("Greater or equal comparison requires numbers".to_string()),
                }
            }

            BinaryOperator::LessEqual => {
                let left_val = self.evaluate(left)?;
                let right_val = self.evaluate(right)?;
                match (left_val, right_val) {
                    (Value::Number(l), Value::Number(r)) => Ok(Value::Boolean(l <= r)),
                    _ => Err("Less or equal comparison requires numbers".to_string()),
                }
            }
        }
    }

    /// Evaluate an action (function call)
    fn evaluate_action(&mut self, name: &str, args: &[AstNode]) -> Result<Value, String> {
        // Check action limit
        if !self.context.can_execute_action() {
            return Err(format!(
                "Action limit reached ({} max)",
                MAX_ACTIONS_PER_TRIGGER
            ));
        }

        self.context.increment_action();

        match name {
            "message" => self.action_message(args),
            "message_room" => self.action_message_room(args),
            "has_item" => self.condition_has_item(args),
            "has_quest" => self.condition_has_quest(args),
            "flag_set" => self.condition_flag_set(args),
            "room_flag" => self.condition_room_flag(args),
            "current_room" => Ok(Value::String(self.context.room_id.clone())),
            "random_chance" => self.condition_random_chance(args),

            // Actions that modify game state (will implement in Phase 4)
            "teleport" => self.action_teleport(args),
            "grant_item" => self.action_grant_item(args),
            "consume" => self.action_consume(args),
            "heal" => self.action_heal(args),
            "unlock_exit" => self.action_unlock_exit(args),
            "lock_exit" => self.action_lock_exit(args),

            _ => Err(format!("Unknown action: {}", name)),
        }
    }

    /// Action: Send message to player
    fn action_message(&mut self, args: &[AstNode]) -> Result<Value, String> {
        if args.len() != 1 {
            return Err(format!("message() expects 1 argument, got {}", args.len()));
        }

        if !self.context.can_send_message() {
            return Err(format!(
                "Message limit reached ({} max)",
                MAX_MESSAGES_PER_TRIGGER
            ));
        }

        let text = self.evaluate(&args[0])?;
        let msg = self.substitute_variables(&text.as_string());

        self.messages.push(msg);
        self.context.increment_message();

        Ok(Value::Boolean(true))
    }

    /// Action: Send message to entire room
    fn action_message_room(&mut self, args: &[AstNode]) -> Result<Value, String> {
        if args.len() != 1 {
            return Err(format!(
                "message_room() expects 1 argument, got {}",
                args.len()
            ));
        }

        if !self.context.can_send_message() {
            return Err(format!(
                "Message limit reached ({} max)",
                MAX_MESSAGES_PER_TRIGGER
            ));
        }

        let text = self.evaluate(&args[0])?;
        let msg = format!("🔊 {}", self.substitute_variables(&text.as_string()));

        self.messages.push(msg);
        self.context.increment_message();

        Ok(Value::Boolean(true))
    }

    /// Condition: Check if player has item
    fn condition_has_item(&mut self, args: &[AstNode]) -> Result<Value, String> {
        if args.len() != 1 {
            return Err(format!("has_item() expects 1 argument, got {}", args.len()));
        }

        let item_id = self.evaluate(&args[0])?.as_string();

        // Check player inventory for this item
        match self.store.get_player(&self.context.player_username) {
            Ok(player) => {
                // Check both inventory systems (legacy Vec<String> and new ItemStack)
                let has_in_legacy = player.inventory.contains(&item_id);
                let has_in_stacks = player
                    .inventory_stacks
                    .iter()
                    .any(|stack| stack.object_id == item_id && stack.quantity > 0);

                Ok(Value::Boolean(has_in_legacy || has_in_stacks))
            }
            Err(_) => Ok(Value::Boolean(false)),
        }
    }

    /// Condition: Check if player has quest
    fn condition_has_quest(&mut self, args: &[AstNode]) -> Result<Value, String> {
        if args.len() != 1 {
            return Err(format!(
                "has_quest() expects 1 argument, got {}",
                args.len()
            ));
        }

        let quest_id = self.evaluate(&args[0])?.as_string();

        // Check if player has this quest (active or completed)
        match self.store.get_player(&self.context.player_username) {
            Ok(player) => {
                let has_quest = player.quests.iter().any(|pq| pq.quest_id == quest_id);
                Ok(Value::Boolean(has_quest))
            }
            Err(_) => Ok(Value::Boolean(false)),
        }
    }

    /// Condition: Check if object/room has flag
    fn condition_flag_set(&mut self, args: &[AstNode]) -> Result<Value, String> {
        if args.len() != 1 {
            return Err(format!("flag_set() expects 1 argument, got {}", args.len()));
        }

        let flag_name = self.evaluate(&args[0])?.as_string().to_lowercase();

        // Check object flags (from trigger context)
        match self.store.get_object(&self.context.object_id) {
            Ok(object) => {
                // Check if any object flag matches (case-insensitive)
                let has_flag = object
                    .flags
                    .iter()
                    .any(|f| format!("{:?}", f).to_lowercase() == flag_name);
                Ok(Value::Boolean(has_flag))
            }
            Err(_) => Ok(Value::Boolean(false)),
        }
    }

    /// Condition: Check if current room has flag
    fn condition_room_flag(&mut self, args: &[AstNode]) -> Result<Value, String> {
        if args.len() != 1 {
            return Err(format!(
                "room_flag() expects 1 argument, got {}",
                args.len()
            ));
        }

        let flag_name = self.evaluate(&args[0])?.as_string().to_lowercase();

        // Check room flags (from trigger context)
        match self.store.get_room(&self.context.room_id) {
            Ok(room) => {
                // Check if any room flag matches (case-insensitive)
                let has_flag = room
                    .flags
                    .iter()
                    .any(|f| format!("{:?}", f).to_lowercase() == flag_name);
                Ok(Value::Boolean(has_flag))
            }
            Err(_) => Ok(Value::Boolean(false)),
        }
    }

    /// Condition: Random chance (percentage 0-100)
    fn condition_random_chance(&mut self, args: &[AstNode]) -> Result<Value, String> {
        if args.len() != 1 {
            return Err(format!(
                "random_chance() expects 1 argument, got {}",
                args.len()
            ));
        }

        let chance = self.evaluate(&args[0])?;

        match chance {
            Value::Number(n) if (0..=100).contains(&n) => {
                use rand::Rng;
                let roll = rand::thread_rng().gen_range(1..=100);
                Ok(Value::Boolean(roll <= n))
            }
            _ => Err("random_chance() expects a number 0-100".to_string()),
        }
    }

    // Action implementations

    fn action_teleport(&mut self, args: &[AstNode]) -> Result<Value, String> {
        if args.len() != 1 {
            return Err(format!("teleport() expects 1 argument, got {}", args.len()));
        }
        let room_id = self.evaluate(&args[0])?.as_string();

        // Verify destination room exists
        if self.store.get_room(&room_id).is_err() {
            return Err(format!("Room '{}' does not exist", room_id));
        }

        // Update player's current room
        match self.store.get_player(&self.context.player_username) {
            Ok(mut player) => {
                player.current_room = room_id.clone();
                player.updated_at = chrono::Utc::now();

                if let Err(e) = self.store.put_player(player) {
                    return Err(format!("Failed to teleport: {}", e));
                }

                self.messages.push(format!("✨ Teleported to {}!", room_id));
                Ok(Value::Boolean(true))
            }
            Err(e) => Err(format!("Failed to get player: {}", e)),
        }
    }

    fn action_grant_item(&mut self, args: &[AstNode]) -> Result<Value, String> {
        if args.len() != 1 {
            return Err(format!(
                "grant_item() expects 1 argument, got {}",
                args.len()
            ));
        }
        let item_id = self.evaluate(&args[0])?.as_string();

        // Verify item exists
        if self.store.get_object(&item_id).is_err() {
            return Err(format!("Object '{}' does not exist", item_id));
        }

        // Add to player's inventory using proper inventory system
        use crate::tmush::inventory::add_item_to_inventory;
        use crate::tmush::types::{InventoryConfig, InventoryResult};

        match self.store.get_player(&self.context.player_username) {
            Ok(mut player) => {
                match self.store.get_object(&item_id) {
                    Ok(object) => {
                        let config = InventoryConfig::default();
                        match add_item_to_inventory(&mut player, &object, 1, &config) {
                            InventoryResult::Added { .. } => {
                                player.updated_at = chrono::Utc::now();

                                if let Err(e) = self.store.put_player(player) {
                                    return Err(format!("Failed to grant item: {}", e));
                                }

                                self.messages.push(format!("🎁 Received: {}!", object.name));
                                Ok(Value::Boolean(true))
                            }
                            InventoryResult::Failed { reason } => {
                                Err(format!("Could not add item: {}", reason))
                            }
                            _ => Ok(Value::Boolean(false)), // Other result types
                        }
                    }
                    Err(e) => Err(format!("Failed to load object: {}", e)),
                }
            }
            Err(e) => Err(format!("Failed to get player: {}", e)),
        }
    }

    fn action_consume(&mut self, _args: &[AstNode]) -> Result<Value, String> {
        // Consume the object that triggered this script (remove from inventory and delete)
        use crate::tmush::inventory::remove_item_from_inventory;
        use crate::tmush::types::InventoryResult;

        let object_id = self.context.object_id.clone();

        // Remove from player's inventory using proper inventory system
        match self.store.get_player(&self.context.player_username) {
            Ok(mut player) => {
                match remove_item_from_inventory(&mut player, &object_id, 1) {
                    InventoryResult::Removed { .. } => {
                        player.updated_at = chrono::Utc::now();

                        if let Err(e) = self.store.put_player(player) {
                            return Err(format!("Failed to remove from inventory: {}", e));
                        }

                        // Delete the object (optional - could leave as orphan)
                        // For now, just remove from inventory

                        // Get object name for better message
                        let object_name = self
                            .store
                            .get_object(&object_id)
                            .map(|obj| obj.name)
                            .unwrap_or_else(|_| object_id.clone());

                        self.messages.push(format!("💨 {} consumed!", object_name));
                        Ok(Value::Boolean(true))
                    }
                    InventoryResult::Failed { .. } => {
                        Ok(Value::Boolean(false)) // Item not in inventory
                    }
                    _ => Ok(Value::Boolean(false)), // Other result types
                }
            }
            Err(e) => Err(format!("Failed to get player: {}", e)),
        }
    }

    fn action_heal(&mut self, args: &[AstNode]) -> Result<Value, String> {
        if args.len() != 1 {
            return Err(format!("heal() expects 1 argument, got {}", args.len()));
        }
        let amount = self.evaluate(&args[0])?;

        // Note: Combat system with HP doesn't exist yet
        // For now, just acknowledge the heal with a message
        match amount {
            Value::Number(n) if n > 0 => {
                self.messages.push(format!("💚 Healed for {} HP!", n));
                Ok(Value::Boolean(true))
            }
            _ => Err("heal() expects a positive number".to_string()),
        }
    }

    fn action_unlock_exit(&mut self, args: &[AstNode]) -> Result<Value, String> {
        if args.len() != 1 {
            return Err(format!(
                "unlock_exit() expects 1 argument, got {}",
                args.len()
            ));
        }
        let direction_str = self.evaluate(&args[0])?.as_string().to_lowercase();

        // Parse direction
        let direction = match direction_str.as_str() {
            "north" | "n" => crate::tmush::types::Direction::North,
            "south" | "s" => crate::tmush::types::Direction::South,
            "east" | "e" => crate::tmush::types::Direction::East,
            "west" | "w" => crate::tmush::types::Direction::West,
            "up" | "u" => crate::tmush::types::Direction::Up,
            "down" | "d" => crate::tmush::types::Direction::Down,
            "northeast" | "ne" => crate::tmush::types::Direction::Northeast,
            "northwest" | "nw" => crate::tmush::types::Direction::Northwest,
            "southeast" | "se" => crate::tmush::types::Direction::Southeast,
            "southwest" | "sw" => crate::tmush::types::Direction::Southwest,
            _ => return Err(format!("Invalid direction: {}", direction_str)),
        };

        // Check if exit exists and add a "locked" flag concept
        // Note: RoomRecord doesn't currently have per-exit locked flags
        // This is a simplified implementation that checks if exit exists
        match self.store.get_room(&self.context.room_id) {
            Ok(room) => {
                if room.exits.contains_key(&direction) {
                    // Exit exists - in a full implementation, we'd remove a "Locked" flag
                    self.messages
                        .push(format!("🔓 Unlocked {} exit", direction_str));
                    Ok(Value::Boolean(true))
                } else {
                    Err(format!("No exit {} from this room", direction_str))
                }
            }
            Err(e) => Err(format!("Failed to get room: {}", e)),
        }
    }

    fn action_lock_exit(&mut self, args: &[AstNode]) -> Result<Value, String> {
        if args.len() != 1 {
            return Err(format!(
                "lock_exit() expects 1 argument, got {}",
                args.len()
            ));
        }
        let direction_str = self.evaluate(&args[0])?.as_string().to_lowercase();

        // Parse direction
        let direction = match direction_str.as_str() {
            "north" | "n" => crate::tmush::types::Direction::North,
            "south" | "s" => crate::tmush::types::Direction::South,
            "east" | "e" => crate::tmush::types::Direction::East,
            "west" | "w" => crate::tmush::types::Direction::West,
            "up" | "u" => crate::tmush::types::Direction::Up,
            "down" | "d" => crate::tmush::types::Direction::Down,
            "northeast" | "ne" => crate::tmush::types::Direction::Northeast,
            "northwest" | "nw" => crate::tmush::types::Direction::Northwest,
            "southeast" | "se" => crate::tmush::types::Direction::Southeast,
            "southwest" | "sw" => crate::tmush::types::Direction::Southwest,
            _ => return Err(format!("Invalid direction: {}", direction_str)),
        };

        // Check if exit exists and add a "locked" flag concept
        // Note: RoomRecord doesn't currently have per-exit locked flags
        // This is a simplified implementation that checks if exit exists
        match self.store.get_room(&self.context.room_id) {
            Ok(room) => {
                if room.exits.contains_key(&direction) {
                    // Exit exists - in a full implementation, we'd add a "Locked" flag
                    self.messages
                        .push(format!("🔒 Locked {} exit", direction_str));
                    Ok(Value::Boolean(true))
                } else {
                    Err(format!("No exit {} from this room", direction_str))
                }
            }
            Err(e) => Err(format!("Failed to get room: {}", e)),
        }
    }

    /// Substitute variables in strings
    fn substitute_variables(&self, text: &str) -> String {
        text.replace("$player", &self.context.player_username)
            .replace("$player_name", &self.context.player_name)
            .replace("$object", &self.context.object_name)
            .replace("$object_id", &self.context.object_id)
            .replace("$room", &self.context.room_name)
            .replace("$room_id", &self.context.room_id)
    }

    /// Get collected messages
    pub fn messages(&self) -> &[String] {
        &self.messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmush::trigger::TriggerContext;
    use crate::tmush::types::{ObjectOwner, RoomOwner, RoomVisibility};
    use crate::tmush::types::{ObjectRecord, PlayerRecord, RoomRecord};
    use chrono::Utc;
    use tempfile::TempDir;

    fn create_test_setup() -> (TempDir, TinyMushStore, TriggerContext) {
        let temp_dir = TempDir::new().unwrap();
        let store = TinyMushStore::open(temp_dir.path()).unwrap();

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
            clone_depth: 0,
            clone_source_id: None,
            clone_count: 0,
            created_by: "world".to_string(),
            ownership_history: vec![],
            schema_version: 1,
        };

        let room = RoomRecord {
            id: "test_room".to_string(),
            name: "Test Room".to_string(),
            short_desc: "A test room".to_string(),
            long_desc: "This is a test room".to_string(),
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

        let context = TriggerContext::new(&player, &object, &room);

        (temp_dir, store, context)
    }

    #[test]
    fn test_evaluate_literals() {
        let (_temp, store, mut context) = create_test_setup();
        let mut evaluator = Evaluator::new(&mut context, &store);

        let node = AstNode::StringLiteral("hello".to_string());
        let result = evaluator.evaluate(&node).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));

        let node = AstNode::NumberLiteral(42);
        let result = evaluator.evaluate(&node).unwrap();
        assert_eq!(result, Value::Number(42));
    }

    #[test]
    fn test_evaluate_variables() {
        let (_temp, store, mut context) = create_test_setup();
        let mut evaluator = Evaluator::new(&mut context, &store);

        let node = AstNode::Variable("player".to_string());
        let result = evaluator.evaluate(&node).unwrap();
        assert_eq!(result, Value::String("test_player".to_string()));
    }

    #[test]
    fn test_evaluate_message() {
        let (_temp, store, mut context) = create_test_setup();
        let mut evaluator = Evaluator::new(&mut context, &store);

        let node = AstNode::Action {
            name: "message".to_string(),
            args: vec![AstNode::StringLiteral("Hello $player!".to_string())],
        };

        evaluator.evaluate(&node).unwrap();

        assert_eq!(evaluator.messages().len(), 1);
        assert_eq!(evaluator.messages()[0], "Hello test_player!");
    }

    #[test]
    fn test_evaluate_ternary() {
        let (_temp, store, mut context) = create_test_setup();
        let mut evaluator = Evaluator::new(&mut context, &store);

        // true ? "yes" : "no"
        let node = AstNode::Ternary {
            condition: Box::new(AstNode::NumberLiteral(1)),
            then_branch: Box::new(AstNode::StringLiteral("yes".to_string())),
            else_branch: Box::new(AstNode::StringLiteral("no".to_string())),
        };

        let result = evaluator.evaluate(&node).unwrap();
        assert_eq!(result, Value::String("yes".to_string()));
    }

    #[test]
    fn test_message_limit() {
        let (_temp, store, mut context) = create_test_setup();
        let mut evaluator = Evaluator::new(&mut context, &store);

        // Send 3 messages (should succeed)
        for i in 0..3 {
            let node = AstNode::Action {
                name: "message".to_string(),
                args: vec![AstNode::StringLiteral(format!("Message {}", i))],
            };
            evaluator.evaluate(&node).unwrap();
        }

        // 4th message should fail
        let node = AstNode::Action {
            name: "message".to_string(),
            args: vec![AstNode::StringLiteral("Message 4".to_string())],
        };
        let result = evaluator.evaluate(&node);
        assert!(result.is_err());
    }

    #[test]
    fn test_has_item_condition() {
        let (_temp, store, mut context) = create_test_setup();

        // Create a player with an item in inventory
        let mut player =
            crate::tmush::types::PlayerRecord::new("test_player", "Test Player", "test_room");
        player.inventory.push("test_sword".to_string());
        store.put_player(player).unwrap();

        let mut evaluator = Evaluator::new(&mut context, &store);

        // Test has_item("test_sword") - should be true
        let node = AstNode::Action {
            name: "has_item".to_string(),
            args: vec![AstNode::StringLiteral("test_sword".to_string())],
        };
        let result = evaluator.evaluate(&node).unwrap();
        assert_eq!(result, Value::Boolean(true));

        // Test has_item("test_potion") - should be false
        let node = AstNode::Action {
            name: "has_item".to_string(),
            args: vec![AstNode::StringLiteral("test_potion".to_string())],
        };
        let result = evaluator.evaluate(&node).unwrap();
        assert_eq!(result, Value::Boolean(false));
    }

    #[test]
    fn test_grant_item_action() {
        let (_temp, store, mut context) = create_test_setup();

        // Create player
        let player =
            crate::tmush::types::PlayerRecord::new("test_player", "Test Player", "test_room");
        store.put_player(player).unwrap();

        // Create object to grant
        use crate::tmush::types::{CurrencyAmount, ObjectOwner, ObjectRecord};
        let object = ObjectRecord {
            id: "magic_key".to_string(),
            name: "Magic Key".to_string(),
            description: "A shiny key".to_string(),
            owner: ObjectOwner::World,
            created_at: chrono::Utc::now(),
            weight: 1,
            currency_value: CurrencyAmount::default(),
            value: 0,
            takeable: true,
            usable: false,
            actions: std::collections::HashMap::new(),
            flags: Vec::new(),
            locked: false,
            ownership_history: Vec::new(),
            schema_version: 1,
            clone_depth: 0,
            clone_source_id: None,
            clone_count: 0,
            created_by: String::new(),
        };
        store.put_object(object).unwrap();

        let mut evaluator = Evaluator::new(&mut context, &store);

        // Grant item to player
        let node = AstNode::Action {
            name: "grant_item".to_string(),
            args: vec![AstNode::StringLiteral("magic_key".to_string())],
        };
        let result = evaluator.evaluate(&node).unwrap();
        assert_eq!(result, Value::Boolean(true));

        // Verify player has the item in inventory_stacks
        let player = store.get_player("test_player").unwrap();
        assert!(player
            .inventory_stacks
            .iter()
            .any(|s| s.object_id == "magic_key"));
    }

    #[test]
    fn test_teleport_action() {
        let (_temp, store, mut context) = create_test_setup();

        // Create player
        let player =
            crate::tmush::types::PlayerRecord::new("test_player", "Test Player", "test_room");
        store.put_player(player).unwrap();

        // Create destination room
        let dest_room = crate::tmush::types::RoomRecord::world(
            "destination_room",
            "Destination",
            "A new place",
            "You've been teleported here!",
        );
        store.put_room(dest_room).unwrap();

        let mut evaluator = Evaluator::new(&mut context, &store);

        // Teleport player
        let node = AstNode::Action {
            name: "teleport".to_string(),
            args: vec![AstNode::StringLiteral("destination_room".to_string())],
        };
        let result = evaluator.evaluate(&node).unwrap();
        assert_eq!(result, Value::Boolean(true));

        // Verify teleport message was added
        let messages = evaluator.messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("Teleported to destination_room"));

        // Verify player moved
        let player = store.get_player("test_player").unwrap();
        assert_eq!(player.current_room, "destination_room");
    }

    #[test]
    fn test_message_and_teleport_compound() {
        let (_temp, store, mut context) = create_test_setup();

        // Create player
        let player =
            crate::tmush::types::PlayerRecord::new("test_player", "Test Player", "test_room");
        store.put_player(player).unwrap();

        // Create destination room
        let dest_room = crate::tmush::types::RoomRecord::world(
            "destination_room",
            "Destination",
            "A new place",
            "You've been teleported here!",
        );
        store.put_room(dest_room).unwrap();

        let mut evaluator = Evaluator::new(&mut context, &store);

        // Parse and execute: message("Flash!") && teleport("destination_room")
        let script = r#"message("✨ Flash!") && teleport("destination_room")"#;
        let ast = crate::tmush::trigger::parser::parse_script(script).unwrap();
        let result = evaluator.evaluate(&ast).unwrap();
        assert_eq!(result, Value::Boolean(true));

        // Verify BOTH messages were added
        let messages = evaluator.messages();
        assert_eq!(
            messages.len(),
            2,
            "Should have 2 messages, got: {:?}",
            messages
        );
        assert_eq!(messages[0], "✨ Flash!");
        assert!(messages[1].contains("Teleported to destination_room"));

        // Verify player moved
        let player = store.get_player("test_player").unwrap();
        assert_eq!(player.current_room, "destination_room");
    }

    #[test]
    fn test_consume_action() {
        let (_temp, store, mut context) = create_test_setup();

        // Create player with an item in inventory_stacks
        let mut player =
            crate::tmush::types::PlayerRecord::new("test_player", "Test Player", "test_room");
        player
            .inventory_stacks
            .push(crate::tmush::types::ItemStack {
                object_id: "potion".to_string(),
                quantity: 1,
                added_at: chrono::Utc::now(),
            });
        store.put_player(player).unwrap();

        // Set context object to the potion
        context.object_id = "potion".to_string();

        let mut evaluator = Evaluator::new(&mut context, &store);

        // Consume the potion
        let node = AstNode::Action {
            name: "consume".to_string(),
            args: vec![],
        };
        let result = evaluator.evaluate(&node).unwrap();
        assert_eq!(result, Value::Boolean(true));

        // Verify item removed from inventory_stacks
        let player = store.get_player("test_player").unwrap();
        assert!(!player
            .inventory_stacks
            .iter()
            .any(|s| s.object_id == "potion"));
    }

    #[test]
    fn test_heal_action() {
        let (_temp, store, mut context) = create_test_setup();
        let mut evaluator = Evaluator::new(&mut context, &store);

        // Heal for 50 HP
        let node = AstNode::Action {
            name: "heal".to_string(),
            args: vec![AstNode::NumberLiteral(50)],
        };
        let result = evaluator.evaluate(&node).unwrap();
        assert_eq!(result, Value::Boolean(true));

        // Verify heal message was sent
        assert!(evaluator.messages().len() > 0);
        assert!(evaluator.messages()[0].contains("Healed for 50 HP"));
    }
}
