// joplin - Meta-crate for Joplin-compatible functionality
//
// This crate re-exports all joplin-* crates for convenience.
// For fine-grained dependencies, use the specific crates directly.

pub use joplin_domain;
pub use joplin_sync;

// Re-exports for convenience
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

    FileMeta,
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

// Re-exports from joplin-sync
pub use joplin_sync::{DeltaContext, ReqwestWebDavClient, SyncEngine, SyncInfo, WebDavConfig};
