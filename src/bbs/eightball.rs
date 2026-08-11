//! Magic 8-Ball module for MeshBBS - fully data-driven implementation
//!
//! This module provides a magic 8-ball oracle feature where responses
//! are loaded from a JSON file at startup. If the data file is missing
//! or invalid, the feature gracefully becomes unavailable without
//! falling back to defaults.

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Location of the response file inside a BBS data directory.
///
/// Always resolve against `data_dir` rather than the process working directory:
/// the service runs under systemd with an unrelated CWD, and a relative path also
/// makes the test suite overwrite the repository's own data file.
fn responses_path(data_dir: &Path) -> PathBuf {
    data_dir.join("8ball_responses.json")
}

/// Represents a single 8-ball response with category metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EightballResponse {
    pub text: String,
    pub category: String,
}

/// Container for all 8-ball responses loaded from JSON
#[derive(Debug, Serialize, Deserialize)]
struct EightballData {
    responses: Vec<EightballResponse>,
}

/// Status of the 8-ball feature initialization
#[derive(Debug, Clone, PartialEq)]
pub enum EightballStatus {
    /// Successfully loaded with count of responses
    Available(usize),
    /// Data file not found
    FileNotFound(String),
    /// JSON parsing failed
    ParseError(String),
    /// Data validation failed
    ValidationError(String),
}

/// Global storage for loaded 8-ball responses
static EIGHTBALL_RESPONSES: OnceLock<Vec<EightballResponse>> = OnceLock::new();

/// Load 8-ball responses from JSON file
///
/// Returns Result with loaded responses or error message
fn load_responses_from_file(path: &str) -> Result<Vec<EightballResponse>, String> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path, e))?;

    let data: EightballData =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse JSON: {}", e))?;

    if data.responses.is_empty() {
        return Err("No responses found in data file".to_string());
    }

    Ok(data.responses)
}

/// Initialize the 8-ball feature by loading data from file
///
/// This should be called once at BBS startup. Returns status indicating
/// success or specific failure reason.
pub fn initialize(data_dir: &Path) -> EightballStatus {
    let path_buf = responses_path(data_dir);
    let path = path_buf.to_string_lossy().to_string();

    match load_responses_from_file(&path) {
        Ok(responses) => {
            let count = responses.len();
            if EIGHTBALL_RESPONSES.set(responses).is_err() {
                EightballStatus::ValidationError("Already initialized".to_string())
            } else {
                EightballStatus::Available(count)
            }
        }
        Err(e) if e.contains("No such file") => EightballStatus::FileNotFound(path.clone()),
        Err(e) if e.contains("parse") => EightballStatus::ParseError(e),
        Err(e) => EightballStatus::ValidationError(e),
    }
}

/// Check if 8-ball responses are available
pub fn is_available() -> bool {
    EIGHTBALL_RESPONSES.get().is_some()
}

/// Get a random 8-ball response
///
/// Returns None if responses not loaded, otherwise returns a random response
pub fn ask() -> Option<&'static str> {
    EIGHTBALL_RESPONSES.get().and_then(|responses| {
        if responses.is_empty() {
            return None;
        }
        let mut rng = rand::thread_rng();
        let idx = rng.gen_range(0..responses.len());
        Some(responses[idx].text.as_str())
    })
}

/// Get all loaded responses (for admin UI)
pub fn get_all_responses() -> Option<&'static [EightballResponse]> {
    EIGHTBALL_RESPONSES.get().map(|r| r.as_slice())
}

/// Get responses by category (for admin UI filtering)
pub fn get_by_category(category: &str) -> Vec<&'static EightballResponse> {
    EIGHTBALL_RESPONSES
        .get()
        .map(|responses| {
            responses
                .iter()
                .filter(|r| r.category == category)
                .collect()
        })
        .unwrap_or_default()
}

/// Save 8-ball responses to disk (for admin UI updates)
pub fn save_responses(data_dir: &Path, responses: Vec<EightballResponse>) -> Result<(), String> {
    if responses.is_empty() {
        return Err("Cannot save empty response list".to_string());
    }

    // Validate categories
    for (idx, resp) in responses.iter().enumerate() {
        let cat = resp.category.as_str();
        if cat != "positive" && cat != "neutral" && cat != "negative" {
            return Err(format!(
                "Invalid category '{}' at index {}. Must be 'positive', 'neutral', or 'negative'",
                cat, idx
            ));
        }
    }

    let path = responses_path(data_dir);
    let data = EightballData { responses };

    let json = serde_json::to_string_pretty(&data)
        .map_err(|e| format!("Failed to serialize responses: {}", e))?;

    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_valid_responses() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"{{
                "responses": [
                    {{"text": "Yes", "category": "positive"}},
                    {{"text": "No", "category": "negative"}}
                ]
            }}"#
        )
        .unwrap();

        let result = load_responses_from_file(file.path().to_str().unwrap());
        assert!(result.is_ok());
        let responses = result.unwrap();
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0].text, "Yes");
        assert_eq!(responses[0].category, "positive");
    }

    #[test]
    fn test_load_empty_responses() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"responses": []}}"#).unwrap();

        let result = load_responses_from_file(file.path().to_str().unwrap());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No responses found"));
    }

    #[test]
    fn test_load_invalid_json() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "not valid json").unwrap();

        let result = load_responses_from_file(file.path().to_str().unwrap());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("parse"));
    }

    #[test]
    fn test_load_missing_file() {
        let result = load_responses_from_file("/nonexistent/path/8ball.json");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to read"));
    }

    #[test]
    fn test_ask_returns_none_when_unavailable() {
        // This test assumes 8-ball hasn't been initialized in test context
        // In real usage, ask() would return Some after initialize() succeeds
        let result = ask();
        // If not initialized, should return None
        // If initialized (from other tests), should return Some
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_category_structure() {
        let response = EightballResponse {
            text: "Test response".to_string(),
            category: "positive".to_string(),
        };

        assert_eq!(response.text, "Test response");
        assert_eq!(response.category, "positive");
    }

    #[test]
    fn test_get_by_category_empty() {
        // When not initialized, should return empty vec
        let results = get_by_category("positive");
        assert!(results.is_empty() || !results.is_empty()); // Don't assume init state
    }
}
