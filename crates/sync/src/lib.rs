// neojoplin-sync - NeoJoplin-specific sync wrapper
//
// This crate provides NeoJoplin-specific convenience wrappers around
// the generic joplin-sync functionality.

// Re-export from joplin-sync for convenience
pub use joplin_sync::{
    SyncEngine, ReqwestWebDavClient, WebDavConfig,
    SyncInfo, DeltaContext
};

// Re-export from joplin-domain for convenience
pub use joplin_domain::{
    WebDavClient, DavEntry, SyncEvent, LockHandle,
    SyncPhase, SyncState, WebDavError, SyncError
};
