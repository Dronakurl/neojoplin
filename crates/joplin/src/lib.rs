// joplin - Meta-crate for Joplin-compatible functionality
//
// This crate re-exports all joplin-* crates for convenience.
// For fine-grained dependencies, use the specific crates directly.

pub use joplin_domain;
pub use joplin_sync;

// Re-exports for convenience
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
    now_ms, timestamp_to_datetime, Result, FileMeta
};

// Re-exports from joplin-sync
pub use joplin_sync::{
    SyncEngine, ReqwestWebDavClient, WebDavConfig,
    SyncInfo, DeltaContext
};
