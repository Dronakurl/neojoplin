// Sync info handling for Joplin compatibility
//
// This module implements sync information storage compatible with Joplin's
// sync.json format. The sync.json file is stored on the WebDAV server and contains
// metadata about the synchronization state.

use serde::{Deserialize, Serialize};
use anyhow::Result;
use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;
use chrono::Utc;
use futures::io::AsyncReadExt;

/// Sync information stored in sync.json for Joplin compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncInfo {
    pub version: i32,

    #[serde(default = "default_app_min_version")]
    pub app_min_version: String,

    #[serde(default)]
    pub e2ee: SyncInfoValueBool,

    #[serde(default)]
    pub active_master_key_id: SyncInfoValueString,

    #[serde(default)]
    pub master_keys: Vec<MasterKeyInfo>,

    #[serde(default)]
    pub revision_service_enabled: SyncInfoValueBool,

    #[serde(default = "default_revision_ttl")]
    pub revision_service_ttl_days: SyncInfoValueInt,
}

fn default_app_min_version() -> String {
    "3.0.0".to_string()
}

fn default_revision_ttl() -> SyncInfoValueInt {
    SyncInfoValueInt {
        value: 90,
        updated_time: 0,
    }
}

/// Boolean sync info value with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncInfoValueBool {
    #[serde(default = "default_bool")]
    pub value: bool,

    #[serde(default)]
    pub updated_time: i64,
}

impl Default for SyncInfoValueBool {
    fn default() -> Self {
        Self {
            value: false,
            updated_time: 0,
        }
    }
}

fn default_bool() -> bool {
    false
}

/// String sync info value with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncInfoValueString {
    #[serde(default = "default_string")]
    pub value: String,

    #[serde(default)]
    pub updated_time: i64,
}

impl Default for SyncInfoValueString {
    fn default() -> Self {
        Self {
            value: String::new(),
            updated_time: 0,
        }
    }
}

fn default_string() -> String {
    String::new()
}

/// Integer sync info value with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncInfoValueInt {
    #[serde(default)]
    pub value: i64,

    #[serde(default)]
    pub updated_time: i64,
}

impl Default for SyncInfoValueInt {
    fn default() -> Self {
        Self {
            value: 0,
            updated_time: 0,
        }
    }
}

/// Master key information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterKeyInfo {
    pub id: String,
    pub created_time: i64,
    pub updated_time: i64,
    pub source_application: String,
    pub encryption_method: i32,
    pub checksum: String,
    pub content: String,

    #[serde(default)]
    pub has_been_used: bool,

    #[serde(default)]
    pub enabled: bool,
}

/// Delta context for tracking sync state (NeoJoplin-specific extension)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaContext {
    #[serde(default)]
    pub timestamp: i64,

    #[serde(default)]
    pub files_at_timestamp: Option<Vec<String>>,

    #[serde(default)]
    pub stats_cache: Option<serde_json::Value>,

    #[serde(default)]
    pub stat_ids_cache: Option<serde_json::Value>,
}

impl Default for DeltaContext {
    fn default() -> Self {
        Self {
            timestamp: 0,
            files_at_timestamp: None,
            stats_cache: None,
            stat_ids_cache: None,
        }
    }
}

impl SyncInfo {
    /// Create a new SyncInfo with default values
    pub fn new() -> Self {
        Self {
            version: 3,
            app_min_version: "3.0.0".to_string(),
            e2ee: SyncInfoValueBool {
                value: false,
                updated_time: 0,
            },
            active_master_key_id: SyncInfoValueString {
                value: String::new(),
                updated_time: 0,
            },
            master_keys: Vec::new(),
            revision_service_enabled: SyncInfoValueBool {
                value: true,
                updated_time: 0,
            },
            revision_service_ttl_days: default_revision_ttl(),
        }
    }

    /// Load sync info from remote WebDAV server
    pub async fn load_from_remote(webdav: &dyn joplin_domain::WebDavClient, remote_path: &str) -> Result<Option<Self>> {
        let sync_json_path = format!("{}/sync.json", remote_path.trim_end_matches('/'));

        let exists = webdav.exists(&sync_json_path).await
            .map_err(|e| anyhow::anyhow!("Failed to check sync.json existence: {:?}", e))?;

        if !exists {
            return Ok(None);
        }

        let mut content = webdav.get(&sync_json_path).await
            .map_err(|e| anyhow::anyhow!("Failed to download sync.json: {:?}", e))?;

        // Read content into bytes using futures::AsyncRead
        let mut bytes = Vec::new();
        futures::io::AsyncReadExt::read_to_end(&mut content, &mut bytes).await
            .map_err(|e| anyhow::anyhow!("Failed to read sync.json: {:?}", e))?;

        let sync_info: SyncInfo = serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("Failed to parse sync.json: {:?}", e))?;

        Ok(Some(sync_info))
    }

    /// Save sync info to remote WebDAV server
    pub async fn save_to_remote(&self, webdav: &dyn joplin_domain::WebDavClient, remote_path: &str) -> Result<()> {
        let sync_json_path = format!("{}/sync.json", remote_path.trim_end_matches('/'));

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize sync.json: {:?}", e))?;

        let bytes = content.as_bytes();
        webdav.put(&sync_json_path, bytes, bytes.len() as u64).await
            .map_err(|e| anyhow::anyhow!("Failed to upload sync.json: {:?}", e))?;

        Ok(())
    }

    /// Get key timestamp for conflict resolution
    pub fn key_timestamp(&self, key: &str) -> i64 {
        match key {
            "e2ee" => self.e2ee.updated_time,
            "activeMasterKeyId" => self.active_master_key_id.updated_time,
            "revisionServiceEnabled" => self.revision_service_enabled.updated_time,
            "revisionServiceTtlDays" => self.revision_service_ttl_days.updated_time,
            _ => 0,
        }
    }

    /// Get delta timestamp (legacy compatibility)
    pub fn delta_timestamp(&self) -> i64 {
        // Return current time as fallback
        Utc::now().timestamp_millis()
    }

    /// Update delta timestamp (legacy compatibility)
    pub fn update_delta_timestamp(&mut self) {
        // Delta timestamp is handled separately
    }
}

impl Default for SyncInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Client ID manager for generating unique client identifiers
pub struct ClientIdManager;

impl ClientIdManager {
    /// Generate a new client ID
    pub fn generate() -> String {
        format!("neojoplin-{}", Uuid::new_v4())
    }

    /// Get or generate a persistent client ID
    pub async fn get_or_generate(client_id_path: &PathBuf) -> Result<String> {
        if let Ok(content) = fs::read_to_string(client_id_path).await {
            return Ok(content.trim().to_string());
        }

        let client_id = Self::generate();
        fs::write(client_id_path, &client_id).await
            .map_err(|e| anyhow::anyhow!("Failed to write client_id file: {:?}", e))?;

        Ok(client_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_info_default() {
        let info = SyncInfo::new();
        assert_eq!(info.version, 3);
        assert_eq!(info.app_min_version, "3.0.0");
        assert_eq!(info.e2ee.value, false);
    }

    #[test]
    fn test_client_id_generation() {
        let id1 = ClientIdManager::generate();
        let id2 = ClientIdManager::generate();

        assert!(id1.starts_with("neojoplin-"));
        assert!(id2.starts_with("neojoplin-"));
        assert_ne!(id1, id2);
    }
}
