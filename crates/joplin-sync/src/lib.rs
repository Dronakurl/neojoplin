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
pub use crypto::{encrypt_aes256_gcm, decrypt_aes256_gcm, generate_key, derive_key};

// Re-export from joplin-domain for convenience
pub use joplin_domain::{
    WebDavError, SyncError, SyncPhase, SyncEvent,
    Storage, Note, Folder, Tag, NoteTag
};
