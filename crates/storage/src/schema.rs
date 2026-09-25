// Joplin-compatible schema creation and migrations.
//
// NeoJoplin uses exactly the same schema and version numbers as upstream Joplin
// (packages/lib/JoplinDatabase.ts + services/database/migrations/*.ts), so that
// Joplin and NeoJoplin can open the same database, even at the same time.

use joplin_domain::{now_ms, DatabaseError};
use sqlx::{Row, SqliteConnection, SqlitePool};

/// Schema version NeoJoplin creates new databases with (latest Joplin release: 3.7).
pub const JOPLIN_DB_VERSION: i32 = 53;

/// Oldest Joplin schema NeoJoplin can work with (v46 added `deleted_time`).
/// Older Joplin databases are upgraded to this version only, never further, so
/// that the Joplin version which owns the database can still open it.
pub const MIN_DB_VERSION: i32 = 46;

/// Oldest version whose migrations NeoJoplin knows (Joplin's inline migrations
/// up to v41 are not ported).
const FIRST_PORTED_MIGRATION: i32 = 42;

const SCHEMA_SQL: &str = include_str!("joplin_schema_v53.sql");

/// Tables of the Joplin schema. Used when converting databases created by
/// older NeoJoplin versions, which had their own, incompatible schema.
const JOPLIN_TABLES: &[&str] = &[
    "folders",
    "tags",
    "note_tags",
    "table_fields",
    "sync_items",
    "version",
    "deleted_items",
    "settings",
    "alarms",
    "master_keys",
    "item_changes",
    "note_resources",
    "resource_local_states",
    "resources",
    "revisions",
    "migrations",
    "resources_to_download",
    "key_values",
    "notes",
    "notes_normalized",
    "items_normalized",
    "note_embeddings_meta",
    "conflict_note_states",
];

/// Queries of Joplin migration `version` (services/database/migrations/<version>.ts).
fn migration_queries(version: i32) -> Result<Vec<String>, DatabaseError> {
    let queries: Vec<String> = match version {
        42 => {
            let now = now_ms();
            vec![format!(
                "INSERT INTO migrations (number, created_time, updated_time) VALUES (42, {now}, {now})"
            )]
        }
        43 => vec![
            "ALTER TABLE `notes` ADD COLUMN `user_data` TEXT NOT NULL DEFAULT \"\"".into(),
            "ALTER TABLE `tags` ADD COLUMN `user_data` TEXT NOT NULL DEFAULT \"\"".into(),
            "ALTER TABLE `folders` ADD COLUMN `user_data` TEXT NOT NULL DEFAULT \"\"".into(),
            "ALTER TABLE `resources` ADD COLUMN `user_data` TEXT NOT NULL DEFAULT \"\"".into(),
        ],
        44 => vec![
            "ALTER TABLE `resources` ADD COLUMN blob_updated_time INT NOT NULL DEFAULT 0".into(),
            "UPDATE `resources` SET blob_updated_time = updated_time".into(),
        ],
        45 => {
            let fields = "id, title, body, item_id, item_type, user_updated_time, reserved1, reserved2, reserved3, reserved4, reserved5, reserved6";
            vec![
                "ALTER TABLE `resources` ADD COLUMN `ocr_text` TEXT NOT NULL DEFAULT \"\"".into(),
                "ALTER TABLE `resources` ADD COLUMN `ocr_details` TEXT NOT NULL DEFAULT \"\"".into(),
                "ALTER TABLE `resources` ADD COLUMN `ocr_status` INT NOT NULL DEFAULT 0".into(),
                "ALTER TABLE `resources` ADD COLUMN `ocr_error` TEXT NOT NULL DEFAULT \"\"".into(),
                "CREATE TABLE items_normalized (id INTEGER PRIMARY KEY AUTOINCREMENT,title TEXT NOT NULL DEFAULT \"\", body TEXT NOT NULL DEFAULT \"\", item_id TEXT NOT NULL, item_type INT NOT NULL, user_updated_time INT NOT NULL DEFAULT 0, reserved1 INT NULL, reserved2 INT NULL, reserved3 INT NULL, reserved4 INT NULL, reserved5 INT NULL, reserved6 INT NULL)".into(),
                "CREATE INDEX items_normalized_id ON items_normalized (id)".into(),
                "CREATE INDEX items_normalized_item_id ON items_normalized (item_id)".into(),
                "CREATE INDEX items_normalized_item_type ON items_normalized (item_type)".into(),
                format!("CREATE VIRTUAL TABLE items_fts USING fts4(content=\"items_normalized\", notindexed=\"id\", notindexed=\"item_id\", notindexed=\"item_type\", notindexed=\"user_updated_time\", notindexed=\"reserved1\", notindexed=\"reserved2\", notindexed=\"reserved3\", notindexed=\"reserved4\", notindexed=\"reserved5\", notindexed=\"reserved6\", {fields})"),
                "CREATE TRIGGER items_fts_before_update BEFORE UPDATE ON items_normalized BEGIN DELETE FROM items_fts WHERE docid=old.rowid; END".into(),
                "CREATE TRIGGER items_fts_before_delete BEFORE DELETE ON items_normalized BEGIN DELETE FROM items_fts WHERE docid=old.rowid; END".into(),
                format!("CREATE TRIGGER items_after_update AFTER UPDATE ON items_normalized BEGIN INSERT INTO items_fts(docid, {fields}) SELECT rowid, {fields} FROM items_normalized WHERE new.rowid = items_normalized.rowid; END"),
                format!("CREATE TRIGGER items_after_insert AFTER INSERT ON items_normalized BEGIN INSERT INTO items_fts(docid, {fields}) SELECT rowid, {fields} FROM items_normalized WHERE new.rowid = items_normalized.rowid; END"),
            ]
        }
        46 => vec![
            "ALTER TABLE `notes` ADD COLUMN `deleted_time` INT NOT NULL DEFAULT 0".into(),
            "ALTER TABLE `folders` ADD COLUMN `deleted_time` INT NOT NULL DEFAULT 0".into(),
            "DROP VIEW tags_with_note_count".into(),
            "CREATE VIEW tags_with_note_count AS SELECT tags.id as id, tags.title as title, tags.created_time as created_time, tags.updated_time as updated_time, COUNT(notes.id) as note_count, SUM(CASE WHEN notes.todo_completed > 0 THEN 1 ELSE 0 END) AS todo_completed_count FROM tags LEFT JOIN note_tags nt on nt.tag_id = tags.id LEFT JOIN notes on notes.id = nt.note_id WHERE notes.id IS NOT NULL AND notes.deleted_time = 0 GROUP BY tags.id".into(),
        ],
        47 => vec![
            "ALTER TABLE sync_items ADD COLUMN sync_warning_ignored INT NOT NULL DEFAULT \"0\"".into(),
        ],
        48 => vec![
            "ALTER TABLE `resources` ADD COLUMN `ocr_driver_id` INT NOT NULL DEFAULT \"1\"".into(),
        ],
        49 => vec![
            "ALTER TABLE sync_items ADD COLUMN remote_item_updated_time INT NOT NULL DEFAULT 0".into(),
        ],
        50 => vec![
            "ALTER TABLE notes ADD COLUMN is_locked INT NOT NULL DEFAULT 0".into(),
            "ALTER TABLE notes ADD COLUMN extracted_resource_ids TEXT NOT NULL DEFAULT \"\"".into(),
            "ALTER TABLE revisions ADD COLUMN is_locked INT NOT NULL DEFAULT 0".into(),
            "ALTER TABLE resources ADD COLUMN is_locked INT NOT NULL DEFAULT 0".into(),
        ],
        51 => vec![
            "ALTER TABLE sync_items ADD COLUMN base_body TEXT NOT NULL DEFAULT \"\"".into(),
            "ALTER TABLE sync_items ADD COLUMN base_title TEXT NOT NULL DEFAULT \"\"".into(),
            "ALTER TABLE sync_items ADD COLUMN base_conflict_note_id TEXT NOT NULL DEFAULT \"\"".into(),
            "CREATE TABLE IF NOT EXISTS conflict_note_states (note_id TEXT PRIMARY KEY, base_body TEXT NOT NULL DEFAULT \"\", base_title TEXT NOT NULL DEFAULT \"\", remote_body TEXT NOT NULL DEFAULT \"\", remote_title TEXT NOT NULL DEFAULT \"\", remote_updated_time INT NOT NULL DEFAULT 0)".into(),
        ],
        52 => vec![
            "CREATE TABLE IF NOT EXISTS note_embeddings_meta (id INTEGER PRIMARY KEY AUTOINCREMENT, note_id TEXT NOT NULL, chunk_index INTEGER NOT NULL, model_id TEXT NOT NULL, chunk_text TEXT NOT NULL, created_time INT NOT NULL DEFAULT 0)".into(),
            "CREATE INDEX IF NOT EXISTS note_embeddings_meta_note_id ON note_embeddings_meta(note_id)".into(),
            "CREATE UNIQUE INDEX IF NOT EXISTS note_embeddings_meta_note_chunk ON note_embeddings_meta(note_id, chunk_index)".into(),
        ],
        53 => vec![
            "DROP TABLE IF EXISTS conflict_note_states".into(),
            "CREATE TABLE conflict_note_states (id INTEGER PRIMARY KEY AUTOINCREMENT, note_id TEXT NOT NULL UNIQUE, base_body TEXT NOT NULL DEFAULT \"\", base_title TEXT NOT NULL DEFAULT \"\", remote_body TEXT NOT NULL DEFAULT \"\", remote_title TEXT NOT NULL DEFAULT \"\", remote_updated_time INT NOT NULL DEFAULT 0)".into(),
        ],
        _ => {
            return Err(DatabaseError::MigrationFailed(format!(
                "No such Joplin migration: {}",
                version
            )))
        }
    };
    Ok(queries)
}

fn migration_error(context: &str, e: impl std::fmt::Display) -> DatabaseError {
    DatabaseError::MigrationFailed(format!("{}: {}", context, e))
}

async fn table_exists(conn: &mut SqliteConnection, name: &str) -> Result<bool, DatabaseError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(name)
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| migration_error("Failed to inspect schema", e))?;
    Ok(count > 0)
}

/// Bring the database to a Joplin-compatible state.
pub async fn initialize(pool: &SqlitePool) -> Result<(), DatabaseError> {
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to acquire: {}", e)))?;

    if !table_exists(&mut conn, "version").await? {
        return create_schema(&mut conn).await;
    }

    let version: i32 = sqlx::query_scalar("SELECT version FROM version LIMIT 1")
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| migration_error("Failed to read database version", e))?
        .unwrap_or(0);

    // Joplin has had `item_changes` since v10; databases created by older
    // NeoJoplin versions never had it and used their own version numbers.
    if !table_exists(&mut conn, "item_changes").await? {
        return convert_legacy_neojoplin(&mut conn, version).await;
    }

    if version > JOPLIN_DB_VERSION {
        tracing::warn!(
            "Database version {} is newer than {} known by NeoJoplin; using it as is",
            version,
            JOPLIN_DB_VERSION
        );
        return Ok(());
    }
    if version >= MIN_DB_VERSION {
        return Ok(());
    }
    if version < FIRST_PORTED_MIGRATION - 1 {
        return Err(DatabaseError::MigrationFailed(format!(
            "Joplin database version {} is too old. Open it once with a recent Joplin to upgrade it.",
            version
        )));
    }
    migrate(&mut conn, version, MIN_DB_VERSION).await
}

/// Apply Joplin migrations `from + 1 ..= to`, one transaction per version like Joplin.
async fn migrate(conn: &mut SqliteConnection, from: i32, to: i32) -> Result<(), DatabaseError> {
    for target in (from + 1)..=to {
        tracing::info!("Upgrading Joplin database to v{}", target);
        let mut tx = sqlx::Connection::begin(&mut *conn)
            .await
            .map_err(|e| migration_error("Failed to begin migration", e))?;
        for sql in migration_queries(target)? {
            sqlx::query(&sql)
                .execute(&mut *tx)
                .await
                .map_err(|e| migration_error(&format!("Migration {} failed", target), e))?;
        }
        sqlx::query("UPDATE version SET version = ?")
            .bind(target)
            .execute(&mut *tx)
            .await
            .map_err(|e| migration_error("Failed to update version", e))?;
        tx.commit()
            .await
            .map_err(|e| migration_error("Failed to commit migration", e))?;
    }
    Ok(())
}

/// Create a new database with the Joplin schema.
async fn create_schema(conn: &mut SqliteConnection) -> Result<(), DatabaseError> {
    tracing::info!("Creating Joplin database schema v{}", JOPLIN_DB_VERSION);
    let mut tx = sqlx::Connection::begin(&mut *conn)
        .await
        .map_err(|e| migration_error("Failed to begin transaction", e))?;
    sqlx::raw_sql(SCHEMA_SQL)
        .execute(&mut *tx)
        .await
        .map_err(|e| migration_error("Failed to create schema", e))?;
    insert_version(&mut tx).await?;
    tx.commit()
        .await
        .map_err(|e| migration_error("Failed to commit schema", e))
}

/// table_fields_version stays 0 so that Joplin fills `table_fields` on first open,
/// as it does for its own fresh profiles.
async fn insert_version(conn: &mut SqliteConnection) -> Result<(), DatabaseError> {
    sqlx::query("INSERT INTO version (version, table_fields_version) VALUES (?, 0)")
        .bind(JOPLIN_DB_VERSION)
        .execute(&mut *conn)
        .await
        .map_err(|e| migration_error("Failed to set version", e))?;
    Ok(())
}

/// Convert a database created by NeoJoplin before it used the Joplin schema
/// (self-invented versions 41-43) into a Joplin v53 database. A backup copy is
/// written next to the database file first.
async fn convert_legacy_neojoplin(
    conn: &mut SqliteConnection,
    legacy_version: i32,
) -> Result<(), DatabaseError> {
    tracing::info!(
        "Converting NeoJoplin database (legacy v{}) to Joplin schema v{}",
        legacy_version,
        JOPLIN_DB_VERSION
    );
    backup_database(conn, legacy_version).await?;

    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await
        .map_err(|e| migration_error("Failed to disable foreign keys", e))?;
    // Keep references in other tables untouched while renaming.
    sqlx::query("PRAGMA legacy_alter_table = ON")
        .execute(&mut *conn)
        .await
        .map_err(|e| migration_error("Failed to set legacy_alter_table", e))?;

    let result = convert_legacy_tables(conn).await;

    let _ = sqlx::query("PRAGMA legacy_alter_table = OFF")
        .execute(&mut *conn)
        .await;
    let _ = sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await;
    result
}

async fn backup_database(conn: &mut SqliteConnection, version: i32) -> Result<(), DatabaseError> {
    let rows = sqlx::query("PRAGMA database_list")
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| migration_error("Failed to locate database file", e))?;
    let file: String = rows
        .iter()
        .find(|r| r.get::<String, _>("name") == "main")
        .map(|r| r.get("file"))
        .unwrap_or_default();
    if file.is_empty() {
        return Ok(());
    }
    let backup = format!("{}.neojoplin-v{}-{}.bak", file, version, now_ms());
    sqlx::query("VACUUM INTO ?")
        .bind(&backup)
        .execute(&mut *conn)
        .await
        .map_err(|e| migration_error("Failed to back up database before conversion", e))?;
    tracing::info!("Backed up database to {}", backup);
    Ok(())
}

async fn convert_legacy_tables(conn: &mut SqliteConnection) -> Result<(), DatabaseError> {
    let mut tx = sqlx::Connection::begin(&mut *conn)
        .await
        .map_err(|e| migration_error("Failed to begin conversion", e))?;

    let mut legacy_tables = Vec::new();
    for table in JOPLIN_TABLES {
        if table_exists(&mut tx, table).await? {
            let legacy = format!("_neojoplin_legacy_{}", table);
            sqlx::query(&format!(
                "ALTER TABLE \"{}\" RENAME TO \"{}\"",
                table, legacy
            ))
            .execute(&mut *tx)
            .await
            .map_err(|e| migration_error(&format!("Failed to rename {}", table), e))?;
            legacy_tables.push((*table, legacy));
        }
    }

    // Index names would clash with the Joplin schema.
    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'index' AND sql IS NOT NULL AND tbl_name GLOB '_neojoplin_legacy_*'",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| migration_error("Failed to list legacy indexes", e))?;
    for index in indexes {
        sqlx::query(&format!("DROP INDEX \"{}\"", index))
            .execute(&mut *tx)
            .await
            .map_err(|e| migration_error("Failed to drop legacy index", e))?;
    }

    // NeoJoplin's own FTS5 index; Joplin uses an FTS4 index fed from notes_normalized.
    sqlx::query("DROP TABLE IF EXISTS notes_fts")
        .execute(&mut *tx)
        .await
        .map_err(|e| migration_error("Failed to drop legacy notes_fts", e))?;

    sqlx::raw_sql(SCHEMA_SQL)
        .execute(&mut *tx)
        .await
        .map_err(|e| migration_error("Failed to create schema", e))?;

    for (table, legacy) in &legacy_tables {
        if *table != "version" && *table != "table_fields" {
            copy_table(&mut tx, legacy, table).await?;
        }
        sqlx::query(&format!("DROP TABLE \"{}\"", legacy))
            .execute(&mut *tx)
            .await
            .map_err(|e| migration_error(&format!("Failed to drop {}", legacy), e))?;
    }

    // Older NeoJoplin versions recorded WebDAV sync state under target 1 (Memory).
    sqlx::query("UPDATE sync_items SET sync_target = 6 WHERE sync_target = 1")
        .execute(&mut *tx)
        .await
        .map_err(|e| migration_error("Failed to fix sync targets", e))?;

    insert_version(&mut tx).await?;
    tx.commit()
        .await
        .map_err(|e| migration_error("Failed to commit conversion", e))
}

struct ColumnInfo {
    name: String,
    col_type: String,
    not_null: bool,
    default: Option<String>,
}

async fn columns(
    conn: &mut SqliteConnection,
    table: &str,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    let rows = sqlx::query(&format!("PRAGMA table_info(\"{}\")", table))
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| migration_error(&format!("Failed to read columns of {}", table), e))?;
    Ok(rows
        .iter()
        .map(|r| ColumnInfo {
            name: r.get("name"),
            col_type: r.get("type"),
            not_null: r.get::<i64, _>("notnull") != 0,
            default: r.get("dflt_value"),
        })
        .collect())
}

/// Copy the columns both tables have, replacing NULLs (allowed in the legacy
/// schema) with the Joplin column default.
async fn copy_table(
    conn: &mut SqliteConnection,
    from: &str,
    to: &str,
) -> Result<(), DatabaseError> {
    let old_columns = columns(conn, from).await?;
    let new_columns = columns(conn, to).await?;

    let mut names = Vec::new();
    let mut exprs = Vec::new();
    for column in &new_columns {
        if !old_columns.iter().any(|c| c.name == column.name) {
            continue;
        }
        let quoted = format!("\"{}\"", column.name);
        let expr = if column.not_null {
            let fallback = match &column.default {
                // Joplin writes string defaults as "" / "0"; make them SQL literals.
                Some(d) if d.starts_with('"') => format!("'{}'", d.trim_matches('"')),
                Some(d) => d.clone(),
                None if column.col_type.to_uppercase().contains("TEXT") => "''".to_string(),
                None => "0".to_string(),
            };
            format!("COALESCE({}, {})", quoted, fallback)
        } else {
            quoted.clone()
        };
        names.push(quoted);
        exprs.push(expr);
    }

    if names.is_empty() {
        return Ok(());
    }
    let sql = format!(
        "INSERT INTO \"{}\" ({}) SELECT {} FROM \"{}\"",
        to,
        names.join(", "),
        exprs.join(", "),
        from
    );
    sqlx::query(&sql)
        .execute(&mut *conn)
        .await
        .map_err(|e| migration_error(&format!("Failed to copy {}", to), e))?;
    Ok(())
}
