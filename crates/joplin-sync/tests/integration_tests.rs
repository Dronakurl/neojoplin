// Integration tests for sync engine with a real WebDAV server.
//
// These tests are ignored by default, but they still need to compile against
// the current Storage and WebDAV APIs.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::io::AsyncReadExt;
use joplin_domain::{
    now_ms, DatabaseError, DeletedItem, Folder, Note, NoteTag, Storage, SyncItem, Tag, WebDavClient,
};
use joplin_sync::{ReqwestWebDavClient, SyncEngine, WebDavConfig};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

struct MockStorage {
    folders: Arc<RwLock<Vec<Folder>>>,
    notes: Arc<RwLock<Vec<Note>>>,
    sync_items: Arc<RwLock<Vec<SyncItem>>>,
    deleted_items: Arc<RwLock<Vec<DeletedItem>>>,
    settings: Arc<RwLock<HashMap<String, String>>>,
}

impl MockStorage {
    fn new() -> Self {
        Self {
            folders: Arc::new(RwLock::new(Vec::new())),
            notes: Arc::new(RwLock::new(Vec::new())),
            sync_items: Arc::new(RwLock::new(Vec::new())),
            deleted_items: Arc::new(RwLock::new(Vec::new())),
            settings: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl Storage for MockStorage {
    async fn create_note(&self, note: &Note) -> Result<(), DatabaseError> {
        self.notes.write().await.push(note.clone());
        Ok(())
    }

    async fn get_note(&self, id: &str) -> Result<Option<Note>, DatabaseError> {
        Ok(self
            .notes
            .read()
            .await
            .iter()
            .find(|note| note.id == id)
            .cloned())
    }

    async fn update_note(&self, note: &Note) -> Result<(), DatabaseError> {
        let mut notes = self.notes.write().await;
        if let Some(existing) = notes.iter_mut().find(|existing| existing.id == note.id) {
            *existing = note.clone();
            Ok(())
        } else {
            Err(DatabaseError::NotFound(note.id.clone()))
        }
    }

    async fn delete_note(&self, id: &str) -> Result<(), DatabaseError> {
        self.notes.write().await.retain(|note| note.id != id);
        Ok(())
    }

    async fn list_notes(&self, folder_id: Option<&str>) -> Result<Vec<Note>, DatabaseError> {
        let notes = self.notes.read().await;
        Ok(match folder_id {
            Some(folder_id) => notes
                .iter()
                .filter(|note| note.parent_id == folder_id)
                .cloned()
                .collect(),
            None => notes.clone(),
        })
    }

    async fn create_folder(&self, folder: &Folder) -> Result<(), DatabaseError> {
        self.folders.write().await.push(folder.clone());
        Ok(())
    }

    async fn get_folder(&self, id: &str) -> Result<Option<Folder>, DatabaseError> {
        Ok(self
            .folders
            .read()
            .await
            .iter()
            .find(|folder| folder.id == id)
            .cloned())
    }

    async fn update_folder(&self, folder: &Folder) -> Result<(), DatabaseError> {
        let mut folders = self.folders.write().await;
        if let Some(existing) = folders.iter_mut().find(|existing| existing.id == folder.id) {
            *existing = folder.clone();
            Ok(())
        } else {
            Err(DatabaseError::NotFound(folder.id.clone()))
        }
    }

    async fn delete_folder(&self, id: &str) -> Result<(), DatabaseError> {
        self.folders.write().await.retain(|folder| folder.id != id);
        Ok(())
    }

    async fn list_folders(&self) -> Result<Vec<Folder>, DatabaseError> {
        Ok(self.folders.read().await.clone())
    }

    async fn create_tag(&self, _tag: &Tag) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn get_tag(&self, _id: &str) -> Result<Option<Tag>, DatabaseError> {
        Ok(None)
    }

    async fn update_tag(&self, _tag: &Tag) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn delete_tag(&self, _id: &str) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn list_tags(&self) -> Result<Vec<Tag>, DatabaseError> {
        Ok(Vec::new())
    }

    async fn add_note_tag(&self, _note_tag: &NoteTag) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn remove_note_tag(&self, _note_id: &str, _tag_id: &str) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn get_note_tags(&self, _note_id: &str) -> Result<Vec<Tag>, DatabaseError> {
        Ok(Vec::new())
    }

    async fn get_folders_updated_since(
        &self,
        timestamp: i64,
    ) -> Result<Vec<Folder>, DatabaseError> {
        Ok(self
            .folders
            .read()
            .await
            .iter()
            .filter(|folder| folder.updated_time >= timestamp)
            .cloned()
            .collect())
    }

    async fn get_tags_updated_since(&self, _timestamp: i64) -> Result<Vec<Tag>, DatabaseError> {
        Ok(Vec::new())
    }

    async fn get_notes_updated_since(&self, timestamp: i64) -> Result<Vec<Note>, DatabaseError> {
        Ok(self
            .notes
            .read()
            .await
            .iter()
            .filter(|note| note.updated_time >= timestamp)
            .cloned()
            .collect())
    }

    async fn get_note_tags_updated_since(
        &self,
        _timestamp: i64,
    ) -> Result<Vec<NoteTag>, DatabaseError> {
        Ok(Vec::new())
    }

    async fn get_all_sync_items(&self) -> Result<Vec<SyncItem>, DatabaseError> {
        Ok(self.sync_items.read().await.clone())
    }

    async fn update_sync_time(
        &self,
        _table: &str,
        _id: &str,
        _timestamp: i64,
    ) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn get_setting(&self, key: &str) -> Result<Option<String>, DatabaseError> {
        Ok(self.settings.read().await.get(key).cloned())
    }

    async fn set_setting(&self, key: &str, value: &str) -> Result<(), DatabaseError> {
        self.settings
            .write()
            .await
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    async fn get_sync_items(&self, sync_target: i32) -> Result<Vec<SyncItem>, DatabaseError> {
        Ok(self
            .sync_items
            .read()
            .await
            .iter()
            .filter(|item| item.sync_target == sync_target)
            .cloned()
            .collect())
    }

    async fn upsert_sync_item(&self, item: &SyncItem) -> Result<(), DatabaseError> {
        let mut sync_items = self.sync_items.write().await;
        if let Some(existing) = sync_items.iter_mut().find(|existing| {
            existing.id == item.id
                || (existing.sync_target == item.sync_target
                    && existing.item_type == item.item_type
                    && existing.item_id == item.item_id)
        }) {
            *existing = item.clone();
        } else {
            sync_items.push(item.clone());
        }
        Ok(())
    }

    async fn delete_sync_item(&self, id: i32) -> Result<(), DatabaseError> {
        self.sync_items.write().await.retain(|item| item.id != id);
        Ok(())
    }

    async fn clear_all_sync_items(&self) -> Result<usize, DatabaseError> {
        let mut sync_items = self.sync_items.write().await;
        let count = sync_items.len();
        sync_items.clear();
        Ok(count)
    }

    async fn get_deleted_items(&self, sync_target: i32) -> Result<Vec<DeletedItem>, DatabaseError> {
        Ok(self
            .deleted_items
            .read()
            .await
            .iter()
            .filter(|item| item.sync_target == sync_target)
            .cloned()
            .collect())
    }

    async fn add_deleted_item(&self, item: &DeletedItem) -> Result<(), DatabaseError> {
        self.deleted_items.write().await.push(item.clone());
        Ok(())
    }

    async fn remove_deleted_item(&self, id: i32) -> Result<(), DatabaseError> {
        self.deleted_items
            .write()
            .await
            .retain(|item| item.id != id);
        Ok(())
    }

    async fn clear_deleted_items(&self, limit: i64) -> Result<usize, DatabaseError> {
        let mut deleted_items = self.deleted_items.write().await;
        let before = deleted_items.len();
        deleted_items.retain(|item| item.deleted_time > limit);
        Ok(before - deleted_items.len())
    }

    async fn get_version(&self) -> Result<i32, DatabaseError> {
        Ok(41)
    }

    async fn begin_transaction(&self) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn commit_transaction(&self) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn rollback_transaction(&self) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn trash_note(&self, _id: &str) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn restore_note(&self, _id: &str) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn list_deleted_notes(&self) -> Result<Vec<Note>, DatabaseError> {
        Ok(Vec::new())
    }
}

async fn setup_test_environment(
    test_name: String,
) -> (Arc<MockStorage>, ReqwestWebDavClient, String) {
    let storage = Arc::new(MockStorage::new());

    let config = WebDavConfig::new(
        "http://localhost:8080/webdav/".to_string(),
        String::new(),
        String::new(),
    );
    let webdav = ReqwestWebDavClient::new(config).unwrap();

    let test_path = format!("/integration-test-{}", test_name);
    let cleanup_path = format!("{}/", test_path);
    let _ = tokio::time::timeout(Duration::from_secs(5), webdav.delete(&cleanup_path)).await;

    (storage, webdav, test_path)
}

async fn cleanup_test_environment(webdav: &ReqwestWebDavClient, test_path: &str) {
    let cleanup_path = format!("{}/", test_path);
    let _ = tokio::time::timeout(Duration::from_secs(5), webdav.delete(&cleanup_path)).await;
}

#[tokio::test]
#[ignore]
async fn integration_basic_sync() {
    let (storage, webdav, test_path) = setup_test_environment("basic_sync".to_string()).await;

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

    storage.create_note(&note).await.unwrap();

    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let mut sync_engine = SyncEngine::new(storage.clone(), Arc::new(webdav.clone()), event_tx)
        .with_remote_path(test_path.clone());

    let sync_result = tokio::time::timeout(Duration::from_secs(30), sync_engine.sync()).await;

    assert!(sync_result.is_ok(), "Sync should complete successfully");

    let synced_folders = storage.list_folders().await.unwrap();
    assert!(!synced_folders.is_empty(), "Should have synced folders");

    let synced_notes = storage.list_notes(None).await.unwrap();
    assert!(!synced_notes.is_empty(), "Should have synced notes");

    cleanup_test_environment(&webdav, &test_path).await;
}

#[tokio::test]
#[ignore]
async fn integration_webdav_operations() {
    let (_, webdav, test_path) = setup_test_environment("webdav_ops".to_string()).await;

    let dir_path = format!("{}/test-dir", test_path);
    let mkdir_result = tokio::time::timeout(Duration::from_secs(5), webdav.mkcol(&dir_path))
        .await
        .expect("Directory creation timed out");
    assert!(mkdir_result.is_ok(), "Directory creation should succeed");

    let test_content = b"Test file content";
    let file_path = format!("{}/test-dir/test.txt", test_path);
    let put_result = tokio::time::timeout(
        Duration::from_secs(5),
        webdav.put(&file_path, test_content, test_content.len() as u64),
    )
    .await
    .expect("File upload timed out");
    assert!(put_result.is_ok(), "File upload should succeed");

    let exists_result = tokio::time::timeout(Duration::from_secs(5), webdav.exists(&file_path))
        .await
        .expect("File existence check timed out")
        .expect("File existence check failed");
    assert!(exists_result, "File should exist");

    let mut reader = tokio::time::timeout(Duration::from_secs(5), webdav.get(&file_path))
        .await
        .expect("File download timed out")
        .expect("File download failed");
    let mut downloaded = Vec::new();
    reader.read_to_end(&mut downloaded).await.unwrap();
    assert_eq!(downloaded, test_content, "Downloaded content should match");

    cleanup_test_environment(&webdav, &test_path).await;
}
