//! Schema introspection and metadata system
//!
//! Provides data-driven schema definitions for all BBS entity types, allowing
//! the UI to dynamically adapt to changes in data structures without code changes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Complete schema definition for an entity type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDefinition {
    /// Entity type name (e.g., "user", "message", "topic")
    pub name: String,
    /// Human-readable display name
    pub display_name: String,
    /// Description of this entity type
    pub description: String,
    /// Fields in this entity
    pub fields: Vec<FieldDefinition>,
    /// Primary key field name
    pub primary_key: String,
    /// Available actions (e.g., "create", "read", "update", "delete")
    pub actions: Vec<String>,
    /// Computed/derived fields (not stored, calculated from other fields)
    pub computed_fields: Vec<ComputedFieldDefinition>,
}

/// Definition of a single field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    /// Field name (matches JSON property name)
    pub name: String,
    /// Human-readable display name
    pub display_name: String,
    /// Data type
    pub field_type: FieldType,
    /// Is this field required?
    pub required: bool,
    /// Is this field read-only?
    pub readonly: bool,
    /// Is this field searchable/filterable?
    pub searchable: bool,
    /// Is this field sortable?
    pub sortable: bool,
    /// Validation constraints
    pub validation: Option<ValidationRules>,
    /// Display hints for UI
    pub display: Option<DisplayHints>,
    /// Help text/description
    pub help_text: Option<String>,
}

/// Field data types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "constraints")]
pub enum FieldType {
    /// String with optional min/max length
    String { min_length: Option<usize>, max_length: Option<usize> },
    /// Integer with optional min/max value
    Integer { min: Option<i64>, max: Option<i64> },
    /// Unsigned integer with optional min/max value
    UnsignedInteger { min: Option<u64>, max: Option<u64> },
    /// Floating point number
    Float { min: Option<f64>, max: Option<f64> },
    /// Boolean
    Boolean,
    /// Date/time (ISO 8601 string)
    DateTime,
    /// Enumeration with possible values
    Enum { values: Vec<String> },
    /// Array of another type
    Array { item_type: Box<FieldType> },
    /// Object/nested structure
    Object { schema: String },
    /// Any JSON value
    Json,
}

/// Computed field definition (derived from other fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputedFieldDefinition {
    /// Field name
    pub name: String,
    /// Human-readable display name
    pub display_name: String,
    /// Description of how this is computed
    pub description: String,
    /// Result type
    pub field_type: FieldType,
    /// Fields this depends on
    pub depends_on: Vec<String>,
    /// Display hints
    pub display: Option<DisplayHints>,
}

/// Validation rules for a field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRules {
    /// Regular expression pattern (for strings)
    pub pattern: Option<String>,
    /// Minimum value/length
    pub min: Option<serde_json::Value>,
    /// Maximum value/length
    pub max: Option<serde_json::Value>,
    /// Custom validation rules (descriptions for frontend)
    pub custom_rules: Vec<String>,
}

/// Display hints for UI rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayHints {
    /// Widget type: "text", "textarea", "select", "number", "checkbox", "date", "color", etc.
    pub widget: String,
    /// Placeholder text
    pub placeholder: Option<String>,
    /// For select widgets: options to display
    pub options: Option<Vec<SelectOption>>,
    /// CSS class hints
    pub css_class: Option<String>,
    /// Icon to display
    pub icon: Option<String>,
    /// Format string (e.g., for dates, numbers)
    pub format: Option<String>,
}

/// Option for select/dropdown widgets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    /// Internal value
    pub value: String,
    /// Display label
    pub label: String,
    /// Optional icon
    pub icon: Option<String>,
    /// Optional color
    pub color: Option<String>,
}

/// Registry of all schema definitions
pub struct SchemaRegistry {
    schemas: HashMap<String, SchemaDefinition>,
}

impl SchemaRegistry {
    /// Create a new schema registry with all BBS entity schemas
    pub fn new() -> Self {
        let mut registry = Self {
            schemas: HashMap::new(),
        };
        
        registry.register_user_schema();
        registry.register_message_schema();
        registry.register_topic_schema();
        
        registry
    }
    
    /// Register the User entity schema
    fn register_user_schema(&mut self) {
        let schema = SchemaDefinition {
            name: "user".to_string(),
            display_name: "User".to_string(),
            description: "BBS user account with authentication and profile data".to_string(),
            primary_key: "username".to_string(),
            actions: vec!["read".to_string(), "update".to_string(), "delete".to_string()],
            fields: vec![
                FieldDefinition {
                    name: "username".to_string(),
                    display_name: "Username".to_string(),
                    field_type: FieldType::String { 
                        min_length: Some(3), 
                        max_length: Some(32) 
                    },
                    required: true,
                    readonly: true,
                    searchable: true,
                    sortable: true,
                    validation: Some(ValidationRules {
                        pattern: Some("^[a-zA-Z0-9_-]+$".to_string()),
                        min: None,
                        max: None,
                        custom_rules: vec![
                            "Must contain only letters, numbers, underscores, and hyphens".to_string()
                        ],
                    }),
                    display: Some(DisplayHints {
                        widget: "text".to_string(),
                        placeholder: Some("Username".to_string()),
                        options: None,
                        css_class: Some("font-mono".to_string()),
                        icon: Some("👤".to_string()),
                        format: None,
                    }),
                    help_text: Some("Unique identifier for this user account".to_string()),
                },
                FieldDefinition {
                    name: "longname".to_string(),
                    display_name: "Display Name".to_string(),
                    field_type: FieldType::String { 
                        min_length: None, 
                        max_length: Some(64) 
                    },
                    required: false,
                    readonly: false,
                    searchable: true,
                    sortable: true,
                    validation: None,
                    display: Some(DisplayHints {
                        widget: "text".to_string(),
                        placeholder: Some("Display name".to_string()),
                        options: None,
                        css_class: None,
                        icon: None,
                        format: None,
                    }),
                    help_text: Some("Full name or display name for this user".to_string()),
                },
                FieldDefinition {
                    name: "level".to_string(),
                    display_name: "Access Level".to_string(),
                    field_type: FieldType::UnsignedInteger { 
                        min: Some(1), 
                        max: Some(10) 
                    },
                    required: true,
                    readonly: false,
                    searchable: true,
                    sortable: true,
                    validation: Some(ValidationRules {
                        pattern: None,
                        min: Some(serde_json::json!(1)),
                        max: Some(serde_json::json!(10)),
                        custom_rules: vec![
                            "Level 1-2: Regular User".to_string(),
                            "Level 3-5: Moderator".to_string(),
                            "Level 6-9: Admin".to_string(),
                            "Level 10: Sysop".to_string(),
                        ],
                    }),
                    display: Some(DisplayHints {
                        widget: "number".to_string(),
                        placeholder: None,
                        options: None,
                        css_class: None,
                        icon: Some("🔢".to_string()),
                        format: None,
                    }),
                    help_text: Some("BBS access level (1-10)".to_string()),
                },
                FieldDefinition {
                    name: "last_on".to_string(),
                    display_name: "Last Login".to_string(),
                    field_type: FieldType::UnsignedInteger { 
                        min: None, 
                        max: None 
                    },
                    required: false,
                    readonly: true,
                    searchable: false,
                    sortable: true,
                    validation: None,
                    display: Some(DisplayHints {
                        widget: "date".to_string(),
                        placeholder: None,
                        options: None,
                        css_class: None,
                        icon: Some("🕐".to_string()),
                        format: Some("timestamp".to_string()),
                    }),
                    help_text: Some("Unix timestamp of last login".to_string()),
                },
                FieldDefinition {
                    name: "created".to_string(),
                    display_name: "Created".to_string(),
                    field_type: FieldType::UnsignedInteger { 
                        min: None, 
                        max: None 
                    },
                    required: false,
                    readonly: true,
                    searchable: false,
                    sortable: true,
                    validation: None,
                    display: Some(DisplayHints {
                        widget: "date".to_string(),
                        placeholder: None,
                        options: None,
                        css_class: None,
                        icon: Some("📅".to_string()),
                        format: Some("timestamp".to_string()),
                    }),
                    help_text: Some("Unix timestamp of account creation".to_string()),
                },
                FieldDefinition {
                    name: "has_password".to_string(),
                    display_name: "Password Set".to_string(),
                    field_type: FieldType::Boolean,
                    required: false,
                    readonly: true,
                    searchable: true,
                    sortable: true,
                    validation: None,
                    display: Some(DisplayHints {
                        widget: "checkbox".to_string(),
                        placeholder: None,
                        options: None,
                        css_class: None,
                        icon: Some("🔐".to_string()),
                        format: None,
                    }),
                    help_text: Some("Whether this user has a password configured".to_string()),
                },
            ],
            computed_fields: vec![
                ComputedFieldDefinition {
                    name: "role".to_string(),
                    display_name: "Role".to_string(),
                    description: "User role derived from access level".to_string(),
                    field_type: FieldType::String { 
                        min_length: None, 
                        max_length: None 
                    },
                    depends_on: vec!["level".to_string()],
                    display: Some(DisplayHints {
                        widget: "badge".to_string(),
                        placeholder: None,
                        options: None,
                        css_class: Some("role-badge".to_string()),
                        icon: None,
                        format: None,
                    }),
                },
            ],
        };
        
        self.schemas.insert("user".to_string(), schema);
    }
    
    /// Register the Message entity schema
    fn register_message_schema(&mut self) {
        let schema = SchemaDefinition {
            name: "message".to_string(),
            display_name: "Message".to_string(),
            description: "BBS message/post in a topic".to_string(),
            primary_key: "id".to_string(),
            actions: vec!["read".to_string(), "delete".to_string()],
            fields: vec![
                FieldDefinition {
                    name: "id".to_string(),
                    display_name: "Message ID".to_string(),
                    field_type: FieldType::String { 
                        min_length: None, 
                        max_length: None 
                    },
                    required: true,
                    readonly: true,
                    searchable: true,
                    sortable: true,
                    validation: None,
                    display: Some(DisplayHints {
                        widget: "text".to_string(),
                        placeholder: None,
                        options: None,
                        css_class: Some("font-mono text-sm".to_string()),
                        icon: Some("🔑".to_string()),
                        format: None,
                    }),
                    help_text: Some("Unique message identifier".to_string()),
                },
                FieldDefinition {
                    name: "author".to_string(),
                    display_name: "Author".to_string(),
                    field_type: FieldType::String { 
                        min_length: None, 
                        max_length: None 
                    },
                    required: true,
                    readonly: true,
                    searchable: true,
                    sortable: true,
                    validation: None,
                    display: Some(DisplayHints {
                        widget: "text".to_string(),
                        placeholder: None,
                        options: None,
                        css_class: Some("font-semibold".to_string()),
                        icon: Some("✍️".to_string()),
                        format: None,
                    }),
                    help_text: Some("Username of message author".to_string()),
                },
                FieldDefinition {
                    name: "subject".to_string(),
                    display_name: "Subject".to_string(),
                    field_type: FieldType::String { 
                        min_length: None, 
                        max_length: Some(256) 
                    },
                    required: false,
                    readonly: true,
                    searchable: true,
                    sortable: true,
                    validation: None,
                    display: Some(DisplayHints {
                        widget: "text".to_string(),
                        placeholder: None,
                        options: None,
                        css_class: Some("font-medium".to_string()),
                        icon: Some("📋".to_string()),
                        format: None,
                    }),
                    help_text: Some("Message subject line".to_string()),
                },
                FieldDefinition {
                    name: "body".to_string(),
                    display_name: "Message Body".to_string(),
                    field_type: FieldType::String { 
                        min_length: None, 
                        max_length: None 
                    },
                    required: true,
                    readonly: true,
                    searchable: true,
                    sortable: false,
                    validation: None,
                    display: Some(DisplayHints {
                        widget: "textarea".to_string(),
                        placeholder: None,
                        options: None,
                        css_class: Some("whitespace-pre-wrap".to_string()),
                        icon: Some("📄".to_string()),
                        format: None,
                    }),
                    help_text: Some("Message content".to_string()),
                },
                FieldDefinition {
                    name: "timestamp".to_string(),
                    display_name: "Posted".to_string(),
                    field_type: FieldType::UnsignedInteger { 
                        min: None, 
                        max: None 
                    },
                    required: true,
                    readonly: true,
                    searchable: false,
                    sortable: true,
                    validation: None,
                    display: Some(DisplayHints {
                        widget: "date".to_string(),
                        placeholder: None,
                        options: None,
                        css_class: None,
                        icon: Some("🕐".to_string()),
                        format: Some("timestamp".to_string()),
                    }),
                    help_text: Some("Unix timestamp when message was posted".to_string()),
                },
                FieldDefinition {
                    name: "reply_to".to_string(),
                    display_name: "Reply To".to_string(),
                    field_type: FieldType::String { 
                        min_length: None, 
                        max_length: None 
                    },
                    required: false,
                    readonly: true,
                    searchable: true,
                    sortable: false,
                    validation: None,
                    display: Some(DisplayHints {
                        widget: "text".to_string(),
                        placeholder: None,
                        options: None,
                        css_class: Some("font-mono text-sm".to_string()),
                        icon: Some("↩️".to_string()),
                        format: None,
                    }),
                    help_text: Some("Message ID this is replying to (if any)".to_string()),
                },
                FieldDefinition {
                    name: "pinned".to_string(),
                    display_name: "Pinned".to_string(),
                    field_type: FieldType::Boolean,
                    required: false,
                    readonly: false,
                    searchable: true,
                    sortable: true,
                    validation: None,
                    display: Some(DisplayHints {
                        widget: "checkbox".to_string(),
                        placeholder: None,
                        options: None,
                        css_class: None,
                        icon: Some("📌".to_string()),
                        format: None,
                    }),
                    help_text: Some("Whether this message is pinned to top of topic".to_string()),
                },
            ],
            computed_fields: vec![],
        };
        
        self.schemas.insert("message".to_string(), schema);
    }
    
    /// Register the Topic entity schema
    fn register_topic_schema(&mut self) {
        let schema = SchemaDefinition {
            name: "topic".to_string(),
            display_name: "Topic".to_string(),
            description: "BBS discussion topic/board".to_string(),
            primary_key: "name".to_string(),
            actions: vec!["read".to_string()],
            fields: vec![
                FieldDefinition {
                    name: "name".to_string(),
                    display_name: "Topic Name".to_string(),
                    field_type: FieldType::String { 
                        min_length: Some(1), 
                        max_length: Some(64) 
                    },
                    required: true,
                    readonly: true,
                    searchable: true,
                    sortable: true,
                    validation: Some(ValidationRules {
                        pattern: Some("^[a-z][a-z0-9_-]*$".to_string()),
                        min: None,
                        max: None,
                        custom_rules: vec![
                            "Lowercase letters, numbers, underscores, hyphens only".to_string(),
                            "Must start with a letter".to_string(),
                        ],
                    }),
                    display: Some(DisplayHints {
                        widget: "text".to_string(),
                        placeholder: None,
                        options: None,
                        css_class: Some("font-mono".to_string()),
                        icon: Some("💬".to_string()),
                        format: None,
                    }),
                    help_text: Some("Topic identifier (used in URLs)".to_string()),
                },
                FieldDefinition {
                    name: "message_count".to_string(),
                    display_name: "Messages".to_string(),
                    field_type: FieldType::UnsignedInteger { 
                        min: Some(0), 
                        max: None 
                    },
                    required: false,
                    readonly: true,
                    searchable: false,
                    sortable: true,
                    validation: None,
                    display: Some(DisplayHints {
                        widget: "number".to_string(),
                        placeholder: None,
                        options: None,
                        css_class: None,
                        icon: Some("💬".to_string()),
                        format: None,
                    }),
                    help_text: Some("Total number of messages in this topic".to_string()),
                },
                FieldDefinition {
                    name: "last_activity".to_string(),
                    display_name: "Last Activity".to_string(),
                    field_type: FieldType::UnsignedInteger { 
                        min: None, 
                        max: None 
                    },
                    required: false,
                    readonly: true,
                    searchable: false,
                    sortable: true,
                    validation: None,
                    display: Some(DisplayHints {
                        widget: "date".to_string(),
                        placeholder: None,
                        options: None,
                        css_class: None,
                        icon: Some("🕐".to_string()),
                        format: Some("timestamp".to_string()),
                    }),
                    help_text: Some("Unix timestamp of most recent message".to_string()),
                },
            ],
            computed_fields: vec![],
        };
        
        self.schemas.insert("topic".to_string(), schema);
    }
    
    /// Get schema by entity name
    pub fn get_schema(&self, name: &str) -> Option<&SchemaDefinition> {
        self.schemas.get(name)
    }
    
    /// Get all schema names
    pub fn list_schemas(&self) -> Vec<String> {
        self.schemas.keys().cloned().collect()
    }
    
    /// Get all schemas
    pub fn get_all_schemas(&self) -> Vec<&SchemaDefinition> {
        self.schemas.values().collect()
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}
