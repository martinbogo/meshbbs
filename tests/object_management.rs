// Integration tests for @OBJECT admin command
// Tests data-driven object management system (Phase 5)

use meshbbs::tmush::types::{ObjectFlag, ObjectRecord};
use meshbbs::tmush::{TinyMushStore, TinyMushStoreBuilder};
use tempfile::TempDir;

fn setup_test_store() -> (TinyMushStore, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = TinyMushStoreBuilder::new(temp_dir.path()).open().unwrap();
    (store, temp_dir)
}

/// Test basic CRUD operations for objects
#[test]
fn object_crud_operations() {
    let (store, _temp) = setup_test_store();

    // Create a test object
    let object = ObjectRecord::new_world("test_torch", "Wooden Torch", "A simple torch.");
    store.put_object(object).unwrap();

    // Read it back
    let retrieved = store.get_object("test_torch").unwrap();
    assert_eq!(retrieved.id, "test_torch");
    assert_eq!(retrieved.name, "Wooden Torch");
    assert_eq!(retrieved.description, "A simple torch.");

    // Update it
    let mut updated = retrieved.clone();
    updated.name = "Bright Torch".to_string();
    store.put_object(updated).unwrap();

    let retrieved_again = store.get_object("test_torch").unwrap();
    assert_eq!(retrieved_again.name, "Bright Torch");

    // Delete it
    store.delete_object_world("test_torch").unwrap();

    // Verify deletion
    assert!(!store.object_exists("test_torch").unwrap());
}

/// Test editing all object fields
#[test]
fn object_edit_all_fields() {
    let (store, _temp) = setup_test_store();

    // Create test object
    let mut object = ObjectRecord::new_world("test_item", "Test Item", "A test object.");
    store.put_object(object.clone()).unwrap();

    // Edit name
    object.name = "Updated Item".to_string();
    store.put_object(object.clone()).unwrap();
    let retrieved = store.get_object("test_item").unwrap();
    assert_eq!(retrieved.name, "Updated Item");

    // Edit description
    object.description = "An updated test object.".to_string();
    store.put_object(object.clone()).unwrap();
    let retrieved = store.get_object("test_item").unwrap();
    assert_eq!(retrieved.description, "An updated test object.");

    // Edit weight
    object.weight = 10;
    store.put_object(object.clone()).unwrap();
    let retrieved = store.get_object("test_item").unwrap();
    assert_eq!(retrieved.weight, 10);

    // Edit value
    use meshbbs::tmush::types::CurrencyAmount;
    object.currency_value = CurrencyAmount::multi_tier(100);
    store.put_object(object.clone()).unwrap();
    let retrieved = store.get_object("test_item").unwrap();
    assert!(matches!(
        retrieved.currency_value,
        CurrencyAmount::MultiTier { base_units: 100 }
    ));

    // Edit takeable
    object.takeable = true;
    store.put_object(object.clone()).unwrap();
    let retrieved = store.get_object("test_item").unwrap();
    assert!(retrieved.takeable);

    // Edit usable
    object.usable = true;
    store.put_object(object.clone()).unwrap();
    let retrieved = store.get_object("test_item").unwrap();
    assert!(retrieved.usable);

    // Edit locked
    object.locked = true;
    store.put_object(object.clone()).unwrap();
    let retrieved = store.get_object("test_item").unwrap();
    assert!(retrieved.locked);

    // Cleanup
    store.delete_object_world("test_item").unwrap();
}

/// Test object flag management
#[test]
fn object_flag_management() {
    let (store, _temp) = setup_test_store();

    // Create object
    let mut object = ObjectRecord::new_world("test_sword", "Iron Sword", "A basic sword.");
    store.put_object(object.clone()).unwrap();

    // Add EQUIPMENT flag
    object.flags.push(ObjectFlag::Equipment);
    store.put_object(object.clone()).unwrap();
    let retrieved = store.get_object("test_sword").unwrap();
    assert!(retrieved.flags.contains(&ObjectFlag::Equipment));

    // Add MAGICAL flag
    object.flags.push(ObjectFlag::Magical);
    store.put_object(object.clone()).unwrap();
    let retrieved = store.get_object("test_sword").unwrap();
    assert!(retrieved.flags.contains(&ObjectFlag::Magical));

    // Add UNIQUE flag
    object.flags.push(ObjectFlag::Unique);
    store.put_object(object.clone()).unwrap();
    let retrieved = store.get_object("test_sword").unwrap();
    assert!(retrieved.flags.contains(&ObjectFlag::Unique));

    // Cleanup
    store.delete_object_world("test_sword").unwrap();
}

/// Test all 12 object flag types
#[test]
fn object_all_flag_types() {
    let (store, _temp) = setup_test_store();

    // Create object
    let mut object = ObjectRecord::new_world("test_all_flags", "Test Object", "Testing all flags.");

    // Test each flag type
    let flags = vec![
        ObjectFlag::QuestItem,
        ObjectFlag::Consumable,
        ObjectFlag::Equipment,
        ObjectFlag::KeyItem,
        ObjectFlag::Container,
        ObjectFlag::Magical,
        ObjectFlag::Companion,
        ObjectFlag::Clonable,
        ObjectFlag::Unique,
        ObjectFlag::NoValue,
        ObjectFlag::NoCloneChildren,
        ObjectFlag::LightSource,
    ];

    for flag in flags.iter() {
        object.flags.push(flag.clone());
    }

    store.put_object(object.clone()).unwrap();
    let retrieved = store.get_object("test_all_flags").unwrap();

    // Verify all 12 flags are present
    assert_eq!(retrieved.flags.len(), 12);
    for flag in flags.iter() {
        assert!(retrieved.flags.contains(flag));
    }

    // Cleanup
    store.delete_object_world("test_all_flags").unwrap();
}

/// Test object_exists() helper method
#[test]
fn object_exists_check() {
    let (store, _temp) = setup_test_store();

    // Object doesn't exist yet
    assert!(!store.object_exists("nonexistent_object").unwrap());

    // Create object
    let object = ObjectRecord::new_world("existing_object", "Existing", "This exists.");
    store.put_object(object).unwrap();

    // Now it exists
    assert!(store.object_exists("existing_object").unwrap());

    // Delete and verify
    store.delete_object_world("existing_object").unwrap();
    assert!(!store.object_exists("existing_object").unwrap());
}

/// Test list_object_ids() method
#[test]
fn object_list_functionality() {
    let (store, _temp) = setup_test_store();

    // Start with clean slate
    let initial_ids = store.list_object_ids().unwrap();
    let initial_count = initial_ids.len();

    // Create multiple objects
    let obj1 = ObjectRecord::new_world("obj_alpha", "Alpha", "First object.");
    let obj2 = ObjectRecord::new_world("obj_beta", "Beta", "Second object.");
    let obj3 = ObjectRecord::new_world("obj_gamma", "Gamma", "Third object.");

    store.put_object(obj1).unwrap();
    store.put_object(obj2).unwrap();
    store.put_object(obj3).unwrap();

    // List objects
    let object_ids = store.list_object_ids().unwrap();
    assert_eq!(object_ids.len(), initial_count + 3);
    assert!(object_ids.contains(&"obj_alpha".to_string()));
    assert!(object_ids.contains(&"obj_beta".to_string()));
    assert!(object_ids.contains(&"obj_gamma".to_string()));

    // Verify alphabetical order (list_object_ids sorts)
    let mut sorted = object_ids.clone();
    sorted.sort();
    assert_eq!(object_ids, sorted);

    // Cleanup
    store.delete_object_world("obj_alpha").unwrap();
    store.delete_object_world("obj_beta").unwrap();
    store.delete_object_world("obj_gamma").unwrap();
}

/// Test that list_object_ids() works correctly
#[test]
fn default_objects_listing() {
    let (store, _temp) = setup_test_store();

    // Check that list_object_ids() returns a valid list
    // Objects may or may not be seeded depending on initialization
    let object_ids = store.list_object_ids().unwrap();

    println!("Found {} world objects in database", object_ids.len());

    // Verify the list is properly sorted (guaranteed by list_object_ids)
    let mut sorted = object_ids.clone();
    sorted.sort();
    assert_eq!(
        object_ids, sorted,
        "Object IDs should be sorted alphabetically"
    );

    // Verify all IDs are non-empty
    for id in object_ids.iter() {
        assert!(!id.is_empty(), "Object IDs should not be empty strings");
    }
}
