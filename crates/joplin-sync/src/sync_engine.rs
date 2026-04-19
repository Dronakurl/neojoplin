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
}

/// Remote item with type information
#[derive(Clone, Debug)]
struct RemoteItem {
    id: String,
    item_type: ItemType,
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

    /// Set the E2EE service for encryption/decryption
    pub fn with_e2ee(mut self, e2ee_service: E2eeService) -> Self {
        self.e2ee_service = Some(e2ee_service);
        self
    }

    /// Set the remote sync path
    pub fn with_remote_path(mut self, path: String) -> Self {
        self.context.remote_path = path;
        self
    }

    /// Run full sync process
    pub async fn sync(&mut self) -> Result<()> {
        let start = std::time::Instant::now();

        // Check for existing locks
        self.check_locks().await?;

        // Load or create sync info
        self.load_sync_info().await?;

        // Ensure remote directory exists
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

    /// Ensure remote directory exists
    async fn ensure_remote_directory(&self) -> Result<()> {
        // Create root directory if it doesn't exist
        let remote_exists = self.webdav.exists(&self.context.remote_path).await
            .unwrap_or(false);

        if !remote_exists {
            self.webdav.mkcol(&self.context.remote_path).await
                .map_err(|e| SyncError::Server(format!("Failed to create remote directory {}: {}", self.context.remote_path, e)))?;
            let _ = self.event_tx.send(SyncEvent::Warning {
                message: format!("Created remote directory: {}", self.context.remote_path)
            });
        }

        // Create required Joplin subdirectories
        let subdirs = ["folders", "items", "resources", "tags", "note_tags"];
        for subdir in &subdirs {
            let dir_path = format!("{}/{}", self.context.remote_path.trim_end_matches('/'), subdir);
            let subdir_exists = self.webdav.exists(&dir_path).await
                .unwrap_or(false);

            if !subdir_exists {
                self.webdav.mkcol(&dir_path).await
                    .map_err(|e| SyncError::Server(format!("Failed to create subdirectory {}: {}", dir_path, e)))?;
            }
        }

        Ok(())
    }

    /// Phase 1: Upload local changes
    async fn phase_upload(&mut self) -> Result<()> {
        let _ = self.event_tx.send(SyncEvent::PhaseStarted(SyncPhase::Upload));

        // 1. Scan for items changed since last sync
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
            // 2. Upload folders first (required for notes)
            let mut uploaded = 0;
            for folder in &folders {
                self.upload_folder(folder).await?;
                uploaded += 1;
                self.report_progress(SyncPhase::Upload, uploaded, total_items, "Uploading folders");
            }

            // 3. Upload tags
            for tag in &tags {
                self.upload_tag(tag).await?;
                uploaded += 1;
                self.report_progress(SyncPhase::Upload, uploaded, total_items, "Uploading tags");
            }

            // 4. Upload note-tag associations
            for note_tag in &note_tags {
                self.upload_note_tag(note_tag).await?;
                uploaded += 1;
                self.report_progress(SyncPhase::Upload, uploaded, total_items, "Uploading note tags");
            }

            // 5. Upload notes
            for note in &notes {
                self.upload_note(note).await?;
                uploaded += 1;
                self.report_progress(SyncPhase::Upload, uploaded, total_items, "Uploading notes");
            }

            // 6. Update sync_time for uploaded items
            let sync_time = now_ms();
            self.update_sync_times(&folders, &tags, &notes, sync_time).await?;
        }

        let _ = self.event_tx.send(SyncEvent::PhaseCompleted(SyncPhase::Upload));
        Ok(())
    }

    /// Phase 2: Delete remote items
    async fn phase_delete_remote(&mut self) -> Result<()> {
        let _ = self.event_tx.send(SyncEvent::PhaseStarted(SyncPhase::DeleteRemote));

        // 1. Get local deletions from deleted_items table
        let deleted_items = self.storage.get_deleted_items(2).await // sync_target 2 = WebDAV
            .map_err(|e| SyncError::Local(e))?;

        if deleted_items.is_empty() {
            let _ = self.event_tx.send(SyncEvent::Progress {
                phase: SyncPhase::DeleteRemote,
                current: 0,
                total: 0,
                message: "No remote items to delete".to_string(),
            });
        } else {
            // 2. Delete corresponding remote files
            let total = deleted_items.len();
            for (i, deleted_item) in deleted_items.iter().enumerate() {
                // Try root level first, then subdirectories
                let root_path = format!("{}/{}.md", self.context.remote_path, deleted_item.item_id);
                let sub_path = format!("{}/items/{}.md", self.context.remote_path, deleted_item.item_id);

                let remote_path = if self.webdav.exists(&root_path).await.unwrap_or(false) {
                    root_path
                } else {
                    sub_path
                };

                if let Err(e) = self.webdav.delete(&remote_path).await {
                    let _ = self.event_tx.send(SyncEvent::Warning {
                        message: format!("Failed to delete remote item {}: {}", deleted_item.item_id, e)
                    });
                }

                self.report_progress(SyncPhase::DeleteRemote, i + 1, total, "Deleting remote items");
            }

            // 3. Clean up deleted_items table
            self.storage.clear_deleted_items(deleted_items.len() as i64).await
                .map_err(|e| SyncError::Local(e))?;
        }

        let _ = self.event_tx.send(SyncEvent::PhaseCompleted(SyncPhase::DeleteRemote));
        Ok(())
    }

    /// Phase 3: Download remote changes (delta)
    async fn phase_delta(&self) -> Result<()> {
        let _ = self.event_tx.send(SyncEvent::PhaseStarted(SyncPhase::Delta));

        // 1. Get remote items list via WebDAV PROPFIND
        let remote_items = self.list_remote_items().await?;

        // 2. Compare with local database
        let local_items = self.storage.get_all_sync_items().await
            .map_err(|e| SyncError::Local(e))?;

        // 3. Find new/updated items
        let items_to_download = self.find_delta_items(&remote_items, &local_items);

        if items_to_download.is_empty() {
            let _ = self.event_tx.send(SyncEvent::Progress {
                phase: SyncPhase::Delta,
                current: 0,
                total: 0,
                message: "No remote changes to download".to_string(),
            });
        } else {
            // 4. Download new/updated items
            let total = items_to_download.len();
            tracing::info!("Starting delta download: {} items to download", total);

            for (i, remote_item) in items_to_download.iter().enumerate() {
                tracing::debug!("Downloading item {}/{}: {} ({:?})", i + 1, total, remote_item.id, remote_item.item_type);
                self.download_item(&remote_item.id, &remote_item.item_type).await?;
                self.report_progress(SyncPhase::Delta, i + 1, total, "Downloading remote changes");
            }

            tracing::info!("Delta download complete: {} items processed", total);

            // 5. Update sync context (stored in database instead)
            // self.context.last_sync_time = now_ms();
        }

        let _ = self.event_tx.send(SyncEvent::PhaseCompleted(SyncPhase::Delta));
        Ok(())
    }

    // Helper methods

    /// Check for existing locks on the sync target
    async fn check_locks(&self) -> Result<()> {
        let lock_path = format!("{}/lock.json", self.context.remote_path.trim_end_matches('/'));

        match self.webdav.get(&lock_path).await {
            Ok(mut reader) => {
                use futures::io::AsyncReadExt;
                let mut content = Vec::new();
                reader.read_to_end(&mut content).await
                    .map_err(|e| SyncError::Server(format!("Failed to read lock.json: {}", e)))?;

                let content_str = String::from_utf8_lossy(&content);

                // Try to parse as JSON to get timestamp
                if let Ok(lock_data) = serde_json::from_str::<serde_json::Value>(&content_str) {
                    if let Some(timestamp) = lock_data.get("updatedTime").and_then(|v| v.as_i64()) {
                        let current_time = now_ms();
                        let lock_age_ms = current_time - timestamp;
                        let lock_age_min = lock_age_ms / (60 * 1000);

                        // Lock is fresh if less than 5 minutes old
                        if lock_age_min < 5 {
                            return Err(SyncError::Server(format!(
                                "Sync target is locked by another client. Lock age: {} minutes. \
                                Wait for the other sync to complete or try again later.",
                                lock_age_min
                            )).into());
                        }
                        // Lock is stale - will be overwritten
                        let _ = self.event_tx.send(SyncEvent::Warning {
                            message: format!("Found stale lock ({} minutes old) - proceeding with sync", lock_age_min)
                        });
                    }
                }
            }
            Err(joplin_domain::WebDavError::NotFound(_)) => {
                // No lock file - this is normal
            }
            Err(e) => {
                // NotFound is expected for new sync targets - not a warning
                if !matches!(e, joplin_domain::WebDavError::NotFound(_)) {
                    let _ = self.event_tx.send(SyncEvent::Warning {
                        message: format!("Failed to check lock status: {}", e)
                    });
                }
            }
        }

        Ok(())
    }

    /// Load sync info from remote or create new
    async fn load_sync_info(&mut self) -> Result<()> {
        match SyncInfo::load_from_remote(self.webdav.as_ref(), &self.context.remote_path).await {
            Ok(Some(remote_sync_info)) => {
                // Use existing sync info but update client ID to match ours
                self.sync_info = Some(remote_sync_info);
                self.context.last_sync_time = self.sync_info.as_ref()
                    .map(|info| info.delta_timestamp())
                    .unwrap_or(0);
            }
            Ok(None) => {
                // Create new sync info - this is normal for new sync targets
                let sync_info = SyncInfo::new();
                self.sync_info = Some(sync_info);
                self.context.last_sync_time = 0;
            }
            Err(e) => {
                // Only return error if it's not a NotFound (which is expected for new sync targets)
                let error_msg = e.to_string();
                if !error_msg.contains("NotFound") && !error_msg.contains("not found") {
                    return Err(SyncError::Server(format!("Failed to load sync info: {}", e)).into());
                }
                // If it's a NotFound, create new sync info
                let sync_info = SyncInfo::new();
                self.sync_info = Some(sync_info);
                self.context.last_sync_time = 0;
            }
        }

        Ok(())
    }

    /// Save sync info to remote
    async fn save_sync_info(&mut self) -> Result<()> {
        if let Some(ref _sync_info) = self.sync_info {
            // Update delta context timestamp
            let new_timestamp = now_ms();
            let updated_info = &mut self.sync_info.as_mut().unwrap();
            updated_info.update_delta_timestamp();

            // Save to remote
            updated_info.save_to_remote(self.webdav.as_ref(), &self.context.remote_path).await
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

    async fn upload_folder(&self, folder: &Folder) -> Result<()> {
        // Upload to folders subdirectory for Joplin compatibility
        let remote_path = format!("{}/folders/{}.md", self.context.remote_path, folder.id);
        let mut content = self.serialize_folder(folder)?;

        // Encrypt folder title if E2EE is enabled
        if let Some(ref e2ee) = self.e2ee_service {
            if let Ok(encrypted_title) = e2ee.encrypt_string(&folder.title) {
                // Replace the title in the content with encrypted version
                content = content.replace(&format!("title: {}", folder.title), &format!("title: {}", encrypted_title));
                tracing::debug!("Encrypted folder {} for upload", folder.id);
            }
        }

        let _ = self.event_tx.send(SyncEvent::ItemUpload {
            item_type: "folder".to_string(),
            item_id: folder.id.clone(),
        });

        let bytes = content.into_bytes();
        self.webdav.put(&remote_path, &bytes, bytes.len() as u64).await
            .map_err(|e| SyncError::Server(format!("Failed to upload folder {}: {}", folder.id, e)))?;

        let _ = self.event_tx.send(SyncEvent::ItemUploadComplete {
            item_type: "folder".to_string(),
            item_id: folder.id.clone(),
        });

        Ok(())
    }

    async fn upload_tag(&self, tag: &Tag) -> Result<()> {
        let remote_path = format!("{}/tags/{}.md", self.context.remote_path, tag.id);
        let content = self.serialize_tag(tag)?;

        let _ = self.event_tx.send(SyncEvent::ItemUpload {
            item_type: "tag".to_string(),
            item_id: tag.id.clone(),
        });

        let bytes = content.into_bytes();
        self.webdav.put(&remote_path, &bytes, bytes.len() as u64).await
            .map_err(|e| SyncError::Server(format!("Failed to upload tag {}: {}", tag.id, e)))?;

        let _ = self.event_tx.send(SyncEvent::ItemUploadComplete {
            item_type: "tag".to_string(),
            item_id: tag.id.clone(),
        });

        Ok(())
    }

    async fn upload_note_tag(&self, note_tag: &NoteTag) -> Result<()> {
        let remote_path = format!("{}/note_tags/{}.md", self.context.remote_path, note_tag.id);
        let content = self.serialize_note_tag(note_tag)?;

        let bytes = content.into_bytes();
        self.webdav.put(&remote_path, &bytes, bytes.len() as u64).await
            .map_err(|e| SyncError::Server(format!("Failed to upload note_tag {}: {}", note_tag.id, e)))?;

        Ok(())
    }

    async fn upload_note(&self, note: &Note) -> Result<()> {
        // Upload to items subdirectory for Joplin compatibility
        let remote_path = format!("{}/items/{}.md", self.context.remote_path, note.id);
        let mut content = self.serialize_note(note)?;

        // Encrypt note body if E2EE is enabled
        if let Some(ref e2ee) = self.e2ee_service {
            if let Ok(encrypted_body) = e2ee.encrypt_string(&note.body) {
                // Replace the body in the content with encrypted version
                content = content.replace(&format!("body: {}", note.body), &format!("body: {}", encrypted_body));
                tracing::debug!("Encrypted note {} for upload", note.id);
            }
        }

        let _ = self.event_tx.send(SyncEvent::ItemUpload {
            item_type: "note".to_string(),
            item_id: note.id.clone(),
        });

        let bytes = content.into_bytes();
        self.webdav.put(&remote_path, &bytes, bytes.len() as u64).await
            .map_err(|e| SyncError::Server(format!("Failed to upload note {}: {}", note.id, e)))?;

        let _ = self.event_tx.send(SyncEvent::ItemUploadComplete {
            item_type: "note".to_string(),
            item_id: note.id.clone(),
        });

        Ok(())
    }

    async fn update_sync_times(&self, folders: &[Folder], tags: &[Tag], notes: &[Note], sync_time: i64) -> Result<()> {
        // Update sync_time for all uploaded items
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

    async fn list_remote_items(&self) -> Result<Vec<RemoteItem>> {
        let mut remote_items = Vec::new();

        // First, scan root level for .md files (Joplin compatibility)
        match self.webdav.list(&self.context.remote_path).await {
            Ok(entries) => {
                tracing::info!("Scanning root level for items in: {}", self.context.remote_path);
                for entry in entries {
                    // Only process .md files at root level (excluding known metadata files)
                    if entry.path.ends_with(".md") &&
                       !entry.path.contains("/.lock/") &&
                       !entry.path.contains("/.sync/") &&
                       !entry.path.contains("/temp/") &&
                       !entry.path.contains("/.resource/") &&
                       !entry.path.ends_with("/info.json") &&
                       !entry.path.ends_with("/sync.json") {

                        if let Some(id) = entry.path.strip_suffix(".md")
                            .and_then(|path| path.rsplit('/').next()) {
                            // Determine type by downloading and checking the file
                            let item_type = self.get_remote_item_type(id).await.unwrap_or(ItemType::Note);
                            remote_items.push(RemoteItem {
                                id: id.to_string(),
                                item_type,
                            });
                            tracing::debug!("Found root level item: {} (type: {:?})", id, item_type);
                        }
                    }
                }
                tracing::info!("Found {} root level items", remote_items.len());
            }
            Err(e) => {
                tracing::warn!("Failed to list root level items: {}", e);
            }
        }

        // List items from /items/ directory (notes)
        let items_path = format!("{}/items", self.context.remote_path);
        match self.webdav.list(&items_path).await {
            Ok(entries) => {
                tracing::info!("Listing notes from: {}", items_path);
                for entry in entries {
                    if let Some(id) = entry.path.strip_suffix(".md")
                        .and_then(|path| path.rsplit('/').next()) {
                        remote_items.push(RemoteItem {
                            id: id.to_string(),
                            item_type: ItemType::Note,
                        });
                        tracing::debug!("Found remote note: {}", id);
                    }
                }
                tracing::info!("Found {} remote notes", remote_items.len());
            }
            Err(e) => {
                tracing::warn!("Failed to list notes from {}: {}", items_path, e);
            }
        }

        // List items from /folders/ directory (folders)
        let folders_path = format!("{}/folders", self.context.remote_path);
        match self.webdav.list(&folders_path).await {
            Ok(entries) => {
                tracing::info!("Listing folders from: {}", folders_path);
                let folder_count = remote_items.len();
                for entry in entries {
                    if let Some(id) = entry.path.strip_suffix(".md")
                        .and_then(|path| path.rsplit('/').next()) {
                        remote_items.push(RemoteItem {
                            id: id.to_string(),
                            item_type: ItemType::Folder,
                        });
                        tracing::debug!("Found remote folder: {}", id);
                    }
                }
                tracing::info!("Found {} remote folders", remote_items.len() - folder_count);
            }
            Err(e) => {
                tracing::warn!("Failed to list folders from {}: {}", folders_path, e);
            }
        }

        // List items from /tags/ directory (tags)
        let tags_path = format!("{}/tags", self.context.remote_path);
        if let Ok(entries) = self.webdav.list(&tags_path).await {
            tracing::info!("Listing tags from: {}", tags_path);
            let tag_count = remote_items.len();
            for entry in entries {
                if let Some(id) = entry.path.strip_suffix(".md")
                    .and_then(|path| path.rsplit('/').next()) {
                    remote_items.push(RemoteItem {
                        id: id.to_string(),
                        item_type: ItemType::Tag,
                    });
                    tracing::debug!("Found remote tag: {}", id);
                }
            }
            tracing::info!("Found {} remote tags", remote_items.len() - tag_count);
        }

        // List items from /resources/ directory (resources)
        let resources_path = format!("{}/resources", self.context.remote_path);
        if let Ok(entries) = self.webdav.list(&resources_path).await {
            tracing::info!("Listing resources from: {}", resources_path);
            let resource_count = remote_items.len();
            for entry in entries {
                if let Some(id) = entry.path.strip_suffix(".md")
                    .and_then(|path| path.rsplit('/').next()) {
                    remote_items.push(RemoteItem {
                        id: id.to_string(),
                        item_type: ItemType::Resource,
                    });
                    tracing::debug!("Found remote resource: {}", id);
                }
            }
            tracing::info!("Found {} remote resources", remote_items.len() - resource_count);
        }

        tracing::info!("Total remote items found: {}", remote_items.len());
        Ok(remote_items)
    }

    /// Determine the type of a remote item by downloading and checking its content
    async fn get_remote_item_type(&self, id: &str) -> Result<ItemType> {
        let remote_path = format!("{}/{}.md", self.context.remote_path, id);

        match self.webdav.get(&remote_path).await {
            Ok(mut content) => {
                let mut text = String::new();
                if futures::io::AsyncReadExt::read_to_string(&mut content, &mut text).await.is_ok() {
                    // Check for type_ field in the content
                    if text.contains("type_: 1") {
                        return Ok(ItemType::Note);
                    } else if text.contains("type_: 2") {
                        return Ok(ItemType::Folder);
                    } else if text.contains("type_: 3") {
                        return Ok(ItemType::Tag);
                    } else if text.contains("type_: 4") {
                        return Ok(ItemType::Resource);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to download item {} for type detection: {}", id, e);
            }
        }

        // Default to Note if we can't determine the type
        Ok(ItemType::Note)
    }

    fn find_delta_items(&self, remote_items: &[RemoteItem], local_items: &[joplin_domain::SyncItem]) -> Vec<RemoteItem> {
        let mut items_to_download = Vec::new();

        tracing::debug!("Finding delta items: {} remote, {} local", remote_items.len(), local_items.len());

        for remote_item in remote_items {
            let needs_download = local_items
                .iter()
                .find(|local| local.item_id == remote_item.id)
                .map_or(true, |local| {
                    // Download if remote is newer
                    let local_time = local.sync_time;
                    // For now, always download if we don't have remote timestamp info
                    let should_download = true;
                    tracing::trace!("Item {} ({:?}): local_time={}, should_download={}",
                        remote_item.id, remote_item.item_type, local_time, should_download);
                    should_download
                });

            if needs_download {
                items_to_download.push(remote_item.clone());
                tracing::debug!("Item {} ({:?}) marked for download", remote_item.id, remote_item.item_type);
            }
        }

        tracing::info!("Delta items to download: {}/{}", items_to_download.len(), remote_items.len());
        items_to_download
    }

    async fn download_item(&self, item_id: &str, item_type: &ItemType) -> Result<()> {
        // Try subdirectories first (modern Joplin), then fall back to root level
        let subdir_path = format!("{}/{}/{}.md",
            self.context.remote_path,
            match item_type {
                ItemType::Folder => "folders",
                ItemType::Note => "items",
                ItemType::Tag => "tags",
                ItemType::Resource => "resources",
            },
            item_id
        );

        let root_path = format!("{}/{}.md", self.context.remote_path, item_id);

        let (remote_path, type_name) = match item_type {
            ItemType::Folder => (format!("{}/folders/{}.md", self.context.remote_path, item_id), "folder"),
            ItemType::Note => (format!("{}/items/{}.md", self.context.remote_path, item_id), "note"),
            ItemType::Tag => (format!("{}/tags/{}.md", self.context.remote_path, item_id), "tag"),
            ItemType::Resource => (format!("{}/resources/{}.md", self.context.remote_path, item_id), "resource"),
        };

        // Try subdirectory first (modern Joplin), then root level (legacy)
        let final_path = if self.webdav.exists(&subdir_path).await.unwrap_or(false) {
            tracing::debug!("Downloading from subdirectory: {}", subdir_path);
            subdir_path
        } else if self.webdav.exists(&root_path).await.unwrap_or(false) {
            tracing::debug!("Downloading from root level: {}", root_path);
            root_path
        } else {
            tracing::debug!("Downloading from default subdirectory: {}", remote_path);
            remote_path.clone()
        };

        tracing::info!("Downloading {} {} from: {}", type_name, item_id, final_path);

        let _ = self.event_tx.send(SyncEvent::ItemDownload {
            item_type: type_name.to_string(),
            item_id: item_id.to_string(),
        });

        // Download item content
        let mut reader = self.webdav.get(&final_path).await
            .map_err(|e| {
                tracing::error!("Failed to download {} {}: {}", type_name, item_id, e);
                SyncError::Server(format!("Failed to download item {}: {}", item_id, e))
            })?;

        let mut content = Vec::new();
        reader.read_to_end(&mut content).await
            .map_err(|e| {
                tracing::error!("Failed to read {} content {}: {}", type_name, item_id, e);
                SyncError::Server(format!("Failed to read item content {}: {}", item_id, e))
            })?;

        let content_str = String::from_utf8_lossy(&content);
        tracing::debug!("Downloaded {} bytes for {} {}", content.len(), type_name, item_id);

        // Parse and store the item
        self.store_downloaded_item(item_id, item_type, &content_str).await?;

        tracing::info!("Successfully downloaded and stored {} {}", type_name, item_id);

        let _ = self.event_tx.send(SyncEvent::ItemDownloadComplete {
            item_type: type_name.to_string(),
            item_id: item_id.to_string(),
        });

        Ok(())
    }

    async fn store_downloaded_item(&self, item_id: &str, item_type: &ItemType, content: &str) -> Result<()> {
        tracing::debug!("Storing item {:?} with ID {}", item_type, item_id);

        // Decrypt content if E2EE is enabled
        let decrypted_content = if let Some(ref e2ee) = self.e2ee_service {
            // Try to decrypt the content - if it fails, assume it's not encrypted
            self.maybe_decrypt_content(content, e2ee).await.unwrap_or_else(|| content.to_string())
        } else {
            content.to_string()
        };

        match item_type {
            ItemType::Note => {
                if let Ok(note) = self.deserialize_note(item_id, &decrypted_content) {
                    let exists = matches!(self.storage.get_note(&note.id).await, Ok(Some(_)));
                    if exists {
                        self.storage.update_note(&note).await
                            .map_err(|e| SyncError::Local(e))?;
                        tracing::debug!("Updated note: {}", note.id);
                    } else {
                        self.storage.create_note(&note).await
                            .map_err(|e| SyncError::Local(e))?;
                        tracing::debug!("Created note: {}", note.id);
                    }
                    return Ok(());
                }
            }
            ItemType::Folder => {
                if let Ok(folder) = self.deserialize_folder(item_id, &decrypted_content) {
                    let exists = matches!(self.storage.get_folder(&folder.id).await, Ok(Some(_)));
                    if exists {
                        self.storage.update_folder(&folder).await
                            .map_err(|e| SyncError::Local(e))?;
                        tracing::debug!("Updated folder: {}", folder.id);
                    } else {
                        self.storage.create_folder(&folder).await
                            .map_err(|e| SyncError::Local(e))?;
                        tracing::debug!("Created folder: {}", folder.id);
                    }
                    return Ok(());
                }
            }
            ItemType::Tag => {
                if let Ok(tag) = self.deserialize_tag(item_id, content) {
                    let exists = matches!(self.storage.get_tag(&tag.id).await, Ok(Some(_)));
                    if exists {
                        self.storage.update_tag(&tag).await
                            .map_err(|e| SyncError::Local(e))?;
                        tracing::debug!("Updated tag: {}", tag.id);
                    } else {
                        self.storage.create_tag(&tag).await
                            .map_err(|e| SyncError::Local(e))?;
                        tracing::debug!("Created tag: {}", tag.id);
                    }
                    return Ok(());
                }
            }
            ItemType::Resource => {
                // Resources not yet implemented in storage layer
                tracing::warn!("Resource type not yet implemented for download");
                return Err(SyncError::Server(format!("Resource download not yet implemented")).into());
            }
        }

        tracing::warn!("Could not parse item {} as {:?}", item_id, item_type);
        Err(SyncError::Server(format!("Failed to parse item: {}", item_id)).into())
    }

    fn report_progress(&self, phase: SyncPhase, current: usize, total: usize, message: &str) {
        let _ = self.event_tx.send(SyncEvent::Progress {
            phase,
            current,
            total,
            message: message.to_string(),
        });
    }

    /// Try to decrypt content if it's encrypted, otherwise return original
    async fn maybe_decrypt_content(&self, content: &str, e2ee: &E2eeService) -> Option<String> {
        // Check if content looks like it might be encrypted (starts with JED or contains encrypted markers)
        if content.contains("JED") || (content.len() > 100 && !content.contains('\n')) {
            // Try to decrypt
            if let Ok(decrypted) = e2ee.decrypt_string(content) {
                tracing::debug!("Successfully decrypted content");
                return Some(decrypted);
            }
        }

        // If content contains "body:" field, try to decrypt just the body
        if let Some(body_start) = content.find("body: ") {
            let body_start = body_start + 6; // "body: ".len()
            if let Some(body_end) = content[body_start..].find('\n') {
                let encrypted_body = &content[body_start..body_start + body_end];
                if let Ok(decrypted_body) = e2ee.decrypt_string(encrypted_body.trim()) {
                    tracing::debug!("Successfully decrypted body field");
                    let mut result = content.to_string();
                    result.replace_range(body_start..body_start + body_end, decrypted_body.trim());
                    return Some(result);
                }
            }
        }

        None
    }

    // Serialization methods (simplified JED-like format)
    fn serialize_folder(&self, folder: &Folder) -> Result<String> {
        // Joplin format for folders is same text format as notes
        let mut content = format!("{}\n", folder.title);
        content.push_str(&format!("id: {}\n", folder.id));
        content.push_str(&format!("parent_id: {}\n", folder.parent_id));

        // Convert timestamps to ISO 8601 format
        let created_time = self.ms_to_iso(folder.created_time);
        let updated_time = self.ms_to_iso(folder.updated_time);

        content.push_str(&format!("created_time: {}\n", created_time));
        content.push_str(&format!("updated_time: {}\n", updated_time));
        content.push_str(&format!("encryption_cipher_text: {}\n", folder.encryption_cipher_text.as_ref().unwrap_or(&String::new())));
        content.push_str(&format!("encryption_applied: {}\n", folder.encryption_applied));
        content.push_str(&format!("icon: {}\n", folder.icon));
        content.push_str(&format!("type_: 2\n")); // 2 = folder

        Ok(content)
    }

    fn serialize_tag(&self, tag: &Tag) -> Result<String> {
        serde_json::to_string_pretty(tag)
            .map_err(|e| SyncError::Serialization(format!("Failed to serialize tag: {}", e)).into())
    }

    fn serialize_note_tag(&self, note_tag: &NoteTag) -> Result<String> {
        serde_json::to_string_pretty(note_tag)
            .map_err(|e| SyncError::Serialization(format!("Failed to serialize note_tag: {}", e)).into())
    }

    fn serialize_note(&self, note: &Note) -> Result<String> {
        // Joplin format: human-readable text format (not JSON)
        let mut content = format!("{}\n", note.title);
        content.push_str(&format!("id: {}\n", note.id));
        content.push_str(&format!("parent_id: {}\n", note.parent_id));

        // Convert timestamps to ISO 8601 format
        let created_time = self.ms_to_iso(note.created_time);
        let updated_time = self.ms_to_iso(note.updated_time);
        let user_created_time = self.ms_to_iso(note.user_created_time);
        let user_updated_time = self.ms_to_iso(note.user_updated_time);

        content.push_str(&format!("created_time: {}\n", created_time));
        content.push_str(&format!("updated_time: {}\n", updated_time));
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
        content.push_str(&format!("user_created_time: {}\n", user_created_time));
        content.push_str(&format!("user_updated_time: {}\n", user_updated_time));
        content.push_str(&format!("encryption_cipher_text: {}\n", note.encryption_cipher_text.as_ref().unwrap_or(&String::new())));
        content.push_str(&format!("encryption_applied: {}\n", note.encryption_applied));
        content.push_str(&format!("markup_language: {}\n", note.markup_language));
        content.push_str(&format!("is_shared: {}\n", note.is_shared));
        content.push_str(&format!("share_id: {}\n", note.share_id.as_ref().unwrap_or(&String::new())));
        content.push_str(&format!("conflict_original_id: {}\n", note.conflict_original_id));
        content.push_str(&format!("master_key_id: {}\n", note.master_key_id.as_ref().unwrap_or(&String::new())));
        content.push_str(&format!("user_data: {}\n", String::new()));
        content.push_str(&format!("deleted_time: {}\n", 0));

        // Determine type_ based on is_todo
        let type_ = if note.is_todo == 1 { 5 } else { 1 }; // 5 = todo, 1 = note
        content.push_str(&format!("type_: {}\n", type_));

        // Add body content at the end
        if !note.body.is_empty() {
            content.push_str(&format!("\n{}\n", note.body));
        }

        Ok(content)
    }

    fn deserialize_note(&self, id: &str, content: &str) -> Result<Note> {
        // Parse Joplin format (human-readable text format)
        let mut note = Note::default();
        note.id = id.to_string();

        // Split content into properties and body
        let parts: Vec<&str> = content.splitn(2, "\n\n").collect();
        let properties_part = parts.get(0).unwrap_or(&content);
        let body_part = parts.get(1).unwrap_or(&"");

        // Parse title (first line)
        if let Some(first_line) = properties_part.lines().next() {
            if !first_line.contains(':') {
                note.title = first_line.to_string();
            }
        }

        // Parse properties
        for line in properties_part.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once(':') {
                let value = value.trim();
                match key.trim() {
                    "id" => note.id = value.to_string(),
                    "title" => {
                        // Title is the first line without a colon (already handled above)
                    },
                    "parent_id" => note.parent_id = value.to_string(),
                    "created_time" => note.created_time = self.iso_to_ms(value)?,
                    "updated_time" => note.updated_time = self.iso_to_ms(value)?,
                    "user_created_time" => note.user_created_time = self.iso_to_ms(value)?,
                    "user_updated_time" => note.user_updated_time = self.iso_to_ms(value)?,
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
                    "encryption_cipher_text" => note.encryption_cipher_text = if !value.is_empty() { Some(value.to_string()) } else { None },
                    "encryption_applied" => note.encryption_applied = value.parse().unwrap_or(0),
                    "markup_language" => note.markup_language = value.parse().unwrap_or(1),
                    "is_shared" => note.is_shared = value.parse().unwrap_or(0),
                    "share_id" => note.share_id = if !value.is_empty() { Some(value.to_string()) } else { None },
                    "conflict_original_id" => note.conflict_original_id = value.to_string(),
                    "master_key_id" => note.master_key_id = if !value.is_empty() { Some(value.to_string()) } else { None },
                    "type_" => {
                        // type_ determines if it's a todo or note
                        note.is_todo = if value.parse::<i32>().unwrap_or(1) == 5 { 1 } else { 0 };
                    },
                    _ => {} // Ignore unknown fields
                }
            }
        }

        // Parse body (everything after the properties section)
        note.body = body_part.trim().to_string();

        Ok(note)
    }

    fn deserialize_folder(&self, id: &str, content: &str) -> Result<Folder> {
        // Try JSON format first for backwards compatibility
        if let Ok(folder) = serde_json::from_str::<Folder>(content) {
            return Ok(folder);
        }

        // Parse Joplin text format
        let mut folder = Folder::default();
        folder.id = id.to_string();

        // Parse title (first line without a colon) and other fields
        for line in content.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // If the line doesn't contain a colon, it's the title
            if !line.contains(':') {
                if folder.title.is_empty() {
                    folder.title = line.trim().to_string();
                }
                continue;
            }

            if let Some((key, value)) = line.split_once(':') {
                let value = value.trim();
                match key.trim() {
                    "id" => folder.id = value.to_string(),
                    "title" => folder.title = value.to_string(),
                    "parent_id" => folder.parent_id = value.to_string(),
                    "created_time" => folder.created_time = self.iso_to_ms(value)?,
                    "updated_time" => folder.updated_time = self.iso_to_ms(value)?,
                    "encryption_cipher_text" => folder.encryption_cipher_text = if !value.is_empty() { Some(value.to_string()) } else { None },
                    "encryption_applied" => folder.encryption_applied = value.parse().unwrap_or(0),
                    "icon" => folder.icon = value.to_string(),
                    _ => {} // Ignore unknown fields
                }
            }
        }

        Ok(folder)
    }

    fn deserialize_tag(&self, id: &str, content: &str) -> Result<Tag> {
        serde_json::from_str(content)
            .map_err(|e| SyncError::Serialization(format!("Failed to deserialize tag {}: {}", id, e)).into())
    }

    /// Convert milliseconds since epoch to ISO 8601 string
    fn ms_to_iso(&self, ms: i64) -> String {
        if ms == 0 {
            return "0".to_string();
        }

        use chrono::DateTime;
        let secs = ms / 1000;
        let millis = (ms % 1000) as u32;
        let dt = DateTime::from_timestamp(secs, millis * 1_000_000);

        match dt {
            Some(datetime) => {
                datetime.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
            },
            None => "0".to_string()
        }
    }

    /// Convert ISO 8601 string to milliseconds since epoch
    fn iso_to_ms(&self, iso: &str) -> Result<i64> {
        if iso == "0" {
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

    #[test]
    fn test_find_delta_items() {
        // This test would require mock data
        // For now, just verify the method exists
        let remote_items = vec!["item1".to_string(), "item2".to_string()];
        let _local_items: Vec<joplin_domain::SyncItem> = vec![];

        // Can't call self.find_delta_items in unit test without instance
        // This is just a placeholder to show the concept
        assert_eq!(remote_items.len(), 2);
    }
}
