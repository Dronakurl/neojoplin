use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use joplin_domain::{Folder, Note, NoteTag, Storage, Tag};
use neojoplin_storage::SqliteStorage;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Row;

#[derive(Debug, Clone, Default)]
pub struct ImportSummary {
    pub folders: usize,
    pub notes: usize,
    pub tags: usize,
    pub note_tags: usize,
}

impl ImportSummary {
    pub fn describe(&self) -> String {
        format!(
            "Imported {} notebooks, {} notes, {} tags, {} tag links",
            self.folders, self.notes, self.tags, self.note_tags
        )
    }
}

pub fn default_cli_database_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("joplin")
        .join("database.sqlite")
}

pub fn default_desktop_database_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("joplin-desktop")
        .join("database.sqlite")
}

pub fn resolve_import_path(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    } else if raw == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }

    PathBuf::from(raw)
}

pub async fn import_database(storage: &SqliteStorage, source_path: &Path) -> Result<ImportSummary> {
    let source_path = source_path
        .canonicalize()
        .with_context(|| format!("Database not found: {}", source_path.display()))?;
    let destination_path = neojoplin_core::Config::database_path()?.canonicalize().ok();

    if destination_path.as_deref() == Some(source_path.as_path()) {
        anyhow::bail!("Refusing to import the currently open NeoJoplin database into itself");
    }

    let options = SqliteConnectOptions::new()
        .filename(&source_path)
        .read_only(true);
    let pool = sqlx::SqlitePool::connect_with(options)
        .await
        .with_context(|| format!("Failed to open {}", source_path.display()))?;

    let folders = load_folders(&pool).await?;
    let notes = load_notes(&pool).await?;
    let tags = load_tags(&pool).await?;
    let note_tags = load_note_tags(&pool).await?;

    let mut summary = ImportSummary::default();

    for folder in folders {
        if storage.get_folder(&folder.id).await?.is_some() {
            storage.update_folder(&folder).await?;
        } else {
            storage.create_folder(&folder).await?;
        }
        summary.folders += 1;
    }

    for note in notes {
        if storage.get_note(&note.id).await?.is_some() {
            storage.update_note(&note).await?;
        } else {
            storage.create_note(&note).await?;
        }
        summary.notes += 1;
    }

    for tag in tags {
        if storage.get_tag(&tag.id).await?.is_some() {
            storage.update_tag(&tag).await?;
        } else {
            storage.create_tag(&tag).await?;
        }
        summary.tags += 1;
    }

    for note_tag in note_tags {
        if storage.get_note(&note_tag.note_id).await?.is_none()
            || storage.get_tag(&note_tag.tag_id).await?.is_none()
        {
            continue;
        }

        let existing_tags = storage.get_note_tags(&note_tag.note_id).await?;
        if existing_tags.iter().any(|tag| tag.id == note_tag.tag_id) {
            continue;
        }

        storage.add_note_tag(&note_tag).await?;
        summary.note_tags += 1;
    }

    Ok(summary)
}

async fn load_folders(pool: &sqlx::SqlitePool) -> Result<Vec<Folder>> {
    sqlx::query_as::<_, Folder>(
        r#"
        SELECT
            id, title, created_time, updated_time,
            user_created_time, user_updated_time, parent_id,
            icon, share_id, master_key_id, is_shared
        FROM folders
        ORDER BY title ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .context("Failed to load folders from source database")
}

async fn load_notes(pool: &sqlx::SqlitePool) -> Result<Vec<Note>> {
    let has_encryption_blob_encrypted =
        table_has_column(pool, "notes", "encryption_blob_encrypted").await?;
    let has_deleted_time = table_has_column(pool, "notes", "deleted_time").await?;

    let query = format!(
        r#"
        SELECT
            id, title, body, created_time, updated_time,
            user_created_time, user_updated_time, parent_id,
            is_conflict, is_todo, todo_completed, todo_due,
            source, source_application, CAST("order" AS INTEGER) AS "order",
            CAST(latitude AS INTEGER) AS latitude,
            CAST(longitude AS INTEGER) AS longitude,
            CAST(altitude AS INTEGER) AS altitude,
            author, source_url, is_shared, application_data,
            markup_language, encryption_cipher_text, encryption_applied,
            {}, master_key_id, share_id,
            conflict_original_id, {}
        FROM notes
        ORDER BY updated_time ASC, title ASC
        "#,
        if has_encryption_blob_encrypted {
            "encryption_blob_encrypted"
        } else {
            "0 AS encryption_blob_encrypted"
        },
        if has_deleted_time {
            "deleted_time"
        } else {
            "0 AS deleted_time"
        }
    );

    sqlx::query_as::<_, Note>(&query)
        .fetch_all(pool)
        .await
        .context("Failed to load notes from source database")
}

async fn table_has_column(pool: &sqlx::SqlitePool, table: &str, column: &str) -> Result<bool> {
    let pragma = format!("PRAGMA table_info({table})");
    let rows = sqlx::query(&pragma)
        .fetch_all(pool)
        .await
        .with_context(|| format!("Failed to inspect schema for table {}", table))?;

    Ok(rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("name").ok())
        .any(|name| name == column))
}

async fn load_tags(pool: &sqlx::SqlitePool) -> Result<Vec<Tag>> {
    sqlx::query_as::<_, Tag>(
        r#"
        SELECT
            id, title, created_time, updated_time,
            user_created_time, user_updated_time, parent_id,
            is_shared
        FROM tags
        ORDER BY title ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .context("Failed to load tags from source database")
}

async fn load_note_tags(pool: &sqlx::SqlitePool) -> Result<Vec<NoteTag>> {
    sqlx::query_as::<_, NoteTag>(
        r#"
        SELECT
            id, note_id, tag_id, created_time, updated_time, is_shared
        FROM note_tags
        ORDER BY updated_time ASC, id ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .context("Failed to load note-tag links from source database")
}
