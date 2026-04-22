// NeoJoplin Core - NeoJoplin-specific functionality
// Joplin domain types are now provided by joplin-domain crate

pub mod config;
pub mod editor;

// Re-exports from joplin-domain for convenience
pub use joplin_domain::{
    // Helpers
    now_ms,
    timestamp_to_datetime,
    AuthError,
    ConflictInfo,
    ConflictResolution,
    // Error types
    DatabaseError,
    DavEntry,
    DeletedItem,
    DomainError,

    Folder,
    ItemError,
    LockHandle,

    MarkupLanguage,

    MasterKey,
    ModelType,
    NetworkError,
    // Domain types
    Note,
    NoteTag,
    PhaseResult,
    Resource,
    Result,
    Setting,
    // Traits
    Storage,
    SyncError,
    SyncEvent,
    SyncItem,
    SyncPhase,

    // Sync types
    SyncState,
    SyncTarget,
    Tag,
    WebDavClient,
    WebDavError,
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
pub use config::{AdvancedConfig, Config, EditorConfig, SyncConfig, UiConfig};
pub use editor::{Editor, EditorConfig as RuntimeEditorConfig};

/// Convenience Result type
pub type NeoJoplinResult<T> = std::result::Result<T, NeoJoplinError>;
