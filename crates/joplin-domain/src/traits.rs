// Core traits for storage and sync operations

use async_trait::async_trait;
use futures::io::AsyncRead;

use crate::domain::*;
use crate::error::*;

/// Storage trait for database operations
///
/// This trait abstracts the database layer, allowing different implementations
/// (SQLite, in-memory for tests, etc.)
#[async_trait]
pub trait Storage: Send + Sync {
    // Note operations
    async fn create_note(&self, note: &Note) -> Result<(), DatabaseError>;
    async fn get_note(&self, id: &str) -> Result<Option<Note>, DatabaseError>;
    async fn update_note(&self, note: &Note) -> Result<(), DatabaseError>;
    async fn delete_note(&self, id: &str) -> Result<(), DatabaseError>;
    async fn list_notes(&self, folder_id: Option<&str>) -> Result<Vec<Note>, DatabaseError>;

    // Folder operations
    async fn create_folder(&self, folder: &Folder) -> Result<(), DatabaseError>;
    async fn get_folder(&self, id: &str) -> Result<Option<Folder>, DatabaseError>;
    async fn update_folder(&self, folder: &Folder) -> Result<(), DatabaseError>;
    async fn delete_folder(&self, id: &str) -> Result<(), DatabaseError>;
    async fn list_folders(&self) -> Result<Vec<Folder>, DatabaseError>;

    // Tag operations
    async fn create_tag(&self, tag: &Tag) -> Result<(), DatabaseError>;
    async fn get_tag(&self, id: &str) -> Result<Option<Tag>, DatabaseError>;
    async fn update_tag(&self, tag: &Tag) -> Result<(), DatabaseError>;
    async fn delete_tag(&self, id: &str) -> Result<(), DatabaseError>;
    async fn list_tags(&self) -> Result<Vec<Tag>, DatabaseError>;

    // Note-Tag association
    async fn add_note_tag(&self, note_tag: &NoteTag) -> Result<(), DatabaseError>;
    async fn remove_note_tag(&self, note_id: &str, tag_id: &str) -> Result<(), DatabaseError>;
    async fn get_note_tags(&self, note_id: &str) -> Result<Vec<Tag>, DatabaseError>;

    // Sync helper methods
    async fn get_folders_updated_since(&self, timestamp: i64) -> Result<Vec<Folder>, DatabaseError>;
    async fn get_tags_updated_since(&self, timestamp: i64) -> Result<Vec<Tag>, DatabaseError>;
    async fn get_notes_updated_since(&self, timestamp: i64) -> Result<Vec<Note>, DatabaseError>;
    async fn get_note_tags_updated_since(&self, timestamp: i64) -> Result<Vec<NoteTag>, DatabaseError>;
    async fn get_all_sync_items(&self) -> Result<Vec<SyncItem>, DatabaseError>;
    async fn update_sync_time(&self, table: &str, id: &str, timestamp: i64) -> Result<(), DatabaseError>;

    // Settings
    async fn get_setting(&self, key: &str) -> Result<Option<String>, DatabaseError>;
    async fn set_setting(&self, key: &str, value: &str) -> Result<(), DatabaseError>;

    // Sync state
    async fn get_sync_items(&self, sync_target: i32) -> Result<Vec<SyncItem>, DatabaseError>;
    async fn upsert_sync_item(&self, item: &SyncItem) -> Result<(), DatabaseError>;
    async fn delete_sync_item(&self, id: i32) -> Result<(), DatabaseError>;
    async fn clear_all_sync_items(&self) -> Result<usize, DatabaseError>;

    // Deleted items
    async fn get_deleted_items(&self, sync_target: i32) -> Result<Vec<DeletedItem>, DatabaseError>;
    async fn add_deleted_item(&self, item: &DeletedItem) -> Result<(), DatabaseError>;
    async fn remove_deleted_item(&self, id: i32) -> Result<(), DatabaseError>;
    async fn clear_deleted_items(&self, limit: i64) -> Result<usize, DatabaseError>;

    // Database info
    async fn get_version(&self) -> Result<i32, DatabaseError>;
    async fn begin_transaction(&self) -> Result<(), DatabaseError>;
    async fn commit_transaction(&self) -> Result<(), DatabaseError>;
    async fn rollback_transaction(&self) -> Result<(), DatabaseError>;
}

/// WebDAV client trait for sync operations
///
/// This trait abstracts WebDAV operations, allowing different implementations
/// (real WebDAV server, fake server for tests, etc.)
#[async_trait]
pub trait WebDavClient: Send + Sync {
    /// List entries in a directory
    async fn list(&self, path: &str) -> Result<Vec<DavEntry>, WebDavError>;

    /// Get file contents
    async fn get(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>, WebDavError>;

    /// Put file contents
    async fn put(&self, path: &str, body: &[u8], size: u64) -> Result<(), WebDavError>;

    /// Delete a file
    async fn delete(&self, path: &str) -> Result<(), WebDavError>;

    /// Create directory
    async fn mkcol(&self, path: &str) -> Result<(), WebDavError>;

    /// Check if file exists
    async fn exists(&self, path: &str) -> Result<bool, WebDavError>;

    /// Get file metadata
    async fn stat(&self, path: &str) -> Result<DavEntry, WebDavError>;

    /// Acquire lock
    async fn lock(&self, path: &str, timeout: std::time::Duration) -> Result<String, WebDavError>;

    /// Refresh lock
    async fn refresh_lock(&self, lock_token: &str) -> Result<(), WebDavError>;

    /// Release lock
    async fn unlock(&self, path: &str, lock_token: &str) -> Result<(), WebDavError>;

    /// Move file
    async fn mv(&self, from: &str, to: &str) -> Result<(), WebDavError>;

    /// Copy file
    async fn copy(&self, from: &str, to: &str) -> Result<(), WebDavError>;
}

/// WebDAV directory entry
#[derive(Debug, Clone)]
pub struct DavEntry {
    pub path: String,
    pub is_directory: bool,
    pub size: Option<u64>,
    pub modified: Option<i64>,
    pub etag: Option<String>,
}

/// Progress events from sync engine
#[derive(Debug, Clone)]
pub enum SyncEvent {
    /// Sync phase started
    PhaseStarted(SyncPhase),

    /// Sync phase completed
    PhaseCompleted(SyncPhase),

    /// Sync phase failed
    PhaseFailed { phase: SyncPhase, error: String },

    /// Item upload started
    ItemUpload { item_type: String, item_id: String },

    /// Item upload completed
    ItemUploadComplete { item_type: String, item_id: String },

    /// Item download started
    ItemDownload { item_type: String, item_id: String },

    /// Item download completed
    ItemDownloadComplete { item_type: String, item_id: String },

    /// Item deleted
    ItemDeleted { item_type: String, item_id: String },

    /// Progress update
    Progress {
        phase: SyncPhase,
        current: usize,
        total: usize,
        message: String,
    },

    /// Warning (non-fatal error)
    Warning { message: String },

    /// Lock acquired
    LockAcquired,

    /// Lock refresh failed
    LockRefreshFailed { error: String },

    /// Sync completed successfully
    Completed { duration: std::time::Duration },

    /// Sync failed
    Failed { error: String },
}

/// Lock handle for sync operations
#[allow(async_fn_in_trait)]
pub trait LockHandle: Send + Sync {
    /// Refresh the lock
    async fn refresh(&self) -> Result<(), WebDavError>;

    /// Release the lock
    async fn release(&self) -> Result<(), WebDavError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_phase_display() {
        // Test that SyncPhase can be converted to string for display
        let phase = SyncPhase::Upload;
        assert_eq!(format!("{:?}", phase), "Upload");
    }
}
