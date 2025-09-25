//! Username validation module for security and compatibility

use std::collections::HashSet;
use uuid;

/// Username validation errors with helpful messages
#[derive(Debug, thiserror::Error)]
pub enum UsernameError {
    #[error("Username is too short (minimum 2 characters)")]
    TooShort,
    
    #[error("Username is too long (maximum {max} characters)")]
    TooLong { max: usize },
    
    #[error("Username cannot start or end with whitespace")]
    InvalidWhitespace,
    
    #[error("Username contains invalid characters: {chars}")]
    InvalidCharacters { chars: String },
    
    #[error("Username contains path separators (/ or \\)")]
    PathTraversal,
    
    #[error("Username contains filesystem reserved characters")]
    FilesystemReserved,
    
    #[error("Username is a reserved system name")]
    Reserved,
}

#[derive(Debug)]
pub enum SecurityError {
    /// The topic name contains invalid characters or is too long
    InvalidTopicName { reason: String },
    
    /// The message ID is not a valid UUID format
    InvalidMessageId { reason: String },
    
    /// Content is too long
    ContentTooLong { max_length: usize },
    
    /// File size exceeds maximum allowed
    FileSizeExceeded { limit: usize },
    
    /// Path contains invalid characters or attempts directory traversal
    InvalidPath,
    
    /// JSON format is invalid or malformed
    InvalidFormat,
}

impl std::fmt::Display for SecurityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityError::InvalidTopicName { reason } => write!(f, "Invalid topic name: {}", reason),
            SecurityError::InvalidMessageId { reason } => write!(f, "Invalid message ID: {}", reason),
            SecurityError::ContentTooLong { max_length } => write!(f, "Content too long (max {} bytes)", max_length),
            SecurityError::FileSizeExceeded { limit } => write!(f, "File size exceeds limit ({} bytes)", limit),
            SecurityError::InvalidPath => write!(f, "Invalid path or path traversal attempt"),
            SecurityError::InvalidFormat => write!(f, "Invalid format"),
        }
    }
}

/// Username validation rules configuration
#[derive(Debug, Clone)]
pub struct UsernameRules {
    pub min_length: usize,
    pub max_length: usize,
    pub allow_spaces: bool,
    pub allow_unicode: bool,
    pub allow_special_chars: bool,
    pub allow_control_chars: bool,
    pub allow_reserved_sysop: bool,
}

impl UsernameRules {
    /// Conservative rules for sysop names - maximum security
    pub fn sysop() -> Self {
        UsernameRules {
            min_length: 2,
            max_length: 20,
            allow_spaces: false,
            allow_unicode: false,
            allow_special_chars: false,
            allow_control_chars: false,
            allow_reserved_sysop: true,
        }
    }
    
    /// Permissive rules for regular users - balanced security and usability
    pub fn user() -> Self {
        UsernameRules {
            min_length: 2,
            max_length: 30,
            allow_spaces: true,
            allow_unicode: true,
            allow_special_chars: true,
            allow_control_chars: false,
            allow_reserved_sysop: false,
        }
    }
}

/// Generate safe filename from username using URL encoding
pub fn safe_filename(username: &str) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
    utf8_percent_encode(username, NON_ALPHANUMERIC).to_string()
}

/// Get set of reserved usernames that should not be allowed
fn reserved_names() -> HashSet<&'static str> {
    [
        // System/admin terms
        "admin", "administrator", "root", "system", "sysop", "operator",
        "guest", "anonymous", "user", "test", "demo",
        // Platform-specific reserved names
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
        "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
        // BBS command terms that could cause confusion
        "mail", "message", "msg", "post", "read", "write", "send", "reply",
        "list", "dir", "ls", "cat", "type", "more", "less", "head", "tail",
        "exit", "quit", "bye", "logoff", "who", "finger", "time", "date",
        "help", "login", "logout", "register", "delete", "remove",
    ].iter().copied().collect()
}

/// Validate a username according to the given rules
pub fn validate_username(username: &str, rules: &UsernameRules) -> Result<String, UsernameError> {
    let trimmed = username.trim();
    
    // Length checks
    if trimmed.len() < rules.min_length {
        return Err(UsernameError::TooShort);
    }
    if trimmed.len() > rules.max_length {
        return Err(UsernameError::TooLong { max: rules.max_length });
    }

    // Whitespace checks
    if trimmed != username {
        return Err(UsernameError::InvalidWhitespace);
    }

    // Reserved name check (case-insensitive)
    if reserved_names().contains(&trimmed.to_lowercase().as_str()) {
        // Allow "sysop" if explicitly permitted by rules
        if rules.allow_reserved_sysop && trimmed.to_lowercase() == "sysop" {
            // Allow it
        } else {
            return Err(UsernameError::Reserved);
        }
    }

    // Path traversal check
    if trimmed.contains("..") || trimmed.contains('/') || trimmed.contains('\\') {
        return Err(UsernameError::PathTraversal);
    }

    // Filesystem reserved characters
    let fs_reserved = ['<', '>', ':', '"', '|', '?', '*', '\0'];
    if trimmed.chars().any(|c| fs_reserved.contains(&c)) {
        return Err(UsernameError::FilesystemReserved);
    }

    // Control character check
    if !rules.allow_control_chars && trimmed.chars().any(|c| c.is_control()) {
        let control_chars: String = trimmed.chars()
            .filter(|c| c.is_control())
            .map(|c| format!("\\u{{{:04x}}}", c as u32))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(UsernameError::InvalidCharacters { chars: control_chars });
    }

    // Character type validation
    let mut invalid_chars = Vec::new();
    
    for ch in trimmed.chars() {
        let valid = if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
            true // Always allowed
        } else if ch == ' ' {
            rules.allow_spaces
        } else if (ch.is_alphabetic() || ch.is_numeric()) && !ch.is_ascii() {
            rules.allow_unicode // Unicode letters or numbers
        } else if "!@#$%^&*()+=[]{}|;':\",.?`~".contains(ch) {
            rules.allow_special_chars
        } else if !ch.is_ascii() {
            rules.allow_unicode // Other Unicode characters (emojis, symbols, etc.)
        } else {
            false // Unknown/disallowed character
        };

        if !valid {
            invalid_chars.push(ch);
        }
    }

    if !invalid_chars.is_empty() {
        let unique_chars: HashSet<char> = invalid_chars.into_iter().collect();
        let chars_str: String = unique_chars.into_iter().collect();
        return Err(UsernameError::InvalidCharacters { chars: chars_str });
    }

    Ok(trimmed.to_string())
}

/// Validate a sysop name with strict rules
pub fn validate_sysop_name(name: &str) -> Result<String, UsernameError> {
    validate_username(name, &UsernameRules::sysop())
}

/// Validate a regular user name with permissive rules
pub fn validate_user_name(name: &str) -> Result<String, UsernameError> {
    validate_username(name, &UsernameRules::user())
}

/// Validate topic name for filesystem safety (prevents path traversal)
pub fn validate_topic_name(topic: &str) -> Result<String, SecurityError> {
    let trimmed = topic.trim();
    
    // Check if empty after trimming
    if trimmed.is_empty() {
        return Err(SecurityError::InvalidTopicName { 
            reason: "Topic name cannot be empty".to_string() 
        });
    }
    
    if trimmed.len() > 50 {
        return Err(SecurityError::InvalidTopicName { 
            reason: "Topic name too long (max 50 characters)".to_string() 
        });
    }
    
    // Only allow alphanumeric, underscore, and hyphen
    if !trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(SecurityError::InvalidTopicName { 
            reason: "Topic name must contain only letters, numbers, underscore, and hyphen".to_string() 
        });
    }
    
    // Check for reserved names that could cause filesystem issues
    let lower = trimmed.to_lowercase();
    if matches!(lower.as_str(), "con" | "prn" | "aux" | "nul" | "com1" | "com2" | "com3" | "com4" | "com5" | "com6" | "com7" | "com8" | "com9" | "lpt1" | "lpt2" | "lpt3" | "lpt4" | "lpt5" | "lpt6" | "lpt7" | "lpt8" | "lpt9" | "." | ".." | "config" | "data" | "admin") {
        return Err(SecurityError::InvalidTopicName { 
            reason: "Topic name is reserved".to_string() 
        });
    }
    
    Ok(trimmed.to_lowercase())
}

/// Validate message ID (must be valid UUID format)
pub fn validate_message_id(id: &str) -> Result<String, SecurityError> {
    let trimmed = id.trim();
    
    // Check if it's a valid UUID format
    if uuid::Uuid::parse_str(trimmed).is_err() {
        return Err(SecurityError::InvalidMessageId { 
            reason: "Message ID must be a valid UUID".to_string() 
        });
    }
    
    Ok(trimmed.to_string())
}

/// Sanitize message content (remove control characters, validate length)
pub fn sanitize_message_content(content: &str, max_bytes: usize) -> Result<String, SecurityError> {
    // Check byte length first
    if content.len() > max_bytes {
        return Err(SecurityError::ContentTooLong { max_length: max_bytes });
    }
    
    // Remove control characters but keep newlines and tabs
    let sanitized: String = content.chars()
        .filter(|&c| !c.is_control() || c == '\n' || c == '\t')
        .collect();
    
    // Check if any unsafe characters were removed
    if sanitized.len() != content.len() {
        // Still allow the sanitized version but with a warning in logs
        // The caller can decide whether to accept or reject
    }
    
    Ok(sanitized)
}

/// Validate file size before reading
pub fn validate_file_size(size: u64, max_size: u64) -> Result<(), SecurityError> {
    if size > max_size {
        return Err(SecurityError::FileSizeExceeded { 
            limit: max_size as usize 
        });
    }
    Ok(())
}

/// Secure path construction for message files
use std::path::Path;

pub fn secure_message_path(data_dir: &str, topic: &str, message_id: &str) -> Result<std::path::PathBuf, SecurityError> {
    let validated_topic = validate_topic_name(topic)?;
    let validated_id = validate_message_id(message_id)?;
    
    // UUIDs are filesystem-safe (only alphanumeric and hyphens), so no need to URL-encode
    Ok(Path::new(data_dir)
        .join("messages")
        .join(&validated_topic)
        .join(format!("{}.json", validated_id)))
}

/// Secure path construction for topic directories
pub fn secure_topic_path(data_dir: &str, topic: &str) -> Result<std::path::PathBuf, SecurityError> {
    let validated_topic = validate_topic_name(topic)?;
    
    let path = std::path::Path::new(data_dir)
        .join("messages")
        .join(&validated_topic);
    
    // Ensure the path is still within our data directory
    if !path.starts_with(data_dir) {
        return Err(SecurityError::InvalidPath);
    }
    
    Ok(path)
}

/// Securely parse JSON with size limits and error handling
pub fn secure_json_parse<T>(content: &str, max_bytes: usize) -> Result<T, SecurityError>
where
    T: serde::de::DeserializeOwned,
{
    // Check content size first
    if content.len() > max_bytes {
        return Err(SecurityError::FileSizeExceeded { limit: max_bytes });
    }
    
    // Attempt to parse the JSON
    serde_json::from_str(content)
        .map_err(|_| SecurityError::InvalidFormat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sysop_validation() {
        assert!(validate_sysop_name("martin").is_ok());
        assert!(validate_sysop_name("admin123").is_ok());
        
        // Should allow "sysop" for sysop validation
        assert!(validate_sysop_name("sysop").is_ok());
        assert!(validate_sysop_name("SYSOP").is_ok());
        
        // Should reject spaces for sysop
        assert!(validate_sysop_name("Al Sayeed").is_err());
        
        // Should reject Unicode for sysop
        assert!(validate_sysop_name("⌗ !! ꒰ 𝐇𝐲𝐮𝐧𝐣𝐢𝐧 ꒱ 🥝").is_err());
        
        // Should still reject other reserved names
        assert!(validate_sysop_name("admin").is_err());
        assert!(validate_sysop_name("system").is_err());
    }

    #[test]
    fn test_user_validation() {
        assert!(validate_user_name("martin").is_ok());
        assert!(validate_user_name("Al Sayeed Bin Ramen").is_ok());
        
        // Test Unicode names
        assert!(validate_user_name("🚀 User").is_ok());
        assert!(validate_user_name("José María").is_ok());
        
        // Should still reject path traversal
        assert!(validate_user_name("../etc/passwd").is_err());
        assert!(validate_user_name("user/file").is_err());
        
        // Should reject reserved names
        assert!(validate_user_name("admin").is_err());
        assert!(validate_user_name("sysop").is_err());
        assert!(validate_user_name("system").is_err());
    }

    #[test]
    fn test_safe_filename() {
        assert_eq!(safe_filename("martin"), "martin");
        assert_eq!(safe_filename("Al Sayeed"), "Al%20Sayeed");
        assert_ne!(safe_filename("../etc/passwd"), "../etc/passwd");
        assert!(!safe_filename("user/file").contains("/"));
    }

    #[test]
    fn test_topic_name_validation() {
        // Valid topic names
        assert!(validate_topic_name("general").is_ok());
        assert!(validate_topic_name("tech-support").is_ok());
        assert!(validate_topic_name("topic_1").is_ok());
        
        // Invalid topic names (path traversal attempts)
        assert!(validate_topic_name("../etc").is_err());
        assert!(validate_topic_name("topic/../other").is_err());
        assert!(validate_topic_name("").is_err());
        assert!(validate_topic_name("topic with spaces").is_err());
        assert!(validate_topic_name("topic/subtopic").is_err());
        
        // Reserved names
        assert!(validate_topic_name("con").is_err());
        assert!(validate_topic_name("admin").is_err());
    }

    #[test]
    fn test_message_id_validation() {
        // Valid UUID
        let valid_uuid = "550e8400-e29b-41d4-a716-446655440000";
        assert!(validate_message_id(valid_uuid).is_ok());
        
        // Invalid message IDs (potential path traversal)
        assert!(validate_message_id("../secret").is_err());
        assert!(validate_message_id("message.txt").is_err());
        assert!(validate_message_id("../../etc/passwd").is_err());
        assert!(validate_message_id("not-a-uuid").is_err());
    }

    #[test]
    fn test_message_content_sanitization() {
        // Normal content should pass through
        assert_eq!(sanitize_message_content("Hello world!", 100).unwrap(), "Hello world!");
        
        // Content with newlines and tabs should be preserved
        let content_with_whitespace = "Line 1\nLine 2\tTabbed";
        assert_eq!(sanitize_message_content(content_with_whitespace, 100).unwrap(), content_with_whitespace);
        
        // Control characters should be removed
        let content_with_controls = "Hello\x00\x01\x02World";
        let sanitized = sanitize_message_content(content_with_controls, 100).unwrap();
        assert_eq!(sanitized, "HelloWorld");
        
        // Too long content should be rejected
        let long_content = "a".repeat(1000);
        assert!(sanitize_message_content(&long_content, 100).is_err());
    }

    #[test]
    fn test_secure_path_construction() {
        let data_dir = "/tmp/bbs_data";
        
        // Valid paths should work
        let path = secure_message_path(data_dir, "general", "550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert!(path.to_string_lossy().contains("/tmp/bbs_data/messages/general/"));
        
        // Path traversal attempts should fail
        assert!(secure_message_path(data_dir, "../etc", "550e8400-e29b-41d4-a716-446655440000").is_err());
        assert!(secure_message_path(data_dir, "general", "../secret").is_err());
    }
}