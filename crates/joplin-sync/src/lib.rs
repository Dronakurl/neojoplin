// joplin-sync - Joplin-compatible WebDAV sync engine
//
// This crate implements the three-phase sync protocol compatible with Joplin:
// 1. UPLOAD - Upload local changes to remote
// 2. DELETE_REMOTE - Delete items that were deleted locally
// 3. DELTA - Download remote changes

pub mod webdav;
pub mod webdav_xml;
pub mod webdav_trait;
pub mod sync_engine;
pub mod sync_info;
pub mod e2ee;
pub mod crypto;

pub use webdav::{ReqwestWebDavClient, WebDavConfig};
pub use sync_engine::SyncEngine;
pub use sync_info::{SyncInfo, DeltaContext};
pub use e2ee::{E2eeService, MasterKey, EncryptionMethod};
pub use crypto::{generate_key, encrypt_chunk, decrypt_chunk, derive_key_pbkdf2};

// Re-export from joplin-domain for convenience
pub use joplin_domain::{
    WebDavError, SyncError, SyncPhase, SyncEvent,
    Storage, Note, Folder, Tag, NoteTag
};
