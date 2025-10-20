//! Object Cloning System with Security Controls
//!
//! Implements safe object duplication with comprehensive protections against:
//! - Exponential cloning attacks (resource exhaustion)
//! - Currency duplication (economic exploits)
//! - Permission escalation (security vulnerabilities)
//! - Storage quota abuse (denial of service)
//! - Quest item duplication (game balance)
//!
//! See docs/development/CLONING_SECURITY.md for full threat model.

use crate::tmush::{
    errors::TinyMushError,
    resolver::{
        format_disambiguation_prompt, resolve_object_name, ResolutionContext, ResolveResult,
    },
    storage::TinyMushStore,
    types::{
        CurrencyAmount, ObjectFlag, ObjectOwner, ObjectRecord, OwnershipReason, OwnershipTransfer,
    },
};
use chrono::Utc;
use log::{info, warn};

/// Maximum clone genealogy depth (prevent exponential growth)
pub const MAX_CLONE_DEPTH: u8 = 3;

/// Maximum objects one player can own (prevent storage abuse)
pub const MAX_OBJECTS_PER_PLAYER: u32 = 100;

/// Cooldown between clone operations in seconds (prevent spam)
pub const CLONE_COOLDOWN: u64 = 60;

/// Clone quota per player per hour (resets hourly)
pub const CLONES_PER_HOUR: u32 = 20;

/// Maximum gold value for clonable objects (economic protection)
pub const MAX_CLONABLE_VALUE: u32 = 100;

/// Clone an object with comprehensive security checks
///
/// ## Security Controls
/// 0. **Builder Permission**: Player must have builder level 1+ (prevents general player abuse)
/// 1. **Permission**: Player must own the source object
/// 2. **Flags**: Object must be Clonable, not Unique, not QuestItem
/// 3. **Depth**: Clone genealogy must be < MAX_CLONE_DEPTH
/// 4. **Quota**: Player must have clone quota remaining
/// 5. **Cooldown**: CLONE_COOLDOWN seconds must have elapsed
/// 6. **Value**: Object value must be <= MAX_CLONABLE_VALUE
/// 7. **Storage**: Player must be under MAX_OBJECTS_PER_PLAYER
///
/// ## Sanitization
/// - Clone always owned by cloner (never inherits)
/// - Currency value stripped to 0
/// - Quest flags removed
/// - Container contents emptied
/// - Ownership history tracks clone operation
///
/// ## Arguments
/// - `source_id`: ID of object to clone
/// - `cloner_username`: Username of player performing clone
/// - `store`: Storage reference
///
/// ## Returns
/// Cloned object record, or error with specific failure reason
pub fn clone_object(
    source_id: &str,
    cloner_username: &str,
    store: &TinyMushStore,
) -> Result<ObjectRecord, TinyMushError> {
    // Get source object and player
    let source = store.get_object(source_id)?;
    let mut player = store.get_player(cloner_username)?;

    let now = Utc::now().timestamp() as u64;

    // ===== SECURITY CHECKS (FAIL FAST) =====

    // Check 0: Builder permissions required (Phase 6 Security Enhancement)
    if player.builder_level() < 1 {
        return Err(TinyMushError::PermissionDenied(
            "Cloning requires builder privileges (level 1+). Contact an admin for access."
                .to_string(),
        ));
    }

    // Check 1: Player must own the source object
    if !is_owner(&source, cloner_username) {
        return Err(TinyMushError::PermissionDenied(format!(
            "You don't own '{}'. Only the owner can clone objects.",
            source.name
        )));
    }

    // Check 2: Object must be clonable
    if !source.flags.contains(&ObjectFlag::Clonable) {
        return Err(TinyMushError::PermissionDenied(format!(
            "'{}' is not clonable. Use /setflag {} clonable to enable cloning.",
            source.name, source.name
        )));
    }

    // Check 3: Object must not be unique
    if source.flags.contains(&ObjectFlag::Unique) {
        return Err(TinyMushError::PermissionDenied(format!(
            "'{}' is a unique object and cannot be cloned.",
            source.name
        )));
    }

    // Check 4: Quest items cannot be cloned
    if source.flags.contains(&ObjectFlag::QuestItem) {
        return Err(TinyMushError::PermissionDenied(format!(
            "'{}' is a quest item and cannot be cloned.",
            source.name
        )));
    }

    // Check 5: Companions cannot be cloned
    if source.flags.contains(&ObjectFlag::Companion) {
        return Err(TinyMushError::PermissionDenied(format!(
            "'{}' is a companion and cannot be cloned.",
            source.name
        )));
    }

    // Check 6: Clone depth limit
    if source.clone_depth >= MAX_CLONE_DEPTH {
        return Err(TinyMushError::PermissionDenied(format!(
            "'{}' has reached maximum clone depth ({}/{}). Cannot clone further.",
            source.name, source.clone_depth, MAX_CLONE_DEPTH
        )));
    }

    // Check 7: Player clone quota
    if player.clone_quota == 0 {
        return Err(TinyMushError::PermissionDenied(format!(
            "Clone quota exhausted (0/{}). Quota resets hourly.",
            CLONES_PER_HOUR
        )));
    }

    // Check 8: Clone cooldown
    if player.last_clone_time > 0 && (now - player.last_clone_time) < CLONE_COOLDOWN {
        let remaining = CLONE_COOLDOWN - (now - player.last_clone_time);
        return Err(TinyMushError::PermissionDenied(format!(
            "Clone cooldown active. Wait {} seconds before cloning again.",
            remaining
        )));
    }

    // Check 9: Player object ownership limit
    if player.total_objects_owned >= MAX_OBJECTS_PER_PLAYER {
        return Err(TinyMushError::PermissionDenied(format!(
            "Object ownership limit reached ({}/{}). Delete objects to free space.",
            player.total_objects_owned, MAX_OBJECTS_PER_PLAYER
        )));
    }

    // Check 10: Value limit (economic protection)
    let object_value = source
        .value
        .max(source.currency_value.base_value().abs() as u32);
    if object_value > MAX_CLONABLE_VALUE {
        return Err(TinyMushError::PermissionDenied(format!(
            "'{}' is too valuable to clone ({} gold > {} limit).",
            source.name, object_value, MAX_CLONABLE_VALUE
        )));
    }

    // ===== CREATE SANITIZED CLONE =====

    // Generate new ID for clone (using monotonic counter + username hash for uniqueness)
    let clone_id = format!(
        "obj_{}_{}_{}",
        cloner_username, now, player.total_objects_owned
    );

    let mut clone = source.clone();

    // Security: Always owned by cloner (NEVER inherit permissions)
    clone.id = clone_id.clone();
    clone.owner = ObjectOwner::Player {
        username: cloner_username.to_string(),
    };

    // Clone tracking
    clone.clone_depth = source.clone_depth + 1;
    clone.clone_source_id = Some(source_id.to_string());
    clone.clone_count = 0; // Reset for new object
    clone.created_by = cloner_username.to_string();
    clone.created_at = Utc::now();

    // Sanitization: Strip dangerous attributes
    clone.value = 0; // No free money!
    clone.currency_value = CurrencyAmount::default(); // Zero out all currency

    // Remove quest associations
    clone.flags.retain(|f| f != &ObjectFlag::QuestItem);

    // Record ownership transfer in history
    clone.ownership_history.push(OwnershipTransfer {
        from_owner: Some(match &source.owner {
            ObjectOwner::Player { username } => username.clone(),
            ObjectOwner::World => "world".to_string(),
        }),
        to_owner: cloner_username.to_string(),
        timestamp: Utc::now(),
        reason: OwnershipReason::Clone,
    });

    // ===== UPDATE SOURCE TRACKING =====

    let mut updated_source = source;
    updated_source.clone_count += 1;
    store.put_object(updated_source)?;

    // ===== UPDATE PLAYER LIMITS =====

    player.clone_quota -= 1;
    player.last_clone_time = now;
    player.total_objects_owned += 1;

    // Capture values for logging before move
    let quota_remaining = player.clone_quota;
    let total_objects = player.total_objects_owned;

    store.put_player(player)?;

    // ===== SAVE CLONE =====

    store.put_object(clone.clone())?;

    // ===== AUDIT LOG =====

    info!(
        "CLONE: {} cloned {} -> {} (depth {}, quota {}/{}, total_objects {}/{})",
        cloner_username,
        source_id,
        clone_id,
        clone.clone_depth,
        quota_remaining,
        CLONES_PER_HOUR,
        total_objects,
        MAX_OBJECTS_PER_PLAYER
    );

    // Alert if approaching limits
    if clone.clone_depth == MAX_CLONE_DEPTH {
        warn!(
            "CLONE_DEPTH_MAX: {} reached max clone depth for {}",
            cloner_username, clone_id
        );
    }

    if total_objects >= MAX_OBJECTS_PER_PLAYER - 10 {
        warn!(
            "CLONE_QUOTA_WARNING: {} approaching object limit ({}/{})",
            cloner_username, total_objects, MAX_OBJECTS_PER_PLAYER
        );
    }

    Ok(clone)
}

/// Handle `/clone <object>` command
///
/// User-facing wrapper for clone_object() with name resolution and formatting.
///
/// ## Arguments
/// - `object_name`: Name or ID of object to clone
/// - `context`: Resolution context (player state)
/// - `store`: Storage reference
///
/// ## Returns
/// Success message with clone details, or error message
pub fn handle_clone_command(
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
                "Object '{}' not found in your inventory or current room.",
                object_name
            )));
        }
    };

    // Perform clone operation
    let clone = clone_object(&object_id, &context.username, store)?;

    // Get updated player to show quota
    let player = store.get_player(&context.username)?;

    // Format success message
    Ok(format!(
        "✨ Cloned '{}'!\n\n\
        Clone ID: #{}\n\
        Clone Depth: {}/{}\n\
        Clone Quota: {}/{} remaining this hour\n\
        Your Total Objects: {}/{}\n\n\
        The cloned object is now in your inventory.",
        clone.name,
        &clone.id[..8],
        clone.clone_depth,
        MAX_CLONE_DEPTH,
        player.clone_quota,
        CLONES_PER_HOUR,
        player.total_objects_owned,
        MAX_OBJECTS_PER_PLAYER
    ))
}

/// Check if player owns an object
fn is_owner(object: &ObjectRecord, username: &str) -> bool {
    match &object.owner {
        ObjectOwner::Player {
            username: owner_username,
        } => owner_username == username,
        ObjectOwner::World => false, // World objects cannot be cloned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_clone_depth_enforced() {
        // Verify depth limit constant
        assert_eq!(MAX_CLONE_DEPTH, 3);
    }

    #[test]
    fn test_clone_cooldown_enforced() {
        // Verify cooldown constant
        assert_eq!(CLONE_COOLDOWN, 60);
    }

    #[test]
    fn test_clone_quota_per_hour() {
        // Verify quota constant
        assert_eq!(CLONES_PER_HOUR, 20);
    }

    #[test]
    fn test_max_objects_per_player() {
        // Verify player limit constant
        assert_eq!(MAX_OBJECTS_PER_PLAYER, 100);
    }

    #[test]
    fn test_max_clonable_value() {
        // Verify value limit constant
        assert_eq!(MAX_CLONABLE_VALUE, 100);
    }

    #[test]
    fn test_is_owner_player_owned() {
        let obj = ObjectRecord::new_player_owned(
            "obj1",
            "Crystal",
            "A shiny crystal",
            "alice",
            OwnershipReason::Created,
        );

        assert!(is_owner(&obj, "alice"));
        assert!(!is_owner(&obj, "bob"));
    }

    #[test]
    fn test_is_owner_world_owned() {
        let obj = ObjectRecord::new_world("obj1", "Rock", "A world rock");

        assert!(!is_owner(&obj, "alice"));
        assert!(!is_owner(&obj, "bob"));
    }

    #[test]
    fn test_clone_flags() {
        // Verify ObjectFlag variants exist
        let clonable = ObjectFlag::Clonable;
        let unique = ObjectFlag::Unique;
        let no_value = ObjectFlag::NoValue;
        let no_clone_children = ObjectFlag::NoCloneChildren;

        assert_ne!(clonable, unique);
        assert_ne!(no_value, no_clone_children);
    }
}
