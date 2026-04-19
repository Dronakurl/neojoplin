// Sync engine implementation

use joplin_domain::{Storage, WebDavClient, SyncEvent, Result, SyncPhase, SyncError, Note, Folder, Tag, NoteTag, now_ms};
use std::sync::Arc;
use tokio::sync::mpsc;
use futures::io::AsyncReadExt;
use serde_json;
use crate::sync_info::SyncInfo;
use crate::e2ee::E2eeService;

/// Sync context to track sync state
#[derive(Debug, Clone)]
struct SyncContext {
    last_sync_time: i64,
    remote_path: String,
}

impl Default for SyncContext {
    fn default() -> Self {
        Self {
            last_sync_time: 0,
            remote_path: "/neojoplin".to_string(),
        }
    }
}

/// Item type for type-safe sync operations
#[derive(Clone, Debug, PartialEq, Copy)]
enum ItemType {
    Note = 1,
    Folder = 2,
    Tag = 3,
    Resource = 4,
    NoteTag = 5,
}

/// Remote item with type information
#[derive(Clone, Debug)]
struct RemoteItem {
    id: String,
    item_type: ItemType,
    modified: Option<i64>,
}

/// Main sync engine
pub struct SyncEngine {
    storage: Arc<dyn Storage>,
    webdav: Arc<dyn WebDavClient>,
    event_tx: mpsc::UnboundedSender<SyncEvent>,
    context: SyncContext,
    sync_info: Option<SyncInfo>,
    e2ee_service: Option<E2eeService>,
}

impl SyncEngine {
    pub fn new(
        storage: Arc<dyn Storage>,
        webdav: Arc<dyn WebDavClient>,
        event_tx: mpsc::UnboundedSender<SyncEvent>,
    ) -> Self {
        Self {
            storage,
            webdav,
            event_tx,
            context: SyncContext::default(),
            sync_info: None,
            e2ee_service: None,
        }
    }

    pub fn with_e2ee(mut self, e2ee_service: E2eeService) -> Self {
        self.e2ee_service = Some(e2ee_service);
        self
    }

    pub fn with_remote_path(mut self, path: String) -> Self {
        self.context.remote_path = path;
        self
    }

    /// Run full sync process
    pub async fn sync(&mut self) -> Result<()> {
        let start = std::time::Instant::now();

        self.check_locks().await?;
        self.load_sync_info().await?;

        // Load master keys from remote info.json if E2EE is available
        self.load_remote_master_keys().await;

        self.ensure_remote_directory().await?;

        // Phase 1: Upload local changes
        self.phase_upload().await?;

        // Phase 2: Delete remote items
        self.phase_delete_remote().await?;

        // Phase 3: Download remote changes (delta)
        self.phase_delta().await?;

        // Save updated sync info
        self.save_sync_info().await?;

        let duration = start.elapsed();
        let _ = self.event_tx.send(SyncEvent::Completed { duration });

        Ok(())
    }

    /// Load master keys from remote info.json and decrypt them
    async fn load_remote_master_keys(&mut self) {
        let sync_info = match &self.sync_info {
            Some(info) if info.e2ee.value && !info.master_keys.is_empty() => info.clone(),
            _ => return,
        };

        let e2ee = match &mut self.e2ee_service {
            Some(e2ee) => e2ee,
            None => return,
        };

        for mk_info in &sync_info.master_keys {
            let key_id = mk_info.id.replace('-', "");
            if e2ee.get_master_key_ids().contains(&key_id) {
                tracing::debug!("Master key {} already loaded", key_id);
                continue;
            }

            let master_key = crate::e2ee::MasterKey {
                id: mk_info.id.clone(),
                created_time: mk_info.created_time,
                updated_time: mk_info.updated_time,
                source_application: mk_info.source_application.clone(),
                encryption_method: mk_info.encryption_method,
                checksum: mk_info.checksum.clone(),
                content: mk_info.content.clone(),
                has_been_used: mk_info.has_been_used,
                enabled: true,
            };

            match e2ee.load_master_key(&master_key) {
                Ok(()) => {
                    tracing::info!("Loaded remote master key: {}", mk_info.id);
                    let active_id = sync_info.active_master_key_id.value.replace('-', "");
                    if key_id == active_id {
                        e2ee.set_active_master_key(key_id);
                    }
                }
                Err(e) => {
                    let _ = self.event_tx.send(SyncEvent::Warning {
                        message: format!("Failed to decrypt master key {}: {}", mk_info.id, e),
                    });
                }
            }
        }
    }

    async fn ensure_remote_directory(&self) -> Result<()> {
        let remote_exists = self.webdav.exists(&self.context.remote_path).await
            .unwrap_or(false);

        if !remote_exists {
            self.webdav.mkcol(&self.context.remote_path).await
                .map_err(|e| SyncError::Server(format!("Failed to create remote directory {}: {}", self.context.remote_path, e)))?;
        }

        // Create locks directory (needed by Joplin protocol)
        let locks_dir = format!("{}/locks", self.context.remote_path.trim_end_matches('/'));
        if !self.webdav.exists(&locks_dir).await.unwrap_or(false) {
            let _ = self.webdav.mkcol(&locks_dir).await;
        }

        Ok(())
    }

    /// Phase 1: Upload local changes
    async fn phase_upload(&mut self) -> Result<()> {
        let _ = self.event_tx.send(SyncEvent::PhaseStarted(SyncPhase::Upload));

        let folders = self.get_changed_folders().await?;
        let tags = self.get_changed_tags().await?;
        let notes = self.get_changed_notes().await?;
        let note_tags = self.get_changed_note_tags().await?;

        let total_items = folders.len() + tags.len() + notes.len() + note_tags.len();

        if total_items == 0 {
            let _ = self.event_tx.send(SyncEvent::Progress {
                phase: SyncPhase::Upload,
                current: 0,
                total: 0,
                message: "No local changes to upload".to_string(),
            });
        } else {
            let mut uploaded = 0;
            for folder in &folders {
                self.upload_item_encrypted(ItemType::Folder, &folder.id, &self.serialize_folder(folder)?).await?;
                uploaded += 1;
                self.report_progress(SyncPhase::Upload, uploaded, total_items, "Uploading folders");
            }

            for tag in &tags {
                self.upload_item_encrypted(ItemType::Tag, &tag.id, &self.serialize_tag(tag)?).await?;
                uploaded += 1;
                self.report_progress(SyncPhase::Upload, uploaded, total_items, "Uploading tags");
            }

            for note_tag in &note_tags {
                self.upload_item_encrypted(ItemType::NoteTag, &note_tag.id, &self.serialize_note_tag(note_tag)?).await?;
                uploaded += 1;
                self.report_progress(SyncPhase::Upload, uploaded, total_items, "Uploading note tags");
            }

            for note in &notes {
                self.upload_item_encrypted(ItemType::Note, &note.id, &self.serialize_note(note)?).await?;
                uploaded += 1;
                self.report_progress(SyncPhase::Upload, uploaded, total_items, "Uploading notes");
            }

            let sync_time = now_ms();
            self.update_sync_times(&folders, &tags, &notes, sync_time).await?;
        }

        let _ = self.event_tx.send(SyncEvent::PhaseCompleted(SyncPhase::Upload));
        Ok(())
    }

    /// Upload an item with E2EE encryption (Joplin compatible format)
    /// Joplin stores all items at root level: {remote_path}/{id}.md
    /// When encrypted, the plaintext content is put in encryption_cipher_text field
    async fn upload_item_encrypted(&self, item_type: ItemType, item_id: &str, plaintext_content: &str) -> Result<()> {
        let type_num = match item_type {
            ItemType::Note => 1,
            ItemType::Folder => 2,
            ItemType::Tag => 5,
            ItemType::NoteTag => 6,
            ItemType::Resource => 4,
        };

        let type_name = match item_type {
            ItemType::Note => "note",
            ItemType::Folder => "folder",
            ItemType::Tag => "tag",
            ItemType::NoteTag => "note_tag",
            ItemType::Resource => "resource",
        };

        let content = if let Some(ref e2ee) = self.e2ee_service {
            if e2ee.is_enabled() {
                // Encrypt the full content and produce Joplin encrypted format
                match e2ee.encrypt_string(plaintext_content) {
                    Ok(encrypted) => {
                        let now = self.ms_to_iso(now_ms());
                        // Build the encrypted item metadata wrapper
                        let mut enc_content = String::new();
                        enc_content.push_str(&format!("id: {}\n", item_id));
                        enc_content.push_str(&format!("created_time: \n"));
                        enc_content.push_str(&format!("updated_time: {}\n", now));
                        enc_content.push_str(&format!("user_created_time: \n"));
                        enc_content.push_str(&format!("user_updated_time: \n"));
                        enc_content.push_str(&format!("encryption_cipher_text: {}\n", encrypted));
                        enc_content.push_str(&format!("encryption_applied: 1\n"));
                        enc_content.push_str(&format!("parent_id: \n"));
                        enc_content.push_str(&format!("is_shared: \n"));
                        enc_content.push_str(&format!("share_id: \n"));
                        enc_content.push_str(&format!("master_key_id: \n"));
                        enc_content.push_str(&format!("user_data: \n"));
                        enc_content.push_str(&format!("deleted_time: 0\n"));
                        // No trailing newline — Joplin's parser interprets trailing \n as body separator
                        enc_content.push_str(&format!("type_: {}", type_num));
                        tracing::info!("Encrypted {} {} for upload", type_name, item_id);
                        enc_content
                    }
                    Err(e) => {
                        tracing::warn!("Failed to encrypt {} {}: {} — uploading unencrypted", type_name, item_id, e);
                        plaintext_content.to_string()
                    }
                }
            } else {
                plaintext_content.to_string()
            }
        } else {
            plaintext_content.to_string()
        };

        // Upload to root level (Joplin 3.5+ format)
        let remote_path = format!("{}/{}.md", self.context.remote_path.trim_end_matches('/'), item_id);

        let _ = self.event_tx.send(SyncEvent::ItemUpload {
            item_type: type_name.to_string(),
            item_id: item_id.to_string(),
        });

        let bytes = content.into_bytes();
        self.webdav.put(&remote_path, &bytes, bytes.len() as u64).await
            .map_err(|e| SyncError::Server(format!("Failed to upload {} {}: {}", type_name, item_id, e)))?;

        let _ = self.event_tx.send(SyncEvent::ItemUploadComplete {
            item_type: type_name.to_string(),
            item_id: item_id.to_string(),
        });

        Ok(())
    }

    /// Phase 2: Delete remote items
    async fn phase_delete_remote(&mut self) -> Result<()> {
        let _ = self.event_tx.send(SyncEvent::PhaseStarted(SyncPhase::DeleteRemote));

        let deleted_items = self.storage.get_deleted_items(2).await
            .map_err(|e| SyncError::Local(e))?;

        if deleted_items.is_empty() {
            let _ = self.event_tx.send(SyncEvent::Progress {
                phase: SyncPhase::DeleteRemote,
                current: 0,
                total: 0,
                message: "No remote items to delete".to_string(),
            });
        } else {
            let total = deleted_items.len();
            for (i, deleted_item) in deleted_items.iter().enumerate() {
                let remote_path = format!("{}/{}.md", self.context.remote_path.trim_end_matches('/'), deleted_item.item_id);
                if let Err(e) = self.webdav.delete(&remote_path).await {
                    let _ = self.event_tx.send(SyncEvent::Warning {
                        message: format!("Failed to delete remote item {}: {}", deleted_item.item_id, e)
                    });
                }
                self.report_progress(SyncPhase::DeleteRemote, i + 1, total, "Deleting remote items");
            }

            self.storage.clear_deleted_items(deleted_items.len() as i64).await
                .map_err(|e| SyncError::Local(e))?;
        }

        let _ = self.event_tx.send(SyncEvent::PhaseCompleted(SyncPhase::DeleteRemote));
        Ok(())
    }

    /// Phase 3: Download remote changes (delta)
    async fn phase_delta(&self) -> Result<()> {
        let _ = self.event_tx.send(SyncEvent::PhaseStarted(SyncPhase::Delta));

        let remote_items = self.list_remote_items().await?;
        let local_items = self.storage.get_all_sync_items().await
            .map_err(|e| SyncError::Local(e))?;
        let items_to_download = self.find_delta_items(&remote_items, &local_items);

        if items_to_download.is_empty() {
            let _ = self.event_tx.send(SyncEvent::Progress {
                phase: SyncPhase::Delta,
                current: 0,
                total: 0,
                message: "No remote changes to download".to_string(),
            });
        } else {
            let total = items_to_download.len();
            tracing::info!("Starting delta download: {} items to download", total);

            for (i, remote_item) in items_to_download.iter().enumerate() {
                tracing::debug!("Downloading item {}/{}: {} ({:?})", i + 1, total, remote_item.id, remote_item.item_type);
                if let Err(e) = self.download_item(&remote_item.id, &remote_item.item_type).await {
                    let _ = self.event_tx.send(SyncEvent::Warning {
                        message: format!("Failed to download {}: {}", remote_item.id, e),
                    });
                }
                self.report_progress(SyncPhase::Delta, i + 1, total, "Downloading remote changes");
            }
        }

        let _ = self.event_tx.send(SyncEvent::PhaseCompleted(SyncPhase::Delta));
        Ok(())
    }

    // === Helper methods ===

    async fn check_locks(&self) -> Result<()> {
        let lock_path = format!("{}/lock.json", self.context.remote_path.trim_end_matches('/'));

        match self.webdav.get(&lock_path).await {
            Ok(mut reader) => {
                let mut content = Vec::new();
                reader.read_to_end(&mut content).await
                    .map_err(|e| SyncError::Server(format!("Failed to read lock.json: {}", e)))?;

                let content_str = String::from_utf8_lossy(&content);
                if let Ok(lock_data) = serde_json::from_str::<serde_json::Value>(&content_str) {
                    if let Some(timestamp) = lock_data.get("updatedTime").and_then(|v| v.as_i64()) {
                        let lock_age_ms = now_ms() - timestamp;
                        let lock_age_min = lock_age_ms / (60 * 1000);
                        if lock_age_min < 5 {
                            return Err(SyncError::Server(format!(
                                "Sync target is locked. Lock age: {} minutes.", lock_age_min
                            )).into());
                        }
                    }
                }
            }
            Err(_) => {} // No lock file — normal
        }

        Ok(())
    }

    async fn load_sync_info(&mut self) -> Result<()> {
        match SyncInfo::load_from_remote(self.webdav.as_ref(), &self.context.remote_path).await {
            Ok(Some(remote_sync_info)) => {
                self.context.last_sync_time = remote_sync_info.delta_timestamp();
                self.sync_info = Some(remote_sync_info);
            }
            Ok(None) => {
                self.sync_info = Some(SyncInfo::new());
                self.context.last_sync_time = 0;
            }
            Err(e) => {
                let error_msg = e.to_string();
                if !error_msg.contains("NotFound") && !error_msg.contains("not found") {
                    return Err(SyncError::Server(format!("Failed to load sync info: {}", e)).into());
                }
                self.sync_info = Some(SyncInfo::new());
                self.context.last_sync_time = 0;
            }
        }
        Ok(())
    }

    async fn save_sync_info(&mut self) -> Result<()> {
        if let Some(ref mut sync_info) = self.sync_info {
            sync_info.update_delta_timestamp();

            // Update E2EE info if encryption is enabled
            if let Some(ref e2ee) = self.e2ee_service {
                if e2ee.is_enabled() {
                    sync_info.e2ee.value = true;
                    if sync_info.e2ee.updated_time == 0 {
                        sync_info.e2ee.updated_time = now_ms();
                    }
                    if let Some(active_id) = e2ee.get_active_master_key_id() {
                        sync_info.active_master_key_id.value = active_id.clone();
                        if sync_info.active_master_key_id.updated_time == 0 {
                            sync_info.active_master_key_id.updated_time = now_ms();
                        }
                    }
                }
            }

            sync_info.save_to_remote(self.webdav.as_ref(), &self.context.remote_path).await
                .map_err(|e| SyncError::Server(format!("Failed to save sync info: {}", e)))?;
        }
        Ok(())
    }

    async fn get_changed_folders(&self) -> Result<Vec<Folder>> {
        self.storage.get_folders_updated_since(self.context.last_sync_time).await
            .map_err(|e| e.into())
    }

    async fn get_changed_tags(&self) -> Result<Vec<Tag>> {
        self.storage.get_tags_updated_since(self.context.last_sync_time).await
            .map_err(|e| e.into())
    }

    async fn get_changed_notes(&self) -> Result<Vec<Note>> {
        self.storage.get_notes_updated_since(self.context.last_sync_time).await
            .map_err(|e| e.into())
    }

    async fn get_changed_note_tags(&self) -> Result<Vec<NoteTag>> {
        self.storage.get_note_tags_updated_since(self.context.last_sync_time).await
            .map_err(|e| e.into())
    }

    async fn update_sync_times(&self, folders: &[Folder], tags: &[Tag], notes: &[Note], sync_time: i64) -> Result<()> {
        for folder in folders {
            self.storage.update_sync_time("folders", &folder.id, sync_time).await
                .map_err(|e| SyncError::Local(e))?;
        }
        for tag in tags {
            self.storage.update_sync_time("tags", &tag.id, sync_time).await
                .map_err(|e| SyncError::Local(e))?;
        }
        for note in notes {
            self.storage.update_sync_time("notes", &note.id, sync_time).await
                .map_err(|e| SyncError::Local(e))?;
        }
        Ok(())
    }

    /// List all remote items (Joplin stores all at root level as {id}.md)
    async fn list_remote_items(&self) -> Result<Vec<RemoteItem>> {
        let mut remote_items = Vec::new();

        match self.webdav.list(&self.context.remote_path).await {
            Ok(entries) => {
                tracing::info!("Scanning remote items in: {}", self.context.remote_path);
                for entry in entries {
                    if entry.path.ends_with(".md") &&
                       !entry.path.contains("/locks/") &&
                       !entry.path.contains("/.sync/") &&
                       !entry.path.contains("/.lock/") &&
                       !entry.path.contains("/temp/") &&
                       !entry.path.contains("/.resource/") {

                        if let Some(id) = entry.path.strip_suffix(".md")
                            .and_then(|path| path.rsplit('/').next()) {
                            // We'll determine type when downloading
                            remote_items.push(RemoteItem {
                                id: id.to_string(),
                                item_type: ItemType::Note, // placeholder, determined during download
                                modified: entry.modified,
                            });
                        }
                    }
                }
                tracing::info!("Found {} remote items", remote_items.len());
            }
            Err(e) => {
                tracing::warn!("Failed to list remote items: {}", e);
            }
        }

        // Also scan subdirectories for legacy compatibility
        for (subdir, item_type) in &[
            ("folders", ItemType::Folder),
            ("items", ItemType::Note),
            ("tags", ItemType::Tag),
            ("note_tags", ItemType::NoteTag),
            ("resources", ItemType::Resource),
        ] {
            let subdir_path = format!("{}/{}", self.context.remote_path.trim_end_matches('/'), subdir);
            if let Ok(entries) = self.webdav.list(&subdir_path).await {
                for entry in entries {
                    if let Some(id) = entry.path.strip_suffix(".md")
                        .and_then(|path| path.rsplit('/').next()) {
                        // Avoid duplicates
                        if !remote_items.iter().any(|r| r.id == id) {
                            remote_items.push(RemoteItem {
                                id: id.to_string(),
                                item_type: *item_type,
                                modified: entry.modified,
                            });
                        }
                    }
                }
            }
        }

        Ok(remote_items)
    }

    fn find_delta_items(&self, remote_items: &[RemoteItem], local_items: &[joplin_domain::SyncItem]) -> Vec<RemoteItem> {
        let mut items_to_download = Vec::new();

        for remote_item in remote_items {
            let local = local_items.iter().find(|l| l.item_id == remote_item.id);
            match local {
                None => {
                    // New item — not tracked locally
                    items_to_download.push(remote_item.clone());
                    tracing::debug!("New item {} marked for download", remote_item.id);
                }
                Some(sync_item) => {
                    // Existing item — check if remote is newer
                    if let Some(remote_modified) = remote_item.modified {
                        if remote_modified > sync_item.sync_time {
                            items_to_download.push(remote_item.clone());
                            tracing::debug!("Updated item {} marked for download (remote={}, local={})",
                                remote_item.id, remote_modified, sync_item.sync_time);
                        }
                    }
                }
            }
        }

        tracing::info!("Delta: {} items to download out of {} remote", items_to_download.len(), remote_items.len());
        items_to_download
    }

    async fn download_item(&self, item_id: &str, _item_type: &ItemType) -> Result<()> {
        // Try root level first (Joplin 3.5+ format), then subdirectories
        let root_path = format!("{}/{}.md", self.context.remote_path.trim_end_matches('/'), item_id);

        let final_path = if self.webdav.exists(&root_path).await.unwrap_or(false) {
            root_path
        } else {
            // Try subdirectories
            for subdir in &["items", "folders", "tags", "note_tags", "resources"] {
                let subdir_path = format!("{}/{}/{}.md", self.context.remote_path.trim_end_matches('/'), subdir, item_id);
                if self.webdav.exists(&subdir_path).await.unwrap_or(false) {
                    break;
                }
            }
            // Default to root
            format!("{}/{}.md", self.context.remote_path.trim_end_matches('/'), item_id)
        };

        let _ = self.event_tx.send(SyncEvent::ItemDownload {
            item_type: "item".to_string(),
            item_id: item_id.to_string(),
        });

        let mut reader = self.webdav.get(&final_path).await
            .map_err(|e| SyncError::Server(format!("Failed to download item {}: {}", item_id, e)))?;

        let mut content = Vec::new();
        reader.read_to_end(&mut content).await
            .map_err(|e| SyncError::Server(format!("Failed to read item {}: {}", item_id, e)))?;

        let content_str = String::from_utf8_lossy(&content);
        tracing::debug!("Downloaded {} bytes for {}", content.len(), item_id);

        self.store_downloaded_item(item_id, &content_str).await?;

        let _ = self.event_tx.send(SyncEvent::ItemDownloadComplete {
            item_type: "item".to_string(),
            item_id: item_id.to_string(),
        });

        Ok(())
    }

    /// Store a downloaded item, handling encryption and type detection
    async fn store_downloaded_item(&self, item_id: &str, content: &str) -> Result<()> {
        // Parse the metadata to determine type and encryption status
        let metadata = self.parse_item_metadata(content);

        let type_num = metadata.get("type_")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(1);

        let encryption_applied = metadata.get("encryption_applied")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);

        let encryption_cipher_text = metadata.get("encryption_cipher_text")
            .cloned()
            .unwrap_or_default();

        // Determine actual content to parse
        let actual_content = if encryption_applied == 1 && !encryption_cipher_text.is_empty() {
            // Item is encrypted — decrypt the cipher text to get the real content
            if let Some(ref e2ee) = self.e2ee_service {
                match e2ee.decrypt_string(&encryption_cipher_text) {
                    Ok(decrypted) => {
                        tracing::info!("Decrypted item {} (type {})", item_id, type_num);
                        decrypted
                    }
                    Err(e) => {
                        tracing::error!("Failed to decrypt item {}: {}", item_id, e);
                        return Err(SyncError::Server(format!(
                            "Failed to decrypt item {}: {}. Is the correct E2EE password set?", item_id, e
                        )).into());
                    }
                }
            } else {
                tracing::warn!("Item {} is encrypted but no E2EE service available", item_id);
                return Err(SyncError::Server(format!(
                    "Item {} is encrypted but E2EE is not configured", item_id
                )).into());
            }
        } else {
            content.to_string()
        };

        // Store based on type
        match type_num {
            1 => {
                // Note (type 1) — could also be a todo (is_todo field inside)
                let note = self.deserialize_note(item_id, &actual_content)?;
                let exists = matches!(self.storage.get_note(&note.id).await, Ok(Some(_)));
                if exists {
                    self.storage.update_note(&note).await.map_err(|e| SyncError::Local(e))?;
                } else {
                    self.storage.create_note(&note).await.map_err(|e| SyncError::Local(e))?;
                }
                // Update sync tracking
                self.storage.update_sync_time("notes", &note.id, now_ms()).await
                    .map_err(|e| SyncError::Local(e))?;
                tracing::info!("Stored note: {} ({})", note.title, note.id);
            }
            2 => {
                // Folder
                let folder = self.deserialize_folder(item_id, &actual_content)?;
                let exists = matches!(self.storage.get_folder(&folder.id).await, Ok(Some(_)));
                if exists {
                    self.storage.update_folder(&folder).await.map_err(|e| SyncError::Local(e))?;
                } else {
                    self.storage.create_folder(&folder).await.map_err(|e| SyncError::Local(e))?;
                }
                self.storage.update_sync_time("folders", &folder.id, now_ms()).await
                    .map_err(|e| SyncError::Local(e))?;
                tracing::info!("Stored folder: {} ({})", folder.title, folder.id);
            }
            5 => {
                // Tag
                let tag = self.deserialize_tag(item_id, &actual_content)?;
                let exists = matches!(self.storage.get_tag(&tag.id).await, Ok(Some(_)));
                if exists {
                    self.storage.update_tag(&tag).await.map_err(|e| SyncError::Local(e))?;
                } else {
                    self.storage.create_tag(&tag).await.map_err(|e| SyncError::Local(e))?;
                }
                self.storage.update_sync_time("tags", &tag.id, now_ms()).await
                    .map_err(|e| SyncError::Local(e))?;
                tracing::info!("Stored tag: {}", tag.id);
            }
            _ => {
                tracing::warn!("Unsupported item type {} for item {}", type_num, item_id);
            }
        }

        Ok(())
    }

    /// Parse metadata fields from a Joplin item file
    fn parse_item_metadata<'a>(&self, content: &'a str) -> std::collections::HashMap<String, String> {
        let mut metadata = std::collections::HashMap::new();
        for line in content.lines() {
            if let Some((key, value)) = line.split_once(": ") {
                metadata.insert(key.trim().to_string(), value.trim().to_string());
            } else if let Some((key, _)) = line.split_once(':') {
                // Handle "key:" with empty value
                metadata.insert(key.trim().to_string(), String::new());
            }
        }
        metadata
    }

    fn report_progress(&self, phase: SyncPhase, current: usize, total: usize, message: &str) {
        let _ = self.event_tx.send(SyncEvent::Progress {
            phase,
            current,
            total,
            message: message.to_string(),
        });
    }

    // === Serialization methods ===

    fn serialize_folder(&self, folder: &Folder) -> Result<String> {
        let mut content = format!("{}\n\n", folder.title);
        content.push_str(&format!("id: {}\n", folder.id));
        content.push_str(&format!("parent_id: {}\n", folder.parent_id));
        content.push_str(&format!("created_time: {}\n", self.ms_to_iso(folder.created_time)));
        content.push_str(&format!("updated_time: {}\n", self.ms_to_iso(folder.updated_time)));
        content.push_str(&format!("user_created_time: {}\n", self.ms_to_iso(folder.user_created_time)));
        content.push_str(&format!("user_updated_time: {}\n", self.ms_to_iso(folder.user_updated_time)));
        content.push_str(&format!("encryption_cipher_text: \n"));
        content.push_str(&format!("encryption_applied: 0\n"));
        content.push_str(&format!("is_shared: {}\n", folder.is_shared));
        content.push_str(&format!("share_id: {}\n", folder.share_id.as_deref().unwrap_or("")));
        content.push_str(&format!("master_key_id: {}\n", folder.master_key_id.as_deref().unwrap_or("")));
        content.push_str(&format!("icon: {}\n", folder.icon));
        content.push_str(&format!("user_data: \n"));
        content.push_str(&format!("deleted_time: 0\n"));
        content.push_str("type_: 2");
        Ok(content)
    }

    fn serialize_tag(&self, tag: &Tag) -> Result<String> {
        let mut content = format!("{}\n\n", tag.title);
        content.push_str(&format!("id: {}\n", tag.id));
        content.push_str(&format!("created_time: {}\n", self.ms_to_iso(tag.created_time)));
        content.push_str(&format!("updated_time: {}\n", self.ms_to_iso(tag.updated_time)));
        content.push_str(&format!("user_created_time: {}\n", self.ms_to_iso(tag.user_created_time)));
        content.push_str(&format!("user_updated_time: {}\n", self.ms_to_iso(tag.user_updated_time)));
        content.push_str(&format!("encryption_cipher_text: \n"));
        content.push_str(&format!("encryption_applied: 0\n"));
        content.push_str(&format!("is_shared: {}\n", tag.is_shared));
        content.push_str(&format!("parent_id: {}\n", tag.parent_id));
        content.push_str(&format!("user_data: \n"));
        content.push_str(&format!("deleted_time: 0\n"));
        content.push_str("type_: 5");
        Ok(content)
    }

    fn serialize_note_tag(&self, note_tag: &NoteTag) -> Result<String> {
        let mut content = String::new();
        content.push_str(&format!("id: {}\n", note_tag.id));
        content.push_str(&format!("note_id: {}\n", note_tag.note_id));
        content.push_str(&format!("tag_id: {}\n", note_tag.tag_id));
        content.push_str(&format!("created_time: {}\n", self.ms_to_iso(note_tag.created_time)));
        content.push_str(&format!("updated_time: {}\n", self.ms_to_iso(note_tag.updated_time)));
        content.push_str(&format!("user_created_time: {}\n", self.ms_to_iso(note_tag.created_time)));
        content.push_str(&format!("user_updated_time: {}\n", self.ms_to_iso(note_tag.updated_time)));
        content.push_str(&format!("encryption_cipher_text: \n"));
        content.push_str(&format!("encryption_applied: 0\n"));
        content.push_str(&format!("is_shared: {}\n", note_tag.is_shared));
        content.push_str(&format!("user_data: \n"));
        content.push_str(&format!("deleted_time: 0\n"));
        content.push_str("type_: 6");
        Ok(content)
    }

    fn serialize_note(&self, note: &Note) -> Result<String> {
        // Joplin format: title, blank line, body, then metadata
        let mut content = format!("{}\n", note.title);
        if !note.body.is_empty() {
            content.push_str(&format!("\n{}\n", note.body));
        }
        content.push('\n');

        content.push_str(&format!("id: {}\n", note.id));
        content.push_str(&format!("parent_id: {}\n", note.parent_id));
        content.push_str(&format!("created_time: {}\n", self.ms_to_iso(note.created_time)));
        content.push_str(&format!("updated_time: {}\n", self.ms_to_iso(note.updated_time)));
        content.push_str(&format!("is_conflict: {}\n", note.is_conflict));
        content.push_str(&format!("latitude: {:.8}\n", note.latitude as f64 / 1e7));
        content.push_str(&format!("longitude: {:.8}\n", note.longitude as f64 / 1e7));
        content.push_str(&format!("altitude: {:.4}\n", note.altitude as f64 / 1e2));
        content.push_str(&format!("author: {}\n", note.author));
        content.push_str(&format!("source_url: {}\n", note.source_url));
        content.push_str(&format!("is_todo: {}\n", note.is_todo));
        content.push_str(&format!("todo_due: {}\n", note.todo_due));
        content.push_str(&format!("todo_completed: {}\n", note.todo_completed));
        content.push_str(&format!("source: {}\n", note.source));
        content.push_str(&format!("source_application: {}\n", note.source_application));
        content.push_str(&format!("application_data: {}\n", note.application_data));
        content.push_str(&format!("order: {}\n", note.order));
        content.push_str(&format!("user_created_time: {}\n", self.ms_to_iso(note.user_created_time)));
        content.push_str(&format!("user_updated_time: {}\n", self.ms_to_iso(note.user_updated_time)));
        content.push_str(&format!("encryption_cipher_text: \n"));
        content.push_str(&format!("encryption_applied: 0\n"));
        content.push_str(&format!("markup_language: {}\n", note.markup_language));
        content.push_str(&format!("is_shared: {}\n", note.is_shared));
        content.push_str(&format!("share_id: {}\n", note.share_id.as_deref().unwrap_or("")));
        content.push_str(&format!("conflict_original_id: {}\n", note.conflict_original_id));
        content.push_str(&format!("master_key_id: {}\n", note.master_key_id.as_deref().unwrap_or("")));
        content.push_str(&format!("user_data: \n"));
        content.push_str(&format!("deleted_time: 0\n"));
        content.push_str("type_: 1");

        Ok(content)
    }

    // === Deserialization methods ===

    fn deserialize_note(&self, id: &str, content: &str) -> Result<Note> {
        let mut note = Note::default();
        note.id = id.to_string();

        // Joplin format: title\n\nbody\n\nid: ...\nparent_id: ...\n...
        // Split at the first metadata field (id:)
        let (text_part, props_part) = self.split_content_and_properties(content);

        // Title is the first line of text_part
        if let Some(first_line) = text_part.lines().next() {
            note.title = first_line.to_string();
        }

        // Body is everything after the title line (minus leading/trailing whitespace)
        let body_start = text_part.find('\n').map(|i| i + 1).unwrap_or(text_part.len());
        let body = text_part[body_start..].trim();
        if !body.is_empty() {
            note.body = body.to_string();
        }

        // Parse properties
        for line in props_part.lines() {
            if let Some((key, value)) = line.split_once(": ") {
                let value = value.trim();
                match key.trim() {
                    "id" => note.id = value.to_string(),
                    "parent_id" => note.parent_id = value.to_string(),
                    "created_time" => note.created_time = self.iso_to_ms(value).unwrap_or(0),
                    "updated_time" => note.updated_time = self.iso_to_ms(value).unwrap_or(0),
                    "user_created_time" => note.user_created_time = self.iso_to_ms(value).unwrap_or(0),
                    "user_updated_time" => note.user_updated_time = self.iso_to_ms(value).unwrap_or(0),
                    "is_conflict" => note.is_conflict = value.parse().unwrap_or(0),
                    "latitude" => note.latitude = (value.parse::<f64>().unwrap_or(0.0) * 1e7) as i64,
                    "longitude" => note.longitude = (value.parse::<f64>().unwrap_or(0.0) * 1e7) as i64,
                    "altitude" => note.altitude = (value.parse::<f64>().unwrap_or(0.0) * 1e2) as i64,
                    "author" => note.author = value.to_string(),
                    "source_url" => note.source_url = value.to_string(),
                    "is_todo" => note.is_todo = value.parse().unwrap_or(0),
                    "todo_due" => note.todo_due = value.parse().unwrap_or(0),
                    "todo_completed" => note.todo_completed = value.parse().unwrap_or(0),
                    "source" => note.source = value.to_string(),
                    "source_application" => note.source_application = value.to_string(),
                    "application_data" => note.application_data = value.to_string(),
                    "order" => note.order = value.parse().unwrap_or(0),
                    "encryption_applied" => note.encryption_applied = value.parse().unwrap_or(0),
                    "markup_language" => note.markup_language = value.parse().unwrap_or(1),
                    "is_shared" => note.is_shared = value.parse().unwrap_or(0),
                    "share_id" => note.share_id = if !value.is_empty() { Some(value.to_string()) } else { None },
                    "conflict_original_id" => note.conflict_original_id = value.to_string(),
                    "master_key_id" => note.master_key_id = if !value.is_empty() { Some(value.to_string()) } else { None },
                    _ => {}
                }
            }
        }

        Ok(note)
    }

    fn deserialize_folder(&self, id: &str, content: &str) -> Result<Folder> {
        let mut folder = Folder::default();
        folder.id = id.to_string();

        let (text_part, props_part) = self.split_content_and_properties(content);

        // Title is the first line
        if let Some(first_line) = text_part.lines().next() {
            folder.title = first_line.to_string();
        }

        for line in props_part.lines() {
            if let Some((key, value)) = line.split_once(": ") {
                let value = value.trim();
                match key.trim() {
                    "id" => folder.id = value.to_string(),
                    "title" => if folder.title.is_empty() { folder.title = value.to_string(); },
                    "parent_id" => folder.parent_id = value.to_string(),
                    "created_time" => folder.created_time = self.iso_to_ms(value).unwrap_or(0),
                    "updated_time" => folder.updated_time = self.iso_to_ms(value).unwrap_or(0),
                    "user_created_time" => folder.user_created_time = self.iso_to_ms(value).unwrap_or(0),
                    "user_updated_time" => folder.user_updated_time = self.iso_to_ms(value).unwrap_or(0),
                    "icon" => folder.icon = value.to_string(),
                    "is_shared" => folder.is_shared = value.parse().unwrap_or(0),
                    "share_id" => folder.share_id = if !value.is_empty() { Some(value.to_string()) } else { None },
                    "master_key_id" => folder.master_key_id = if !value.is_empty() { Some(value.to_string()) } else { None },
                    _ => {}
                }
            }
        }

        Ok(folder)
    }

    fn deserialize_tag(&self, id: &str, content: &str) -> Result<Tag> {
        // Try Joplin text format first
        let mut tag = Tag::default();
        tag.id = id.to_string();

        let (text_part, props_part) = self.split_content_and_properties(content);

        if let Some(first_line) = text_part.lines().next() {
            tag.title = first_line.to_string();
        }

        for line in props_part.lines() {
            if let Some((key, value)) = line.split_once(": ") {
                let value = value.trim();
                match key.trim() {
                    "id" => tag.id = value.to_string(),
                    "title" => if tag.title.is_empty() { tag.title = value.to_string(); },
                    "created_time" => tag.created_time = self.iso_to_ms(value).unwrap_or(0),
                    "updated_time" => tag.updated_time = self.iso_to_ms(value).unwrap_or(0),
                    "parent_id" => tag.parent_id = value.to_string(),
                    _ => {}
                }
            }
        }

        Ok(tag)
    }

    /// Split Joplin item content into text (title+body) and properties sections.
    /// Properties start after the last sequence of "key: value" lines ending with "type_: N".
    fn split_content_and_properties<'a>(&self, content: &'a str) -> (&'a str, &'a str) {
        // Find the start of the properties block.
        // In Joplin format, the properties are at the end, starting with "id: " line.
        // We look for "id: " followed by a UUID-like pattern.
        if let Some(props_start) = content.find("\nid: ") {
            let text_part = &content[..props_start];
            let props_part = &content[props_start + 1..]; // skip the newline
            (text_part, props_part)
        } else if content.starts_with("id: ") {
            // No text part, just properties
            ("", content)
        } else {
            // Can't find properties, treat all as text
            (content, "")
        }
    }

    fn ms_to_iso(&self, ms: i64) -> String {
        if ms == 0 {
            return String::new();
        }
        use chrono::DateTime;
        let secs = ms / 1000;
        let millis = (ms % 1000) as u32;
        DateTime::from_timestamp(secs, millis * 1_000_000)
            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
            .unwrap_or_default()
    }

    fn iso_to_ms(&self, iso: &str) -> Result<i64> {
        if iso.is_empty() || iso == "0" {
            return Ok(0);
        }
        use chrono::DateTime;
        let dt = DateTime::parse_from_rfc3339(iso)
            .map_err(|e| SyncError::Serialization(format!("Failed to parse timestamp {}: {}", iso, e)))?;
        Ok(dt.timestamp_millis())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_context_default() {
        let context = SyncContext::default();
        assert_eq!(context.last_sync_time, 0);
        assert_eq!(context.remote_path, "/neojoplin");
    }
}
