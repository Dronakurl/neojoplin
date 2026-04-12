// Sync info handling for Joplin compatibility

use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};
use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;

/// Sync information stored in sync.json for Joplin compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncInfo {
    pub client_id: String,
    pub sync_version: i32,
    pub app_min_version: String,

    #[serde(default)]
    pub neojoplin: NeoJoplinSyncInfo,
}

/// NeoJoplin-specific sync information (namespaced for compatibility)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeoJoplinSyncInfo {
    #[serde(default = "default_neojoplin_version")]
    pub version: String,

    #[serde(default)]
    pub delta_context: DeltaContext,
}

impl Default for NeoJoplinSyncInfo {
    fn default() -> Self {
        Self {
            version: default_neojoplin_version(),
            delta_context: DeltaContext::default(),
        }
    }
}

fn default_neojoplin_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Minimal delta context - Phase 1 implementation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaContext {
    #[serde(default)]
    pub timestamp: i64,
}

impl Default for DeltaContext {
    fn default() -> Self {
        Self {
            timestamp: 0,
        }
    }
}

impl SyncInfo {
    /// Create new sync info with a generated client ID
    pub fn new() -> Self {
        Self {
            client_id: format!("neojoplin-{}", Uuid::new_v4()),
            sync_version: 3, // Joplin's current sync protocol version
            app_min_version: "3.0.0".to_string(),
            neojoplin: NeoJoplinSyncInfo::default(),
        }
    }

    /// Load sync info from remote WebDAV server
    pub async fn load_from_remote(webdav: &dyn neojoplin_core::WebDavClient, remote_path: &str) -> Result<Option<Self>> {
        let sync_json_path = format!("{}/sync.json", remote_path.trim_end_matches('/'));

        match webdav.get(&sync_json_path).await {
            Ok(mut reader) => {
                use futures::io::AsyncReadExt;
                let mut content = Vec::new();
                reader.read_to_end(&mut content).await
                    .context("Failed to read sync.json from remote")?;

                let content_str = String::from_utf8_lossy(&content);
                let sync_info: SyncInfo = serde_json::from_str(&content_str)
                    .context("Failed to parse sync.json")?;

                Ok(Some(sync_info))
            }
            Err(neojoplin_core::WebDavError::NotFound(_)) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Failed to load sync.json: {}", e)),
        }
    }

    /// Save sync info to remote WebDAV server
    pub async fn save_to_remote(&self, webdav: &dyn neojoplin_core::WebDavClient, remote_path: &str) -> Result<()> {
        let sync_json_path = format!("{}/sync.json", remote_path.trim_end_matches('/'));

        let json_content = serde_json::to_string_pretty(self)
            .context("Failed to serialize sync info")?;

        let bytes = json_content.into_bytes();
        webdav.put(&sync_json_path, &bytes, bytes.len() as u64).await
            .context("Failed to upload sync.json")?;

        Ok(())
    }

    /// Update the delta context timestamp
    pub fn update_delta_timestamp(&mut self, timestamp: i64) {
        self.neojoplin.delta_context.timestamp = timestamp;
    }

    /// Get the current delta context timestamp
    pub fn delta_timestamp(&self) -> i64 {
        self.neojoplin.delta_context.timestamp
    }
}

/// Client ID manager - generates and persists a stable client ID
pub struct ClientIdManager {
    client_id_path: PathBuf,
}

impl ClientIdManager {
    /// Create a new client ID manager
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            client_id_path: data_dir.join("client_id"),
        })
    }

    /// Load existing client ID or generate a new one
    pub async fn load_or_generate(&self) -> Result<String> {
        if self.client_id_path.exists() {
            let client_id = fs::read_to_string(&self.client_id_path).await
                .context("Failed to read client_id file")?;
            Ok(client_id.trim().to_string())
        } else {
            let client_id = format!("neojoplin-{}", Uuid::new_v4());

            // Ensure parent directory exists
            if let Some(parent) = self.client_id_path.parent() {
                fs::create_dir_all(parent).await
                    .context("Failed to create client_id directory")?;
            }

            fs::write(&self.client_id_path, &client_id).await
                .context("Failed to write client_id file")?;

            Ok(client_id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_info_new() {
        let sync_info = SyncInfo::new();
        assert_eq!(sync_info.sync_version, 3);
        assert_eq!(sync_info.app_min_version, "3.0.0");
        assert!(sync_info.client_id.starts_with("neojoplin-"));
    }

    #[test]
    fn test_delta_context_default() {
        let context = DeltaContext::default();
        assert_eq!(context.timestamp, 0);
    }

    #[test]
    fn test_neojoplin_sync_info_default() {
        let info = NeoJoplinSyncInfo::default();
        assert_eq!(info.delta_context.timestamp, 0);
        assert!(!info.version.is_empty());
    }

    #[test]
    fn test_sync_info_serialization() {
        let sync_info = SyncInfo::new();
        let json = serde_json::to_string_pretty(&sync_info).unwrap();
        println!("{}", json);

        // Verify it can be deserialized back
        let deserialized: SyncInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.sync_version, 3);
        assert_eq!(deserialized.client_id, sync_info.client_id);
    }

    #[test]
    fn test_sync_info_update_delta_timestamp() {
        let mut sync_info = SyncInfo::new();
        assert_eq!(sync_info.delta_timestamp(), 0);

        sync_info.update_delta_timestamp(1234567890);
        assert_eq!(sync_info.delta_timestamp(), 1234567890);
    }
}
