// neojoplin-sync - NeoJoplin-specific sync wrapper
//
// This crate provides NeoJoplin-specific convenience wrappers around
// the generic joplin-sync functionality.

// Re-export from joplin-sync for convenience
pub use joplin_sync::{DeltaContext, ReqwestWebDavClient, SyncEngine, SyncInfo, WebDavConfig};

mod fake_webdav;

pub use fake_webdav::FakeWebDavClient;

// Re-export from joplin-domain for convenience
pub use joplin_domain::{
    DavEntry, LockHandle, SyncError, SyncEvent, SyncPhase, SyncState, WebDavClient, WebDavError,
};
