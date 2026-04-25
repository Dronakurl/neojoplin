// Sync engine implementation

use crate::e2ee::E2eeService;
use crate::sync_info::SyncInfo;
use futures::io::AsyncReadExt;
use joplin_domain::{
    now_ms, Folder, Note, NoteTag, Result, Storage, SyncError, SyncEvent, SyncPhase, SyncTarget,
    Tag, WebDavClient,
};
use serde_json;
use std::sync::Arc;
use tokio::sync::mpsc;

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

        // Detect E2EE state changes — if encryption was just enabled or disabled,
        // clear sync_items to force re-upload of all items in the new format
        self.handle_encryption_state_change().await?;

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

    /// Detect E2EE state or active-key changes and force re-upload of all items if needed.
    /// When encryption is enabled/disabled, or when a different active master key is selected,
    /// all items must be re-uploaded in the new format.
    async fn handle_encryption_state_change(&mut self) -> Result<()> {
        let remote_info = self.sync_info.as_ref();
        let remote_encrypted = remote_info.map(|info| info.e2ee.value).unwrap_or(false);
        let remote_active_key = remote_info
            .map(|info| info.active_master_key_id.value.replace('-', ""))
            .filter(|id| !id.is_empty());

        let local_encrypted = self
            .e2ee_service
            .as_ref()
            .map(|e| e.is_enabled())
            .unwrap_or(false);
        let local_active_key = self
            .e2ee_service
            .as_ref()
            .and_then(|e| e.get_active_master_key_id().cloned())
            .map(|id| id.replace('-', ""));

        let active_key_changed =
            local_encrypted && remote_encrypted && local_active_key != remote_active_key;

        if local_encrypted != remote_encrypted || active_key_changed {
            let direction = if active_key_changed {
                "Active master key changed — re-uploading all items with the new encryption key"
            } else if local_encrypted {
                "Encryption enabled — re-uploading all items encrypted"
            } else {
                "Encryption disabled — re-uploading all items unencrypted"
            };

            let _ = self.event_tx.send(SyncEvent::Warning {
                message: direction.to_string(),
            });

            // Clear sync_items so all items appear as "changed" and get re-uploaded.
            let cleared = self
                .storage
                .clear_all_sync_items()
                .await
                .map_err(SyncError::Local)?;
            tracing::info!("Cleared {} sync items for E2EE state change", cleared);

            // Reset last_sync_time to 0 so everything is considered new.
            self.context.last_sync_time = 0;
            // Clear local sync time so next load doesn't restore the old value
            let key = self.last_sync_setting_key();
            let _ = self.storage.set_setting(&key, "0").await;
        }

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
        let remote_exists = self
            .webdav
            .exists(&self.context.remote_path)
            .await
            .unwrap_or(false);

        if !remote_exists {
            self.webdav
                .mkcol(&self.context.remote_path)
                .await
                .map_err(|e| {
                    SyncError::Server(format!(
                        "Failed to create remote directory {}: {}",
                        self.context.remote_path, e
                    ))
                })?;
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
        let _ = self
            .event_tx
            .send(SyncEvent::PhaseStarted(SyncPhase::Upload));

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
                self.upload_item_encrypted(
                    ItemType::Folder,
                    &folder.id,
                    &self.serialize_folder(folder)?,
                )
                .await?;
                uploaded += 1;
                self.report_progress(
                    SyncPhase::Upload,
                    uploaded,
                    total_items,
                    "Uploading folders",
                );
            }

            for tag in &tags {
                self.upload_item_encrypted(ItemType::Tag, &tag.id, &self.serialize_tag(tag)?)
                    .await?;
                uploaded += 1;
                self.report_progress(SyncPhase::Upload, uploaded, total_items, "Uploading tags");
            }

            for note_tag in &note_tags {
                self.upload_item_encrypted(
                    ItemType::NoteTag,
                    &note_tag.id,
                    &self.serialize_note_tag(note_tag)?,
                )
                .await?;
                uploaded += 1;
                self.report_progress(
                    SyncPhase::Upload,
                    uploaded,
                    total_items,
                    "Uploading note tags",
                );
            }

            for note in &notes {
                self.upload_item_encrypted(ItemType::Note, &note.id, &self.serialize_note(note)?)
                    .await?;
                uploaded += 1;
                self.report_progress(SyncPhase::Upload, uploaded, total_items, "Uploading notes");
            }

            let sync_time = now_ms();
            self.update_sync_times(&folders, &tags, &notes, sync_time)
                .await?;
        }

        let _ = self
            .event_tx
            .send(SyncEvent::PhaseCompleted(SyncPhase::Upload));
        Ok(())
    }

    /// Upload an item with E2EE encryption (Joplin compatible format)
    /// Joplin stores all items at root level: {remote_path}/{id}.md
    /// When encrypted, the plaintext content is put in encryption_cipher_text field
    async fn upload_item_encrypted(
        &self,
        item_type: ItemType,
        item_id: &str,
        plaintext_content: &str,
    ) -> Result<()> {
        let type_num = match item_type {
            ItemType::Note => 1,
            ItemType::Folder => 2,
            ItemType::Tag => 5,
            ItemType::NoteTag => 6,
        };

        let type_name = match item_type {
            ItemType::Note => "note",
            ItemType::Folder => "folder",
            ItemType::Tag => "tag",
            ItemType::NoteTag => "note_tag",
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
                        enc_content.push_str("created_time: \n");
                        enc_content.push_str(&format!("updated_time: {}\n", now));
                        enc_content.push_str("user_created_time: \n");
                        enc_content.push_str("user_updated_time: \n");
                        enc_content.push_str(&format!("encryption_cipher_text: {}\n", encrypted));
                        enc_content.push_str("encryption_applied: 1\n");
                        enc_content.push_str("parent_id: \n");
                        enc_content.push_str("is_shared: \n");
                        enc_content.push_str("share_id: \n");
                        enc_content.push_str("master_key_id: \n");
                        enc_content.push_str("user_data: \n");
                        enc_content.push_str("deleted_time: 0\n");
                        // No trailing newline — Joplin's parser interprets trailing \n as body separator
                        enc_content.push_str(&format!("type_: {}", type_num));
                        tracing::info!("Encrypted {} {} for upload", type_name, item_id);
                        enc_content
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to encrypt {} {}: {} — uploading unencrypted",
                            type_name,
                            item_id,
                            e
                        );
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
        let remote_path = format!(
            "{}/{}.md",
            self.context.remote_path.trim_end_matches('/'),
            item_id
        );

        let _ = self.event_tx.send(SyncEvent::ItemUpload {
            item_type: type_name.to_string(),
            item_id: item_id.to_string(),
        });

        let bytes = content.into_bytes();
        self.webdav
            .put(&remote_path, &bytes, bytes.len() as u64)
            .await
            .map_err(|e| {
                SyncError::Server(format!("Failed to upload {} {}: {}", type_name, item_id, e))
            })?;

        let _ = self.event_tx.send(SyncEvent::ItemUploadComplete {
            item_type: type_name.to_string(),
            item_id: item_id.to_string(),
        });

        Ok(())
    }

    /// Phase 2: Delete remote items
    async fn phase_delete_remote(&mut self) -> Result<()> {
        let _ = self
            .event_tx
            .send(SyncEvent::PhaseStarted(SyncPhase::DeleteRemote));

        let deleted_items = self
            .storage
            .get_deleted_items(SyncTarget::WebDAV as i32)
            .await
            .map_err(SyncError::Local)?;

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
                let remote_path = format!(
                    "{}/{}.md",
                    self.context.remote_path.trim_end_matches('/'),
                    deleted_item.item_id
                );
                if let Err(e) = self.webdav.delete(&remote_path).await {
                    let _ = self.event_tx.send(SyncEvent::Warning {
                        message: format!(
                            "Failed to delete remote item {}: {}",
                            deleted_item.item_id, e
                        ),
                    });
                }
                self.report_progress(
                    SyncPhase::DeleteRemote,
                    i + 1,
                    total,
                    "Deleting remote items",
                );
            }

            self.storage
                .clear_deleted_items(deleted_items.len() as i64)
                .await
                .map_err(SyncError::Local)?;
        }

        let _ = self
            .event_tx
            .send(SyncEvent::PhaseCompleted(SyncPhase::DeleteRemote));
        Ok(())
    }

    /// Phase 3: Download remote changes (delta)
    async fn phase_delta(&self) -> Result<()> {
        let _ = self
            .event_tx
            .send(SyncEvent::PhaseStarted(SyncPhase::Delta));

        let remote_items = self.list_remote_items().await?;
        let local_items = self
            .storage
            .get_all_sync_items()
            .await
            .map_err(SyncError::Local)?;
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
                tracing::debug!(
                    "Downloading item {}/{}: {} ({:?})",
                    i + 1,
                    total,
                    remote_item.id,
                    remote_item.item_type
                );
                if let Err(e) = self
                    .download_item(&remote_item.id, &remote_item.item_type)
                    .await
                {
                    let _ = self.event_tx.send(SyncEvent::Warning {
                        message: format!("Failed to download {}: {}", remote_item.id, e),
                    });
                }
                self.report_progress(SyncPhase::Delta, i + 1, total, "Downloading remote changes");
            }
        }

        let _ = self
            .event_tx
            .send(SyncEvent::PhaseCompleted(SyncPhase::Delta));
        Ok(())
    }

    // === Helper methods ===

    async fn check_locks(&self) -> Result<()> {
        let lock_path = format!(
            "{}/lock.json",
            self.context.remote_path.trim_end_matches('/')
        );

        if let Ok(mut reader) = self.webdav.get(&lock_path).await {
            let mut content = Vec::new();
            reader
                .read_to_end(&mut content)
                .await
                .map_err(|e| SyncError::Server(format!("Failed to read lock.json: {}", e)))?;

            let content_str = String::from_utf8_lossy(&content);
            if let Ok(lock_data) = serde_json::from_str::<serde_json::Value>(&content_str) {
                if let Some(timestamp) = lock_data.get("updatedTime").and_then(|v| v.as_i64()) {
                    let lock_age_ms = now_ms() - timestamp;
                    let lock_age_min = lock_age_ms / (60 * 1000);
                    if lock_age_min < 5 {
                        return Err(SyncError::Server(format!(
                            "Sync target is locked. Lock age: {} minutes.",
                            lock_age_min
                        ))
                        .into());
                    }
                }
            }
        }

        Ok(())
    }

    /// Setting key for storing last sync time locally (per remote path)
    fn last_sync_setting_key(&self) -> String {
        let path_key = self.context.remote_path.trim_matches('/').replace('/', "_");
        format!("sync.last_sync_time.{}", path_key)
    }

    async fn load_sync_info(&mut self) -> Result<()> {
        match SyncInfo::load_from_remote(self.webdav.as_ref(), &self.context.remote_path).await {
            Ok(Some(remote_sync_info)) => {
                // Use locally-stored last sync time (immune to Joplin overwriting info.json)
                let local_last_sync = self.load_local_last_sync_time().await;
                self.context.last_sync_time = if local_last_sync > 0 {
                    local_last_sync
                } else {
                    remote_sync_info.delta_timestamp()
                };
                self.sync_info = Some(remote_sync_info);
            }
            Ok(None) => {
                self.sync_info = Some(SyncInfo::new());
                self.context.last_sync_time = 0;
            }
            Err(e) => {
                let error_msg = e.to_string();
                if !error_msg.contains("NotFound") && !error_msg.contains("not found") {
                    return Err(
                        SyncError::Server(format!("Failed to load sync info: {}", e)).into(),
                    );
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
            let sync_info_updated_at = now_ms();

            // Update E2EE state in sync info
            let e2ee_enabled = self
                .e2ee_service
                .as_ref()
                .map(|e| e.is_enabled())
                .unwrap_or(false);

            if e2ee_enabled {
                if !sync_info.e2ee.value {
                    sync_info.e2ee.value = true;
                    sync_info.e2ee.updated_time = sync_info_updated_at;
                } else if sync_info.e2ee.updated_time == 0 {
                    sync_info.e2ee.updated_time = sync_info_updated_at;
                }
                if let Some(ref e2ee) = self.e2ee_service {
                    if let Some(active_id) = e2ee.get_active_master_key_id() {
                        if sync_info.active_master_key_id.value != *active_id {
                            sync_info.active_master_key_id.value = active_id.clone();
                            sync_info.active_master_key_id.updated_time = sync_info_updated_at;
                        } else if sync_info.active_master_key_id.updated_time == 0 {
                            sync_info.active_master_key_id.updated_time = sync_info_updated_at;
                        }
                    }
                    // Populate masterKeys array so other clients (Joplin) can find the keys
                    for mk in e2ee.get_all_master_keys() {
                        let mk_id = mk.id.replace('-', "");
                        if !sync_info
                            .master_keys
                            .iter()
                            .any(|existing| existing.id.replace('-', "") == mk_id)
                        {
                            sync_info.master_keys.push(crate::sync_info::MasterKeyInfo {
                                id: mk_id,
                                created_time: mk.created_time,
                                updated_time: mk.updated_time,
                                source_application: mk.source_application.clone(),
                                encryption_method: mk.encryption_method,
                                checksum: mk.checksum.clone(),
                                content: mk.content.clone(),
                                has_been_used: true,
                            });
                        }
                    }
                }
            } else if sync_info.e2ee.value {
                // E2EE was previously enabled but now disabled
                sync_info.e2ee.value = false;
                sync_info.e2ee.updated_time = sync_info_updated_at;
            }

            sync_info
                .save_to_remote(self.webdav.as_ref(), &self.context.remote_path)
                .await
                .map_err(|e| SyncError::Server(format!("Failed to save sync info: {}", e)))?;

            // Save last sync time locally so Joplin overwriting info.json doesn't reset it
            let now = now_ms();
            let key = self.last_sync_setting_key();
            let _ = self.storage.set_setting(&key, &now.to_string()).await;
        }
        Ok(())
    }

    async fn load_local_last_sync_time(&self) -> i64 {
        let key = self.last_sync_setting_key();
        match self.storage.get_setting(&key).await {
            Ok(Some(val)) => val.parse().unwrap_or(0),
            _ => 0,
        }
    }

    async fn get_changed_folders(&self) -> Result<Vec<Folder>> {
        self.storage
            .get_folders_updated_since(self.context.last_sync_time)
            .await
            .map_err(|e| e.into())
    }

    async fn get_changed_tags(&self) -> Result<Vec<Tag>> {
        self.storage
            .get_tags_updated_since(self.context.last_sync_time)
            .await
            .map_err(|e| e.into())
    }

    async fn get_changed_notes(&self) -> Result<Vec<Note>> {
        self.storage
            .get_notes_updated_since(self.context.last_sync_time)
            .await
            .map_err(|e| e.into())
    }

    async fn get_changed_note_tags(&self) -> Result<Vec<NoteTag>> {
        self.storage
            .get_note_tags_updated_since(self.context.last_sync_time)
            .await
            .map_err(|e| e.into())
    }

    async fn update_sync_times(
        &self,
        folders: &[Folder],
        tags: &[Tag],
        notes: &[Note],
        sync_time: i64,
    ) -> Result<()> {
        for folder in folders {
            self.storage
                .update_sync_time("folders", &folder.id, sync_time)
                .await
                .map_err(SyncError::Local)?;
        }
        for tag in tags {
            self.storage
                .update_sync_time("tags", &tag.id, sync_time)
                .await
                .map_err(SyncError::Local)?;
        }
        for note in notes {
            self.storage
                .update_sync_time("notes", &note.id, sync_time)
                .await
                .map_err(SyncError::Local)?;
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
                    if entry.path.ends_with(".md")
                        && !entry.path.contains("/locks/")
                        && !entry.path.contains("/.sync/")
                        && !entry.path.contains("/.lock/")
                        && !entry.path.contains("/temp/")
                        && !entry.path.contains("/.resource/")
                    {
                        if let Some(id) = entry
                            .path
                            .strip_suffix(".md")
                            .and_then(|path| path.rsplit('/').next())
                        {
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

        Ok(remote_items)
    }

    fn find_delta_items(
        &self,
        remote_items: &[RemoteItem],
        local_items: &[joplin_domain::SyncItem],
    ) -> Vec<RemoteItem> {
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
                            tracing::debug!(
                                "Updated item {} marked for download (remote={}, local={})",
                                remote_item.id,
                                remote_modified,
                                sync_item.sync_time
                            );
                        }
                    }
                }
            }
        }

        tracing::info!(
            "Delta: {} items to download out of {} remote",
            items_to_download.len(),
            remote_items.len()
        );
        items_to_download
    }

    async fn download_item(&self, item_id: &str, _item_type: &ItemType) -> Result<()> {
        // Try root level first (Joplin 3.5+ format), then subdirectories
        let root_path = format!(
            "{}/{}.md",
            self.context.remote_path.trim_end_matches('/'),
            item_id
        );

        let final_path = if self.webdav.exists(&root_path).await.unwrap_or(false) {
            root_path
        } else {
            // Try subdirectories
            for subdir in &["items", "folders", "tags", "note_tags", "resources"] {
                let subdir_path = format!(
                    "{}/{}/{}.md",
                    self.context.remote_path.trim_end_matches('/'),
                    subdir,
                    item_id
                );
                if self.webdav.exists(&subdir_path).await.unwrap_or(false) {
                    break;
                }
            }
            // Default to root
            format!(
                "{}/{}.md",
                self.context.remote_path.trim_end_matches('/'),
                item_id
            )
        };

        let _ = self.event_tx.send(SyncEvent::ItemDownload {
            item_type: "item".to_string(),
            item_id: item_id.to_string(),
        });

        let mut reader = self.webdav.get(&final_path).await.map_err(|e| {
            SyncError::Server(format!("Failed to download item {}: {}", item_id, e))
        })?;

        let mut content = Vec::new();
        reader
            .read_to_end(&mut content)
            .await
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

        let type_num = metadata
            .get("type_")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(1);

        let encryption_applied = metadata
            .get("encryption_applied")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);

        let encryption_cipher_text = metadata
            .get("encryption_cipher_text")
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
                            "Failed to decrypt item {}: {}. Is the correct E2EE password set?",
                            item_id, e
                        ))
                        .into());
                    }
                }
            } else {
                tracing::warn!(
                    "Item {} is encrypted but no E2EE service available",
                    item_id
                );
                return Err(SyncError::Server(format!(
                    "Item {} is encrypted but E2EE is not configured",
                    item_id
                ))
                .into());
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
                    self.storage
                        .update_note(&note)
                        .await
                        .map_err(SyncError::Local)?;
                } else {
                    self.storage
                        .create_note(&note)
                        .await
                        .map_err(SyncError::Local)?;
                }
                // Update sync tracking
                self.storage
                    .update_sync_time("notes", &note.id, now_ms())
                    .await
                    .map_err(SyncError::Local)?;
                tracing::info!("Stored note: {} ({})", note.title, note.id);
            }
            2 => {
                // Folder
                let folder = self.deserialize_folder(item_id, &actual_content)?;
                let exists = matches!(self.storage.get_folder(&folder.id).await, Ok(Some(_)));
                if exists {
                    self.storage
                        .update_folder(&folder)
                        .await
                        .map_err(SyncError::Local)?;
                } else {
                    self.storage
                        .create_folder(&folder)
                        .await
                        .map_err(SyncError::Local)?;
                }
                self.storage
                    .update_sync_time("folders", &folder.id, now_ms())
                    .await
                    .map_err(SyncError::Local)?;
                tracing::info!("Stored folder: {} ({})", folder.title, folder.id);
            }
            5 => {
                // Tag
                let tag = self.deserialize_tag(item_id, &actual_content)?;
                let exists = matches!(self.storage.get_tag(&tag.id).await, Ok(Some(_)));
                if exists {
                    self.storage
                        .update_tag(&tag)
                        .await
                        .map_err(SyncError::Local)?;
                } else {
                    self.storage
                        .create_tag(&tag)
                        .await
                        .map_err(SyncError::Local)?;
                }
                self.storage
                    .update_sync_time("tags", &tag.id, now_ms())
                    .await
                    .map_err(SyncError::Local)?;
                tracing::info!("Stored tag: {}", tag.id);
            }
            _ => {
                tracing::warn!("Unsupported item type {} for item {}", type_num, item_id);
            }
        }

        Ok(())
    }

    /// Parse metadata fields from a Joplin item file
    fn parse_item_metadata(&self, content: &str) -> std::collections::HashMap<String, String> {
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
        content.push_str(&format!(
            "created_time: {}\n",
            self.ms_to_iso(folder.created_time)
        ));
        content.push_str(&format!(
            "updated_time: {}\n",
            self.ms_to_iso(folder.updated_time)
        ));
        content.push_str(&format!(
            "user_created_time: {}\n",
            self.ms_to_iso(folder.user_created_time)
        ));
        content.push_str(&format!(
            "user_updated_time: {}\n",
            self.ms_to_iso(folder.user_updated_time)
        ));
        content.push_str("encryption_cipher_text: \n");
        content.push_str("encryption_applied: 0\n");
        content.push_str(&format!("is_shared: {}\n", folder.is_shared));
        content.push_str(&format!(
            "share_id: {}\n",
            folder.share_id.as_deref().unwrap_or("")
        ));
        content.push_str(&format!(
            "master_key_id: {}\n",
            folder.master_key_id.as_deref().unwrap_or("")
        ));
        content.push_str(&format!("icon: {}\n", folder.icon));
        content.push_str("user_data: \n");
        content.push_str("deleted_time: 0\n");
        content.push_str("type_: 2");
        Ok(content)
    }

    fn serialize_tag(&self, tag: &Tag) -> Result<String> {
        let mut content = format!("{}\n\n", tag.title);
        content.push_str(&format!("id: {}\n", tag.id));
        content.push_str(&format!(
            "created_time: {}\n",
            self.ms_to_iso(tag.created_time)
        ));
        content.push_str(&format!(
            "updated_time: {}\n",
            self.ms_to_iso(tag.updated_time)
        ));
        content.push_str(&format!(
            "user_created_time: {}\n",
            self.ms_to_iso(tag.user_created_time)
        ));
        content.push_str(&format!(
            "user_updated_time: {}\n",
            self.ms_to_iso(tag.user_updated_time)
        ));
        content.push_str("encryption_cipher_text: \n");
        content.push_str("encryption_applied: 0\n");
        content.push_str(&format!("is_shared: {}\n", tag.is_shared));
        content.push_str(&format!("parent_id: {}\n", tag.parent_id));
        content.push_str("user_data: \n");
        content.push_str("deleted_time: 0\n");
        content.push_str("type_: 5");
        Ok(content)
    }

    fn serialize_note_tag(&self, note_tag: &NoteTag) -> Result<String> {
        let mut content = String::new();
        content.push_str(&format!("id: {}\n", note_tag.id));
        content.push_str(&format!("note_id: {}\n", note_tag.note_id));
        content.push_str(&format!("tag_id: {}\n", note_tag.tag_id));
        content.push_str(&format!(
            "created_time: {}\n",
            self.ms_to_iso(note_tag.created_time)
        ));
        content.push_str(&format!(
            "updated_time: {}\n",
            self.ms_to_iso(note_tag.updated_time)
        ));
        content.push_str(&format!(
            "user_created_time: {}\n",
            self.ms_to_iso(note_tag.created_time)
        ));
        content.push_str(&format!(
            "user_updated_time: {}\n",
            self.ms_to_iso(note_tag.updated_time)
        ));
        content.push_str("encryption_cipher_text: \n");
        content.push_str("encryption_applied: 0\n");
        content.push_str(&format!("is_shared: {}\n", note_tag.is_shared));
        content.push_str("user_data: \n");
        content.push_str("deleted_time: 0\n");
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
        content.push_str(&format!(
            "created_time: {}\n",
            self.ms_to_iso(note.created_time)
        ));
        content.push_str(&format!(
            "updated_time: {}\n",
            self.ms_to_iso(note.updated_time)
        ));
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
        content.push_str(&format!(
            "source_application: {}\n",
            note.source_application
        ));
        content.push_str(&format!("application_data: {}\n", note.application_data));
        content.push_str(&format!("order: {}\n", note.order));
        content.push_str(&format!(
            "user_created_time: {}\n",
            self.ms_to_iso(note.user_created_time)
        ));
        content.push_str(&format!(
            "user_updated_time: {}\n",
            self.ms_to_iso(note.user_updated_time)
        ));
        content.push_str("encryption_cipher_text: \n");
        content.push_str("encryption_applied: 0\n");
        content.push_str(&format!("markup_language: {}\n", note.markup_language));
        content.push_str(&format!("is_shared: {}\n", note.is_shared));
        content.push_str(&format!(
            "share_id: {}\n",
            note.share_id.as_deref().unwrap_or("")
        ));
        content.push_str(&format!(
            "conflict_original_id: {}\n",
            note.conflict_original_id
        ));
        content.push_str(&format!(
            "master_key_id: {}\n",
            note.master_key_id.as_deref().unwrap_or("")
        ));
        content.push_str("user_data: \n");
        content.push_str(&format!("deleted_time: {}\n", note.deleted_time));
        content.push_str("type_: 1");

        Ok(content)
    }

    // === Deserialization methods ===

    fn deserialize_note(&self, id: &str, content: &str) -> Result<Note> {
        let mut note = Note {
            id: id.to_string(),
            ..Default::default()
        };

        // Joplin format: title\n\nbody\n\nid: ...\nparent_id: ...\n...
        // Split at the first metadata field (id:)
        let (text_part, props_part) = self.split_content_and_properties(content);

        // Title is the first line of text_part
        if let Some(first_line) = text_part.lines().next() {
            note.title = first_line.to_string();
        }

        // Body is everything after the title line (minus leading/trailing whitespace)
        let body_start = text_part
            .find('\n')
            .map(|i| i + 1)
            .unwrap_or(text_part.len());
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
                    "user_created_time" => {
                        note.user_created_time = self.iso_to_ms(value).unwrap_or(0)
                    }
                    "user_updated_time" => {
                        note.user_updated_time = self.iso_to_ms(value).unwrap_or(0)
                    }
                    "is_conflict" => note.is_conflict = value.parse().unwrap_or(0),
                    "latitude" => {
                        note.latitude = (value.parse::<f64>().unwrap_or(0.0) * 1e7) as i64
                    }
                    "longitude" => {
                        note.longitude = (value.parse::<f64>().unwrap_or(0.0) * 1e7) as i64
                    }
                    "altitude" => {
                        note.altitude = (value.parse::<f64>().unwrap_or(0.0) * 1e2) as i64
                    }
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
                    "share_id" => {
                        note.share_id = if !value.is_empty() {
                            Some(value.to_string())
                        } else {
                            None
                        }
                    }
                    "conflict_original_id" => note.conflict_original_id = value.to_string(),
                    "deleted_time" => note.deleted_time = value.parse().unwrap_or(0),
                    "master_key_id" => {
                        note.master_key_id = if !value.is_empty() {
                            Some(value.to_string())
                        } else {
                            None
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(note)
    }

    fn deserialize_folder(&self, id: &str, content: &str) -> Result<Folder> {
        let mut folder = Folder {
            id: id.to_string(),
            ..Default::default()
        };

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
                    "title" if folder.title.is_empty() => {
                        folder.title = value.to_string();
                    }
                    "parent_id" => folder.parent_id = value.to_string(),
                    "created_time" => folder.created_time = self.iso_to_ms(value).unwrap_or(0),
                    "updated_time" => folder.updated_time = self.iso_to_ms(value).unwrap_or(0),
                    "user_created_time" => {
                        folder.user_created_time = self.iso_to_ms(value).unwrap_or(0)
                    }
                    "user_updated_time" => {
                        folder.user_updated_time = self.iso_to_ms(value).unwrap_or(0)
                    }
                    "icon" => folder.icon = value.to_string(),
                    "is_shared" => folder.is_shared = value.parse().unwrap_or(0),
                    "share_id" => {
                        folder.share_id = if !value.is_empty() {
                            Some(value.to_string())
                        } else {
                            None
                        }
                    }
                    "master_key_id" => {
                        folder.master_key_id = if !value.is_empty() {
                            Some(value.to_string())
                        } else {
                            None
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(folder)
    }

    fn deserialize_tag(&self, id: &str, content: &str) -> Result<Tag> {
        // Try Joplin text format first
        let mut tag = Tag {
            id: id.to_string(),
            ..Default::default()
        };

        let (text_part, props_part) = self.split_content_and_properties(content);

        if let Some(first_line) = text_part.lines().next() {
            tag.title = first_line.to_string();
        }

        for line in props_part.lines() {
            if let Some((key, value)) = line.split_once(": ") {
                let value = value.trim();
                match key.trim() {
                    "id" => tag.id = value.to_string(),
                    "title" if tag.title.is_empty() => {
                        tag.title = value.to_string();
                    }
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
        let dt = DateTime::parse_from_rfc3339(iso).map_err(|e| {
            SyncError::Serialization(format!("Failed to parse timestamp {}: {}", iso, e))
        })?;
        Ok(dt.timestamp_millis())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use joplin_domain::{DeletedItem, SyncItem, SyncTarget};
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_sync_context_default() {
        let context = SyncContext::default();
        assert_eq!(context.last_sync_time, 0);
        assert_eq!(context.remote_path, "/neojoplin");
    }

    /// Verify serialized note has no trailing newline (critical for Joplin compatibility)
    #[test]
    fn test_serialize_note_no_trailing_newline() {
        let engine = make_test_engine();
        let note = Note {
            id: "abc123".to_string(),
            title: "Test Note".to_string(),
            body: "Hello world".to_string(),
            is_todo: 0,
            ..Note::default()
        };
        let serialized = engine.serialize_note(&note).unwrap();
        assert!(
            serialized.ends_with("type_: 1"),
            "Must end with type_: 1 without trailing newline"
        );
        assert!(!serialized.ends_with('\n'), "Must NOT end with newline");
    }

    /// Verify serialized todo note includes todo fields
    #[test]
    fn test_serialize_todo_fields() {
        let engine = make_test_engine();
        let todo = Note {
            id: "todo123".to_string(),
            title: "Buy groceries".to_string(),
            body: String::new(),
            is_todo: 1,
            todo_due: 1700000000000,
            todo_completed: 1700001000000,
            ..Note::default()
        };
        let serialized = engine.serialize_note(&todo).unwrap();
        assert!(serialized.contains("is_todo: 1"));
        assert!(serialized.contains("todo_due: 1700000000000"));
        assert!(serialized.contains("todo_completed: 1700001000000"));
    }

    /// Verify serialized folder has no trailing newline
    #[test]
    fn test_serialize_folder_no_trailing_newline() {
        let engine = make_test_engine();
        let folder = Folder {
            id: "folder123".to_string(),
            title: "My Folder".to_string(),
            ..Folder::default()
        };
        let serialized = engine.serialize_folder(&folder).unwrap();
        assert!(serialized.ends_with("type_: 2"));
        assert!(!serialized.ends_with('\n'));
    }

    /// Verify serialized tag has no trailing newline
    #[test]
    fn test_serialize_tag_no_trailing_newline() {
        let engine = make_test_engine();
        let tag = Tag {
            id: "tag123".to_string(),
            title: "important".to_string(),
            ..Tag::default()
        };
        let serialized = engine.serialize_tag(&tag).unwrap();
        assert!(serialized.ends_with("type_: 5"));
        assert!(!serialized.ends_with('\n'));
    }

    /// Verify serialized note_tag has no trailing newline
    #[test]
    fn test_serialize_note_tag_no_trailing_newline() {
        let engine = make_test_engine();
        let note_tag = NoteTag {
            id: "nt123".to_string(),
            note_id: "note1".to_string(),
            tag_id: "tag1".to_string(),
            ..NoteTag::default()
        };
        let serialized = engine.serialize_note_tag(&note_tag).unwrap();
        assert!(serialized.ends_with("type_: 6"));
        assert!(!serialized.ends_with('\n'));
    }

    /// Verify deserialization of a todo note from Joplin format
    #[test]
    fn test_deserialize_todo_note() {
        let engine = make_test_engine();
        let content = "Buy groceries\n\nMilk, eggs, bread\n\n\
            id: todo456\n\
            parent_id: folder1\n\
            created_time: 2024-01-01T00:00:00.000Z\n\
            updated_time: 2024-01-01T12:00:00.000Z\n\
            is_todo: 1\n\
            todo_due: 1700000000000\n\
            todo_completed: 0\n\
            type_: 1";
        let note = engine.deserialize_note("todo456", content).unwrap();
        assert_eq!(note.title, "Buy groceries");
        assert_eq!(note.body, "Milk, eggs, bread");
        assert_eq!(note.is_todo, 1);
        assert_eq!(note.todo_due, 1700000000000);
        assert_eq!(note.todo_completed, 0);
    }

    /// Verify deserialization of a completed todo
    #[test]
    fn test_deserialize_completed_todo() {
        let engine = make_test_engine();
        let content = "Done Task\n\n\
            id: done789\n\
            is_todo: 1\n\
            todo_completed: 1700001000000\n\
            type_: 1";
        let note = engine.deserialize_note("done789", content).unwrap();
        assert_eq!(note.is_todo, 1);
        assert_eq!(note.todo_completed, 1700001000000);
    }

    /// Verify roundtrip: serialize then deserialize produces same data
    #[test]
    fn test_note_serialization_roundtrip() {
        let engine = make_test_engine();
        let original = Note {
            id: "rt123".to_string(),
            title: "Roundtrip Test".to_string(),
            body: "This is the body\nWith multiple lines".to_string(),
            parent_id: "folder1".to_string(),
            is_todo: 1,
            todo_due: 1700000000000,
            todo_completed: 1700001000000,
            markup_language: 1,
            ..Note::default()
        };
        let serialized = engine.serialize_note(&original).unwrap();
        let deserialized = engine.deserialize_note("rt123", &serialized).unwrap();

        assert_eq!(deserialized.title, original.title);
        assert_eq!(deserialized.body, original.body);
        assert_eq!(deserialized.parent_id, original.parent_id);
        assert_eq!(deserialized.is_todo, original.is_todo);
        assert_eq!(deserialized.todo_due, original.todo_due);
        assert_eq!(deserialized.todo_completed, original.todo_completed);
    }

    /// Verify deserialization of folder content
    #[test]
    fn test_deserialize_folder() {
        let engine = make_test_engine();
        let content = "My Notebook\n\n\
            id: f123\n\
            parent_id: \n\
            created_time: 2024-01-01T00:00:00.000Z\n\
            updated_time: 2024-06-15T10:30:00.000Z\n\
            type_: 2";
        let folder = engine.deserialize_folder("f123", content).unwrap();
        assert_eq!(folder.title, "My Notebook");
        assert_eq!(folder.id, "f123");
    }

    /// Verify type detection from content
    #[test]
    fn test_detect_item_type() {
        // The store_downloaded_item uses type_num from metadata
        // Type numbers: 1=Note, 2=Folder, 5=Tag, 6=NoteTag
        let engine = make_test_engine();
        let meta1 = engine.parse_item_metadata("id: x\ntype_: 1");
        assert_eq!(
            meta1.get("type_").and_then(|v| v.parse::<i32>().ok()),
            Some(1)
        );
        let meta2 = engine.parse_item_metadata("id: x\ntype_: 2");
        assert_eq!(
            meta2.get("type_").and_then(|v| v.parse::<i32>().ok()),
            Some(2)
        );
        let meta5 = engine.parse_item_metadata("id: x\ntype_: 5");
        assert_eq!(
            meta5.get("type_").and_then(|v| v.parse::<i32>().ok()),
            Some(5)
        );
        let meta6 = engine.parse_item_metadata("id: x\ntype_: 6");
        assert_eq!(
            meta6.get("type_").and_then(|v| v.parse::<i32>().ok()),
            Some(6)
        );
        let meta_none = engine.parse_item_metadata("no type here");
        assert_eq!(meta_none.get("type_"), None);
    }

    /// Test ms_to_iso and iso_to_ms conversion roundtrip
    #[test]
    fn test_timestamp_conversion() {
        let engine = make_test_engine();
        let ts = 1700000000000i64;
        let iso = engine.ms_to_iso(ts);
        assert!(iso.contains("2023-11-14"), "Expected 2023-11-14 in {}", iso);
        let back = engine.iso_to_ms(&iso).unwrap();
        assert_eq!(back, ts);
    }

    #[tokio::test]
    async fn test_phase_delete_remote_uses_webdav_deleted_items() {
        let (mut engine, storage) = make_test_engine_with_storage();
        storage.deleted_items.lock().unwrap().push(DeletedItem {
            id: 1,
            item_type: 1,
            item_id: "deleted-note".to_string(),
            deleted_time: now_ms(),
            sync_target: SyncTarget::WebDAV as i32,
        });

        engine.phase_delete_remote().await.unwrap();

        assert_eq!(
            storage.requested_sync_targets.lock().unwrap().as_slice(),
            &[SyncTarget::WebDAV as i32]
        );
        assert_eq!(storage.cleared_limits.lock().unwrap().as_slice(), &[1]);
    }

    /// Helper: create a test engine with minimal mocks
    fn make_test_engine() -> SyncEngine {
        make_test_engine_with_storage().0
    }

    fn make_test_engine_with_storage() -> (SyncEngine, Arc<FakeStorage>) {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let storage = Arc::new(FakeStorage::default());
        (
            SyncEngine {
                storage: storage.clone(),
                webdav: std::sync::Arc::new(FakeWebDav),
                context: SyncContext::default(),
                sync_info: None,
                e2ee_service: None,
                event_tx: tx,
            },
            storage,
        )
    }

    /// Fake storage for tests (not used, just needed for type)
    #[derive(Default)]
    struct FakeStorage {
        deleted_items: Mutex<Vec<DeletedItem>>,
        requested_sync_targets: Mutex<Vec<i32>>,
        cleared_limits: Mutex<Vec<i64>>,
    }
    #[async_trait::async_trait]
    impl joplin_domain::Storage for FakeStorage {
        async fn create_note(
            &self,
            _: &Note,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn get_note(
            &self,
            _: &str,
        ) -> std::result::Result<Option<Note>, joplin_domain::DatabaseError> {
            Ok(None)
        }
        async fn update_note(
            &self,
            _: &Note,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn delete_note(
            &self,
            _: &str,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn list_notes(
            &self,
            _: Option<&str>,
        ) -> std::result::Result<Vec<Note>, joplin_domain::DatabaseError> {
            Ok(vec![])
        }
        async fn create_folder(
            &self,
            _: &Folder,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn get_folder(
            &self,
            _: &str,
        ) -> std::result::Result<Option<Folder>, joplin_domain::DatabaseError> {
            Ok(None)
        }
        async fn update_folder(
            &self,
            _: &Folder,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn delete_folder(
            &self,
            _: &str,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn list_folders(
            &self,
        ) -> std::result::Result<Vec<Folder>, joplin_domain::DatabaseError> {
            Ok(vec![])
        }
        async fn create_tag(
            &self,
            _: &Tag,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn get_tag(
            &self,
            _: &str,
        ) -> std::result::Result<Option<Tag>, joplin_domain::DatabaseError> {
            Ok(None)
        }
        async fn update_tag(
            &self,
            _: &Tag,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn delete_tag(
            &self,
            _: &str,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn list_tags(&self) -> std::result::Result<Vec<Tag>, joplin_domain::DatabaseError> {
            Ok(vec![])
        }
        async fn add_note_tag(
            &self,
            _: &NoteTag,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn remove_note_tag(
            &self,
            _: &str,
            _: &str,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn get_note_tags(
            &self,
            _: &str,
        ) -> std::result::Result<Vec<Tag>, joplin_domain::DatabaseError> {
            Ok(vec![])
        }
        async fn get_folders_updated_since(
            &self,
            _: i64,
        ) -> std::result::Result<Vec<Folder>, joplin_domain::DatabaseError> {
            Ok(vec![])
        }
        async fn get_tags_updated_since(
            &self,
            _: i64,
        ) -> std::result::Result<Vec<Tag>, joplin_domain::DatabaseError> {
            Ok(vec![])
        }
        async fn get_notes_updated_since(
            &self,
            _: i64,
        ) -> std::result::Result<Vec<Note>, joplin_domain::DatabaseError> {
            Ok(vec![])
        }
        async fn get_note_tags_updated_since(
            &self,
            _: i64,
        ) -> std::result::Result<Vec<NoteTag>, joplin_domain::DatabaseError> {
            Ok(vec![])
        }
        async fn get_all_sync_items(
            &self,
        ) -> std::result::Result<Vec<SyncItem>, joplin_domain::DatabaseError> {
            Ok(vec![])
        }
        async fn update_sync_time(
            &self,
            _: &str,
            _: &str,
            _: i64,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn get_setting(
            &self,
            _: &str,
        ) -> std::result::Result<Option<String>, joplin_domain::DatabaseError> {
            Ok(None)
        }
        async fn set_setting(
            &self,
            _: &str,
            _: &str,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn get_sync_items(
            &self,
            _: i32,
        ) -> std::result::Result<Vec<SyncItem>, joplin_domain::DatabaseError> {
            Ok(vec![])
        }
        async fn upsert_sync_item(
            &self,
            _: &SyncItem,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn delete_sync_item(
            &self,
            _: i32,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn clear_all_sync_items(
            &self,
        ) -> std::result::Result<usize, joplin_domain::DatabaseError> {
            Ok(0)
        }
        async fn get_deleted_items(
            &self,
            sync_target: i32,
        ) -> std::result::Result<Vec<DeletedItem>, joplin_domain::DatabaseError> {
            self.requested_sync_targets
                .lock()
                .unwrap()
                .push(sync_target);
            Ok(self
                .deleted_items
                .lock()
                .unwrap()
                .iter()
                .filter(|item| item.sync_target == sync_target)
                .cloned()
                .collect())
        }
        async fn add_deleted_item(
            &self,
            _: &DeletedItem,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn remove_deleted_item(
            &self,
            _: i32,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn clear_deleted_items(
            &self,
            limit: i64,
        ) -> std::result::Result<usize, joplin_domain::DatabaseError> {
            self.cleared_limits.lock().unwrap().push(limit);
            let mut items = self.deleted_items.lock().unwrap();
            let removed = (limit.max(0) as usize).min(items.len());
            items.drain(0..removed);
            Ok(removed)
        }
        async fn get_version(&self) -> std::result::Result<i32, joplin_domain::DatabaseError> {
            Ok(41)
        }
        async fn begin_transaction(&self) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn commit_transaction(
            &self,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn rollback_transaction(
            &self,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn trash_note(
            &self,
            _: &str,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn restore_note(
            &self,
            _: &str,
        ) -> std::result::Result<(), joplin_domain::DatabaseError> {
            Ok(())
        }
        async fn list_deleted_notes(
            &self,
        ) -> std::result::Result<Vec<Note>, joplin_domain::DatabaseError> {
            Ok(vec![])
        }
    }

    /// Fake WebDAV for tests (not used, just needed for type)
    struct FakeWebDav;
    #[async_trait::async_trait]
    impl joplin_domain::WebDavClient for FakeWebDav {
        async fn list(
            &self,
            _: &str,
        ) -> std::result::Result<Vec<joplin_domain::DavEntry>, joplin_domain::WebDavError> {
            Ok(vec![])
        }
        async fn get(
            &self,
            _: &str,
        ) -> std::result::Result<
            Box<dyn futures::io::AsyncRead + Unpin + Send>,
            joplin_domain::WebDavError,
        > {
            Err(joplin_domain::WebDavError::NotFound("test".into()))
        }
        async fn put(
            &self,
            _: &str,
            _: &[u8],
            _: u64,
        ) -> std::result::Result<(), joplin_domain::WebDavError> {
            Ok(())
        }
        async fn delete(&self, _: &str) -> std::result::Result<(), joplin_domain::WebDavError> {
            Ok(())
        }
        async fn mkcol(&self, _: &str) -> std::result::Result<(), joplin_domain::WebDavError> {
            Ok(())
        }
        async fn exists(&self, _: &str) -> std::result::Result<bool, joplin_domain::WebDavError> {
            Ok(false)
        }
        async fn stat(
            &self,
            _: &str,
        ) -> std::result::Result<joplin_domain::DavEntry, joplin_domain::WebDavError> {
            Err(joplin_domain::WebDavError::NotFound("test".into()))
        }
        async fn lock(
            &self,
            _: &str,
            _: std::time::Duration,
        ) -> std::result::Result<String, joplin_domain::WebDavError> {
            Ok("lock".into())
        }
        async fn refresh_lock(
            &self,
            _: &str,
        ) -> std::result::Result<(), joplin_domain::WebDavError> {
            Ok(())
        }
        async fn unlock(
            &self,
            _: &str,
            _: &str,
        ) -> std::result::Result<(), joplin_domain::WebDavError> {
            Ok(())
        }
        async fn mv(
            &self,
            _: &str,
            _: &str,
        ) -> std::result::Result<(), joplin_domain::WebDavError> {
            Ok(())
        }
        async fn copy(
            &self,
            _: &str,
            _: &str,
        ) -> std::result::Result<(), joplin_domain::WebDavError> {
            Ok(())
        }
    }
}
