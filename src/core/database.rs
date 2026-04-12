// Database layer for Joplin-compatible SQLite storage

use sqlx::{sqlite::SqliteConnectOptions, SqlitePool, Row};
use sqlx::sqlite::SqlitePoolOptions;
use anyhow::{Result, Context};
use std::str::FromStr;
use std::path::PathBuf;
use dirs::home_dir;
use crate::core::models::{Note, Folder, now_ms};

/// Database manager for Joplin SQLite database
#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create a new database connection, initializing if necessary
    pub async fn new() -> Result<Self> {
        let db_path = Self::db_path()?;

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        let db = Self { pool };

        // Initialize schema if needed
        db.initialize().await?;

        Ok(db)
    }

    /// Get the database path
    fn db_path() -> Result<PathBuf> {
        let data_dir = home_dir()
            .context("Could not find home directory")?
            .join(".local/share/neojoplin");

        Ok(data_dir.join("joplin.db"))
    }

    /// Initialize the database schema
    async fn initialize(&self) -> Result<()> {
        // Check if database is already initialized
        let version_result: Result<Option<i32>, _> = sqlx::query_scalar(
            "SELECT version FROM version LIMIT 1"
        )
        .fetch_optional(&self.pool)
        .await;

        if let Ok(Some(version)) = version_result {
            if version >= 41 {
                tracing::info!("Database already initialized at version {}", version);
                return Ok(());
            }
        }

        tracing::info!("Initializing database schema v41");

        // Create tables
        self.create_schema().await?;

        Ok(())
    }

    /// Create the database schema
    async fn create_schema(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // Create version table first
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS version (
                version INTEGER NOT NULL,
                table_fields_version INTEGER DEFAULT 0
            )
            "#
        )
        .execute(&mut *tx)
        .await?;

        // Create notes table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS notes (
                id TEXT PRIMARY KEY,
                title TEXT,
                body TEXT,
                created_time INTEGER NOT NULL,
                updated_time INTEGER NOT NULL,
                user_created_time INTEGER DEFAULT 0,
                user_updated_time INTEGER DEFAULT 0,
                is_conflict INTEGER DEFAULT 0,
                is_todo INTEGER DEFAULT 0,
                todo_completed INTEGER DEFAULT 0,
                todo_due INTEGER DEFAULT 0,
                source TEXT,
                source_application TEXT,
                "order" NUMERIC DEFAULT 0,
                latitude NUMERIC DEFAULT 0,
                longitude NUMERIC DEFAULT 0,
                altitude NUMERIC DEFAULT 0,
                author TEXT,
                source_url TEXT,
                is_shared INTEGER DEFAULT 0,
                application_data TEXT,
                markup_language INTEGER DEFAULT 1,
                parent_id TEXT,
                encryption_cipher_text TEXT,
                encryption_applied INTEGER DEFAULT 0,
                encryption_blob_encrypted INTEGER DEFAULT 0,
                master_key_id TEXT,
                share_id TEXT,
                conflict_original_id TEXT
            )
            "#
        )
        .execute(&mut *tx)
        .await?;

        // Create folders table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS folders (
                id TEXT PRIMARY KEY,
                title TEXT,
                created_time INTEGER NOT NULL,
                updated_time INTEGER NOT NULL,
                user_created_time INTEGER DEFAULT 0,
                user_updated_time INTEGER DEFAULT 0,
                parent_id TEXT,
                icon TEXT,
                share_id TEXT,
                master_key_id TEXT,
                is_shared INTEGER DEFAULT 0
            )
            "#
        )
        .execute(&mut *tx)
        .await?;

        // Create tags table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tags (
                id TEXT PRIMARY KEY,
                title TEXT,
                created_time INTEGER NOT NULL,
                updated_time INTEGER NOT NULL,
                user_created_time INTEGER DEFAULT 0,
                user_updated_time INTEGER DEFAULT 0,
                parent_id TEXT,
                is_shared INTEGER DEFAULT 0
            )
            "#
        )
        .execute(&mut *tx)
        .await?;

        // Create note_tags table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS note_tags (
                id TEXT PRIMARY KEY,
                note_id TEXT NOT NULL,
                tag_id TEXT NOT NULL,
                created_time INTEGER NOT NULL,
                updated_time INTEGER NOT NULL,
                is_shared INTEGER DEFAULT 0,
                FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE,
                FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
            )
            "#
        )
        .execute(&mut *tx)
        .await?;

        // Create resources table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS resources (
                id TEXT PRIMARY KEY,
                title TEXT,
                filename TEXT,
                file_extension TEXT,
                mime TEXT,
                size INTEGER DEFAULT -1,
                created_time INTEGER NOT NULL,
                updated_time INTEGER NOT NULL,
                user_created_time INTEGER DEFAULT 0,
                user_updated_time INTEGER DEFAULT 0,
                blob_updated_time INTEGER DEFAULT 0,
                encryption_cipher_text TEXT,
                encryption_applied INTEGER DEFAULT 0,
                encryption_blob_encrypted INTEGER DEFAULT 0,
                share_id TEXT,
                master_key_id TEXT,
                is_shared INTEGER DEFAULT 0
            )
            "#
        )
        .execute(&mut *tx)
        .await?;

        // Create settings table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT,
                type INTEGER
            )
            "#
        )
        .execute(&mut *tx)
        .await?;

        // Create sync_items table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sync_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                sync_target INTEGER NOT NULL,
                sync_time INTEGER DEFAULT 0,
                item_type INTEGER NOT NULL,
                item_id TEXT NOT NULL,
                sync_disabled INTEGER DEFAULT 0,
                sync_disabled_reason TEXT,
                item_location INTEGER DEFAULT 1
            )
            "#
        )
        .execute(&mut *tx)
        .await?;

        // Create deleted_items table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS deleted_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                item_type INTEGER NOT NULL,
                item_id TEXT NOT NULL,
                deleted_time INTEGER NOT NULL,
                sync_target INTEGER NOT NULL
            )
            "#
        )
        .execute(&mut *tx)
        .await?;

        // Create full-text search table
        sqlx::query(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
                id, title, body,
                content='notes',
                content_rowid='rowid'
            )
            "#
        )
        .execute(&mut *tx)
        .await?;

        // Set database version
        sqlx::query("INSERT INTO version (version) VALUES (41)")
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        tracing::info!("Database schema created successfully");
        Ok(())
    }

    /// Create a new note
    pub async fn create_note(&self, note: &Note) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO notes (
                id, title, body, created_time, updated_time,
                user_created_time, user_updated_time, parent_id,
                is_conflict, is_todo, todo_completed, todo_due,
                source, source_application, "order", latitude, longitude,
                altitude, author, source_url, is_shared, application_data,
                markup_language, encryption_cipher_text, encryption_applied,
                encryption_blob_encrypted, master_key_id, share_id,
                conflict_original_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&note.id)
        .bind(&note.title)
        .bind(&note.body)
        .bind(note.created_time)
        .bind(note.updated_time)
        .bind(note.user_created_time)
        .bind(note.user_updated_time)
        .bind(&note.parent_id)
        .bind(note.is_conflict)
        .bind(note.is_todo)
        .bind(note.todo_completed)
        .bind(note.todo_due)
        .bind(&note.source)
        .bind(&note.source_application)
        .bind(note.order)
        .bind(note.latitude)
        .bind(note.longitude)
        .bind(note.altitude)
        .bind(&note.author)
        .bind(&note.source_url)
        .bind(note.is_shared)
        .bind(&note.application_data)
        .bind(note.markup_language)
        .bind(&note.encryption_cipher_text)
        .bind(note.encryption_applied)
        .bind(note.encryption_blob_encrypted)
        .bind(&note.master_key_id)
        .bind(&note.share_id)
        .bind(&note.conflict_original_id)
        .execute(&self.pool)
        .await?;

        // Update full-text search
        self.update_note_fts(note).await?;

        Ok(())
    }

    /// Update full-text search index for a note
    async fn update_note_fts(&self, note: &Note) -> Result<()> {
        // First delete existing entry
        sqlx::query("DELETE FROM notes_fts WHERE id = ?")
            .bind(&note.id)
            .execute(&self.pool)
            .await?;

        // Then insert new entry
        sqlx::query(
            r#"
            INSERT INTO notes_fts (id, title, body)
            VALUES (?, ?, ?)
            "#
        )
        .bind(&note.id)
        .bind(&note.title)
        .bind(&note.body)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get a note by ID
    pub async fn get_note(&self, id: &str) -> Result<Option<Note>> {
        let row = sqlx::query_as::<_, Note>(
            r#"
            SELECT * FROM notes WHERE id = ?
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// List all notes in a folder
    pub async fn list_notes(&self, folder_id: Option<&str>) -> Result<Vec<Note>> {
        let notes = if let Some(folder_id) = folder_id {
            sqlx::query_as::<_, Note>(
                r#"
                SELECT * FROM notes
                WHERE parent_id = ?
                ORDER BY "order" ASC, title ASC
                "#
            )
            .bind(folder_id)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, Note>(
                r#"
                SELECT * FROM notes
                ORDER BY "order" ASC, title ASC
                "#
            )
            .fetch_all(&self.pool)
            .await?
        };

        Ok(notes)
    }

    /// Update a note
    pub async fn update_note(&self, note: &mut Note) -> Result<()> {
        note.updated_time = now_ms();

        sqlx::query(
            r#"
            UPDATE notes SET
                title = ?, body = ?, updated_time = ?,
                user_updated_time = ?, parent_id = ?,
                is_conflict = ?, is_todo = ?, todo_completed = ?,
                todo_due = ?, source = ?, source_application = ?,
                "order" = ?, latitude = ?, longitude = ?,
                altitude = ?, author = ?, source_url = ?,
                is_shared = ?, application_data = ?,
                markup_language = ?, encryption_cipher_text = ?,
                encryption_applied = ?, encryption_blob_encrypted = ?,
                master_key_id = ?, share_id = ?,
                conflict_original_id = ?
            WHERE id = ?
            "#
        )
        .bind(&note.title)
        .bind(&note.body)
        .bind(note.updated_time)
        .bind(note.user_updated_time)
        .bind(&note.parent_id)
        .bind(note.is_conflict)
        .bind(note.is_todo)
        .bind(note.todo_completed)
        .bind(note.todo_due)
        .bind(&note.source)
        .bind(&note.source_application)
        .bind(note.order)
        .bind(note.latitude)
        .bind(note.longitude)
        .bind(note.altitude)
        .bind(&note.author)
        .bind(&note.source_url)
        .bind(note.is_shared)
        .bind(&note.application_data)
        .bind(note.markup_language)
        .bind(&note.encryption_cipher_text)
        .bind(note.encryption_applied)
        .bind(note.encryption_blob_encrypted)
        .bind(&note.master_key_id)
        .bind(&note.share_id)
        .bind(&note.conflict_original_id)
        .bind(&note.id)
        .execute(&self.pool)
        .await?;

        // Update full-text search
        self.update_note_fts(note).await?;

        Ok(())
    }

    /// Delete a note
    pub async fn delete_note(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM notes WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Create a new folder
    pub async fn create_folder(&self, folder: &Folder) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO folders (
                id, title, created_time, updated_time,
                user_created_time, user_updated_time, parent_id,
                icon, share_id, master_key_id, is_shared
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&folder.id)
        .bind(&folder.title)
        .bind(folder.created_time)
        .bind(folder.updated_time)
        .bind(folder.user_created_time)
        .bind(folder.user_updated_time)
        .bind(&folder.parent_id)
        .bind(&folder.icon)
        .bind(&folder.share_id)
        .bind(&folder.master_key_id)
        .bind(folder.is_shared)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get a folder by ID
    pub async fn get_folder(&self, id: &str) -> Result<Option<Folder>> {
        let row = sqlx::query_as::<_, Folder>(
            r#"
            SELECT * FROM folders WHERE id = ?
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// List all folders
    pub async fn list_folders(&self) -> Result<Vec<Folder>> {
        let folders = sqlx::query_as::<_, Folder>(
            r#"
            SELECT * FROM folders
            ORDER BY title ASC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(folders)
    }

    /// Get setting value
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let value: Option<String> = sqlx::query_scalar(
            "SELECT value FROM settings WHERE key = ?"
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(value)
    }

    /// Set setting value
    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO settings (key, value, type) VALUES (?, ?, 2)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get database version
    pub async fn get_version(&self) -> Result<i32> {
        let version: Option<i32> = sqlx::query_scalar(
            "SELECT version FROM version LIMIT 1"
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(version.unwrap_or(0))
    }
}

// Implement sqlx::FromRow for Note
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
            share_id: row.try_get("share_id")?,
            master_key_id: row.try_get("master_key_id")?,
            encryption_applied: row.try_get("encryption_applied")?,
            encryption_cipher_text: row.try_get("encryption_cipher_text")?,
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

// Implement sqlx::FromRow for Folder
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
            encryption_applied: 0, // Folders don't have encryption_applied in schema
            encryption_cipher_text: None, // Folders don't have encryption_cipher_text in schema
            parent_id: row.try_get("parent_id").unwrap_or_else(|_| String::new()),
            icon: row.try_get("icon").unwrap_or_else(|_| String::new()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_creation() {
        let db = Database::new().await.unwrap();
        let version = db.get_version().await.unwrap();
        assert_eq!(version, 41);
    }

    #[tokio::test]
    async fn test_create_note() {
        let db = Database::new().await.unwrap();

        let note = Note {
            title: "Test Note".to_string(),
            body: "Test content".to_string(),
            parent_id: "test-folder".to_string(),
            ..Default::default()
        };

        db.create_note(&note).await.unwrap();

        let retrieved = db.get_note(&note.id).await.unwrap().unwrap();
        assert_eq!(retrieved.title, "Test Note");
        assert_eq!(retrieved.body, "Test content");
    }

    #[tokio::test]
    async fn test_folder_operations() {
        let db = Database::new().await.unwrap();

        let folder = Folder {
            title: "Test Folder".to_string(),
            icon: r#"{"emoji":"📁"}"#.to_string(),
            ..Default::default()
        };

        db.create_folder(&folder).await.unwrap();

        let retrieved = db.get_folder(&folder.id).await.unwrap().unwrap();
        assert_eq!(retrieved.title, "Test Folder");
    }

    #[tokio::test]
    async fn test_settings() {
        let db = Database::new().await.unwrap();

        db.set_setting("test_key", "test_value").await.unwrap();
        let value = db.get_setting("test_key").await.unwrap().unwrap();
        assert_eq!(value, "test_value");
    }
}
