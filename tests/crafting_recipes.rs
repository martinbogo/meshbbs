//! Integration tests for data-driven crafting recipe system
//!
//! Tests the complete flow:
//! - Recipe storage (CRUD operations)
//! - Default recipes seeded on initialization
//! - Recipe data structure integrity

use meshbbs::tmush::types::{CraftingRecipe, RecipeMaterial};
use meshbbs::tmush::TinyMushStoreBuilder;
use tempfile::TempDir;

#[test]
fn recipe_crud_operations() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .without_world_seed()
        .open()
        .expect("store");

    // Create a recipe
    let recipe = CraftingRecipe::new("goat_cheese", "Goat Milk Cheese", "goat_cheese", "admin")
        .with_material("goat_milk", 2)
        .with_station("cheese_press")
        .with_description("Creamy artisan cheese made from fresh goat milk");

    // Put recipe
    store.put_recipe(recipe.clone()).expect("put recipe");

    // Get recipe
    let retrieved = store.get_recipe("goat_cheese").expect("get recipe");
    assert_eq!(retrieved.id, "goat_cheese");
    assert_eq!(retrieved.name, "Goat Milk Cheese");
    assert_eq!(retrieved.materials.len(), 1);
    assert_eq!(retrieved.materials[0].item_id, "goat_milk");
    assert_eq!(retrieved.materials[0].quantity, 2);
    assert!(retrieved.materials[0].consumed);
    assert_eq!(retrieved.requires_station, Some("cheese_press".to_string()));
    assert_eq!(
        retrieved.description,
        "Creamy artisan cheese made from fresh goat milk"
    );

    // List recipes
    let recipes = store.list_recipes(None).expect("list recipes");
    assert!(recipes.iter().any(|r| r.id == "goat_cheese"));

    // List recipes by station
    let cheese_recipes = store
        .list_recipes(Some("cheese_press"))
        .expect("list by station");
    assert_eq!(cheese_recipes.len(), 1);
    assert_eq!(cheese_recipes[0].id, "goat_cheese");

    // Delete recipe
    store.delete_recipe("goat_cheese").expect("delete recipe");

    // Verify deleted
    assert!(store.get_recipe("goat_cheese").is_err());
}

#[test]
fn recipe_builder_pattern() {
    let recipe = CraftingRecipe::new("test", "Test Recipe", "result", "builder")
        .with_material("wood", 3)
        .with_material("nails", 6)
        .with_tool("hammer")
        .with_station("workbench")
        .with_description("A test recipe");

    assert_eq!(recipe.materials.len(), 3);

    // Check materials
    let wood = recipe
        .materials
        .iter()
        .find(|m| m.item_id == "wood")
        .unwrap();
    assert_eq!(wood.quantity, 3);
    assert!(wood.consumed);

    let nails = recipe
        .materials
        .iter()
        .find(|m| m.item_id == "nails")
        .unwrap();
    assert_eq!(nails.quantity, 6);
    assert!(nails.consumed);

    // Check tool (not consumed)
    let hammer = recipe
        .materials
        .iter()
        .find(|m| m.item_id == "hammer")
        .unwrap();
    assert_eq!(hammer.quantity, 1);
    assert!(!hammer.consumed);

    assert_eq!(recipe.requires_station, Some("workbench".to_string()));
    assert_eq!(recipe.description, "A test recipe");
}

#[test]
fn default_recipes_seeded() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .open() // Will seed recipes
        .expect("store");

    // Verify signal_booster exists
    let signal_booster = store
        .get_recipe("signal_booster")
        .expect("signal_booster exists");
    assert_eq!(signal_booster.name, "Signal Booster");
    assert_eq!(signal_booster.result_item_id, "signal_booster");
    assert_eq!(
        signal_booster.requires_station,
        Some("crafting_bench".to_string())
    );

    // Check materials (2x copper_wire, circuit_board, antenna_rod)
    assert_eq!(signal_booster.materials.len(), 3);
    let copper_wire = signal_booster
        .materials
        .iter()
        .find(|m| m.item_id == "copper_wire")
        .unwrap();
    assert_eq!(copper_wire.quantity, 2);

    // Verify basic_antenna exists
    let basic_antenna = store
        .get_recipe("basic_antenna")
        .expect("basic_antenna exists");
    assert_eq!(basic_antenna.name, "Basic Antenna");
    assert_eq!(basic_antenna.result_item_id, "basic_antenna");
    assert_eq!(
        basic_antenna.requires_station,
        Some("crafting_bench".to_string())
    );

    // Check materials (2x copper_wire, antenna_rod)
    assert_eq!(basic_antenna.materials.len(), 2);
}

#[test]
fn recipe_material_types() {
    let recipe = CraftingRecipe::new("test", "Test", "result", "tester")
        .with_material("wood", 5) // Consumed material
        .with_tool("saw"); // Non-consumed tool

    let wood = recipe
        .materials
        .iter()
        .find(|m| m.item_id == "wood")
        .unwrap();
    assert!(wood.consumed, "Materials should be consumed");

    let saw = recipe
        .materials
        .iter()
        .find(|m| m.item_id == "saw")
        .unwrap();
    assert!(!saw.consumed, "Tools should not be consumed");
}

#[test]
fn recipe_batch_crafting() {
    let recipe = CraftingRecipe {
        id: "batch_bread".to_string(),
        name: "Batch of Bread".to_string(),
        description: "Makes 6 loaves at once".to_string(),
        materials: vec![
            RecipeMaterial::new("flour", 3),
            RecipeMaterial::new("yeast", 1),
        ],
        result_item_id: "bread_loaf".to_string(),
        result_quantity: 6, // Batch crafting!
        requires_station: Some("oven".to_string()),
        skill_required: None,
        skill_level: 0,
        crafting_time_seconds: 0,
        created_by: "baker".to_string(),
        created_at: chrono::Utc::now(),
        schema_version: 1,
    };

    assert_eq!(recipe.result_quantity, 6, "Should create 6 items");
}

#[test]
fn recipe_exists_check() {
    let dir = TempDir::new().expect("tempdir");
    let store = TinyMushStoreBuilder::new(dir.path())
        .without_world_seed()
        .open()
        .expect("store");

    // Should not exist initially
    assert!(!store.recipe_exists("nonexistent").expect("exists check"));

    // Create recipe
    let recipe = CraftingRecipe::new("test", "Test", "result", "tester");
    store.put_recipe(recipe).expect("put recipe");

    // Should exist now
    assert!(store.recipe_exists("test").expect("exists check"));
}
