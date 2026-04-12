// Database models matching Joplin schema v41

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Item types as defined in Joplin database
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum ModelType {
    Note = 1,
    Folder = 2,
    Tag = 3,
    NoteTag = 4,
    Resource = 5,
}

/// Sync target types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum SyncTarget {
    Memory = 1,
    Filesystem = 2,
    OneDrive = 3,
    Dropbox = 4,
    AmazonS3 = 5,
    WebDAV = 6,
    JoplinServer = 11,
}

/// Markup language types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum MarkupLanguage {
    Markdown = 1,
    HTML = 2,
}

/// Base item fields that are common across notes, folders, tags, and resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseItem {
    pub id: String,
    pub title: String,
    pub created_time: i64,
    pub updated_time: i64,
    pub user_created_time: i64,
    pub user_updated_time: i64,
    pub is_shared: i32,
    pub share_id: Option<String>,
    pub master_key_id: Option<String>,
    pub encryption_applied: i32,
    pub encryption_cipher_text: Option<String>,
}

impl Default for BaseItem {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: String::new(),
            created_time: now_ms(),
            updated_time: now_ms(),
            user_created_time: 0,
            user_updated_time: 0,
            is_shared: 0,
            share_id: None,
            master_key_id: None,
            encryption_applied: 0,
            encryption_cipher_text: None,
        }
    }
}

/// Note structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    // Base fields
    pub id: String,
    pub title: String,
    pub body: String,
    pub created_time: i64,
    pub updated_time: i64,
    pub user_created_time: i64,
    pub user_updated_time: i64,
    pub is_shared: i32,
    pub share_id: Option<String>,
    pub master_key_id: Option<String>,
    pub encryption_applied: i32,
    pub encryption_cipher_text: Option<String>,

    // Note-specific fields
    pub parent_id: String,
    pub is_conflict: i32,
    pub is_todo: i32,
    pub todo_completed: i64,
    pub todo_due: i64,
    pub source: String,
    pub source_application: String,
    pub order: i64, // Changed from f64 to i64 to match database INTEGER
    pub latitude: i64, // Changed from f64 to i64 to match database INTEGER
    pub longitude: i64, // Changed from f64 to i64 to match database INTEGER
    pub altitude: i64, // Changed from f64 to i64 to match database INTEGER
    pub author: String,
    pub source_url: String,
    pub application_data: String,
    pub markup_language: i32,
    pub encryption_blob_encrypted: i32,
    pub conflict_original_id: String,
}

impl Default for Note {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: String::new(),
            body: String::new(),
            created_time: now_ms(),
            updated_time: now_ms(),
            user_created_time: 0,
            user_updated_time: 0,
            is_shared: 0,
            share_id: None,
            master_key_id: None,
            encryption_applied: 0,
            encryption_cipher_text: None,
            parent_id: String::new(),
            is_conflict: 0,
            is_todo: 0,
            todo_completed: 0,
            todo_due: 0,
            source: String::new(),
            source_application: String::new(),
            order: 0,
            latitude: 0,
            longitude: 0,
            altitude: 0,
            author: String::new(),
            source_url: String::new(),
            application_data: String::new(),
            markup_language: 1, // Default to Markdown
            encryption_blob_encrypted: 0,
            conflict_original_id: String::new(),
        }
    }
}

impl Note {
    /// Check if this note is a todo
    pub fn is_todo(&self) -> bool {
        self.is_todo == 1
    }

    /// Check if this todo is completed
    pub fn is_todo_completed(&self) -> bool {
        self.is_todo() && self.todo_completed > 0
    }

    /// Check if this note is encrypted
    pub fn is_encrypted(&self) -> bool {
        self.encryption_applied == 1
    }

    /// Check if this is a conflict note
    pub fn is_conflict(&self) -> bool {
        self.is_conflict == 1
    }
}

/// Folder (notebook) structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    // Base fields
    pub id: String,
    pub title: String,
    pub created_time: i64,
    pub updated_time: i64,
    pub user_created_time: i64,
    pub user_updated_time: i64,
    pub is_shared: i32,
    pub share_id: Option<String>,
    pub master_key_id: Option<String>,
    pub encryption_applied: i32,
    pub encryption_cipher_text: Option<String>,

    // Folder-specific fields
    pub parent_id: String,
    pub icon: String, // JSON string: {"emoji":"📝"} or similar
}

impl Default for Folder {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: String::new(),
            created_time: now_ms(),
            updated_time: now_ms(),
            user_created_time: 0,
            user_updated_time: 0,
            is_shared: 0,
            share_id: None,
            master_key_id: None,
            encryption_applied: 0,
            encryption_cipher_text: None,
            parent_id: String::new(),
            icon: String::new(),
        }
    }
}

/// Tag structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub title: String,
    pub created_time: i64,
    pub updated_time: i64,
    pub user_created_time: i64,
    pub user_updated_time: i64,
    pub parent_id: String,
    pub is_shared: i32,
}

impl Default for Tag {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: String::new(),
            created_time: now_ms(),
            updated_time: now_ms(),
            user_created_time: 0,
            user_updated_time: 0,
            parent_id: String::new(),
            is_shared: 0,
        }
    }
}

/// Note-Tag association
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteTag {
    pub id: String,
    pub note_id: String,
    pub tag_id: String,
    pub created_time: i64,
    pub updated_time: i64,
    pub is_shared: i32,
}

impl Default for NoteTag {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            note_id: String::new(),
            tag_id: String::new(),
            created_time: now_ms(),
            updated_time: now_ms(),
            is_shared: 0,
        }
    }
}

/// Resource (file attachment) structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    pub title: String,
    pub filename: String,
    pub file_extension: String,
    pub mime: String,
    pub size: i64,
    pub created_time: i64,
    pub updated_time: i64,
    pub user_created_time: i64,
    pub user_updated_time: i64,
    pub blob_updated_time: i64,
    pub encryption_cipher_text: String,
    pub encryption_applied: i32,
    pub encryption_blob_encrypted: i32,
    pub share_id: String,
    pub master_key_id: String,
    pub is_shared: i32,
}

impl Default for Resource {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: String::new(),
            filename: String::new(),
            file_extension: String::new(),
            mime: String::new(),
            size: -1,
            created_time: now_ms(),
            updated_time: now_ms(),
            user_created_time: 0,
            user_updated_time: 0,
            blob_updated_time: 0,
            encryption_cipher_text: String::new(),
            encryption_applied: 0,
            encryption_blob_encrypted: 0,
            share_id: String::new(),
            master_key_id: String::new(),
            is_shared: 0,
        }
    }
}

/// Master key for encryption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterKey {
    pub id: String,
    pub created_time: i64,
    pub updated_time: i64,
    pub source_application: String,
    pub encryption_method: i32,
    pub checksum: String,
    pub content: String,
}

/// Sync item tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncItem {
    pub id: i32,
    pub sync_target: i32,
    pub sync_time: i64,
    pub item_type: i32,
    pub item_id: String,
    pub sync_disabled: i32,
    pub sync_disabled_reason: String,
    pub item_location: i32,
}

/// Deleted item tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletedItem {
    pub id: i32,
    pub item_type: i32,
    pub item_id: String,
    pub deleted_time: i64,
    pub sync_target: i32,
}

/// Setting key-value pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub type_: i32,
}

/// Helper function to get current timestamp in milliseconds
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Helper function to convert timestamp to DateTime
pub fn timestamp_to_datetime(ts: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(ts / 1000, (ts % 1000) as u32 * 1_000_000)
        .unwrap_or(DateTime::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_default() {
        let note = Note::default();
        assert!(!note.id.is_empty());
        assert_eq!(note.is_todo, 0);
        assert_eq!(note.markup_language, 1);
    }

    #[test]
    fn test_note_is_todo() {
        let mut note = Note::default();
        assert!(!note.is_todo());

        note.is_todo = 1;
        assert!(note.is_todo());
    }

    #[test]
    fn test_note_is_encrypted() {
        let mut note = Note::default();
        assert!(!note.is_encrypted());

        note.encryption_applied = 1;
        assert!(note.is_encrypted());
    }

    #[test]
    fn test_now_ms() {
        let ts = now_ms();
        assert!(ts > 0);
        assert!(ts < 1_000_000_000_000_000); // Reasonable upper bound
    }
}
