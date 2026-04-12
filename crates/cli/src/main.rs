// NeoJoplin CLI - Main entry point

use clap::{Parser, Subcommand};
use neojoplin_core::{now_ms, Note, Folder, Storage, Editor};
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
    command: Commands,
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize storage
    let storage = Arc::new(SqliteStorage::new().await?);

    match cli.command {
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
            use neojoplin_sync::{SyncEngine, ReqwestWebDavClient, WebDavConfig};
            use tokio::sync::mpsc;
            use neojoplin_core::SyncEvent;

            // TODO: Load credentials from config if not provided
            let url = url.ok_or_else(|| anyhow::anyhow!("WebDAV URL is required"))?;
            let username = username.ok_or_else(|| anyhow::anyhow!("Username is required"))?;
            let password = password.ok_or_else(|| anyhow::anyhow!("Password is required"))?;

            let config = WebDavConfig::new(url, username, password);
            let webdav = Arc::new(ReqwestWebDavClient::new(config)?);

            let (event_tx, mut event_rx) = mpsc::unbounded_channel();

            let mut sync_engine = SyncEngine::new(
                storage.clone(),
                webdav,
                event_tx,
            ).with_remote_path(remote);

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
    }
}

fn get_db_path() -> Result<PathBuf> {
    use neojoplin_core::Config;
    Ok(Config::data_dir()?.join("joplin.db"))
}
