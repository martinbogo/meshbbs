//! Integration tests for shop storage persistence

use chrono::Utc;
use meshbbs::tmush::types::{CurrencyAmount, ObjectOwner, ObjectRecord};
use meshbbs::tmush::{ShopItem, ShopRecord, TinyMushStoreBuilder};
use tempfile::tempdir;

#[test]
fn test_shop_round_trip() {
    let tmp = tempdir().unwrap();
    let store = TinyMushStoreBuilder::new(tmp.path())
        .without_world_seed()
        .open()
        .unwrap();

    let mut shop = ShopRecord::new(
        "blacksmith".to_string(),
        "The Iron Anvil".to_string(),
        "room_plaza".to_string(),
        "npc_blacksmith".to_string(),
    );

    shop.description = "A sturdy blacksmith shop with weapons and armor.".to_string();
    shop.currency = CurrencyAmount::decimal(5000); // $50.00

    // Add some items
    let mut sword = ShopItem::limited("sword_iron".to_string(), 10);
    sword.markup = Some(1.3); // 30% markup
    sword.restock_threshold = Some(3);
    sword.restock_to = Some(10);

    shop.add_item(sword).unwrap();
    shop.add_item(ShopItem::infinite("potion_health".to_string()))
        .unwrap();

    // Save shop
    store.put_shop(shop.clone()).unwrap();

    // Retrieve shop
    let retrieved = store.get_shop("blacksmith").unwrap();

    assert_eq!(retrieved.id, "blacksmith");
    assert_eq!(retrieved.name, "The Iron Anvil");
    assert_eq!(retrieved.location, "room_plaza");
    assert_eq!(retrieved.owner, "npc_blacksmith");
    assert_eq!(retrieved.currency.base_value(), 5000);
    assert_eq!(retrieved.inventory.len(), 2);

    // Check sword item
    let sword_item = retrieved.get_item("sword_iron").unwrap();
    assert_eq!(sword_item.quantity, Some(10));
    assert_eq!(sword_item.markup, Some(1.3));
}

#[test]
fn test_shop_list_and_delete() {
    let tmp = tempdir().unwrap();
    let store = TinyMushStoreBuilder::new(tmp.path())
        .without_world_seed()
        .open()
        .unwrap();

    // Create multiple shops
    let shop1 = ShopRecord::new(
        "shop1".to_string(),
        "Weapon Shop".to_string(),
        "room_plaza".to_string(),
        "vendor1".to_string(),
    );

    let shop2 = ShopRecord::new(
        "shop2".to_string(),
        "Potion Shop".to_string(),
        "room_market".to_string(),
        "vendor2".to_string(),
    );

    store.put_shop(shop1).unwrap();
    store.put_shop(shop2).unwrap();

    // List all shops
    let shop_ids = store.list_shop_ids().unwrap();
    assert_eq!(shop_ids.len(), 2);
    assert!(shop_ids.contains(&"shop1".to_string()));
    assert!(shop_ids.contains(&"shop2".to_string()));

    // Delete one shop
    store.delete_shop("shop1").unwrap();

    // Verify deletion
    let shop_ids = store.list_shop_ids().unwrap();
    assert_eq!(shop_ids.len(), 1);
    assert_eq!(shop_ids[0], "shop2");

    // Verify get returns error
    assert!(store.get_shop("shop1").is_err());
}

#[test]
fn test_get_shops_in_location() {
    let tmp = tempdir().unwrap();
    let store = TinyMushStoreBuilder::new(tmp.path())
        .without_world_seed()
        .open()
        .unwrap();

    // Create shops in different locations
    let shop1 = ShopRecord::new(
        "shop1".to_string(),
        "Plaza Vendor".to_string(),
        "room_plaza".to_string(),
        "vendor1".to_string(),
    );

    let shop2 = ShopRecord::new(
        "shop2".to_string(),
        "Plaza Merchant".to_string(),
        "room_plaza".to_string(),
        "vendor2".to_string(),
    );

    let shop3 = ShopRecord::new(
        "shop3".to_string(),
        "Market Vendor".to_string(),
        "room_market".to_string(),
        "vendor3".to_string(),
    );

    store.put_shop(shop1).unwrap();
    store.put_shop(shop2).unwrap();
    store.put_shop(shop3).unwrap();

    // Get shops in plaza
    let plaza_shops = store.get_shops_in_location("room_plaza").unwrap();
    assert_eq!(plaza_shops.len(), 2);
    assert!(plaza_shops.iter().any(|s| s.id == "shop1"));
    assert!(plaza_shops.iter().any(|s| s.id == "shop2"));

    // Get shops in market
    let market_shops = store.get_shops_in_location("room_market").unwrap();
    assert_eq!(market_shops.len(), 1);
    assert_eq!(market_shops[0].id, "shop3");

    // Non-existent location
    let empty = store.get_shops_in_location("room_nowhere").unwrap();
    assert_eq!(empty.len(), 0);
}

#[test]
fn test_shop_update_and_persistence() {
    let tmp = tempdir().unwrap();
    let store = TinyMushStoreBuilder::new(tmp.path())
        .without_world_seed()
        .open()
        .unwrap();

    let mut shop = ShopRecord::new(
        "blacksmith".to_string(),
        "The Iron Anvil".to_string(),
        "room_plaza".to_string(),
        "npc_blacksmith".to_string(),
    );

    // Add item and save
    shop.add_item(ShopItem::limited("sword".to_string(), 5))
        .unwrap();
    shop.currency = CurrencyAmount::decimal(1000);
    store.put_shop(shop.clone()).unwrap();

    // Simulate a sale transaction
    let mut retrieved = store.get_shop("blacksmith").unwrap();
    let object = ObjectRecord {
        id: "sword".to_string(),
        name: "Iron Sword".to_string(),
        description: "A basic sword".to_string(),
        owner: ObjectOwner::World,
        created_at: Utc::now(),
        weight: 5,
        currency_value: CurrencyAmount::decimal(100),
        value: 100,
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

    let (price, qty) = retrieved.process_buy("sword", 2, &object).unwrap();
    assert_eq!(qty, 2);
    assert_eq!(price.base_value(), 240); // 100 * 1.2 * 2

    // Check stock reduced
    assert_eq!(retrieved.get_item("sword").unwrap().available(), Some(3));

    // Save updated shop
    store.put_shop(retrieved.clone()).unwrap();

    // Verify persistence
    let final_shop = store.get_shop("blacksmith").unwrap();
    assert_eq!(final_shop.get_item("sword").unwrap().available(), Some(3));
    assert_eq!(final_shop.currency.base_value(), 1240); // 1000 + 240
}

#[test]
fn test_shop_config_persistence() {
    let tmp = tempdir().unwrap();
    let store = TinyMushStoreBuilder::new(tmp.path())
        .without_world_seed()
        .open()
        .unwrap();

    let mut shop = ShopRecord::new(
        "shop1".to_string(),
        "Test Shop".to_string(),
        "room1".to_string(),
        "owner1".to_string(),
    );

    // Customize config
    shop.config.max_unique_items = 100;
    shop.config.default_buy_markup = 1.5;
    shop.config.default_sell_markdown = 0.6;
    shop.config.enable_restocking = false;

    store.put_shop(shop).unwrap();

    // Retrieve and verify config
    let retrieved = store.get_shop("shop1").unwrap();
    assert_eq!(retrieved.config.max_unique_items, 100);
    assert_eq!(retrieved.config.default_buy_markup, 1.5);
    assert_eq!(retrieved.config.default_sell_markdown, 0.6);
    assert_eq!(retrieved.config.enable_restocking, false);
}
