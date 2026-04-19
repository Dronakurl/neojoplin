// NeoJoplin - Main entry point (CLI + TUI)

use clap::{Parser, Subcommand};
use joplin_domain::{now_ms, Note, Folder, Storage};
use neojoplin_core::Editor;
use neojoplin_storage::SqliteStorage;
use std::sync::Arc;
use std::path::PathBuf;
use anyhow::Result;

#[derive(Parser)]
#[command(name = "neojoplin")]
#[command(about = "A fast, terminal-based Joplin client", long_about = None)]
#[command(version = "0.1.0")]
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
        /// WebDAV URL
        #[arg(long)]
        url: Option<String>,
        /// Username
        #[arg(short = 'U', long)]
        username: Option<String>,
        /// Password
        #[arg(short = 'P', long)]
        password: Option<String>,
        /// Remote path
        #[arg(short = 'r', long, default_value = "/neojoplin")]
        remote: String,
    },

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

        Commands::MkNote { title, parent, body } => {
            let note_body = match body {
                Some(body) => body,
                None => {
                    // Launch external editor
                    println!("Opening editor for new note: {}", title);
                    let editor = Editor::new()
                        .map_err(|e| anyhow::anyhow!("Failed to initialize editor: {}", e))?;

                    let initial_content = format!("# {}\n\n", title);
                    editor.edit(&initial_content, &title).await
                        .map_err(|e| anyhow::anyhow!("Editor failed: {}", e))?
                }
            };

            let note = Note {
                id: uuid::Uuid::new_v4().to_string(),
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
            };

            storage.create_note(&note).await?;
            println!("Created note: {} ({})", title, note.id);
            Ok(())
        }

        Commands::Edit { note } => {
            // Find the note
            let note_obj = if let Some(found) = storage.get_note(&note).await? {
                found
            } else {
                // Try to find by title
                let notes = storage.list_notes(None).await?;
                let found = notes.iter()
                    .find(|n| n.title == note)
                    .ok_or_else(|| anyhow::anyhow!("Note not found: {}", note))?;
                storage.get_note(&found.id).await?.unwrap()
            };

            println!("Editing note: {}", note_obj.title);

            // Launch editor
            let editor = Editor::new()
                .map_err(|e| anyhow::anyhow!("Failed to initialize editor: {}", e))?;

            let updated_body = editor.edit(&note_obj.body, &note_obj.title).await
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
                id: uuid::Uuid::new_v4().to_string(),
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

        Commands::Ls { pattern, folders, notes } => {
            if folders || !notes {
                let folders = storage.list_folders().await?;
                for folder in folders {
                    if pattern.as_ref().map_or(true, |p| folder.title.contains(p)) {
                        println!("📁 {} ({})", folder.title, folder.id);
                    }
                }
            }

            if notes || !folders {
                let notes = storage.list_notes(None).await?;
                for note in notes {
                    if pattern.as_ref().map_or(true, |p| note.title.contains(p)) {
                        println!("📝 {} ({})", note.title, note.id);
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
                let found = notes.iter()
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

        Commands::Sync { url, username, password, remote } => {
            use joplin_sync::{SyncEngine, ReqwestWebDavClient, WebDavConfig};
            use tokio::sync::mpsc;
            use joplin_domain::SyncEvent;

            let url = url.ok_or_else(|| anyhow::anyhow!("WebDAV URL is required"))?;

            // For testing with local WebDAV servers, allow empty credentials
            let username = username.unwrap_or_else(|| "".to_string());
            let password = password.unwrap_or_else(|| "".to_string());

            let config = WebDavConfig::new(url, username, password);
            let webdav = Arc::new(ReqwestWebDavClient::new(config)?);

            let (event_tx, mut event_rx) = mpsc::unbounded_channel();

            let mut sync_engine = SyncEngine::new(
                storage.clone(),
                webdav,
                event_tx,
            )
            .with_remote_path(remote);

            println!("Starting sync...");

            // Handle progress events
            let handle = tokio::spawn(async move {
                while let Some(event) = event_rx.recv().await {
                    match event {
                        SyncEvent::PhaseStarted(phase) => {
                            println!("Phase started: {:?}", phase);
                        }
                        SyncEvent::PhaseCompleted(phase) => {
                            println!("Phase completed: {:?}", phase);
                        }
                        SyncEvent::Progress { phase, current, total, message } => {
                            println!("[{:?}] {}/{}: {}", phase, current, total, message);
                        }
                        SyncEvent::Completed { duration } => {
                            println!("Sync completed in {:?}", duration);
                        }
                        SyncEvent::Failed { error } => {
                            eprintln!("Sync failed: {}", error);
                        }
                        SyncEvent::Warning { message } => {
                            eprintln!("Warning: {}", message);
                        }
                        SyncEvent::ItemDownload { item_type, item_id } => {
                            println!("Downloading {} {}", item_type, item_id);
                        }
                        SyncEvent::ItemDownloadComplete { item_type, item_id } => {
                            println!("Downloaded {} {}", item_type, item_id);
                        }
                        SyncEvent::ItemUpload { item_type, item_id } => {
                            println!("Uploading {} {}", item_type, item_id);
                        }
                        SyncEvent::ItemUploadComplete { item_type, item_id } => {
                            println!("Uploaded {} {}", item_type, item_id);
                        }
                        _ => {}
                    }
                }
            });

            match sync_engine.sync().await {
                Ok(_) => {
                    handle.abort();
                    println!("Sync finished successfully");
                    Ok(())
                }
                Err(e) => {
                    handle.abort();
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
                let found = notes.iter()
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
                let found = folders.iter()
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
            use neojoplin_e2ee::{EncryptionContext, MasterKey};
            use dialoguer::Confirm;
            use dialoguer::Password;

            let data_dir = neojoplin_core::Config::data_dir()?;
            let keys_dir = data_dir.join("keys");

            match command {
                E2eeCommands::Enable { password } => {
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
                    if password.len() < 8 {
                        return Err(anyhow::anyhow!("Password must be at least 8 characters"));
                    }

                    // Create master key
                    let master_key = MasterKey::new();
                    let key_id = master_key.id.clone();

                    // Encrypt master key with password
                    let encrypted_master_key = master_key.encrypt_with_password(&password)?;

                    // Save to file
                    tokio::fs::create_dir_all(&keys_dir).await?;
                    let key_path = keys_dir.join(format!("{}.json", key_id));
                    tokio::fs::write(&key_path, encrypted_master_key).await?;

                    // Save active key ID to config
                    let config_path = data_dir.join("encryption.json");
                    let config = serde_json::json!({
                        "enabled": true,
                        "active_master_key_id": key_id
                    });
                    tokio::fs::write(&config_path, config.to_string()).await?;

                    println!("✓ Encryption enabled successfully");
                    println!("✓ Master key generated and encrypted");
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
                            .with_prompt("Are you sure you want to disable encryption? This will not decrypt your existing notes.")
                            .default(false)
                            .interact()?;

                        if !confirmed {
                            println!("Operation cancelled");
                            return Ok(());
                        }
                    }

                    // Remove encryption config
                    tokio::fs::remove_file(&config_path).await.ok();

                    println!("✓ Encryption disabled");
                    println!("Note: Existing encrypted notes remain encrypted");

                    Ok(())
                }

                E2eeCommands::Status => {
                    let config_path = data_dir.join("encryption.json");

                    if config_path.exists() {
                        let config_content = tokio::fs::read_to_string(&config_path).await?;
                        let config: serde_json::Value = serde_json::from_str(&config_content)?;

                        let enabled = config["enabled"].as_bool().unwrap_or(false);
                        let active_key = config["active_master_key_id"].as_str().unwrap_or("none");

                        println!("Encryption: {}", if enabled { "Enabled" } else { "Disabled" });
                        println!("Active master key: {}", active_key);

                        // List available keys
                        if keys_dir.exists() {
                            let mut entries = tokio::fs::read_dir(&keys_dir).await?;
                            let mut key_count = 0;
                            while let Some(entry) = entries.next_entry().await? {
                                if entry.path().extension().map_or(false, |e| e == "json") {
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
                    use neojoplin_e2ee::jed_format::JedFormat;

                    // Check if it's JED format
                    if !JedFormat::is_jed_format(&encrypted) {
                        return Err(anyhow::anyhow!("Input is not in JED format"));
                    }

                    // Extract key ID
                    let key_id = JedFormat::extract_key_id(&encrypted)?;

                    // Load master key
                    let key_path = keys_dir.join(format!("{}.json", key_id));
                    if !key_path.exists() {
                        return Err(anyhow::anyhow!("Master key not found: {}", key_id));
                    }

                    let encrypted_key = tokio::fs::read_to_string(&key_path).await?;

                    // Prompt for password
                    let password = Password::new()
                        .with_prompt("Enter master password")
                        .interact()?;

                    // Decrypt master key
                    let master_key = neojoplin_e2ee::MasterKey::decrypt_from_password(&encrypted_key, &password)?;
                    let key_data = master_key.data.clone();

                    // Create encryption context
                    let mut context = EncryptionContext::new();
                    context.load_master_key(key_id, key_data.clone());

                    // Decrypt the data
                    let decrypted = neojoplin_e2ee::JedDecoder::decode(&encrypted, &key_data)?;

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
