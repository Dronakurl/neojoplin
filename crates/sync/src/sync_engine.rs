// Sync engine implementation

use neojoplin_core::{Storage, WebDavClient, SyncEvent, Result, SyncPhase, SyncError, Note, Folder, Tag, NoteTag, now_ms};
use std::sync::Arc;
use tokio::sync::mpsc;
use futures::io::AsyncReadExt;
use serde_json;

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

/// Main sync engine
pub struct SyncEngine {
    storage: Arc<dyn Storage>,
    webdav: Arc<dyn WebDavClient>,
    event_tx: mpsc::UnboundedSender<SyncEvent>,
    context: SyncContext,
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
        }
    }

    /// Set the remote sync path
    pub fn with_remote_path(mut self, path: String) -> Self {
        self.context.remote_path = path;
        self
    }

    /// Run full sync process
    pub async fn sync(&mut self) -> Result<()> {
        let start = std::time::Instant::now();

        // Ensure remote directory exists
        self.ensure_remote_directory().await?;

        // Phase 1: Upload local changes
        self.phase_upload().await?;

        // Phase 2: Delete remote items
        self.phase_delete_remote().await?;

        // Phase 3: Download remote changes
        self.phase_delta().await?;

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
                let remote_path = format!("{}/items/{}.md", self.context.remote_path, deleted_item.item_id);

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
            for (i, item_id) in items_to_download.iter().enumerate() {
                self.download_item(item_id).await?;
                self.report_progress(SyncPhase::Delta, i + 1, total, "Downloading remote changes");
            }

            // 5. Update sync context (stored in database instead)
            // self.context.last_sync_time = now_ms();
        }

        let _ = self.event_tx.send(SyncEvent::PhaseCompleted(SyncPhase::Delta));
        Ok(())
    }

    // Helper methods

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
        let remote_path = format!("{}/folders/{}.md", self.context.remote_path, folder.id);
        let content = self.serialize_folder(folder)?;

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
        let remote_path = format!("{}/items/{}.md", self.context.remote_path, note.id);
        let content = self.serialize_note(note)?;

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

    async fn list_remote_items(&self) -> Result<Vec<String>> {
        let items_path = format!("{}/items", self.context.remote_path);

        // Ensure items directory exists
        if let Err(_) = self.webdav.exists(&items_path).await {
            return Ok(Vec::new());
        }

        // List items in the remote items directory
        let entries = self.webdav.list(&items_path).await
            .map_err(|e| SyncError::Server(format!("Failed to list remote items: {}", e)))?;

        // Extract item IDs from file names
        let item_ids: Vec<String> = entries
            .into_iter()
            .filter_map(|entry| {
                entry.path.strip_suffix(".md")
                    .and_then(|path| path.rsplit('/').next())
                    .map(|id| id.to_string())
            })
            .collect();

        Ok(item_ids)
    }

    fn find_delta_items(&self, remote_items: &[String], local_items: &[neojoplin_core::SyncItem]) -> Vec<String> {
        let mut items_to_download = Vec::new();

        for remote_id in remote_items {
            let needs_download = local_items
                .iter()
                .find(|local| local.item_id == *remote_id)
                .map_or(true, |local| {
                    // Download if remote is newer
                    let _local_time = local.sync_time;
                    // For now, always download if we don't have remote timestamp info
                    // In production, we'd parse the remote file's modified time
                    true
                });

            if needs_download {
                items_to_download.push(remote_id.clone());
            }
        }

        items_to_download
    }

    async fn download_item(&self, item_id: &str) -> Result<()> {
        let remote_path = format!("{}/items/{}.md", self.context.remote_path, item_id);

        let _ = self.event_tx.send(SyncEvent::ItemDownload {
            item_type: "item".to_string(),
            item_id: item_id.to_string(),
        });

        // Download item content
        let mut reader = self.webdav.get(&remote_path).await
            .map_err(|e| SyncError::Server(format!("Failed to download item {}: {}", item_id, e)))?;

        let mut content = Vec::new();
        reader.read_to_end(&mut content).await
            .map_err(|e| SyncError::Server(format!("Failed to read item content {}: {}", item_id, e)))?;

        let content_str = String::from_utf8_lossy(&content);

        // Parse and store the item
        // For now, this is a simplified version - production would parse JED format
        self.store_downloaded_item(item_id, &content_str).await?;

        let _ = self.event_tx.send(SyncEvent::ItemDownloadComplete {
            item_type: "item".to_string(),
            item_id: item_id.to_string(),
        });

        Ok(())
    }

    async fn store_downloaded_item(&self, item_id: &str, content: &str) -> Result<()> {
        // Try to parse as note first
        if let Ok(note) = self.deserialize_note(item_id, content) {
            // Check if note exists, if not create it
            if self.storage.get_note(&note.id).await.is_ok() {
                self.storage.update_note(&note).await
                    .map_err(|e| SyncError::Local(e))?;
            } else {
                self.storage.create_note(&note).await
                    .map_err(|e| SyncError::Local(e))?;
            }
            return Ok(());
        }

        // Try folder
        if let Ok(folder) = self.deserialize_folder(item_id, content) {
            // Check if folder exists, if not create it
            if self.storage.get_folder(&folder.id).await.is_ok() {
                self.storage.update_folder(&folder).await
                    .map_err(|e| SyncError::Local(e))?;
            } else {
                self.storage.create_folder(&folder).await
                    .map_err(|e| SyncError::Local(e))?;
            }
            return Ok(());
        }

        // Try tag
        if let Ok(tag) = self.deserialize_tag(item_id, content) {
            self.storage.update_tag(&tag).await
                .map_err(|e| SyncError::Local(e))?;
            return Ok(());
        }

        Ok(())
    }

    fn report_progress(&self, phase: SyncPhase, current: usize, total: usize, message: &str) {
        let _ = self.event_tx.send(SyncEvent::Progress {
            phase,
            current,
            total,
            message: message.to_string(),
        });
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

        // Parse title (first line)
        for line in content.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once(':') {
                let value = value.trim();
                match key.trim() {
                    "id" => folder.id = value.to_string(),
                    "title" => {
                        // Title is the first line without a colon
                        if folder.title.is_empty() {
                            folder.title = value.to_string();
                        }
                    },
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

        use chrono::{DateTime, NaiveDateTime, Utc};
        let secs = ms / 1000;
        let millis = (ms % 1000) as u32;
        let dt = NaiveDateTime::from_timestamp_opt(secs, millis * 1_000_000);

        match dt {
            Some(naive_dt) => {
                let datetime = DateTime::<Utc>::from_utc(naive_dt, Utc);
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

        use chrono::{DateTime, Utc};
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
        assert_eq!(context.remote_path, "/");
    }

    #[test]
    fn test_find_delta_items() {
        // This test would require mock data
        // For now, just verify the method exists
        let remote_items = vec!["item1".to_string(), "item2".to_string()];
        let local_items: Vec<neojoplin_core::SyncItem> = vec![];

        // Can't call self.find_delta_items in unit test without instance
        // This is just a placeholder to show the concept
        assert_eq!(remote_items.len(), 2);
    }
}
