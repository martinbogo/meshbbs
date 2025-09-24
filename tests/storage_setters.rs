use meshbbs::storage::Storage;

#[tokio::test]
async fn exercise_storage_setters() {
    let tmp = "./tmp-test-storage-setters";
    let mut storage = Storage::new(tmp).await.expect("storage new");
    let mut map = std::collections::HashMap::new();
    map.insert("general".to_string(), (0u8,0u8));
    storage.set_topic_levels(map);
    storage.set_max_message_bytes(500);
    assert!(storage.get_topic_levels("general").is_some());
}
