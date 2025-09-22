use meshbbs::storage::Storage;
use tokio::runtime::Runtime;
use std::fs;

#[test]
fn password_set_and_change() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let tmpdir = tempfile::tempdir().unwrap();
        let datadir = tmpdir.path().join("data");
        let mut storage = Storage::new(datadir.to_str().unwrap()).await.unwrap();
        storage.register_user("alice", "initialPass1", None).await.unwrap();
        let (_u, ok) = storage.verify_user_password("alice", "initialPass1").await.unwrap();
        assert!(ok, "initial password should verify");
        storage.update_user_password("alice", "NewPassw0rd!").await.unwrap();
        let (_u2, ok2) = storage.verify_user_password("alice", "NewPassw0rd!").await.unwrap();
        assert!(ok2, "new password should verify");
        let (_u3, bad) = storage.verify_user_password("alice", "initialPass1").await.unwrap();
        assert!(!bad, "old password should fail after change");
    });
}
