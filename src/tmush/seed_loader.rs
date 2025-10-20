//! Seed data loaders for data-driven content initialization
//!
//! This module provides functions to load seed data from JSON files in data/seeds/.
//! This approach allows admins to customize initial content without recompiling.

use crate::tmush::types::{
    AchievementCategory, AchievementRecord, AchievementTrigger, CompanionRecord, CraftingRecipe,
    NpcRecord, QuestRecord, RoomRecord,
};
use crate::tmush::TinyMushError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Load NPCs from data/seeds/npcs.json
pub fn load_npcs_from_json<P: AsRef<Path>>(path: P) -> Result<Vec<NpcRecord>, TinyMushError> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path)?;

    let npcs: Vec<NpcSeed> = serde_json::from_str(&contents).map_err(|e| {
        TinyMushError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to parse {}: {}", path.display(), e),
        ))
    })?;

    // Convert seed format to NpcRecord
    let records: Vec<NpcRecord> = npcs
        .into_iter()
        .map(|seed| {
            let mut npc = NpcRecord::new(
                &seed.id,
                &seed.name,
                &seed.title,
                &seed.description,
                &seed.location,
            );

            // Add dialogues
            for (topic, text) in seed.dialogues {
                npc = npc.with_dialog(&topic, &text);
            }

            // Add flags
            for flag_str in seed.flags {
                if let Ok(flag) = serde_json::from_value(serde_json::json!(flag_str)) {
                    npc = npc.with_flag(flag);
                }
            }

            npc
        })
        .collect();

    Ok(records)
}

/// Load companions from data/seeds/companions.json
pub fn load_companions_from_json<P: AsRef<Path>>(
    path: P,
) -> Result<Vec<CompanionRecord>, TinyMushError> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path)?;

    let companions: Vec<CompanionSeed> = serde_json::from_str(&contents).map_err(|e| {
        TinyMushError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to parse {}: {}", path.display(), e),
        ))
    })?;

    // Convert seed format to CompanionRecord
    let records: Vec<CompanionRecord> = companions
        .into_iter()
        .map(|seed| {
            use crate::tmush::types::CompanionType;
            let companion_type = match seed.companion_type.as_str() {
                "Horse" => CompanionType::Horse,
                "Dog" => CompanionType::Dog,
                "Cat" => CompanionType::Cat,
                _ => CompanionType::Dog, // Default fallback
            };

            let mut companion =
                CompanionRecord::new(&seed.id, &seed.name, companion_type, &seed.location);
            if let Some(desc) = seed.description {
                companion = companion.with_description(&desc);
            }
            companion
        })
        .collect();

    Ok(records)
}

/// Load rooms from data/seeds/rooms.json
pub fn load_rooms_from_json<P: AsRef<Path>>(path: P) -> Result<Vec<RoomRecord>, TinyMushError> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path)?;

    let rooms: Vec<RoomSeed> = serde_json::from_str(&contents).map_err(|e| {
        TinyMushError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to parse {}: {}", path.display(), e),
        ))
    })?;

    use crate::tmush::types::{Direction, RoomFlag};
    use chrono::Utc;

    // Convert seed format to RoomRecord
    let records: Vec<RoomRecord> = rooms
        .into_iter()
        .map(|seed| {
            let mut room = RoomRecord::world(
                &seed.id,
                &seed.name,
                &seed.short_description,
                &seed.description,
            )
            .with_created_at(Utc::now());

            // Add exits
            for (dir_str, target) in seed.exits {
                let direction = match dir_str.as_str() {
                    "North" => Direction::North,
                    "South" => Direction::South,
                    "East" => Direction::East,
                    "West" => Direction::West,
                    "Up" => Direction::Up,
                    "Down" => Direction::Down,
                    "Northeast" => Direction::Northeast,
                    "Northwest" => Direction::Northwest,
                    "Southeast" => Direction::Southeast,
                    "Southwest" => Direction::Southwest,
                    _ => continue, // Skip invalid directions
                };
                room = room.with_exit(direction, &target);
            }

            // Set capacity if specified
            if let Some(cap) = seed.capacity {
                room = room.with_capacity(cap as u16);
            }

            // Add flags
            for flag_str in seed.flags {
                let flag = match flag_str.as_str() {
                    "Safe" => RoomFlag::Safe,
                    "Dark" => RoomFlag::Dark,
                    "Indoor" => RoomFlag::Indoor,
                    "Moderated" => RoomFlag::Moderated,
                    "QuestLocation" => RoomFlag::QuestLocation,
                    "Shop" => RoomFlag::Shop,
                    "PvpEnabled" => RoomFlag::PvpEnabled,
                    "PlayerCreated" => RoomFlag::PlayerCreated,
                    "Private" => RoomFlag::Private,
                    "Instanced" => RoomFlag::Instanced,
                    "Crowded" => RoomFlag::Crowded,
                    "HousingOffice" => RoomFlag::HousingOffice,
                    "NoTeleportOut" => RoomFlag::NoTeleportOut,
                    _ => continue, // Skip invalid flags
                };
                room = room.with_flag(flag);
            }

            room
        })
        .collect();

    Ok(records)
}

/// Load achievements from data/seeds/achievements.json
pub fn load_achievements_from_json<P: AsRef<Path>>(
    path: P,
) -> Result<Vec<AchievementRecord>, TinyMushError> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path)?;

    // Deserialize directly to AchievementRecord since it already has serde support
    let achievements: Vec<AchievementSeed> = serde_json::from_str(&contents).map_err(|e| {
        TinyMushError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to parse {}: {}", path.display(), e),
        ))
    })?;

    let records: Vec<AchievementRecord> = achievements
        .into_iter()
        .map(|seed| {
            let mut achievement = AchievementRecord::new(
                &seed.id,
                &seed.name,
                &seed.description,
                seed.category,
                seed.trigger,
            );

            if let Some(title) = seed.title {
                achievement = achievement.with_title(&title);
            }

            if seed.hidden {
                achievement = achievement.as_hidden();
            }

            achievement
        })
        .collect();

    Ok(records)
}

/// Load quests from data/seeds/quests.json
pub fn load_quests_from_json<P: AsRef<Path>>(path: P) -> Result<Vec<QuestRecord>, TinyMushError> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path)?;

    // QuestRecord should already have Deserialize support
    let quests: Vec<QuestRecord> = serde_json::from_str(&contents).map_err(|e| {
        TinyMushError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to parse {}: {}", path.display(), e),
        ))
    })?;

    Ok(quests)
}

/// Load crafting recipes from data/seeds/recipes.json
pub fn load_recipes_from_json<P: AsRef<Path>>(
    path: P,
) -> Result<Vec<CraftingRecipe>, TinyMushError> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path)?;

    // CraftingRecipe should already have Deserialize support
    let recipes: Vec<CraftingRecipe> = serde_json::from_str(&contents).map_err(|e| {
        TinyMushError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to parse {}: {}", path.display(), e),
        ))
    })?;

    Ok(recipes)
}

// ============================================================================
// Seed data structures that match JSON format
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
struct NpcSeed {
    id: String,
    name: String,
    title: String,
    description: String,
    location: String,
    #[serde(default)]
    dialogues: HashMap<String, String>,
    #[serde(default)]
    flags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CompanionSeed {
    id: String,
    name: String,
    companion_type: String,
    location: String,
    description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RoomSeed {
    id: String,
    name: String,
    short_description: String,
    description: String,
    #[serde(default)]
    exits: HashMap<String, String>,
    capacity: Option<usize>,
    #[serde(default)]
    flags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AchievementSeed {
    id: String,
    name: String,
    description: String,
    category: AchievementCategory,
    trigger: AchievementTrigger,
    title: Option<String>,
    #[serde(default)]
    hidden: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_nonexistent_file() {
        let result = load_npcs_from_json("nonexistent.json");
        assert!(result.is_err());
    }

    // Additional tests should be added once actual seed files exist
}
