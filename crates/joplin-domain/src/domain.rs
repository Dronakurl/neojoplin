// Domain types matching Joplin database schema v41

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Generate a Joplin-compatible ID (32-char hex, no dashes)
pub fn joplin_id() -> String {
    Uuid::new_v4().simple().to_string()
}

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

/// Sync target types (Joplin sync.target values)
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

/// Note structure matching Joplin schema
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
    pub order: i64,
    pub latitude: i64,
    pub longitude: i64,
    pub altitude: i64,
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
            id: joplin_id(),
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

/// Folder (notebook) structure matching Joplin schema
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
            id: joplin_id(),
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

/// Tag structure matching Joplin schema
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
            id: joplin_id(),
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

/// Note-Tag association matching Joplin schema
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
            id: joplin_id(),
            note_id: String::new(),
            tag_id: String::new(),
            created_time: now_ms(),
            updated_time: now_ms(),
            is_shared: 0,
        }
    }
}

/// Resource (file attachment) structure matching Joplin schema
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
            id: joplin_id(),
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

/// Master key for encryption matching Joplin schema
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

/// Sync item tracking matching Joplin schema
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

/// Deleted item tracking matching Joplin schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletedItem {
    pub id: i32,
    pub item_type: i32,
    pub item_id: String,
    pub deleted_time: i64,
    pub sync_target: i32,
}

/// Setting key-value pair matching Joplin schema
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
        .unwrap_or_default()
}

// Implement sqlx::FromRow for domain types (needed by storage implementations)
#[cfg(feature = "sqlx")]
use sqlx::Row;

#[cfg(feature = "sqlx")]
impl sqlx::FromRow<'_, sqlx::sqlite::SqliteRow> for Note {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> sqlx::Result<Self> {
        Ok(Note {
            id: row.try_get("id")?,
            title: row.try_get("title")?,
            body: row.try_get("body")?,
            created_time: row.try_get("created_time")?,
            updated_time: row.try_get("updated_time")?,
            user_created_time: row.try_get("user_created_time")?,
            user_updated_time: row.try_get("user_updated_time")?,
            is_shared: row.try_get("is_shared")?,
            share_id: row.try_get("share_id").ok(),
            master_key_id: row.try_get("master_key_id").ok(),
            encryption_applied: row.try_get("encryption_applied")?,
            encryption_cipher_text: row.try_get("encryption_cipher_text").ok(),
            parent_id: row.try_get("parent_id")?,
            is_conflict: row.try_get("is_conflict")?,
            is_todo: row.try_get("is_todo")?,
            todo_completed: row.try_get("todo_completed")?,
            todo_due: row.try_get("todo_due")?,
            source: row.try_get("source")?,
            source_application: row.try_get("source_application")?,
            order: row.try_get("order")?,
            latitude: row.try_get("latitude")?,
            longitude: row.try_get("longitude")?,
            altitude: row.try_get("altitude")?,
            author: row.try_get("author")?,
            source_url: row.try_get("source_url")?,
            application_data: row.try_get("application_data")?,
            markup_language: row.try_get("markup_language")?,
            encryption_blob_encrypted: row.try_get("encryption_blob_encrypted")?,
            conflict_original_id: row.try_get("conflict_original_id")?,
        })
    }
}

#[cfg(feature = "sqlx")]
impl sqlx::FromRow<'_, sqlx::sqlite::SqliteRow> for Folder {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> sqlx::Result<Self> {
        Ok(Folder {
            id: row.try_get("id")?,
            title: row.try_get("title")?,
            created_time: row.try_get("created_time")?,
            updated_time: row.try_get("updated_time")?,
            user_created_time: row.try_get("user_created_time")?,
            user_updated_time: row.try_get("user_updated_time")?,
            is_shared: row.try_get("is_shared").unwrap_or(0),
            share_id: row.try_get("share_id").ok(),
            master_key_id: row.try_get("master_key_id").ok(),
            encryption_applied: 0,
            encryption_cipher_text: None,
            parent_id: row.try_get("parent_id").unwrap_or_else(|_| String::new()),
            icon: row.try_get("icon").unwrap_or_else(|_| String::new()),
        })
    }
}

#[cfg(feature = "sqlx")]
impl sqlx::FromRow<'_, sqlx::sqlite::SqliteRow> for Tag {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> sqlx::Result<Self> {
        Ok(Tag {
            id: row.try_get("id")?,
            title: row.try_get("title")?,
            created_time: row.try_get("created_time")?,
            updated_time: row.try_get("updated_time")?,
            user_created_time: row.try_get("user_created_time")?,
            user_updated_time: row.try_get("user_updated_time")?,
            parent_id: row.try_get("parent_id").unwrap_or_else(|_| String::new()),
            is_shared: row.try_get("is_shared").unwrap_or(0),
        })
    }
}

#[cfg(feature = "sqlx")]
impl sqlx::FromRow<'_, sqlx::sqlite::SqliteRow> for NoteTag {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> sqlx::Result<Self> {
        Ok(NoteTag {
            id: row.try_get("id")?,
            note_id: row.try_get("note_id")?,
            tag_id: row.try_get("tag_id")?,
            created_time: row.try_get("created_time")?,
            updated_time: row.try_get("updated_time")?,
            is_shared: row.try_get("is_shared")?,
        })
    }
}

#[cfg(feature = "sqlx")]
impl sqlx::FromRow<'_, sqlx::sqlite::SqliteRow> for SyncItem {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> sqlx::Result<Self> {
        Ok(SyncItem {
            id: row.try_get("id")?,
            sync_target: row.try_get("sync_target")?,
            sync_time: row.try_get("sync_time")?,
            item_type: row.try_get("item_type")?,
            item_id: row.try_get("item_id")?,
            sync_disabled: row.try_get("sync_disabled")?,
            sync_disabled_reason: row.try_get("sync_disabled_reason")?,
            item_location: row.try_get("item_location")?,
        })
    }
}

#[cfg(feature = "sqlx")]
impl sqlx::FromRow<'_, sqlx::sqlite::SqliteRow> for DeletedItem {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> sqlx::Result<Self> {
        Ok(DeletedItem {
            id: row.try_get("id")?,
            item_type: row.try_get("item_type")?,
            item_id: row.try_get("item_id")?,
            deleted_time: row.try_get("deleted_time")?,
            sync_target: row.try_get("sync_target")?,
        })
    }
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
