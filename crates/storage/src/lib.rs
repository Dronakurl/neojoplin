// SQLite storage implementation for NeoJoplin

use serde::Deserialize;
use serde_json::{Map, Value};
use sqlx::{sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions, SqlitePool};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use joplin_domain::{
    now_ms, DatabaseError, DeletedItem, Folder, ModelType, Note, NoteRevision, NoteTag, Storage,
    SyncItem, SyncTarget, Tag,
};

/// SQLite storage implementation
pub struct SqliteStorage {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
pub struct NoteRevisionSnapshot {
    pub revision: NoteRevision,
    pub title: String,
    pub body: String,
    pub metadata: Map<String, Value>,
}

impl SqliteStorage {
    /// Create a new SQLite storage connection
    pub async fn new() -> Result<Self, DatabaseError> {
        let db_path = Self::db_path()?;

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                DatabaseError::ConnectionFailed(format!("Failed to create directory: {}", e))
            })?;
        }

        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))
            .map_err(|e| {
                DatabaseError::ConnectionFailed(format!("Invalid connection string: {}", e))
            })?
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
        let data_dir = neojoplin_core::Config::data_dir().map_err(|e| {
            DatabaseError::ConnectionFailed(format!("Could not determine data directory: {}", e))
        })?;
        Ok(data_dir.join("joplin.db"))
    }

    /// Create storage for testing with a specific path
    pub async fn with_path(path: &Path) -> Result<Self, DatabaseError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                DatabaseError::ConnectionFailed(format!("Failed to create directory: {}", e))
            })?;
        }

        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", path.display()))
            .map_err(|e| {
                DatabaseError::ConnectionFailed(format!("Invalid connection string: {}", e))
            })?
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
        let version_result: Result<Option<i32>, _> =
            sqlx::query_scalar("SELECT version FROM version LIMIT 1")
                .fetch_optional(&self.pool)
                .await;

        if let Ok(Some(version)) = version_result {
            if version == 41 {
                tracing::info!("Migrating database from v41 to v42");
                sqlx::query("ALTER TABLE notes ADD COLUMN deleted_time INTEGER DEFAULT 0")
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        DatabaseError::MigrationFailed(format!(
                            "Failed to add deleted_time column: {}",
                            e
                        ))
                    })?;
                sqlx::query("UPDATE version SET version = 42")
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        DatabaseError::MigrationFailed(format!("Failed to update version: {}", e))
                    })?;
                tracing::info!("Database migrated to v42");
                self.ensure_revision_table().await?;
                return Ok(());
            }
            if version == 42 {
                tracing::info!("Migrating database from v42 to v43");
                // Add encryption fields to folders table
                sqlx::query("ALTER TABLE folders ADD COLUMN encryption_cipher_text TEXT")
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        DatabaseError::MigrationFailed(format!(
                            "Failed to add encryption_cipher_text to folders: {}",
                            e
                        ))
                    })?;
                sqlx::query("ALTER TABLE folders ADD COLUMN encryption_applied INTEGER DEFAULT 0")
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        DatabaseError::MigrationFailed(format!(
                            "Failed to add encryption_applied to folders: {}",
                            e
                        ))
                    })?;
                sqlx::query(
                    "ALTER TABLE folders ADD COLUMN encryption_blob_encrypted INTEGER DEFAULT 0",
                )
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    DatabaseError::MigrationFailed(format!(
                        "Failed to add encryption_blob_encrypted to folders: {}",
                        e
                    ))
                })?;
                // Add encryption fields to tags table
                sqlx::query("ALTER TABLE tags ADD COLUMN encryption_cipher_text TEXT")
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        DatabaseError::MigrationFailed(format!(
                            "Failed to add encryption_cipher_text to tags: {}",
                            e
                        ))
                    })?;
                sqlx::query("ALTER TABLE tags ADD COLUMN encryption_applied INTEGER DEFAULT 0")
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        DatabaseError::MigrationFailed(format!(
                            "Failed to add encryption_applied to tags: {}",
                            e
                        ))
                    })?;
                sqlx::query(
                    "ALTER TABLE tags ADD COLUMN encryption_blob_encrypted INTEGER DEFAULT 0",
                )
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    DatabaseError::MigrationFailed(format!(
                        "Failed to add encryption_blob_encrypted to tags: {}",
                        e
                    ))
                })?;
                sqlx::query("ALTER TABLE tags ADD COLUMN master_key_id TEXT")
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        DatabaseError::MigrationFailed(format!(
                            "Failed to add master_key_id to tags: {}",
                            e
                        ))
                    })?;
                // Add encryption fields to note_tags table
                sqlx::query("ALTER TABLE note_tags ADD COLUMN encryption_cipher_text TEXT")
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        DatabaseError::MigrationFailed(format!(
                            "Failed to add encryption_cipher_text to note_tags: {}",
                            e
                        ))
                    })?;
                sqlx::query(
                    "ALTER TABLE note_tags ADD COLUMN encryption_applied INTEGER DEFAULT 0",
                )
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    DatabaseError::MigrationFailed(format!(
                        "Failed to add encryption_applied to note_tags: {}",
                        e
                    ))
                })?;
                sqlx::query(
                    "ALTER TABLE note_tags ADD COLUMN encryption_blob_encrypted INTEGER DEFAULT 0",
                )
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    DatabaseError::MigrationFailed(format!(
                        "Failed to add encryption_blob_encrypted to note_tags: {}",
                        e
                    ))
                })?;
                sqlx::query("ALTER TABLE note_tags ADD COLUMN master_key_id TEXT")
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        DatabaseError::MigrationFailed(format!(
                            "Failed to add master_key_id to note_tags: {}",
                            e
                        ))
                    })?;
                sqlx::query("UPDATE version SET version = 43")
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        DatabaseError::MigrationFailed(format!("Failed to update version: {}", e))
                    })?;
                tracing::info!("Database migrated to v43");
                self.ensure_revision_table().await?;
                return Ok(());
            }
            if version >= 43 {
                tracing::info!("Database already initialized at version {}", version);
                self.ensure_revision_table().await?;
                return Ok(());
            }
        }

        tracing::info!("Initializing database schema v42");
        self.create_schema().await?;
        self.ensure_revision_table().await?;
        Ok(())
    }

    async fn ensure_revision_table(&self) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS revisions (
                id TEXT PRIMARY KEY,
                parent_id TEXT NOT NULL DEFAULT "",
                item_type INTEGER NOT NULL,
                item_id TEXT NOT NULL,
                item_updated_time INTEGER NOT NULL,
                title_diff TEXT NOT NULL DEFAULT "",
                body_diff TEXT NOT NULL DEFAULT "",
                metadata_diff TEXT NOT NULL DEFAULT "",
                encryption_cipher_text TEXT NOT NULL DEFAULT "",
                encryption_applied INTEGER NOT NULL DEFAULT 0,
                updated_time INTEGER NOT NULL,
                created_time INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            DatabaseError::MigrationFailed(format!("Failed to create revisions table: {}", e))
        })?;

        for index_sql in [
            "CREATE INDEX IF NOT EXISTS revisions_parent_id ON revisions(parent_id)",
            "CREATE INDEX IF NOT EXISTS revisions_item_type ON revisions(item_type)",
            "CREATE INDEX IF NOT EXISTS revisions_item_id ON revisions(item_id)",
            "CREATE INDEX IF NOT EXISTS revisions_item_updated_time ON revisions(item_updated_time)",
            "CREATE INDEX IF NOT EXISTS revisions_updated_time ON revisions(updated_time)",
        ] {
            sqlx::query(index_sql)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    DatabaseError::MigrationFailed(format!(
                        "Failed to create revisions index: {}",
                        e
                    ))
                })?;
        }

        Ok(())
    }

    pub async fn list_note_revisions(
        &self,
        note_id: &str,
    ) -> Result<Vec<NoteRevision>, DatabaseError> {
        sqlx::query_as::<_, NoteRevision>(
            r#"
            SELECT
                id, parent_id, item_type, item_id, item_updated_time,
                title_diff, body_diff, metadata_diff, encryption_cipher_text,
                encryption_applied, updated_time, created_time
            FROM revisions
            WHERE item_type = 1 AND item_id = ?
            ORDER BY item_updated_time DESC, created_time DESC
            "#,
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to list note revisions: {}", e)))
    }

    pub async fn get_note_revision_snapshot(
        &self,
        note_id: &str,
        revision_id: &str,
    ) -> Result<NoteRevisionSnapshot, DatabaseError> {
        let revision = sqlx::query_as::<_, NoteRevision>(
            r#"
            SELECT
                id, parent_id, item_type, item_id, item_updated_time,
                title_diff, body_diff, metadata_diff, encryption_cipher_text,
                encryption_applied, updated_time, created_time
            FROM revisions
            WHERE id = ? AND item_type = 1 AND item_id = ?
            LIMIT 1
            "#,
        )
        .bind(revision_id)
        .bind(note_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to load revision: {}", e)))?
        .ok_or_else(|| DatabaseError::NotFound(format!("Revision not found: {}", revision_id)))?;

        if revision.encryption_applied == 1 {
            return Err(DatabaseError::InvalidData(format!(
                "Revision is encrypted and cannot be read: {}",
                revision_id
            )));
        }

        let mut revisions = sqlx::query_as::<_, NoteRevision>(
            r#"
            SELECT
                id, parent_id, item_type, item_id, item_updated_time,
                title_diff, body_diff, metadata_diff, encryption_cipher_text,
                encryption_applied, updated_time, created_time
            FROM revisions
            WHERE item_type = 1 AND item_id = ? AND item_updated_time <= ?
            ORDER BY item_updated_time ASC, created_time ASC
            "#,
        )
        .bind(note_id)
        .bind(revision.item_updated_time)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to load revision chain: {}", e)))?;

        if revisions.iter().all(|r| r.id != revision.id) {
            revisions.push(revision.clone());
            revisions.sort_by_key(|r| (r.item_updated_time, r.created_time));
        }

        build_revision_snapshot(revision, revisions)
    }

    pub async fn restore_note_to_revision(
        &self,
        note_id: &str,
        revision_id: &str,
    ) -> Result<Note, DatabaseError> {
        let snapshot = self
            .get_note_revision_snapshot(note_id, revision_id)
            .await?;
        let mut note = self
            .get_note(note_id)
            .await?
            .ok_or_else(|| DatabaseError::NotFound(format!("Note not found: {}", note_id)))?;
        apply_snapshot_to_note(&mut note, &snapshot);
        note.updated_time = now_ms();
        self.update_note(&note).await?;
        Ok(note)
    }

    async fn insert_note_revision(
        &self,
        note: &Note,
        item_updated_time: i64,
    ) -> Result<(), DatabaseError> {
        let now = now_ms();
        let title_diff = revision_text_patch(&note.title)?;
        let body_diff = revision_text_patch(&note.body)?;
        let metadata_diff = note_metadata_diff(note)?;

        sqlx::query(
            r#"
            INSERT INTO revisions (
                id, parent_id, item_type, item_id, item_updated_time,
                title_diff, body_diff, metadata_diff, encryption_cipher_text,
                encryption_applied, updated_time, created_time
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(joplin_domain::joplin_id())
        .bind("")
        .bind(1)
        .bind(&note.id)
        .bind(item_updated_time)
        .bind(title_diff)
        .bind(body_diff)
        .bind(metadata_diff)
        .bind("")
        .bind(0)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to insert note revision: {}", e))
        })?;

        Ok(())
    }

    /// Create the database schema
    async fn create_schema(&self) -> Result<(), DatabaseError> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            DatabaseError::ConnectionFailed(format!("Failed to begin transaction: {}", e))
        })?;

        // Create version table first
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS version (
                version INTEGER NOT NULL,
                table_fields_version INTEGER DEFAULT 0
            )
            "#,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            DatabaseError::MigrationFailed(format!("Failed to create version table: {}", e))
        })?;

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
                conflict_original_id TEXT,
                deleted_time INTEGER DEFAULT 0
            )
            "#,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            DatabaseError::MigrationFailed(format!("Failed to create notes table: {}", e))
        })?;

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
                is_shared INTEGER DEFAULT 0,
                encryption_cipher_text TEXT,
                encryption_applied INTEGER DEFAULT 0,
                encryption_blob_encrypted INTEGER DEFAULT 0
            )
            "#,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            DatabaseError::MigrationFailed(format!("Failed to create folders table: {}", e))
        })?;

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
                is_shared INTEGER DEFAULT 0,
                encryption_cipher_text TEXT,
                encryption_applied INTEGER DEFAULT 0,
                encryption_blob_encrypted INTEGER DEFAULT 0,
                master_key_id TEXT
            )
            "#,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            DatabaseError::MigrationFailed(format!("Failed to create tags table: {}", e))
        })?;

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
                encryption_cipher_text TEXT,
                encryption_applied INTEGER DEFAULT 0,
                encryption_blob_encrypted INTEGER DEFAULT 0,
                master_key_id TEXT,
                FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE,
                FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            DatabaseError::MigrationFailed(format!("Failed to create note_tags table: {}", e))
        })?;

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
            "#,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            DatabaseError::MigrationFailed(format!("Failed to create resources table: {}", e))
        })?;

        // Create settings table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT,
                type INTEGER
            )
            "#,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            DatabaseError::MigrationFailed(format!("Failed to create settings table: {}", e))
        })?;

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
            "#,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            DatabaseError::MigrationFailed(format!("Failed to create sync_items table: {}", e))
        })?;

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
            "#,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            DatabaseError::MigrationFailed(format!("Failed to create deleted_items table: {}", e))
        })?;

        // Create full-text search table (standalone, not external content)
        sqlx::query(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
                id UNINDEXED, title, body
            )
            "#,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            DatabaseError::MigrationFailed(format!("Failed to create notes_fts table: {}", e))
        })?;

        // Set database version
        sqlx::query("INSERT INTO version (version) VALUES (43)")
            .execute(&mut *tx)
            .await
            .map_err(|e| DatabaseError::MigrationFailed(format!("Failed to set version: {}", e)))?;

        tx.commit().await.map_err(|e| {
            DatabaseError::MigrationFailed(format!("Failed to commit transaction: {}", e))
        })?;

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
            "#,
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

#[derive(Debug, Deserialize)]
struct TextPatchChunk {
    diffs: Vec<(i32, String)>,
    start1: usize,
}

#[derive(Debug, Deserialize)]
struct MetadataPatch {
    #[serde(default)]
    new: Map<String, Value>,
    #[serde(default)]
    deleted: Vec<String>,
}

fn build_revision_snapshot(
    revision: NoteRevision,
    revisions: Vec<NoteRevision>,
) -> Result<NoteRevisionSnapshot, DatabaseError> {
    let mut revisions_by_id: std::collections::HashMap<&str, &NoteRevision> =
        std::collections::HashMap::new();
    for rev in &revisions {
        revisions_by_id.insert(rev.id.as_str(), rev);
    }

    let mut chain = Vec::new();
    let mut current_id = Some(revision.id.clone());
    while let Some(id) = current_id {
        let Some(rev) = revisions_by_id.get(id.as_str()) else {
            break;
        };
        chain.push((*rev).clone());
        current_id = if rev.parent_id.is_empty() {
            None
        } else {
            Some(rev.parent_id.clone())
        };
    }
    chain.reverse();

    let mut title = String::new();
    let mut body = String::new();
    let mut metadata = Map::new();
    for rev in &chain {
        if rev.encryption_applied == 1 {
            return Err(DatabaseError::InvalidData(format!(
                "Revision is encrypted and cannot be read: {}",
                rev.id
            )));
        }
        title = apply_text_patch(&title, &rev.title_diff)?;
        body = apply_text_patch(&body, &rev.body_diff)?;
        apply_metadata_patch(&mut metadata, &rev.metadata_diff)?;
    }

    Ok(NoteRevisionSnapshot {
        revision,
        title,
        body,
        metadata,
    })
}

fn apply_text_patch(base: &str, patch: &str) -> Result<String, DatabaseError> {
    if patch.is_empty() || patch == "[]" {
        return Ok(base.to_string());
    }

    if patch.starts_with("@@") {
        return Err(DatabaseError::InvalidData(
            "Legacy revision patches are not supported yet".to_string(),
        ));
    }

    let chunks: Vec<TextPatchChunk> = serde_json::from_str(patch).map_err(|e| {
        DatabaseError::InvalidData(format!("Invalid revision text patch format: {}", e))
    })?;

    let mut chars: Vec<char> = base.chars().collect();
    let mut offset: isize = 0;

    for chunk in chunks {
        let mut cursor = chunk.start1 as isize + offset;
        if cursor < 0 {
            cursor = 0;
        }
        let mut cursor = cursor as usize;
        for (op, text) in chunk.diffs {
            let diff_chars: Vec<char> = text.chars().collect();
            cursor = cursor.min(chars.len());
            match op {
                0 => {
                    cursor = cursor.saturating_add(diff_chars.len());
                }
                -1 => {
                    let end = cursor.saturating_add(diff_chars.len()).min(chars.len());
                    if cursor <= end {
                        chars.drain(cursor..end);
                        offset -= (end - cursor) as isize;
                    }
                }
                1 => {
                    chars.splice(cursor..cursor, diff_chars.clone());
                    cursor = cursor.saturating_add(diff_chars.len());
                    offset += diff_chars.len() as isize;
                }
                _ => {
                    return Err(DatabaseError::InvalidData(format!(
                        "Unknown revision text patch operation: {}",
                        op
                    )));
                }
            }
        }
    }

    Ok(chars.into_iter().collect())
}

fn apply_metadata_patch(
    metadata: &mut Map<String, Value>,
    patch: &str,
) -> Result<(), DatabaseError> {
    if patch.is_empty() {
        return Ok(());
    }

    let parsed: MetadataPatch =
        serde_json::from_str(&patch.replace(['\n', '\r'], "")).map_err(|e| {
            DatabaseError::InvalidData(format!("Invalid revision metadata patch format: {}", e))
        })?;
    for (k, v) in parsed.new {
        metadata.insert(k, v);
    }
    for k in parsed.deleted {
        metadata.remove(&k);
    }
    Ok(())
}

fn apply_snapshot_to_note(note: &mut Note, snapshot: &NoteRevisionSnapshot) {
    note.title = snapshot.title.clone();
    note.body = snapshot.body.clone();
    for (key, value) in &snapshot.metadata {
        match key.as_str() {
            "parent_id" => {
                if let Some(v) = value.as_str() {
                    note.parent_id = v.to_string();
                }
            }
            "is_todo" => {
                if let Some(v) = value.as_i64() {
                    note.is_todo = v as i32;
                }
            }
            "todo_completed" => {
                if let Some(v) = value.as_i64() {
                    note.todo_completed = v;
                }
            }
            "todo_due" => {
                if let Some(v) = value.as_i64() {
                    note.todo_due = v;
                }
            }
            "source_url" => {
                if let Some(v) = value.as_str() {
                    note.source_url = v.to_string();
                }
            }
            "author" => {
                if let Some(v) = value.as_str() {
                    note.author = v.to_string();
                }
            }
            "source_application" => {
                if let Some(v) = value.as_str() {
                    note.source_application = v.to_string();
                }
            }
            "application_data" => {
                if let Some(v) = value.as_str() {
                    note.application_data = v.to_string();
                }
            }
            "markup_language" => {
                if let Some(v) = value.as_i64() {
                    note.markup_language = v as i32;
                }
            }
            "user_created_time" => {
                if let Some(v) = value.as_i64() {
                    note.user_created_time = v;
                }
            }
            "user_updated_time" => {
                if let Some(v) = value.as_i64() {
                    note.user_updated_time = v;
                }
            }
            "latitude" => {
                if let Some(v) = value.as_i64() {
                    note.latitude = v;
                }
            }
            "longitude" => {
                if let Some(v) = value.as_i64() {
                    note.longitude = v;
                }
            }
            "altitude" => {
                if let Some(v) = value.as_i64() {
                    note.altitude = v;
                }
            }
            "order" => {
                if let Some(v) = value.as_i64() {
                    note.order = v;
                }
            }
            _ => {}
        }
    }
}

fn revision_text_patch(text: &str) -> Result<String, DatabaseError> {
    if text.is_empty() {
        return Ok("[]".to_string());
    }

    let patch = serde_json::json!([{
        "diffs": [[1, text]],
        "start1": 0,
        "start2": 0,
        "length1": 0,
        "length2": text.chars().count(),
    }]);
    serde_json::to_string(&patch)
        .map_err(|e| DatabaseError::InvalidData(format!("Failed to serialize patch: {}", e)))
}

fn note_metadata_diff(note: &Note) -> Result<String, DatabaseError> {
    let patch = serde_json::json!({
        "new": {
            "parent_id": note.parent_id,
            "is_todo": note.is_todo,
            "todo_completed": note.todo_completed,
            "todo_due": note.todo_due,
            "source_url": note.source_url,
            "author": note.author,
            "source_application": note.source_application,
            "application_data": note.application_data,
            "markup_language": note.markup_language,
            "user_created_time": note.user_created_time,
            "user_updated_time": note.user_updated_time,
            "latitude": note.latitude,
            "longitude": note.longitude,
            "altitude": note.altitude,
            "order": note.order
        },
        "deleted": []
    });
    serde_json::to_string(&patch)
        .map_err(|e| DatabaseError::InvalidData(format!("Failed to serialize metadata: {}", e)))
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
                conflict_original_id, deleted_time
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        .bind(note.deleted_time)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to create note: {}", e)))?;

        // Update full-text search
        self.update_note_fts(note).await?;
        self.insert_note_revision(note, note.updated_time).await?;

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
                conflict_original_id, deleted_time
            FROM notes WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get note: {}", e)))?;

        Ok(row)
    }

    async fn update_note(&self, note: &Note) -> Result<(), DatabaseError> {
        let updated_time = now_ms();
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
                conflict_original_id = ?,
                deleted_time = ?
            WHERE id = ?
            "#,
        )
        .bind(&note.title)
        .bind(&note.body)
        .bind(updated_time)
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
        .bind(note.deleted_time)
        .bind(&note.id)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to update note: {}", e)))?;

        // Update full-text search
        self.update_note_fts(note).await?;
        let mut revision_note = note.clone();
        revision_note.updated_time = updated_time;
        self.insert_note_revision(&revision_note, updated_time)
            .await?;

        Ok(())
    }

    async fn delete_note(&self, id: &str) -> Result<(), DatabaseError> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to start note delete transaction: {}", e))
        })?;

        sqlx::query("DELETE FROM notes WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to delete note: {}", e)))?;

        sqlx::query(
            "INSERT INTO deleted_items (item_type, item_id, deleted_time, sync_target) VALUES (?, ?, ?, ?)",
        )
        .bind(ModelType::Note as i32)
        .bind(id)
        .bind(now_ms())
        .bind(SyncTarget::WebDAV as i32)
        .execute(&mut *tx)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to track deleted note: {}", e)))?;

        tx.commit().await.map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to commit note deletion: {}", e))
        })?;

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
                    conflict_original_id, deleted_time
                FROM notes
                WHERE parent_id = ? AND COALESCE(deleted_time, 0) = 0
                ORDER BY "order" ASC, title ASC
                "#,
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
                    conflict_original_id, deleted_time
                FROM notes
                WHERE COALESCE(deleted_time, 0) = 0
                ORDER BY "order" ASC, title ASC
                "#,
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
                icon, share_id, master_key_id, is_shared,
                encryption_cipher_text, encryption_applied, encryption_blob_encrypted
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
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
        .bind(&folder.encryption_cipher_text)
        .bind(folder.encryption_applied)
        .bind(folder.encryption_blob_encrypted)
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
                icon, share_id, master_key_id, is_shared,
                encryption_cipher_text, encryption_applied, encryption_blob_encrypted
            FROM folders WHERE id = ?
            "#,
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
                icon = ?, share_id = ?, master_key_id = ?, is_shared = ?,
                encryption_cipher_text = ?, encryption_applied = ?, encryption_blob_encrypted = ?
            WHERE id = ?
            "#,
        )
        .bind(&folder.title)
        .bind(folder.updated_time)
        .bind(folder.user_updated_time)
        .bind(&folder.parent_id)
        .bind(&folder.icon)
        .bind(&folder.share_id)
        .bind(&folder.master_key_id)
        .bind(folder.is_shared)
        .bind(&folder.encryption_cipher_text)
        .bind(folder.encryption_applied)
        .bind(folder.encryption_blob_encrypted)
        .bind(&folder.id)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to update folder: {}", e)))?;

        Ok(())
    }

    async fn delete_folder(&self, id: &str) -> Result<(), DatabaseError> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to start folder delete transaction: {}", e))
        })?;

        sqlx::query("DELETE FROM folders WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to delete folder: {}", e)))?;

        sqlx::query(
            "INSERT INTO deleted_items (item_type, item_id, deleted_time, sync_target) VALUES (?, ?, ?, ?)",
        )
        .bind(ModelType::Folder as i32)
        .bind(id)
        .bind(now_ms())
        .bind(SyncTarget::WebDAV as i32)
        .execute(&mut *tx)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to track deleted folder: {}", e)))?;

        tx.commit().await.map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to commit folder deletion: {}", e))
        })?;

        Ok(())
    }

    async fn list_folders(&self) -> Result<Vec<Folder>, DatabaseError> {
        let folders = sqlx::query_as::<_, Folder>(
            r#"
            SELECT
                id, title, created_time, updated_time,
                user_created_time, user_updated_time, parent_id,
                icon, share_id, master_key_id, is_shared,
                encryption_cipher_text, encryption_applied, encryption_blob_encrypted
            FROM folders
            ORDER BY title ASC
            "#,
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
                user_created_time, user_updated_time, parent_id, is_shared,
                encryption_cipher_text, encryption_applied, encryption_blob_encrypted, master_key_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&tag.id)
        .bind(&tag.title)
        .bind(tag.created_time)
        .bind(tag.updated_time)
        .bind(tag.user_created_time)
        .bind(tag.user_updated_time)
        .bind(&tag.parent_id)
        .bind(tag.is_shared)
        .bind(&tag.encryption_cipher_text)
        .bind(tag.encryption_applied)
        .bind(tag.encryption_blob_encrypted)
        .bind(&tag.master_key_id)
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
                user_created_time, user_updated_time, parent_id, is_shared,
                encryption_cipher_text, encryption_applied, encryption_blob_encrypted, master_key_id
            FROM tags WHERE id = ?
            "#,
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
                user_updated_time = ?, parent_id = ?,
                encryption_cipher_text = ?, encryption_applied = ?, encryption_blob_encrypted = ?,
                master_key_id = ?
            WHERE id = ?
            "#,
        )
        .bind(&tag.title)
        .bind(tag.updated_time)
        .bind(tag.user_updated_time)
        .bind(&tag.parent_id)
        .bind(&tag.encryption_cipher_text)
        .bind(tag.encryption_applied)
        .bind(tag.encryption_blob_encrypted)
        .bind(&tag.master_key_id)
        .bind(&tag.id)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to update tag: {}", e)))?;

        Ok(())
    }

    async fn delete_tag(&self, id: &str) -> Result<(), DatabaseError> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to start tag delete transaction: {}", e))
        })?;

        sqlx::query("DELETE FROM tags WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to delete tag: {}", e)))?;

        sqlx::query("DELETE FROM note_tags WHERE tag_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                DatabaseError::QueryFailed(format!("Failed to delete note tag links: {}", e))
            })?;

        sqlx::query(
            "INSERT INTO deleted_items (item_type, item_id, deleted_time, sync_target) VALUES (?, ?, ?, ?)",
        )
        .bind(ModelType::Tag as i32)
        .bind(id)
        .bind(now_ms())
        .bind(SyncTarget::WebDAV as i32)
        .execute(&mut *tx)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to track deleted tag: {}", e)))?;

        tx.commit().await.map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to commit tag deletion: {}", e))
        })?;

        Ok(())
    }

    async fn list_tags(&self) -> Result<Vec<Tag>, DatabaseError> {
        let tags = sqlx::query_as::<_, Tag>(
            r#"
            SELECT
                id, title, created_time, updated_time,
                user_created_time, user_updated_time, parent_id, is_shared
            FROM tags
            ORDER BY title ASC
            "#,
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
                id, note_id, tag_id, created_time, updated_time, is_shared,
                encryption_cipher_text, encryption_applied, encryption_blob_encrypted, master_key_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&note_tag.id)
        .bind(&note_tag.note_id)
        .bind(&note_tag.tag_id)
        .bind(note_tag.created_time)
        .bind(note_tag.updated_time)
        .bind(note_tag.is_shared)
        .bind(&note_tag.encryption_cipher_text)
        .bind(note_tag.encryption_applied)
        .bind(note_tag.encryption_blob_encrypted)
        .bind(&note_tag.master_key_id)
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
            "#,
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get note tags: {}", e)))?;

        Ok(tags)
    }

    // Settings
    async fn get_setting(&self, key: &str) -> Result<Option<String>, DatabaseError> {
        let value: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
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
            "#,
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
            "#,
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
            "#,
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
            .map_err(|e| {
                DatabaseError::QueryFailed(format!("Failed to delete sync item: {}", e))
            })?;

        Ok(())
    }

    async fn clear_all_sync_items(&self) -> Result<usize, DatabaseError> {
        let result = sqlx::query("DELETE FROM sync_items")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DatabaseError::QueryFailed(format!("Failed to clear sync items: {}", e))
            })?;

        Ok(result.rows_affected() as usize)
    }

    // Deleted items
    async fn get_deleted_items(&self, sync_target: i32) -> Result<Vec<DeletedItem>, DatabaseError> {
        let items = sqlx::query_as::<_, DeletedItem>(
            r#"
            SELECT
                id, item_type, item_id, deleted_time, sync_target
            FROM deleted_items
            WHERE sync_target = ?
            "#,
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
            "#,
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
            .map_err(|e| {
                DatabaseError::QueryFailed(format!("Failed to remove deleted item: {}", e))
            })?;

        Ok(())
    }

    async fn clear_deleted_items(&self, limit: i64) -> Result<usize, DatabaseError> {
        let result = sqlx::query(
            "DELETE FROM deleted_items WHERE id IN (SELECT id FROM deleted_items LIMIT ?)",
        )
        .bind(limit)
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to clear deleted items: {}", e)))?;

        Ok(result.rows_affected() as usize)
    }

    // Sync helper methods
    async fn get_folders_updated_since(
        &self,
        timestamp: i64,
    ) -> Result<Vec<Folder>, DatabaseError> {
        let folders = sqlx::query_as::<_, Folder>(
            "SELECT * FROM folders WHERE updated_time > ? ORDER BY updated_time ASC",
        )
        .bind(timestamp)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            DatabaseError::QueryFailed(format!(
                "Failed to get folders updated since {}: {}",
                timestamp, e
            ))
        })?;

        Ok(folders)
    }

    async fn get_tags_updated_since(&self, timestamp: i64) -> Result<Vec<Tag>, DatabaseError> {
        let tags = sqlx::query_as::<_, Tag>(
            "SELECT * FROM tags WHERE updated_time > ? ORDER BY updated_time ASC",
        )
        .bind(timestamp)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            DatabaseError::QueryFailed(format!(
                "Failed to get tags updated since {}: {}",
                timestamp, e
            ))
        })?;

        Ok(tags)
    }

    async fn get_notes_updated_since(&self, timestamp: i64) -> Result<Vec<Note>, DatabaseError> {
        let notes = sqlx::query_as::<_, Note>(
            "SELECT * FROM notes WHERE updated_time > ? AND COALESCE(deleted_time, 0) = 0 ORDER BY updated_time ASC",
        )
        .bind(timestamp)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            DatabaseError::QueryFailed(format!(
                "Failed to get notes updated since {}: {}",
                timestamp, e
            ))
        })?;

        Ok(notes)
    }

    async fn get_note_tags_updated_since(
        &self,
        timestamp: i64,
    ) -> Result<Vec<NoteTag>, DatabaseError> {
        let note_tags = sqlx::query_as::<_, NoteTag>(
            "SELECT * FROM note_tags WHERE updated_time > ? ORDER BY updated_time ASC",
        )
        .bind(timestamp)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            DatabaseError::QueryFailed(format!(
                "Failed to get note_tags updated since {}: {}",
                timestamp, e
            ))
        })?;

        Ok(note_tags)
    }

    async fn get_all_sync_items(&self) -> Result<Vec<SyncItem>, DatabaseError> {
        let items =
            sqlx::query_as::<_, SyncItem>("SELECT * FROM sync_items ORDER BY sync_time DESC")
                .fetch_all(&self.pool)
                .await
                .map_err(|e| {
                    DatabaseError::QueryFailed(format!("Failed to get all sync items: {}", e))
                })?;

        Ok(items)
    }

    async fn update_sync_time(
        &self,
        table: &str,
        id: &str,
        timestamp: i64,
    ) -> Result<(), DatabaseError> {
        // Convert table name to model type
        let item_type = match table {
            "notes" => 1,     // Note
            "folders" => 2,   // Folder
            "tags" => 3,      // Tag
            "note_tags" => 4, // NoteTag
            "resources" => 5, // Resource
            _ => {
                return Err(DatabaseError::InvalidData(format!(
                    "Unknown table: {}",
                    table
                )))
            }
        };

        self.update_sync_time_for_item_type(item_type, id, timestamp)
            .await
    }

    async fn update_sync_time_for_item_type(
        &self,
        item_type: i32,
        id: &str,
        timestamp: i64,
    ) -> Result<(), DatabaseError> {
        // First, check if the item exists in sync_items
        let existing = sqlx::query_as::<_, SyncItem>(
            "SELECT * FROM sync_items WHERE item_id = ? AND item_type = ?",
        )
        .bind(id)
        .bind(item_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to check sync item: {}", e)))?;

        if let Some(existing_item) = existing {
            // Update existing
            sqlx::query("UPDATE sync_items SET sync_time = ? WHERE id = ?")
                .bind(timestamp)
                .bind(existing_item.id)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    DatabaseError::QueryFailed(format!("Failed to update sync time: {}", e))
                })?;
        } else {
            // Insert new
            sqlx::query(
                r#"
                INSERT INTO sync_items (
                    sync_target, sync_time, item_type, item_id, sync_disabled, sync_disabled_reason, item_location
                ) VALUES (?, ?, ?, ?, ?, ?, ?)
                "#
            )
            .bind(1) // sync_target
            .bind(timestamp)
            .bind(item_type)
            .bind(id)
            .bind(0) // sync_disabled
            .bind("") // sync_disabled_reason
            .bind(1) // item_location
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to insert sync item: {}", e)))?;
        }

        Ok(())
    }

    async fn purge_sync_item(&self, item_type: i32, item_id: &str) -> Result<(), DatabaseError> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to start purge transaction: {}", e))
        })?;

        match item_type {
            1 => {
                sqlx::query("DELETE FROM notes WHERE id = ?")
                    .bind(item_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| {
                        DatabaseError::QueryFailed(format!("Failed to purge note: {}", e))
                    })?;
                sqlx::query("DELETE FROM notes_fts WHERE id = ?")
                    .bind(item_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| {
                        DatabaseError::QueryFailed(format!("Failed to purge note FTS row: {}", e))
                    })?;
            }
            2 => {
                sqlx::query("DELETE FROM folders WHERE id = ?")
                    .bind(item_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| {
                        DatabaseError::QueryFailed(format!("Failed to purge folder: {}", e))
                    })?;
            }
            3 => {
                sqlx::query("DELETE FROM tags WHERE id = ?")
                    .bind(item_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| {
                        DatabaseError::QueryFailed(format!("Failed to purge tag: {}", e))
                    })?;
            }
            4 => {
                sqlx::query("DELETE FROM note_tags WHERE id = ?")
                    .bind(item_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| {
                        DatabaseError::QueryFailed(format!("Failed to purge note_tag: {}", e))
                    })?;
            }
            5 => {
                sqlx::query("DELETE FROM resources WHERE id = ?")
                    .bind(item_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| {
                        DatabaseError::QueryFailed(format!("Failed to purge resource: {}", e))
                    })?;
            }
            13 => {
                // Type 13 is for item_changes (Joplin sync metadata)
                // neojoplin doesn't have an item_changes table, so just skip content deletion
                // The sync_items record will be deleted below
            }
            _ => {
                return Err(DatabaseError::InvalidData(format!(
                    "Unknown sync item type for purge: {}",
                    item_type
                )));
            }
        }

        sqlx::query("DELETE FROM sync_items WHERE item_id = ? AND item_type = ?")
            .bind(item_id)
            .bind(item_type)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                DatabaseError::QueryFailed(format!("Failed to purge sync item record: {}", e))
            })?;

        tx.commit().await.map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to commit purge transaction: {}", e))
        })?;

        Ok(())
    }

    // Database info
    async fn get_version(&self) -> Result<i32, DatabaseError> {
        let version: Option<i32> = sqlx::query_scalar("SELECT version FROM version LIMIT 1")
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get version: {}", e)))?;

        Ok(version.unwrap_or(0))
    }

    async fn begin_transaction(&self) -> Result<(), DatabaseError> {
        sqlx::query("BEGIN TRANSACTION")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DatabaseError::QueryFailed(format!("Failed to begin transaction: {}", e))
            })?;
        Ok(())
    }

    async fn commit_transaction(&self) -> Result<(), DatabaseError> {
        sqlx::query("COMMIT")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DatabaseError::QueryFailed(format!("Failed to commit transaction: {}", e))
            })?;
        Ok(())
    }

    async fn rollback_transaction(&self) -> Result<(), DatabaseError> {
        sqlx::query("ROLLBACK")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DatabaseError::QueryFailed(format!("Failed to rollback transaction: {}", e))
            })?;
        Ok(())
    }

    async fn trash_note(&self, id: &str) -> Result<(), DatabaseError> {
        sqlx::query("UPDATE notes SET deleted_time = ? WHERE id = ?")
            .bind(now_ms())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to trash note: {}", e)))?;
        Ok(())
    }

    async fn restore_note(&self, id: &str) -> Result<(), DatabaseError> {
        sqlx::query("UPDATE notes SET deleted_time = 0 WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to restore note: {}", e)))?;
        Ok(())
    }

    async fn list_deleted_notes(&self) -> Result<Vec<Note>, DatabaseError> {
        let notes = sqlx::query_as::<_, Note>(
            r#"
            SELECT
                id, title, body, created_time, updated_time,
                user_created_time, user_updated_time, parent_id,
                is_conflict, is_todo, todo_completed, todo_due,
                source, source_application, "order", latitude, longitude,
                altitude, author, source_url, is_shared, application_data,
                markup_language, encryption_cipher_text, encryption_applied,
                encryption_blob_encrypted, master_key_id, share_id,
                conflict_original_id, deleted_time
            FROM notes
            WHERE COALESCE(deleted_time, 0) > 0
            ORDER BY deleted_time DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to list deleted notes: {}", e)))?;
        Ok(notes)
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
        assert_eq!(version, 42);
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

    #[tokio::test]
    async fn test_delete_note_tracks_webdav_deletion() {
        let db = setup_test_db().await;

        let note = Note {
            title: "Tracked".to_string(),
            ..Default::default()
        };
        db.create_note(&note).await.unwrap();

        db.delete_note(&note.id).await.unwrap();

        let deleted = db
            .get_deleted_items(SyncTarget::WebDAV as i32)
            .await
            .unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].item_id, note.id);
        assert_eq!(deleted[0].item_type, ModelType::Note as i32);
    }

    #[tokio::test]
    async fn test_delete_folder_tracks_webdav_deletion() {
        let db = setup_test_db().await;

        let folder = Folder {
            title: "Tracked Folder".to_string(),
            ..Default::default()
        };
        db.create_folder(&folder).await.unwrap();

        db.delete_folder(&folder.id).await.unwrap();

        let deleted = db
            .get_deleted_items(SyncTarget::WebDAV as i32)
            .await
            .unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].item_id, folder.id);
        assert_eq!(deleted[0].item_type, ModelType::Folder as i32);
    }
}
