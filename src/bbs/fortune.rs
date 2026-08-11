//! Unix fortune cookie mini-feature - fully data-driven from JSON file.
//!
//! This module provides a stateless fortune cookie system inspired by the classic Unix
//! `fortune` command. Fortunes are loaded from `data/fortunes.json` - if the file is
//! missing or malformed, the fortune feature will be disabled.
//!
//! ## Data Format
//!
//! The `data/fortunes.json` file should contain:
//! ```json
//! {
//!   "fortunes": [
//!     "Your fortune text here",
//!     "Another fortune..."
//!   ]
//! }
//! ```

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;
use tracing::{error, info};

/// Container for fortune data loaded from JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FortuneData {
    pub fortunes: Vec<String>,
}

/// Result of fortune initialization
#[derive(Debug)]
pub enum FortuneStatus {
    /// Fortunes loaded successfully
    Ready(usize),
    /// File not found or malformed
    Disabled(String),
}

/// Static cell holding the loaded fortunes (loaded once on first use)
static LOADED_FORTUNES: OnceLock<Result<Vec<String>, String>> = OnceLock::new();

/// Load fortunes from the JSON file. Returns error if file missing or malformed.
/// This is a hard requirement - no fallback data.
fn load_fortunes_from_file(data_dir: &Path) -> Result<Vec<String>, String> {
    let fortune_path = data_dir.join("fortunes.json");

    // Read file
    let content = std::fs::read_to_string(&fortune_path)
        .map_err(|e| format!("Failed to read {}: {}", fortune_path.display(), e))?;

    // Parse JSON
    let data: FortuneData = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {}", fortune_path.display(), e))?;

    // Validate
    if data.fortunes.is_empty() {
        return Err(format!("{} contains no fortunes", fortune_path.display()));
    }

    info!(
        "Loaded {} fortunes from {}",
        data.fortunes.len(),
        fortune_path.display()
    );
    Ok(data.fortunes)
}

/// Initialize the fortune system by loading data from the JSON file.
/// Must be called during BBS startup. Returns status indicating success or failure.
pub fn initialize(data_dir: &Path) -> FortuneStatus {
    let result = LOADED_FORTUNES.get_or_init(|| load_fortunes_from_file(data_dir));

    match result {
        Ok(fortunes) => FortuneStatus::Ready(fortunes.len()),
        Err(e) => {
            error!("Fortune feature disabled: {}", e);
            FortuneStatus::Disabled(e.clone())
        }
    }
}

/// Check if fortunes are available
pub fn is_available() -> bool {
    matches!(LOADED_FORTUNES.get(), Some(Ok(_)))
}

/// Get the number of loaded fortunes (0 if not available)
pub fn fortune_count() -> usize {
    LOADED_FORTUNES
        .get()
        .and_then(|r| r.as_ref().ok())
        .map(|f| f.len())
        .unwrap_or(0)
}

/// Pick a random fortune. Returns None if fortunes are not loaded.
pub fn random_fortune() -> Option<&'static str> {
    let fortunes = LOADED_FORTUNES.get()?.as_ref().ok()?;

    if fortunes.is_empty() {
        return None;
    }

    let mut rng = rand::thread_rng();
    let idx = rng.gen_range(0..fortunes.len());
    Some(&fortunes[idx])
}

/// Get all fortunes (for admin UI display)
pub fn get_all_fortunes() -> Option<Vec<String>> {
    LOADED_FORTUNES.get()?.as_ref().ok().map(|f| f.clone())
}

/// Save fortunes to disk (for admin UI updates)
pub fn save_fortunes(data_dir: &Path, fortunes: Vec<String>) -> Result<(), String> {
    if fortunes.is_empty() {
        return Err("Cannot save empty fortune list".to_string());
    }

    let fortune_file = data_dir.join("fortunes.json");
    let data = FortuneData { fortunes };

    let json = serde_json::to_string_pretty(&data)
        .map_err(|e| format!("Failed to serialize fortunes: {}", e))?;

    fs::write(&fortune_file, json)
        .map_err(|e| format!("Failed to write {}: {}", fortune_file.display(), e))?;

    Ok(())
}

/// Reload fortunes from disk (for admin UI updates)
pub fn reload(data_dir: &Path) -> Result<usize, String> {
    // We can't actually reload into OnceLock, but we can verify the file is valid
    // and return the count. The actual reload happens on BBS restart.
    let fortunes = load_fortunes_from_file(data_dir)?;
    Ok(fortunes.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn load_fortunes_from_valid_file() {
        let temp_dir = TempDir::new().unwrap();
        let fortune_file = temp_dir.path().join("fortunes.json");

        let data = FortuneData {
            fortunes: vec!["Test fortune 1".to_string(), "Test fortune 2".to_string()],
        };

        fs::write(&fortune_file, serde_json::to_string(&data).unwrap()).unwrap();

        let result = load_fortunes_from_file(temp_dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn load_fortunes_file_missing() {
        let temp_dir = TempDir::new().unwrap();
        let result = load_fortunes_from_file(temp_dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to read"));
    }

    #[test]
    fn load_fortunes_malformed_json() {
        let temp_dir = TempDir::new().unwrap();
        let fortune_file = temp_dir.path().join("fortunes.json");
        fs::write(&fortune_file, "{ invalid json }").unwrap();

        let result = load_fortunes_from_file(temp_dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse"));
    }

    #[test]
    fn load_fortunes_empty_array() {
        let temp_dir = TempDir::new().unwrap();
        let fortune_file = temp_dir.path().join("fortunes.json");

        let data = FortuneData { fortunes: vec![] };

        fs::write(&fortune_file, serde_json::to_string(&data).unwrap()).unwrap();

        let result = load_fortunes_from_file(temp_dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("contains no fortunes"));
    }

    #[test]
    fn random_fortune_returns_some_when_loaded() {
        let temp_dir = TempDir::new().unwrap();
        let fortune_file = temp_dir.path().join("fortunes.json");

        let data = FortuneData {
            fortunes: vec!["Only fortune".to_string()],
        };

        fs::write(&fortune_file, serde_json::to_string(&data).unwrap()).unwrap();

        // Initialize with test data
        let status = initialize(temp_dir.path());
        assert!(matches!(status, FortuneStatus::Ready(1)));

        // Should get the fortune
        let fortune = random_fortune();
        assert!(fortune.is_some());
        assert_eq!(fortune.unwrap(), "Only fortune");
    }
}
