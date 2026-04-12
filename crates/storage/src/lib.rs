// SQLite storage implementation for NeoJoplin

use sqlx::{sqlite::SqliteConnectOptions, SqlitePool, sqlite::SqlitePoolOptions};
use std::path::PathBuf;
use std::str::FromStr;

use neojoplin_core::{
    Storage, DatabaseError, Note, Folder, Tag, NoteTag,
    SyncItem, DeletedItem, now_ms
};


/// SQLite storage implementation
pub struct SqliteStorage {
    pool: SqlitePool,
}

impl SqliteStorage {
    /// Create a new SQLite storage connection
    pub async fn new() -> Result<Self, DatabaseError> {
        let db_path = Self::db_path()?;

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to create directory: {}", e)))?;
        }

        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Invalid connection string: {}", e)))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to connect: {}", e)))?;

        let storage = Self { pool };
        storage.initialize().await?;

        Ok(storage)
    }

    /// Get the database path
    fn db_path() -> Result<PathBuf, DatabaseError> {
        let data_dir = neojoplin_core::Config::data_dir()
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Could not determine data directory: {}", e)))?;
        Ok(data_dir.join("joplin.db"))
    }

    /// Create storage for testing with a specific path
    pub async fn with_path(path: &PathBuf) -> Result<Self, DatabaseError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to create directory: {}", e)))?;
        }

        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", path.display()))
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Invalid connection string: {}", e)))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to connect: {}", e)))?;

        let storage = Self { pool };
        storage.initialize().await?;

        Ok(storage)
    }

    /// Initialize the database schema
    async fn initialize(&self) -> Result<(), DatabaseError> {
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
        self.create_schema().await
    }

    /// Create the database schema
    async fn create_schema(&self) -> Result<(), DatabaseError> {
        let mut tx = self.pool.begin()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to begin transaction: {}", e)))?;

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
        .await
        .map_err(|e| DatabaseError::MigrationFailed(format!("Failed to create version table: {}", e)))?;

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
                "order" INTEGER DEFAULT 0,
                latitude INTEGER DEFAULT 0,
                longitude INTEGER DEFAULT 0,
                altitude INTEGER DEFAULT 0,
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
        .await
        .map_err(|e| DatabaseError::MigrationFailed(format!("Failed to create notes table: {}", e)))?;

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
        .await
        .map_err(|e| DatabaseError::MigrationFailed(format!("Failed to create folders table: {}", e)))?;

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
        .await
        .map_err(|e| DatabaseError::MigrationFailed(format!("Failed to create tags table: {}", e)))?;

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
        .await
        .map_err(|e| DatabaseError::MigrationFailed(format!("Failed to create note_tags table: {}", e)))?;

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
        .await
        .map_err(|e| DatabaseError::MigrationFailed(format!("Failed to create resources table: {}", e)))?;

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
        .await
        .map_err(|e| DatabaseError::MigrationFailed(format!("Failed to create settings table: {}", e)))?;

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
        .await
        .map_err(|e| DatabaseError::MigrationFailed(format!("Failed to create sync_items table: {}", e)))?;

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
        .await
        .map_err(|e| DatabaseError::MigrationFailed(format!("Failed to create deleted_items table: {}", e)))?;

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
        .await
        .map_err(|e| DatabaseError::MigrationFailed(format!("Failed to create notes_fts table: {}", e)))?;

        // Set database version
        sqlx::query("INSERT INTO version (version) VALUES (41)")
            .execute(&mut *tx)
            .await
            .map_err(|e| DatabaseError::MigrationFailed(format!("Failed to set version: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| DatabaseError::MigrationFailed(format!("Failed to commit transaction: {}", e)))?;

        tracing::info!("Database schema created successfully");
        Ok(())
    }

    /// Update full-text search index for a note
    async fn update_note_fts(&self, note: &Note) -> Result<(), DatabaseError> {
        // First delete existing entry
        sqlx::query("DELETE FROM notes_fts WHERE id = ?")
            .bind(&note.id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to delete from FTS: {}", e)))?;

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
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to insert into FTS: {}", e)))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl Storage for SqliteStorage {
    // Note operations
    async fn create_note(&self, note: &Note) -> Result<(), DatabaseError> {
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
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to create note: {}", e)))?;

        // Update full-text search
        self.update_note_fts(note).await?;

        Ok(())
    }

    async fn get_note(&self, id: &str) -> Result<Option<Note>, DatabaseError> {
        let row = sqlx::query_as::<_, Note>(
            r#"
            SELECT
                id, title, body, created_time, updated_time,
                user_created_time, user_updated_time, parent_id,
                is_conflict, is_todo, todo_completed, todo_due,
                source, source_application, "order", latitude, longitude,
                altitude, author, source_url, is_shared, application_data,
                markup_language, encryption_cipher_text, encryption_applied,
                encryption_blob_encrypted, master_key_id, share_id,
                conflict_original_id
            FROM notes WHERE id = ?
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get note: {}", e)))?;

        Ok(row)
    }

    async fn update_note(&self, note: &Note) -> Result<(), DatabaseError> {

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
        .bind(now_ms())
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
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to update note: {}", e)))?;

        // Update full-text search
        self.update_note_fts(note).await?;

        Ok(())
    }

    async fn delete_note(&self, id: &str) -> Result<(), DatabaseError> {
        sqlx::query("DELETE FROM notes WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to delete note: {}", e)))?;

        Ok(())
    }

    async fn list_notes(&self, folder_id: Option<&str>) -> Result<Vec<Note>, DatabaseError> {
        let notes = if let Some(folder_id) = folder_id {
            sqlx::query_as::<_, Note>(
                r#"
                SELECT
                    id, title, body, created_time, updated_time,
                    user_created_time, user_updated_time, parent_id,
                    is_conflict, is_todo, todo_completed, todo_due,
                    source, source_application, "order", latitude, longitude,
                    altitude, author, source_url, is_shared, application_data,
                    markup_language, encryption_cipher_text, encryption_applied,
                    encryption_blob_encrypted, master_key_id, share_id,
                    conflict_original_id
                FROM notes
                WHERE parent_id = ?
                ORDER BY "order" ASC, title ASC
                "#
            )
            .bind(folder_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to list notes: {}", e)))?
        } else {
            sqlx::query_as::<_, Note>(
                r#"
                SELECT
                    id, title, body, created_time, updated_time,
                    user_created_time, user_updated_time, parent_id,
                    is_conflict, is_todo, todo_completed, todo_due,
                    source, source_application, "order", latitude, longitude,
                    altitude, author, source_url, is_shared, application_data,
                    markup_language, encryption_cipher_text, encryption_applied,
                    encryption_blob_encrypted, master_key_id, share_id,
                    conflict_original_id
                FROM notes
                ORDER BY "order" ASC, title ASC
                "#
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to list notes: {}", e)))?
        };

        Ok(notes)
    }

    // Folder operations
    async fn create_folder(&self, folder: &Folder) -> Result<(), DatabaseError> {
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
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to create folder: {}", e)))?;

        Ok(())
    }

    async fn get_folder(&self, id: &str) -> Result<Option<Folder>, DatabaseError> {
        let row = sqlx::query_as::<_, Folder>(
            r#"
            SELECT
                id, title, created_time, updated_time,
                user_created_time, user_updated_time, parent_id,
                icon, share_id, master_key_id, is_shared
            FROM folders WHERE id = ?
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get folder: {}", e)))?;

        Ok(row)
    }

    async fn update_folder(&self, folder: &Folder) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            UPDATE folders SET
                title = ?, updated_time = ?,
                user_updated_time = ?, parent_id = ?,
                icon = ?, share_id = ?, master_key_id = ?, is_shared = ?
            WHERE id = ?
            "#
        )
        .bind(&folder.title)
        .bind(folder.updated_time)
        .bind(folder.user_updated_time)
        .bind(&folder.parent_id)
        .bind(&folder.icon)
        .bind(&folder.share_id)
        .bind(&folder.master_key_id)
        .bind(&folder.is_shared)
        .bind(&folder.id)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to update folder: {}", e)))?;

        Ok(())
    }

    async fn delete_folder(&self, id: &str) -> Result<(), DatabaseError> {
        sqlx::query("DELETE FROM folders WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to delete folder: {}", e)))?;

        Ok(())
    }

    async fn list_folders(&self) -> Result<Vec<Folder>, DatabaseError> {
        let folders = sqlx::query_as::<_, Folder>(
            r#"
            SELECT
                id, title, created_time, updated_time,
                user_created_time, user_updated_time, parent_id,
                icon, share_id, master_key_id, is_shared
            FROM folders
            ORDER BY title ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to list folders: {}", e)))?;

        Ok(folders)
    }

    // Tag operations
    async fn create_tag(&self, tag: &Tag) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO tags (
                id, title, created_time, updated_time,
                user_created_time, user_updated_time, parent_id,
                is_shared
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&tag.id)
        .bind(&tag.title)
        .bind(tag.created_time)
        .bind(tag.updated_time)
        .bind(tag.user_created_time)
        .bind(tag.user_updated_time)
        .bind(&tag.parent_id)
        .bind(tag.is_shared)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to create tag: {}", e)))?;

        Ok(())
    }

    async fn get_tag(&self, id: &str) -> Result<Option<Tag>, DatabaseError> {
        let row = sqlx::query_as::<_, Tag>(
            r#"
            SELECT
                id, title, created_time, updated_time,
                user_created_time, user_updated_time, parent_id,
                is_shared
            FROM tags WHERE id = ?
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get tag: {}", e)))?;

        Ok(row)
    }

    async fn update_tag(&self, tag: &Tag) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            UPDATE tags SET
                title = ?, updated_time = ?,
                user_updated_time = ?, parent_id = ?
            WHERE id = ?
            "#
        )
        .bind(&tag.title)
        .bind(tag.updated_time)
        .bind(tag.user_updated_time)
        .bind(&tag.parent_id)
        .bind(&tag.id)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to update tag: {}", e)))?;

        Ok(())
    }

    async fn delete_tag(&self, id: &str) -> Result<(), DatabaseError> {
        sqlx::query("DELETE FROM tags WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to delete tag: {}", e)))?;

        Ok(())
    }

    async fn list_tags(&self) -> Result<Vec<Tag>, DatabaseError> {
        let tags = sqlx::query_as::<_, Tag>(
            r#"
            SELECT
                id, title, created_time, updated_time,
                user_created_time, user_updated_time, parent_id,
                is_shared
            FROM tags
            ORDER BY title ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to list tags: {}", e)))?;

        Ok(tags)
    }

    // Note-Tag association
    async fn add_note_tag(&self, note_tag: &NoteTag) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO note_tags (
                id, note_id, tag_id, created_time, updated_time, is_shared
            ) VALUES (?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&note_tag.id)
        .bind(&note_tag.note_id)
        .bind(&note_tag.tag_id)
        .bind(note_tag.created_time)
        .bind(note_tag.updated_time)
        .bind(note_tag.is_shared)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to add note tag: {}", e)))?;

        Ok(())
    }

    async fn remove_note_tag(&self, note_id: &str, tag_id: &str) -> Result<(), DatabaseError> {
        sqlx::query("DELETE FROM note_tags WHERE note_id = ? AND tag_id = ?")
            .bind(note_id)
            .bind(tag_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to remove note tag: {}", e)))?;

        Ok(())
    }

    async fn get_note_tags(&self, note_id: &str) -> Result<Vec<Tag>, DatabaseError> {
        let tags = sqlx::query_as::<_, Tag>(
            r#"
            SELECT
                t.id, t.title, t.created_time, t.updated_time,
                t.user_created_time, t.user_updated_time, t.parent_id,
                t.is_shared
            FROM tags t
            INNER JOIN note_tags nt ON t.id = nt.tag_id
            WHERE nt.note_id = ?
            ORDER BY t.title ASC
            "#
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get note tags: {}", e)))?;

        Ok(tags)
    }

    // Settings
    async fn get_setting(&self, key: &str) -> Result<Option<String>, DatabaseError> {
        let value: Option<String> = sqlx::query_scalar(
            "SELECT value FROM settings WHERE key = ?"
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get setting: {}", e)))?;

        Ok(value)
    }

    async fn set_setting(&self, key: &str, value: &str) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO settings (key, value, type) VALUES (?, ?, 2)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to set setting: {}", e)))?;

        Ok(())
    }

    // Sync state
    async fn get_sync_items(&self, sync_target: i32) -> Result<Vec<SyncItem>, DatabaseError> {
        let items = sqlx::query_as::<_, SyncItem>(
            r#"
            SELECT
                id, sync_target, sync_time, item_type, item_id,
                sync_disabled, sync_disabled_reason, item_location
            FROM sync_items
            WHERE sync_target = ?
            "#
        )
        .bind(sync_target)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get sync items: {}", e)))?;

        Ok(items)
    }

    async fn upsert_sync_item(&self, item: &SyncItem) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO sync_items (
                sync_target, sync_time, item_type, item_id,
                sync_disabled, sync_disabled_reason, item_location
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                sync_target = excluded.sync_target,
                sync_time = excluded.sync_time,
                item_type = excluded.item_type,
                item_id = excluded.item_id,
                sync_disabled = excluded.sync_disabled,
                sync_disabled_reason = excluded.sync_disabled_reason,
                item_location = excluded.item_location
            "#
        )
        .bind(item.sync_target)
        .bind(item.sync_time)
        .bind(item.item_type)
        .bind(&item.item_id)
        .bind(item.sync_disabled)
        .bind(&item.sync_disabled_reason)
        .bind(item.item_location)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to upsert sync item: {}", e)))?;

        Ok(())
    }

    async fn delete_sync_item(&self, id: i32) -> Result<(), DatabaseError> {
        sqlx::query("DELETE FROM sync_items WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to delete sync item: {}", e)))?;

        Ok(())
    }

    // Deleted items
    async fn get_deleted_items(&self, sync_target: i32) -> Result<Vec<DeletedItem>, DatabaseError> {
        let items = sqlx::query_as::<_, DeletedItem>(
            r#"
            SELECT
                id, item_type, item_id, deleted_time, sync_target
            FROM deleted_items
            WHERE sync_target = ?
            "#
        )
        .bind(sync_target)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get deleted items: {}", e)))?;

        Ok(items)
    }

    async fn add_deleted_item(&self, item: &DeletedItem) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO deleted_items (
                item_type, item_id, deleted_time, sync_target
            ) VALUES (?, ?, ?, ?)
            "#
        )
        .bind(item.item_type)
        .bind(&item.item_id)
        .bind(item.deleted_time)
        .bind(item.sync_target)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to add deleted item: {}", e)))?;

        Ok(())
    }

    async fn remove_deleted_item(&self, id: i32) -> Result<(), DatabaseError> {
        sqlx::query("DELETE FROM deleted_items WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to remove deleted item: {}", e)))?;

        Ok(())
    }

    // Database info
    async fn get_version(&self) -> Result<i32, DatabaseError> {
        let version: Option<i32> = sqlx::query_scalar(
            "SELECT version FROM version LIMIT 1"
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get version: {}", e)))?;

        Ok(version.unwrap_or(0))
    }

    async fn begin_transaction(&self) -> Result<(), DatabaseError> {
        sqlx::query("BEGIN TRANSACTION")
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin transaction: {}", e)))?;
        Ok(())
    }

    async fn commit_transaction(&self) -> Result<(), DatabaseError> {
        sqlx::query("COMMIT")
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to commit transaction: {}", e)))?;
        Ok(())
    }

    async fn rollback_transaction(&self) -> Result<(), DatabaseError> {
        sqlx::query("ROLLBACK")
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to rollback transaction: {}", e)))?;
        Ok(())
    }
}

// FromRow implementations for Note, Folder, Tag, etc. are now in the core crate
// due to Rust orphan rules. They are conditionally compiled with #[cfg(feature = "sqlx")]

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup_test_db() -> SqliteStorage {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        SqliteStorage::with_path(&db_path).await.unwrap()
    }

    #[tokio::test]
    async fn test_database_creation() {
        let db = setup_test_db().await;
        let version = db.get_version().await.unwrap();
        assert_eq!(version, 41);
    }

    #[tokio::test]
    async fn test_create_note() {
        let db = setup_test_db().await;

        let note = Note {
            title: "Test Note".to_string(),
            body: "Test content".to_string(),
            parent_id: String::new(),
            ..Default::default()
        };

        db.create_note(&note).await.unwrap();

        let retrieved = db.get_note(&note.id).await.unwrap().unwrap();
        assert_eq!(retrieved.title, "Test Note");
        assert_eq!(retrieved.body, "Test content");
    }

    #[tokio::test]
    async fn test_folder_operations() {
        let db = setup_test_db().await;

        let folder = Folder {
            title: "Test Folder".to_string(),
            ..Default::default()
        };

        db.create_folder(&folder).await.unwrap();

        let retrieved = db.get_folder(&folder.id).await.unwrap().unwrap();
        assert_eq!(retrieved.title, "Test Folder");
    }

    #[tokio::test]
    async fn test_settings() {
        let db = setup_test_db().await;

        db.set_setting("test_key", "test_value").await.unwrap();
        let value = db.get_setting("test_key").await.unwrap().unwrap();
        assert_eq!(value, "test_value");
    }
}
