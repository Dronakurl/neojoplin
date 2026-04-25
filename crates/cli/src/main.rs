// NeoJoplin - Main entry point (CLI + TUI)

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use joplin_domain::{now_ms, Folder, Note, Storage};
use joplin_sync::{E2eeService, MasterKey};
use neojoplin_core::Editor;
use neojoplin_storage::SqliteStorage;
use neojoplin_tui::importer::{
    default_cli_database_path, default_desktop_database_path, import_database, resolve_import_path,
};
use neojoplin_tui::settings::{Settings, SyncTarget};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "neojoplin")]
#[command(about = "A fast, terminal-based Joplin client", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Launch TUI interface (default when no command specified)
    #[arg(short, long)]
    tui: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize the database
    Init,

    /// Create a new note
    #[command(name = "mknote", visible_alias = "mk-note")]
    MkNote {
        /// Note title
        title: String,
        /// Parent folder ID (optional, uses root if not specified)
        #[arg(short, long)]
        parent: Option<String>,
        /// Note body (optional, opens editor if not specified)
        #[arg(short, long)]
        body: Option<String>,
    },

    /// Edit an existing note
    Edit {
        /// Note ID or title
        note: String,
    },

    /// Create a new folder/notebook
    #[command(name = "mkbook", visible_alias = "mk-book")]
    MkBook {
        /// Folder title
        title: String,
        /// Parent folder ID (optional, uses root if not specified)
        #[arg(short, long)]
        parent: Option<String>,
    },

    /// List notes and folders
    Ls {
        /// Pattern to filter by
        pattern: Option<String>,
        /// List only folders
        #[arg(short, long)]
        folders: bool,
        /// List only notes
        #[arg(short, long)]
        notes: bool,
    },

    /// Display note content
    Cat {
        /// Note ID or title
        note: String,
    },

    /// Synchronize with WebDAV server
    Sync {
        /// WebDAV base URL (uses the configured target if omitted)
        #[arg(long)]
        url: Option<String>,
        /// WebDAV username (uses the configured target if omitted)
        #[arg(short = 'U', long)]
        username: Option<String>,
        /// WebDAV password (uses the configured target if omitted)
        #[arg(short = 'P', long)]
        password: Option<String>,
        /// Remote path (uses the configured target if omitted)
        #[arg(short = 'r', long)]
        remote: Option<String>,
        /// E2EE master password (overrides E2EE_PASSWORD env var and .env file)
        #[arg(long)]
        e2ee_password: Option<String>,
    },

    /// Create a new todo
    #[command(name = "mktodo", visible_alias = "mk-todo")]
    MkTodo {
        /// Todo title
        title: String,
        /// Parent folder ID (optional, uses root if not specified)
        #[arg(short, long)]
        parent: Option<String>,
        /// Todo body (optional)
        #[arg(short, long)]
        body: Option<String>,
        /// Due date in ISO 8601 format (e.g., 2026-04-20T12:00:00Z)
        #[arg(short, long)]
        due: Option<String>,
    },

    /// Toggle a todo's completion status
    TodoToggle {
        /// Todo ID or title
        todo: String,
    },

    /// Import notes, notebooks, and tags from the Joplin CLI database
    Import {
        /// SQLite database path (defaults to the Joplin CLI database)
        path: Option<String>,
    },

    /// Import notes, notebooks, and tags from the Joplin Desktop database
    ImportDesktop,

    /// List all folders
    ListBooks,

    /// Delete a note
    RmNote {
        /// Note ID or title
        note: String,
        /// Force deletion without confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Delete a folder
    RmBook {
        /// Folder ID or title
        folder: String,
        /// Force deletion without confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Manage end-to-end encryption
    E2ee {
        #[command(subcommand)]
        command: E2eeCommands,
    },
}

#[derive(Subcommand)]
enum E2eeCommands {
    /// Enable encryption with a master password
    Enable {
        /// Master password (not recommended - will prompt if not provided)
        #[arg(short, long)]
        password: Option<String>,
        /// Force generating a new master key instead of reusing an existing compatible one
        #[arg(long)]
        new_key: bool,
    },

    /// Disable encryption
    Disable {
        /// Force disable without confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Show encryption status
    Status,

    /// Decrypt an encrypted string
    Decrypt {
        /// Encrypted string (JED format)
        encrypted: String,
    },
}

/// Load E2EE service if encryption is enabled
async fn load_e2ee_service(password_override: Option<String>) -> Result<E2eeService> {
    use joplin_sync::MasterKey;
    use neojoplin_core::Config;

    let data_dir = Config::data_dir()?;
    let encryption_config_path = data_dir.join("encryption.json");

    let stored_password = if encryption_config_path.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&encryption_config_path).await {
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                config
                    .get("master_password")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let env_password = std::env::var("E2EE_PASSWORD").ok().or_else(|| {
        if let Ok(env_path) = std::env::current_dir() {
            let env_file = env_path.join(".env");
            if env_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&env_file) {
                    for line in content.lines() {
                        if let Some((key, value)) = line.split_once('=') {
                            if key.trim() == "E2EE_PASSWORD" {
                                return Some(value.trim().to_string());
                            }
                        }
                    }
                }
            }
        }
        None
    });

    // Priority: CLI arg > stored password > environment
    let master_password = password_override
        .clone()
        .or(stored_password)
        .or(env_password)
        .unwrap_or_default();

    // Create E2EE service
    let mut e2ee = E2eeService::new();
    if !master_password.is_empty() {
        e2ee.set_master_password(master_password.clone());
    }

    // Check if encryption is enabled
    if encryption_config_path.exists() {
        // Load encryption configuration
        let config_content = tokio::fs::read_to_string(&encryption_config_path).await?;
        let mut config: serde_json::Value = serde_json::from_str(&config_content)?;

        let enabled = config
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !enabled {
            return Ok(e2ee);
        }

        // If a CLI password override was provided, save it for subsequent syncs.
        if let Some(ref password_override) = password_override {
            let stored = config
                .get("master_password")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if stored != password_override {
                config["master_password"] = serde_json::json!(password_override);
                tokio::fs::write(
                    &encryption_config_path,
                    serde_json::to_string_pretty(&config)?,
                )
                .await?;
            }
        }

        // Get active master key ID
        let active_key_id = config
            .get("active_master_key_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                anyhow::anyhow!("Invalid encryption config: missing active_master_key_id")
            })?;

        // Load the active master key from disk
        let keys_dir = data_dir.join("keys");
        let key_file_path = keys_dir.join(format!("{}.json", active_key_id));

        if key_file_path.exists() {
            let key_content = tokio::fs::read_to_string(&key_file_path).await?;
            let encrypted_master_key: MasterKey = serde_json::from_str(&key_content)
                .map_err(|e| anyhow::anyhow!("Failed to parse master key: {}", e))?;
            e2ee.load_master_key(&encrypted_master_key)
                .map_err(|e| anyhow::anyhow!("Failed to load master key: {}", e))?;
            e2ee.set_active_master_key(active_key_id.to_string());
        } else {
            tracing::warn!("Master key file not found: {}", key_file_path.display());
        }
    } else if !master_password.is_empty() {
        // No encryption.json but password provided — auto-enable E2EE
        let (key_id, master_key) = e2ee.generate_master_key(&master_password)?;
        e2ee.load_master_key(&master_key)?;
        e2ee.set_active_master_key(key_id.clone());

        // Persist encryption config with password
        let keys_dir = data_dir.join("keys");
        tokio::fs::create_dir_all(&keys_dir).await?;
        let key_path = keys_dir.join(format!("{}.json", key_id));
        tokio::fs::write(&key_path, serde_json::to_string_pretty(&master_key)?).await?;

        let config = serde_json::json!({
            "enabled": true,
            "active_master_key_id": key_id,
            "master_password": master_password
        });
        tokio::fs::write(
            &encryption_config_path,
            serde_json::to_string_pretty(&config)?,
        )
        .await?;

        eprintln!(
            "✓ E2EE auto-enabled with provided password (key: {})",
            key_id
        );
    }

    Ok(e2ee)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Launch TUI if no command specified or --tui flag is used
    if cli.command.is_none() || cli.tui {
        return neojoplin_tui::run_app().await;
    }

    // Initialize storage for CLI commands
    let storage = Arc::new(SqliteStorage::new().await?);

    match cli.command.unwrap() {
        Commands::Init => {
            println!("Database initialized at: {}", get_db_path()?.display());
            Ok(())
        }

        Commands::MkNote {
            title,
            parent,
            body,
        } => {
            let note_body = match body {
                Some(body) => body,
                None => {
                    // Launch external editor
                    println!("Opening editor for new note: {}", title);
                    let editor = Editor::new()
                        .map_err(|e| anyhow::anyhow!("Failed to initialize editor: {}", e))?;

                    let initial_content = format!("# {}\n\n", title);
                    editor
                        .edit(&initial_content, &title)
                        .await
                        .map_err(|e| anyhow::anyhow!("Editor failed: {}", e))?
                }
            };

            let note = Note {
                id: joplin_domain::joplin_id(),
                title: title.clone(),
                body: note_body,
                parent_id: parent.unwrap_or_default(),
                created_time: now_ms(),
                updated_time: now_ms(),
                user_created_time: 0,
                user_updated_time: 0,
                is_shared: 0,
                share_id: None,
                master_key_id: None,
                encryption_applied: 0,
                encryption_cipher_text: None,
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
                markup_language: 1,
                encryption_blob_encrypted: 0,
                conflict_original_id: String::new(),
                deleted_time: 0,
            };

            storage.create_note(&note).await?;
            println!("Created note: {} ({})", title, note.id);
            Ok(())
        }

        Commands::MkTodo {
            title,
            parent,
            body,
            due,
        } => {
            let todo_body = body.unwrap_or_default();
            let todo_due = if let Some(due_str) = due {
                chrono::DateTime::parse_from_rfc3339(&due_str)
                    .map(|dt| dt.timestamp_millis())
                    .unwrap_or_else(|_| {
                        eprintln!("Warning: Invalid date format '{}', using 0", due_str);
                        0
                    })
            } else {
                0
            };

            let note = Note {
                id: joplin_domain::joplin_id(),
                title: title.clone(),
                body: todo_body,
                parent_id: parent.unwrap_or_default(),
                created_time: now_ms(),
                updated_time: now_ms(),
                user_created_time: 0,
                user_updated_time: 0,
                is_shared: 0,
                share_id: None,
                master_key_id: None,
                encryption_applied: 0,
                encryption_cipher_text: None,
                is_conflict: 0,
                is_todo: 1,
                todo_completed: 0,
                todo_due,
                source: String::new(),
                source_application: String::new(),
                order: 0,
                latitude: 0,
                longitude: 0,
                altitude: 0,
                author: String::new(),
                source_url: String::new(),
                application_data: String::new(),
                markup_language: 1,
                encryption_blob_encrypted: 0,
                conflict_original_id: String::new(),
                deleted_time: 0,
            };

            storage.create_note(&note).await?;
            println!("Created todo: {} ({})", title, note.id);
            Ok(())
        }

        Commands::TodoToggle { todo } => {
            // Find the note by ID or title
            let note_obj = if let Some(found) = storage.get_note(&todo).await? {
                found
            } else {
                let notes = storage.list_notes(None).await?;
                let found = notes
                    .iter()
                    .find(|n| n.title == todo)
                    .ok_or_else(|| anyhow::anyhow!("Todo not found: {}", todo))?;
                storage.get_note(&found.id).await?.unwrap()
            };

            if note_obj.is_todo != 1 {
                return Err(anyhow::anyhow!("'{}' is not a todo", note_obj.title));
            }

            let mut updated = note_obj.clone();
            if updated.todo_completed > 0 {
                updated.todo_completed = 0;
                println!("󰄱 Uncompleted: {}", updated.title);
            } else {
                updated.todo_completed = now_ms();
                println!("󰄲 Completed: {}", updated.title);
            }
            updated.updated_time = now_ms();
            storage.update_note(&updated).await?;
            Ok(())
        }

        Commands::Import { path } => {
            let import_path = path
                .map(|value| resolve_import_path(&value))
                .unwrap_or_else(default_cli_database_path);
            let summary = import_database(storage.as_ref(), &import_path).await?;
            println!("{}", summary.describe());
            Ok(())
        }

        Commands::ImportDesktop => {
            let summary =
                import_database(storage.as_ref(), &default_desktop_database_path()).await?;
            println!("{}", summary.describe());
            Ok(())
        }

        Commands::Edit { note } => {
            // Find the note
            let note_obj = if let Some(found) = storage.get_note(&note).await? {
                found
            } else {
                // Try to find by title
                let notes = storage.list_notes(None).await?;
                let found = notes
                    .iter()
                    .find(|n| n.title == note)
                    .ok_or_else(|| anyhow::anyhow!("Note not found: {}", note))?;
                storage.get_note(&found.id).await?.unwrap()
            };

            println!("Editing note: {}", note_obj.title);

            // Launch editor
            let editor =
                Editor::new().map_err(|e| anyhow::anyhow!("Failed to initialize editor: {}", e))?;

            let updated_body = editor
                .edit(&note_obj.body, &note_obj.title)
                .await
                .map_err(|e| anyhow::anyhow!("Editor failed: {}", e))?;

            // Update note if content changed
            if updated_body != note_obj.body {
                let mut updated_note = note_obj.clone();
                updated_note.body = updated_body;
                updated_note.updated_time = now_ms();

                storage.update_note(&updated_note).await?;
                println!("Note updated successfully");
            } else {
                println!("No changes made to note");
            }

            Ok(())
        }

        Commands::MkBook { title, parent } => {
            let folder = Folder {
                id: joplin_domain::joplin_id(),
                title: title.clone(),
                parent_id: parent.unwrap_or_default(),
                created_time: now_ms(),
                updated_time: now_ms(),
                user_created_time: 0,
                user_updated_time: 0,
                is_shared: 0,
                share_id: None,
                master_key_id: None,
                encryption_applied: 0,
                encryption_cipher_text: None,
                icon: String::new(),
            };

            storage.create_folder(&folder).await?;
            println!("Created folder: {} ({})", title, folder.id);
            Ok(())
        }

        Commands::Ls {
            pattern,
            folders,
            notes,
        } => {
            if folders || !notes {
                let folders = storage.list_folders().await?;
                for folder in folders {
                    if pattern.as_ref().is_none_or(|p| folder.title.contains(p)) {
                        println!("📁 {} ({})", folder.title, folder.id);
                    }
                }
            }

            if notes || !folders {
                let notes = storage.list_notes(None).await?;
                for note in notes {
                    if pattern.as_ref().is_none_or(|p| note.title.contains(p)) {
                        let icon = if note.is_todo == 1 {
                            if note.todo_completed > 0 {
                                "󰄲"
                            } else {
                                "󰄱"
                            }
                        } else {
                            "📝"
                        };
                        println!("{} {} ({})", icon, note.title, note.id);
                    }
                }
            }

            Ok(())
        }

        Commands::Cat { note } => {
            // Try to find note by ID or title
            let note_obj = if let Some(found) = storage.get_note(&note).await? {
                found
            } else {
                // Try to find by title
                let notes = storage.list_notes(None).await?;
                let found = notes
                    .iter()
                    .find(|n| n.title == note)
                    .ok_or_else(|| anyhow::anyhow!("Note not found: {}", note))?;
                storage.get_note(&found.id).await?.unwrap()
            };

            println!("Title: {}", note_obj.title);
            println!("ID: {}", note_obj.id);
            println!();
            println!("{}", note_obj.body);
            Ok(())
        }

        Commands::Sync {
            url,
            username,
            password,
            remote,
            e2ee_password,
        } => {
            use joplin_domain::SyncEvent;
            use joplin_sync::{ReqwestWebDavClient, SyncEngine, WebDavConfig};
            use tokio::sync::mpsc;

            let configured_target = load_configured_sync_target().await?;
            let (url, username, password, remote) =
                resolve_sync_target(url, username, password, remote, configured_target)?;

            let config = WebDavConfig::new(url, username, password);
            let webdav = Arc::new(ReqwestWebDavClient::new(config)?);

            let (event_tx, mut event_rx) = mpsc::unbounded_channel();

            let mut sync_engine =
                SyncEngine::new(storage.clone(), webdav, event_tx).with_remote_path(remote);

            // Add E2EE service if available
            let e2ee_service = load_e2ee_service(e2ee_password).await?;
            if e2ee_service.is_enabled() {
                sync_engine = sync_engine.with_e2ee(e2ee_service);
            }

            // Handle progress events — track download/upload counts
            let handle = tokio::spawn(async move {
                let mut uploaded = 0u32;
                let mut downloaded = 0u32;
                let deleted = 0u32;
                while let Some(event) = event_rx.recv().await {
                    match event {
                        SyncEvent::Failed { error } => {
                            eprintln!("Sync error: {}", error);
                        }
                        SyncEvent::Warning { message } => {
                            eprintln!("Sync warning: {}", message);
                        }
                        SyncEvent::ItemUploadComplete { .. } => {
                            uploaded += 1;
                        }
                        SyncEvent::ItemDownloadComplete { .. } => {
                            downloaded += 1;
                        }
                        SyncEvent::Completed { duration } => {
                            let secs = duration.as_secs_f32();
                            if uploaded > 0 || downloaded > 0 || deleted > 0 {
                                println!(
                                    "  Uploaded: {}, Downloaded: {}, Deleted: {} ({:.1}s)",
                                    uploaded, downloaded, deleted, secs
                                );
                            } else {
                                println!("  No changes ({:.1}s)", secs);
                            }
                        }
                        _ => {}
                    }
                }
            });

            match sync_engine.sync().await {
                Ok(_) => {
                    // Drop the sync engine to close the event channel
                    drop(sync_engine);
                    // Wait for event handler to process remaining events
                    let _ = handle.await;
                    println!("✓ Sync completed successfully");
                    Ok(())
                }
                Err(e) => {
                    handle.abort();
                    eprintln!("✗ Sync failed: {}", e);
                    Err(e.into())
                }
            }
        }

        Commands::ListBooks => {
            let folders = storage.list_folders().await?;
            println!("Folders ({}):", folders.len());
            for folder in folders {
                println!("  📁 {} ({})", folder.title, folder.id);
            }
            Ok(())
        }

        Commands::RmNote { note, force } => {
            // Try to find note by ID or title
            let note_id = if let Some(found) = storage.get_note(&note).await? {
                found.id
            } else {
                // Try to find by title
                let notes = storage.list_notes(None).await?;
                let found = notes
                    .iter()
                    .find(|n| n.title == note)
                    .ok_or_else(|| anyhow::anyhow!("Note not found: {}", note))?;
                found.id.clone()
            };

            if !force {
                println!("Are you sure you want to delete note '{}'? (y/N)", note);
                // TODO: Add proper confirmation prompt
            }

            storage.delete_note(&note_id).await?;
            println!("Deleted note: {}", note);
            Ok(())
        }

        Commands::RmBook { folder, force } => {
            // Try to find folder by ID or title
            let folder_id = if let Some(found) = storage.get_folder(&folder).await? {
                found.id
            } else {
                // Try to find by title
                let folders = storage.list_folders().await?;
                let found = folders
                    .iter()
                    .find(|f| f.title == folder)
                    .ok_or_else(|| anyhow::anyhow!("Folder not found: {}", folder))?;
                found.id.clone()
            };

            if !force {
                println!("Are you sure you want to delete folder '{}'? (y/N)", folder);
                // TODO: Add proper confirmation prompt
            }

            storage.delete_folder(&folder_id).await?;
            println!("Deleted folder: {}", folder);
            Ok(())
        }

        Commands::E2ee { command } => {
            use dialoguer::Confirm;
            use dialoguer::Password;
            use joplin_sync::E2eeService;

            let data_dir = neojoplin_core::Config::data_dir()?;
            let keys_dir = data_dir.join("keys");

            match command {
                E2eeCommands::Enable { password, new_key } => {
                    // Prompt for password if not provided
                    let password = match password {
                        Some(pwd) => pwd,
                        None => {
                            println!("Setting up master password for encryption");
                            Password::new()
                                .with_prompt("Enter master password")
                                .interact()?
                        }
                    };

                    // Confirm password if not provided via flag
                    if password.is_empty() {
                        return Err(anyhow::anyhow!("Password cannot be empty"));
                    }

                    // Verify password strength (basic check)
                    if password.is_empty() {
                        return Err(anyhow::anyhow!("Password cannot be empty"));
                    }

                    let reusable_key = if new_key {
                        None
                    } else {
                        find_reusable_master_key(&data_dir, &password).await?
                    };

                    // Create E2EE service and reuse or generate a master key
                    let mut e2ee_service = E2eeService::new();
                    e2ee_service.set_master_password(password.clone());
                    let (key_id, master_key, reused_existing_key) =
                        if let Some((key_id, master_key)) = reusable_key {
                            (key_id, master_key, true)
                        } else {
                            let (key_id, master_key) =
                                e2ee_service.generate_master_key(&password)?;
                            (key_id, master_key, false)
                        };

                    // Load the master key into the service
                    e2ee_service.load_master_key(&master_key)?;
                    e2ee_service.set_active_master_key(key_id.clone());

                    // Save master key to file
                    tokio::fs::create_dir_all(&keys_dir).await?;
                    let key_path = keys_dir.join(format!("{}.json", key_id));
                    if !reused_existing_key || !key_path.exists() {
                        let key_json = serde_json::to_string_pretty(&master_key)?;
                        tokio::fs::write(&key_path, key_json).await?;
                    }

                    // Save active key ID to config
                    let config_path = data_dir.join("encryption.json");
                    let config = serde_json::json!({
                        "enabled": true,
                        "active_master_key_id": key_id,
                        "master_password": password
                    });
                    tokio::fs::write(&config_path, serde_json::to_string_pretty(&config)?).await?;

                    println!("✓ Encryption enabled successfully");
                    if reused_existing_key {
                        println!("✓ Reused existing master key");
                    } else {
                        println!("✓ Master key generated and encrypted");
                    }
                    println!("✓ Master key ID: {}", key_id);
                    println!();
                    println!("Important: Store your master password in a secure location.");
                    println!("If you lose it, you will not be able to decrypt your notes.");

                    Ok(())
                }

                E2eeCommands::Disable { force } => {
                    let config_path = data_dir.join("encryption.json");

                    if !config_path.exists() {
                        println!("Encryption is not enabled");
                        return Ok(());
                    }

                    if !force {
                        let confirmed = Confirm::new()
                            .with_prompt(
                                "Disable encryption? The next sync will re-upload items decrypted.",
                            )
                            .default(false)
                            .interact()?;

                        if !confirmed {
                            println!("Operation cancelled");
                            return Ok(());
                        }
                    }

                    let existing_config = tokio::fs::read_to_string(&config_path)
                        .await
                        .ok()
                        .and_then(|content| {
                            serde_json::from_str::<serde_json::Value>(&content).ok()
                        })
                        .unwrap_or_else(|| serde_json::json!({}));
                    let mut config = existing_config;
                    config["enabled"] = serde_json::json!(false);
                    tokio::fs::write(&config_path, serde_json::to_string_pretty(&config)?).await?;

                    println!("✓ Encryption disabled");
                    println!("Note: The next sync will re-upload items without encryption");

                    Ok(())
                }

                E2eeCommands::Status => {
                    let config_path = data_dir.join("encryption.json");

                    if config_path.exists() {
                        let config_content = tokio::fs::read_to_string(&config_path).await?;
                        let config: serde_json::Value = serde_json::from_str(&config_content)?;

                        let enabled = config["enabled"].as_bool().unwrap_or(false);
                        let active_key = config["active_master_key_id"].as_str().unwrap_or("none");

                        println!(
                            "Encryption: {}",
                            if enabled { "Enabled" } else { "Disabled" }
                        );
                        println!("Active master key: {}", active_key);

                        // List available keys
                        if keys_dir.exists() {
                            let mut entries = tokio::fs::read_dir(&keys_dir).await?;
                            let mut key_count = 0;
                            while let Some(entry) = entries.next_entry().await? {
                                if entry.path().extension().is_some_and(|e| e == "json") {
                                    key_count += 1;
                                }
                            }
                            println!("Available master keys: {}", key_count);
                        }
                    } else {
                        println!("Encryption: Disabled");
                        println!("No master keys configured");
                    }

                    Ok(())
                }

                E2eeCommands::Decrypt { encrypted } => {
                    let encrypted = if encrypted.starts_with("JED") {
                        encrypted
                    } else {
                        encrypted
                            .lines()
                            .find_map(|line| {
                                line.strip_prefix("encryption_cipher_text: ")
                                    .map(str::trim)
                                    .filter(|value| value.starts_with("JED"))
                                    .map(ToOwned::to_owned)
                            })
                            .ok_or_else(|| anyhow::anyhow!("Input is not in JED format"))?
                    };

                    // Extract key ID from the JED header.
                    if encrypted.len() < 45 {
                        return Err(anyhow::anyhow!("JED data too short to extract key ID"));
                    }

                    let key_id_hex = &encrypted[13..45];
                    let key_id = format!(
                        "{}-{}-{}-{}-{}",
                        &key_id_hex[0..8],
                        &key_id_hex[8..12],
                        &key_id_hex[12..16],
                        &key_id_hex[16..20],
                        &key_id_hex[20..32]
                    );

                    // Load master key
                    let key_path = keys_dir.join(format!("{}.json", key_id));
                    if !key_path.exists() {
                        return Err(anyhow::anyhow!("Master key not found: {}", key_id));
                    }

                    let encrypted_key_json = tokio::fs::read_to_string(&key_path).await?;
                    let encrypted_master_key: joplin_sync::MasterKey =
                        serde_json::from_str(&encrypted_key_json)
                            .map_err(|e| anyhow::anyhow!("Failed to parse master key: {}", e))?;

                    // Prompt for password
                    let password = Password::new()
                        .with_prompt("Enter master password")
                        .interact()?;

                    // Create E2EE service and load master key
                    let mut e2ee_service = E2eeService::new();
                    e2ee_service.set_master_password(password);
                    e2ee_service.load_master_key(&encrypted_master_key)?;

                    // Decrypt the data
                    let decrypted = e2ee_service.decrypt_string(&encrypted)?;

                    println!("{}", decrypted);
                    Ok(())
                }
            }
        }
    }
}

fn get_db_path() -> Result<PathBuf> {
    use neojoplin_core::Config;
    Ok(Config::data_dir()?.join("joplin.db"))
}

async fn find_reusable_master_key(
    data_dir: &std::path::Path,
    password: &str,
) -> Result<Option<(String, MasterKey)>> {
    let config_path = data_dir.join("encryption.json");
    let preferred_key_id = if config_path.exists() {
        let content = tokio::fs::read_to_string(&config_path).await?;
        serde_json::from_str::<serde_json::Value>(&content)
            .ok()
            .and_then(|config| {
                config
                    .get("active_master_key_id")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
            })
    } else {
        None
    };

    let keys_dir = data_dir.join("keys");
    if !keys_dir.exists() {
        return Ok(None);
    }

    let mut candidate_paths = Vec::new();
    if let Some(key_id) = preferred_key_id {
        let preferred_path = keys_dir.join(format!("{}.json", key_id));
        if preferred_path.exists() {
            candidate_paths.push(preferred_path);
        }
    }

    let mut entries = tokio::fs::read_dir(&keys_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") && !candidate_paths.contains(&path) {
            candidate_paths.push(path);
        }
    }

    for path in candidate_paths {
        let content = tokio::fs::read_to_string(&path).await?;
        let master_key: MasterKey = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse master key {}", path.display()))?;
        let mut e2ee = E2eeService::new();
        e2ee.set_master_password(password.to_string());
        if e2ee.load_master_key(&master_key).is_ok() {
            return Ok(Some((master_key.id.clone(), master_key)));
        }
    }

    Ok(None)
}

async fn load_configured_sync_target() -> Result<Option<SyncTarget>> {
    let data_dir = neojoplin_core::Config::data_dir()?;
    let mut settings = Settings::default();
    settings.load_all_settings(&data_dir).await?;
    Ok(settings
        .sync
        .current_target_index
        .and_then(|index| settings.sync.targets.get(index).cloned()))
}

fn resolve_sync_target(
    url: Option<String>,
    username: Option<String>,
    password: Option<String>,
    remote: Option<String>,
    configured_target: Option<SyncTarget>,
) -> Result<(String, String, String, String)> {
    if let Some(url) = url {
        return Ok((
            url,
            username.unwrap_or_default(),
            password.unwrap_or_default(),
            remote.unwrap_or_else(|| "/neojoplin".to_string()),
        ));
    }

    let configured_target = configured_target.ok_or_else(|| {
        anyhow::anyhow!(
            "WebDAV URL is required or configure a sync target in the TUI settings first"
        )
    })?;

    let (base_url, configured_remote) = split_webdav_url(&configured_target.url);
    Ok((
        base_url,
        username.unwrap_or(configured_target.username),
        password.unwrap_or(configured_target.password),
        remote.unwrap_or(configured_remote),
    ))
}

fn split_webdav_url(full_url: &str) -> (String, String) {
    let trimmed = full_url.trim_end_matches('/');
    let scheme_end = trimmed.find("://").map(|i| i + 3).unwrap_or(0);
    if let Some(slash_pos) = trimmed[scheme_end..].rfind('/') {
        let abs_pos = scheme_end + slash_pos;
        let base = &trimmed[..abs_pos];
        let path = &trimmed[abs_pos..];
        if !path.is_empty() && path != "/" {
            return (base.to_string(), path.to_string());
        }
    }

    (trimmed.to_string(), "/neojoplin".to_string())
}
