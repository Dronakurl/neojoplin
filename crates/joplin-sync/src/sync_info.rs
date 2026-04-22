// Sync info handling for Joplin compatibility
//
// This module implements sync information storage compatible with Joplin's
// info.json format. The info.json file is stored on the WebDAV server and contains
// metadata about the synchronization state including E2EE master keys.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;

/// Sync information stored in info.json for Joplin compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncInfo {
    pub version: i32,

    #[serde(default = "default_app_min_version")]
    pub app_min_version: String,

    #[serde(default)]
    pub e2ee: SyncInfoValueBool,

    #[serde(default)]
    pub active_master_key_id: SyncInfoValueString,

    #[serde(default, rename = "masterKeys")]
    pub master_keys: Vec<MasterKeyInfo>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ppk: Option<serde_json::Value>,

    /// Timestamp of last delta sync for change detection (NeoJoplin extension)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub delta_timestamp: i64,
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}

fn default_app_min_version() -> String {
    "3.0.0".to_string()
}

/// Boolean sync info value with timestamp (Joplin format)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncInfoValueBool {
    #[serde(default)]
    pub value: bool,
    #[serde(default)]
    pub updated_time: i64,
}

/// String sync info value with timestamp (Joplin format)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncInfoValueString {
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub updated_time: i64,
}

/// Integer sync info value with timestamp
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncInfoValueInt {
    #[serde(default)]
    pub value: i64,
    #[serde(default)]
    pub updated_time: i64,
}

/// Master key information as stored in info.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterKeyInfo {
    pub id: String,
    pub created_time: i64,
    pub updated_time: i64,
    pub source_application: String,
    pub encryption_method: i32,
    #[serde(default)]
    pub checksum: String,
    pub content: String,
    #[serde(default, rename = "hasBeenUsed")]
    pub has_been_used: bool,
}

/// Delta context for tracking sync state (NeoJoplin-specific extension)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

impl SyncInfo {
    pub fn new() -> Self {
        Self {
            version: 3,
            app_min_version: "3.0.0".to_string(),
            e2ee: SyncInfoValueBool {
                value: false,
                updated_time: 0,
            },
            active_master_key_id: SyncInfoValueString::default(),
            master_keys: Vec::new(),
            ppk: None,
            delta_timestamp: 0,
        }
    }

    /// Load sync info from remote WebDAV server (reads info.json)
    pub async fn load_from_remote(
        webdav: &dyn joplin_domain::WebDavClient,
        remote_path: &str,
    ) -> Result<Option<Self>> {
        let info_json_path = format!("{}/info.json", remote_path.trim_end_matches('/'));

        let exists = webdav
            .exists(&info_json_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to check info.json existence: {:?}", e))?;

        if !exists {
            return Ok(None);
        }

        let mut content = webdav
            .get(&info_json_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to download info.json: {:?}", e))?;

        use futures::io::AsyncReadExt;
        let mut bytes = Vec::new();
        AsyncReadExt::read_to_end(&mut content, &mut bytes)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read info.json: {:?}", e))?;

        let sync_info: SyncInfo = serde_json::from_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("Failed to parse info.json: {:?}", e))?;

        Ok(Some(sync_info))
    }

    /// Save sync info to remote WebDAV server (writes info.json)
    pub async fn save_to_remote(
        &self,
        webdav: &dyn joplin_domain::WebDavClient,
        remote_path: &str,
    ) -> Result<()> {
        let info_json_path = format!("{}/info.json", remote_path.trim_end_matches('/'));

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize info.json: {:?}", e))?;

        let bytes = content.as_bytes();
        webdav
            .put(&info_json_path, bytes, bytes.len() as u64)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to upload info.json: {:?}", e))?;

        Ok(())
    }

    pub fn key_timestamp(&self, key: &str) -> i64 {
        match key {
            "e2ee" => self.e2ee.updated_time,
            "activeMasterKeyId" => self.active_master_key_id.updated_time,
            _ => 0,
        }
    }

    pub fn delta_timestamp(&self) -> i64 {
        self.delta_timestamp
    }

    pub fn update_delta_timestamp(&mut self) {
        self.delta_timestamp = Utc::now().timestamp_millis();
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
    pub fn generate() -> String {
        format!("neojoplin-{}", Uuid::new_v4())
    }

    pub async fn get_or_generate(client_id_path: &PathBuf) -> Result<String> {
        if let Ok(content) = fs::read_to_string(client_id_path).await {
            return Ok(content.trim().to_string());
        }

        let client_id = Self::generate();
        fs::write(client_id_path, &client_id)
            .await
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
        assert!(!info.e2ee.value);
    }

    #[test]
    fn test_client_id_generation() {
        let id1 = ClientIdManager::generate();
        let id2 = ClientIdManager::generate();
        assert!(id1.starts_with("neojoplin-"));
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_parse_real_joplin_info_json() {
        // This is a real info.json from Joplin CLI 3.5.1
        let json = r#"{
            "version": 3,
            "e2ee": {"value": true, "updatedTime": 1776621665140},
            "activeMasterKeyId": {"value": "b892c8028cb246c5b124ac5880478be9", "updatedTime": 1776621665139},
            "masterKeys": [{
                "checksum": "",
                "encryption_method": 8,
                "content": "{\"salt\":\"abc\",\"iv\":\"def\",\"ct\":\"ghi\"}",
                "created_time": 1776621665137,
                "updated_time": 1776621679942,
                "source_application": "net.cozic.joplin-cli",
                "hasBeenUsed": true,
                "id": "b892c8028cb246c5b124ac5880478be9"
            }],
            "appMinVersion": "3.0.0"
        }"#;

        let info: SyncInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.version, 3);
        assert!(info.e2ee.value);
        assert_eq!(
            info.active_master_key_id.value,
            "b892c8028cb246c5b124ac5880478be9"
        );
        assert_eq!(info.master_keys.len(), 1);
        assert_eq!(info.master_keys[0].id, "b892c8028cb246c5b124ac5880478be9");
        assert!(info.master_keys[0].has_been_used);
        assert_eq!(info.master_keys[0].encryption_method, 8);
    }

    #[test]
    fn test_delta_context_default() {
        let ctx = DeltaContext::default();
        assert_eq!(ctx.timestamp, 0);
    }
}
