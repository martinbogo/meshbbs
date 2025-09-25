//! Test utilities & fixtures.
//! Provides access to relocated integration test data under `tests/test-data-int`.

use std::path::{Path, PathBuf};

/// Return the path to the static integration test fixture directory.
/// Kept small & deterministic. Tests should copy to a temp dir if they mutate.
pub fn fixture_root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("test-data-int") }

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
    for user in ["alice.json", "carol.json"] { let _ = std::fs::copy(src.join("users").join(user), root.join("users").join(user)); }
    tmp
}
