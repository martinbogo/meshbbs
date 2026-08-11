//! Test utilities & fixtures.
//! Provides access to relocated integration test data under `tests/test-data-int`.

use std::path::{Path, PathBuf};

/// Return the path to the static integration test fixture directory.
/// Kept small & deterministic. Tests should copy to a temp dir if they mutate.
pub fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("test-data-int")
}

/// Return a writable copy (temp dir) of the fixture tree. Only copies minimal structure
/// needed by current tests (topics.json + users + messages/hello).
#[allow(dead_code)] // Future tests may invoke this; silenced to keep build clean when unused.
pub fn writable_fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let src = fixture_root();

    // copy topics.json
    std::fs::create_dir_all(root.join("messages/hello")).unwrap();
    std::fs::create_dir_all(root.join("users")).unwrap();
    std::fs::copy(src.join("topics.json"), root.join("topics.json")).unwrap();
    // fortunes.json is required for the fortune feature to load at server startup
    std::fs::copy(src.join("fortunes.json"), root.join("fortunes.json")).unwrap();
    for user in ["alice.json", "carol.json"] {
        let _ = std::fs::copy(src.join("users").join(user), root.join("users").join(user));
    }
    tmp
}

/// Create an empty GameRegistry for tests that don't need any door games
pub fn empty_game_registry() -> meshbbs::bbs::GameRegistry {
    meshbbs::bbs::GameRegistry::new()
}

#[test]
fn empty_registry_helper_creates_no_games() {
    let registry = empty_game_registry();
    assert!(
        registry.get_tinymush_store().is_none(),
        "Helper should return a registry with no games registered"
    );
}
