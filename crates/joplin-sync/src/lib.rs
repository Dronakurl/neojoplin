// joplin-sync - Joplin-compatible WebDAV sync engine
//
// This crate implements the three-phase sync protocol compatible with Joplin:
// 1. UPLOAD - Upload local changes to remote
// 2. DELETE_REMOTE - Delete items that were deleted locally
// 3. DELTA - Download remote changes

pub mod crypto;
pub mod e2ee;
pub mod sync_engine;
pub mod sync_info;
pub mod webdav;
pub mod webdav_trait;
pub mod webdav_xml;

pub use crypto::{decrypt_chunk, derive_key_pbkdf2, encrypt_chunk, generate_key};
pub use e2ee::{E2eeService, EncryptionMethod, MasterKey};
pub use sync_engine::SyncEngine;
pub use sync_info::{DeltaContext, SyncInfo};
pub use webdav::{ReqwestWebDavClient, WebDavConfig};

// Re-export from joplin-domain for convenience
pub use joplin_domain::{
    Folder, Note, NoteTag, Storage, SyncError, SyncEvent, SyncPhase, Tag, WebDavError,
};
