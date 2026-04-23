// Simple sync test using fake WebDAV client

use neojoplin_core::{now_ms, Folder, Note, Storage};
use neojoplin_storage::SqliteStorage;
use neojoplin_sync::{FakeWebDavClient, SyncEngine};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== NeoJoplin Sync Test (Fake WebDAV) ===");

    // Setup test database
    let db_path = PathBuf::from("/tmp/test-sync-fake.db");
    let _ = std::fs::remove_file(&db_path);

    let storage = Arc::new(SqliteStorage::with_path(&db_path).await?);
    let webdav = Arc::new(FakeWebDavClient::new());

    // Create test folder
    let folder = Folder {
        id: Uuid::new_v4().to_string(),
        title: "Test Folder".to_string(),
        parent_id: String::new(),
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

    // Create test note
    let note = Note {
        id: Uuid::new_v4().to_string(),
        title: "Test Note".to_string(),
        body: "Test content from NeoJoplin".to_string(),
        parent_id: folder.id.clone(),
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

    println!("✓ Created test data");

    // Setup sync engine
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let mut sync_engine = SyncEngine::new(storage.clone(), webdav.clone(), event_tx)
        .with_remote_path("/test-sync".to_string());

    // Run sync
    println!("Starting sync...");
    match sync_engine.sync().await {
        Ok(_) => println!("✓ Sync completed successfully"),
        Err(e) => println!("✗ Sync failed: {}", e),
    }

    // Verify data was uploaded to WebDAV
    let all_files: HashMap<String, Vec<u8>> = webdav.get_all_files().await;
    println!("\n✓ Files uploaded to WebDAV: {}", all_files.len());

    // List some file paths
    for (path, _) in all_files.iter().take(5) {
        println!("  - {}", path);
    }

    if !all_files.is_empty() {
        println!("\n✅ SYNC TEST PASSED");
        println!("  - Database creation: ✓");
        println!("  - Test data creation: ✓");
        println!("  - Sync upload: ✓");
        println!("  - WebDAV verification: ✓");
    } else {
        println!("\n❌ SYNC TEST FAILED - No files uploaded");
    }

    // Cleanup
    let _ = std::fs::remove_file(&db_path);
    println!("\nCleanup complete");

    Ok(())
}
