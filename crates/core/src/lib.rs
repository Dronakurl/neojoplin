// NeoJoplin Core - NeoJoplin-specific functionality
// Joplin domain types are now provided by joplin-domain crate

pub mod config;
pub mod editor;

// Re-exports from joplin-domain for convenience
pub use joplin_domain::{
    // Domain types
    Note, Folder, Tag, NoteTag, Resource, MasterKey, SyncItem, DeletedItem, Setting,
    ModelType, SyncTarget, MarkupLanguage,

    // Error types
    DatabaseError, SyncError, NetworkError, AuthError, WebDavError, DomainError,

    // Traits
    Storage, WebDavClient, DavEntry, SyncEvent, LockHandle,

    // Sync types
    SyncState, PhaseResult, ItemError, ConflictInfo, ConflictResolution, SyncPhase,

    // Helpers
    now_ms, timestamp_to_datetime, Result,
};

// NeoJoplin-specific error types
#[derive(Debug, thiserror::Error)]
pub enum NeoJoplinError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Domain error: {0}")]
    Domain(#[from] joplin_domain::DomainError),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

/// Configuration-related errors (NeoJoplin-specific)
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Config file not found: {0}")]
    NotFound(String),

    #[error("Invalid config format: {0}")]
    InvalidFormat(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid value for {field}: {value}")]
    InvalidValue { field: String, value: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// Re-export config and editor types
pub use config::{Config, SyncConfig, EditorConfig, UiConfig, AdvancedConfig};
pub use editor::{Editor, EditorConfig as RuntimeEditorConfig};

/// Convenience Result type
pub type NeoJoplinResult<T> = std::result::Result<T, NeoJoplinError>;
