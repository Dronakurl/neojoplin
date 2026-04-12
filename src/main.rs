// NeoJoplin - A Rust terminal client for Joplin note-taking

use clap::{Parser, Subcommand};
use anyhow::Result;

#[derive(Parser)]
#[command(name = "neojoplin")]
#[command(about = "A Rust terminal client for Joplin note-taking", long_about = None)]
#[command(version = "0.1.0")]
struct Cli {
    /// Enable debug logging
    #[arg(long, global = true)]
    debug: bool,

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
        /// Note body (optional)
        #[arg(short, long)]
        body: Option<String>,
    },
    /// List notes
    Ls {
        /// Search pattern (optional)
        pattern: Option<String>,
    },
    /// Display note content
    Cat {
        /// Note title or ID
        note: String,
    },
    /// Edit a note
    Edit {
        /// Note title or ID
        note: String,
    },
    /// Create a new folder
    MkBook {
        /// Folder title
        title: String,
    },
    /// List folders
    ListBooks,
    /// Show version information
    Version,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    if cli.debug {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init();
    }

    // Run command
    match cli.command {
        Commands::Init => {
            println!("Initializing NeoJoplin database...");
            let db = neojoplin::Database::new().await?;
            let version = db.get_version().await?;
            println!("Database initialized at version {}", version);
            Ok(())
        }
        Commands::Version => {
            println!("NeoJoplin version {}", env!("CARGO_PKG_VERSION"));
            println!("Joplin compatibility: v2.12+ (database schema 41)");
            Ok(())
        }
        Commands::MkNote { title, body } => {
            println!("Creating note: {}", title);
            let db = neojoplin::core::Database::new().await?;
            let note = neojoplin::core::Note {
                title: title.clone(),
                body: body.unwrap_or_default(),
                parent_id: String::new(), // Root folder
                ..Default::default()
            };
            db.create_note(&note).await?;
            println!("✓ Note created with ID: {}", note.id);
            Ok(())
        }
        Commands::ListBooks => {
            let db = neojoplin::core::Database::new().await?;
            let folders = db.list_folders().await?;
            if folders.is_empty() {
                println!("No folders found. Create one with: neojoplin mkbook <title>");
            } else {
                println!("Folders:");
                for folder in folders {
                    println!("  - {}", folder.title);
                }
            }
            Ok(())
        }
        Commands::MkBook { title } => {
            println!("Creating folder: {}", title);
            let db = neojoplin::Database::new().await?;
            let folder = neojoplin::core::Folder {
                title: title.clone(),
                ..Default::default()
            };
            db.create_folder(&folder).await?;
            println!("✓ Folder created with ID: {}", folder.id);
            Ok(())
        }
        Commands::Ls { pattern } => {
            let db = neojoplin::core::Database::new().await?;
            let notes = db.list_notes(None).await?;
            if notes.is_empty() {
                println!("No notes found. Create one with: neojoplin mknote <title>");
            } else {
                println!("Notes:");
                for note in notes {
                    if let Some(pat) = &pattern {
                        if note.title.contains(pat) || note.body.contains(pat) {
                            println!("  - {}", note.title);
                        }
                    } else {
                        println!("  - {}", note.title);
                    }
                }
            }
            Ok(())
        }
        Commands::Cat { note } => {
            let db = neojoplin::core::Database::new().await?;
            // Try to find by title first
            let notes = db.list_notes(None).await?;
            let found = notes.iter().find(|n| n.title == note);

            if let Some(note) = found {
                println!("--- {} ---", note.title);
                println!("{}", note.body);
            } else {
                // Try by ID
                if let Some(note) = db.get_note(&note).await? {
                    println!("--- {} ---", note.title);
                    println!("{}", note.body);
                } else {
                    println!("Note not found: {}", note);
                }
            }
            Ok(())
        }
        Commands::Edit { note } => {
            println!("Editing note: {}", note);
            println!("Editor integration will be implemented in the next phase.");
            Ok(())
        }
    }
}
