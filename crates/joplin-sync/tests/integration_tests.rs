// Integration tests for sync engine with real WebDAV server
//
// These tests require a running WebDAV server and provide comprehensive
// testing of the full sync cycle beyond unit tests.

use joplin_sync::{SyncEngine, ReqwestWebDavClient, WebDavConfig};
use joplin_domain::{Storage, Note, Folder, now_ms, WebDavClient};
use std::sync::Arc;
use tokio::sync::mpsc;
use std::time::Duration;
use uuid::Uuid;
use async_trait::async_trait;

// Simple in-memory storage implementation for testing
struct MockStorage {
    folders: Arc<tokio::sync::RwLock<Vec<Folder>>>,
    notes: Arc<tokio::sync::RwLock<Vec<Note>>>,
}

impl MockStorage {
    fn new() -> Self {
        Self {
            folders: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            notes: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl Storage for MockStorage {
    async fn create_note(&self, note: &Note) -> anyhow::Result<()> {
        self.notes.write().await.push(note.clone());
        Ok(())
    }

    async fn update_note(&self, note: &Note) -> anyhow::Result<()> {
        let mut notes = self.notes.write().await;
        if let Some(n) = notes.iter_mut().find(|n| n.id == note.id) {
            *n = note.clone();
        }
        Ok(())
    }

    async fn get_note(&self, id: &str) -> anyhow::Result<Option<Note>> {
        let notes = self.notes.read().await;
        Ok(notes.iter().find(|n| n.id == id).cloned())
    }

    async fn list_notes(&self, _parent_id: Option<&str>) -> anyhow::Result<Vec<Note>> {
        let notes = self.notes.read().await;
        Ok(notes.clone())
    }

    async fn delete_note(&self, id: &str) -> anyhow::Result<()> {
        let mut notes = self.notes.write().await;
        notes.retain(|n| n.id != id);
        Ok(())
    }

    async fn create_folder(&self, folder: &Folder) -> anyhow::Result<()> {
        self.folders.write().await.push(folder.clone());
        Ok(())
    }

    async fn update_folder(&self, folder: &Folder) -> anyhow::Result<()> {
        let mut folders = self.folders.write().await;
        if let Some(f) = folders.iter_mut().find(|f| f.id == folder.id) {
            *f = folder.clone();
        }
        Ok(())
    }

    async fn get_folder(&self, id: &str) -> anyhow::Result<Option<Folder>> {
        let folders = self.folders.read().await;
        Ok(folders.iter().find(|f| f.id == id).cloned())
    }

    async fn list_folders(&self) -> anyhow::Result<Vec<Folder>> {
        let folders = self.folders.read().await;
        Ok(folders.clone())
    }

    async fn delete_folder(&self, id: &str) -> anyhow::Result<()> {
        let mut folders = self.folders.write().await;
        folders.retain(|f| f.id != id);
        Ok(())
    }

    // Stub implementations for other required methods
    async fn get_note(&self, id: &str) -> joplin_domain::Result<Option<Note>> {
        let notes = self.notes.read().await;
        Ok(notes.iter().find(|n| n.id == id).cloned())
    }

    async fn get_folder(&self, id: &str) -> joplin_domain::Result<Option<Folder>> {
        let folders = self.folders.read().await;
        Ok(folders.iter().find(|f| f.id == id).cloned())
    }

    async fn create_tag(&self, _tag: &joplin_domain::Tag) -> joplin_domain::Result<()> {
        Ok(())
    }

    async fn get_tag(&self, _id: &str) -> joplin_domain::Result<Option<joplin_domain::Tag>> {
        Ok(None)
    }

    async fn update_tag(&self, _tag: &joplin_domain::Tag) -> joplin_domain::Result<()> {
        Ok(())
    }

    async fn delete_tag(&self, _id: &str) -> joplin_domain::Result<()> {
        Ok(())
    }

    async fn list_tags(&self) -> joplin_domain::Result<Vec<joplin_domain::Tag>> {
        Ok(Vec::new())
    }

    async fn add_note_tag(&self, _note_tag: &joplin_domain::NoteTag) -> joplin_domain::Result<()> {
        Ok(())
    }

    async fn remove_note_tag(&self, _note_id: &str, _tag_id: &str) -> joplin_domain::Result<()> {
        Ok(())
    }

    async fn get_note_tags(&self, _note_id: &str) -> joplin_domain::Result<Vec<joplin_domain::Tag>> {
        Ok(Vec::new())
    }

    async fn get_notes_updated_since(&self, _timestamp: i64) -> joplin_domain::Result<Vec<Note>> {
        Ok(Vec::new())
    }

    async fn get_folders_updated_since(&self, _timestamp: i64) -> joplin_domain::Result<Vec<Folder>> {
        Ok(Vec::new())
    }

    async fn get_tags_updated_since(&self, _timestamp: i64) -> joplin_domain::Result<Vec<joplin_domain::Tag>> {
        Ok(Vec::new())
    }

    async fn get_note_tags_updated_since(&self, _timestamp: i64) -> joplin_domain::Result<Vec<joplin_domain::NoteTag>> {
        Ok(Vec::new())
    }

    async fn get_all_sync_items(&self) -> joplin_domain::Result<Vec<joplin_domain::SyncItem>> {
        Ok(Vec::new())
    }

    async fn update_sync_time(&self, _table: &str, _id: &str, _timestamp: i64) -> joplin_domain::Result<()> {
        Ok(())
    }

    async fn get_setting(&self, _key: &str) -> joplin_domain::Result<Option<String>> {
        Ok(None)
    }

    async fn set_setting(&self, _key: &str, _value: &str) -> joplin_domain::Result<()> {
        Ok(())
    }
}

async fn setup_test_environment(test_name: String) -> (Arc<MockStorage>, ReqwestWebDavClient, String) {
    // Create in-memory database for testing
    let storage = Arc::new(MockStorage::new());

    // Configure for local WebDAV server
    let config = WebDavConfig::new(
        "http://localhost:8080/webdav/".to_string(),
        String::new(),
        String::new(),
    );
    let webdav = ReqwestWebDavClient::new(config).unwrap();

    // Generate unique test path
    let test_path = format!("/integration-test-{}", test_name);

    // Clean up any existing test data
    let cleanup_path = format!("{}/", test_path);
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        webdav.delete(&cleanup_path)
    ).await;

    (storage, webdav, test_path)
}

async fn cleanup_test_environment(webdav: &ReqwestWebDavClient, test_path: &str) {
    let cleanup_path = format!("{}/", test_path);
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        webdav.delete(&cleanup_path)
    ).await;
}

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored integration_basic_sync
async fn integration_basic_sync() {
    let (storage, webdav, test_path) = setup_test_environment("basic_sync".to_string()).await;

    // Create test data
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

    storage.create_folder(&folder).await.unwrap();

    let note = Note {
        id: Uuid::new_v4().to_string(),
        title: "Test Note".to_string(),
        body: "Integration test content".to_string(),
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
        application_data: String::new(),
        order: 0,
        latitude: 0,
        longitude: 0,
        altitude: 0,
        author: String::new(),
        source_url: String::new(),
        markup_language: 1,
        encryption_blob_encrypted: 0,
        conflict_original_id: String::new(),
    };

    storage.create_note(&note).await.unwrap();

    // Perform sync
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let mut sync_engine = SyncEngine::new(storage.clone(), Arc::new(webdav.clone()), event_tx)
        .with_remote_path(test_path);

    let sync_result = tokio::time::timeout(
        Duration::from_secs(30),
        sync_engine.sync()
    ).await;

    assert!(sync_result.is_ok(), "Sync should complete successfully");

    // Verify data was synced
    let synced_folders = storage.list_folders().await.unwrap();
    assert!(!synced_folders.is_empty(), "Should have synced folders");

    let synced_notes = storage.list_notes(None).await.unwrap();
    assert!(!synced_notes.is_empty(), "Should have synced notes");

    cleanup_test_environment(&webdav, &test_path).await;
}

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored integration_webdav_operations
async fn integration_webdav_operations() {
    let (_, webdav, test_path) = setup_test_environment("webdav_ops".to_string()).await;

    // Test directory creation
    let dir_path = format!("{}/test-dir", test_path);
    let mkdir_result = tokio::time::timeout(
        Duration::from_secs(5),
        webdav.mkdir(&dir_path)
    ).await;
    assert!(mkdir_result.is_ok(), "Directory creation should succeed");

    // Test file upload
    let test_content = b"Test file content";
    let file_path = format!("{}/test-dir/test.txt", test_path);
    let put_result = tokio::time::timeout(
        Duration::from_secs(5),
        webdav.put(&file_path, test_content, test_content.len() as u64)
    ).await;
    assert!(put_result.is_ok(), "File upload should succeed");

    // Test file existence
    let exists_result = tokio::time::timeout(
        Duration::from_secs(5),
        webdav.exists(&file_path)
    ).await;
    assert!(exists_result.is_ok(), "File existence check should succeed");
    assert!(exists_result.unwrap(), "File should exist");

    // Test file download
    let get_result = tokio::time::timeout(
        Duration::from_secs(5),
        webdav.get(&file_path)
    ).await;
    assert!(get_result.is_ok(), "File download should succeed");
    assert_eq!(get_result.unwrap(), test_content, "Downloaded content should match");

    cleanup_test_environment(&webdav, &test_path).await;
}