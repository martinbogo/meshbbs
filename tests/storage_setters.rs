use meshbbs::storage::Storage;

#[tokio::test]
async fn exercise_storage_setters() {
    // Use an isolated temporary directory like other tests
    let tmpdir = tempfile::tempdir().expect("tempdir");
    let data_dir = tmpdir.path().to_string_lossy().to_string();
    let mut storage = Storage::new(&data_dir).await.expect("storage new");
    let mut map = std::collections::HashMap::new();
    map.insert("general".to_string(), (0u8,0u8));
    storage.set_topic_levels(map);
    storage.set_max_message_bytes(500);
    assert!(storage.get_topic_levels("general").is_some());
}
