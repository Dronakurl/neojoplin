// Sync info handling for Joplin compatibility
//
// This module implements sync information storage compatible with Joplin's
// info.json format. The info.json file is stored on the WebDAV server and contains
// metadata about the synchronization state including E2EE master keys.

use chrono::Utc;
use joplin_domain::{WebDavClient, WebDavError};
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::fs;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SyncInfoError {
    #[error("failed to check info.json existence: {0}")]
    CheckFailed(WebDavError),

    #[error("failed to download info.json: {0}")]
    DownloadFailed(WebDavError),

    #[error("failed to read info.json: {0}")]
    ReadFailed(std::io::Error),

    #[error("failed to parse info.json: {0}")]
    ParseFailed(serde_json::Error),

    #[error("failed to serialize info.json: {0}")]
    SerializeFailed(serde_json::Error),

    #[error("failed to upload info.json: {0}")]
    UploadFailed(WebDavError),
}

#[derive(Debug, Error)]
pub enum ClientIdError {
    #[error("failed to read client ID from {path}: {source}")]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("client ID file is blank: {0}")]
    InvalidClientId(PathBuf),

    #[error("failed to create client ID directory {path}: {source}")]
    CreateParentDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to persist client ID to {path}: {source}")]
    PersistFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

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
    #[serde(default = "default_master_key_enabled")]
    pub enabled: i32,
}

fn default_master_key_enabled() -> i32 {
    1
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
        webdav: &dyn WebDavClient,
        remote_path: &str,
    ) -> Result<Option<Self>, SyncInfoError> {
        let info_json_path = format!("{}/info.json", remote_path.trim_end_matches('/'));

        let exists = webdav
            .exists(&info_json_path)
            .await
            .map_err(SyncInfoError::CheckFailed)?;

        if !exists {
            return Ok(None);
        }

        let mut content = match webdav.get(&info_json_path).await {
            Ok(content) => content,
            Err(WebDavError::NotFound(_)) => return Ok(None),
            Err(err) => return Err(SyncInfoError::DownloadFailed(err)),
        };

        use futures::io::AsyncReadExt;
        let mut bytes = Vec::new();
        AsyncReadExt::read_to_end(&mut content, &mut bytes)
            .await
            .map_err(SyncInfoError::ReadFailed)?;

        let sync_info: SyncInfo =
            serde_json::from_slice(&bytes).map_err(SyncInfoError::ParseFailed)?;

        Ok(Some(sync_info))
    }

    /// Save sync info to remote WebDAV server (writes info.json)
    pub async fn save_to_remote(
        &self,
        webdav: &dyn WebDavClient,
        remote_path: &str,
    ) -> Result<(), SyncInfoError> {
        let info_json_path = format!("{}/info.json", remote_path.trim_end_matches('/'));

        let content = serde_json::to_string_pretty(self).map_err(SyncInfoError::SerializeFailed)?;

        let bytes = content.as_bytes();
        webdav
            .put(&info_json_path, bytes, bytes.len() as u64)
            .await
            .map_err(SyncInfoError::UploadFailed)?;

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

    pub async fn get_or_generate(client_id_path: &Path) -> Result<String, ClientIdError> {
        match fs::read_to_string(client_id_path).await {
            Ok(content) => {
                let client_id = content.trim();
                if client_id.is_empty() {
                    return Err(ClientIdError::InvalidClientId(client_id_path.to_path_buf()));
                }

                return Ok(client_id.to_string());
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                return Err(ClientIdError::ReadFailed {
                    path: client_id_path.to_path_buf(),
                    source: err,
                });
            }
        }

        if let Some(parent) = client_id_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|source| ClientIdError::CreateParentDir {
                    path: parent.to_path_buf(),
                    source,
                })?;
        }

        let client_id = Self::generate();
        fs::write(client_id_path, &client_id)
            .await
            .map_err(|source| ClientIdError::PersistFailed {
                path: client_id_path.to_path_buf(),
                source,
            })?;

        Ok(client_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::io::Cursor;
    use joplin_domain::{DavEntry, WebDavError};
    use std::sync::atomic::{AtomicBool, Ordering};
    use tempfile::tempdir;
    use tokio::sync::Mutex;

    #[derive(Default)]
    struct RaceyWebDav {
        exists_called: AtomicBool,
    }

    #[async_trait]
    impl WebDavClient for RaceyWebDav {
        async fn list(&self, _path: &str) -> Result<Vec<DavEntry>, WebDavError> {
            unimplemented!()
        }

        async fn get(
            &self,
            path: &str,
        ) -> Result<Box<dyn futures::io::AsyncRead + Unpin + Send>, WebDavError> {
            if self.exists_called.load(Ordering::SeqCst) {
                Err(WebDavError::NotFound(path.to_string()))
            } else {
                Ok(Box::new(Cursor::new(Vec::new())))
            }
        }

        async fn put(&self, _path: &str, _body: &[u8], _size: u64) -> Result<(), WebDavError> {
            unimplemented!()
        }

        async fn delete(&self, _path: &str) -> Result<(), WebDavError> {
            unimplemented!()
        }

        async fn mkcol(&self, _path: &str) -> Result<(), WebDavError> {
            unimplemented!()
        }

        async fn exists(&self, _path: &str) -> Result<bool, WebDavError> {
            self.exists_called.store(true, Ordering::SeqCst);
            Ok(true)
        }

        async fn stat(&self, _path: &str) -> Result<DavEntry, WebDavError> {
            unimplemented!()
        }

        async fn lock(
            &self,
            _path: &str,
            _timeout: std::time::Duration,
        ) -> Result<String, WebDavError> {
            unimplemented!()
        }

        async fn refresh_lock(&self, _lock_token: &str) -> Result<(), WebDavError> {
            unimplemented!()
        }

        async fn unlock(&self, _path: &str, _lock_token: &str) -> Result<(), WebDavError> {
            unimplemented!()
        }

        async fn mv(&self, _from: &str, _to: &str) -> Result<(), WebDavError> {
            unimplemented!()
        }

        async fn copy(&self, _from: &str, _to: &str) -> Result<(), WebDavError> {
            unimplemented!()
        }
    }

    #[derive(Default)]
    struct MemoryWebDav {
        exists: bool,
        bytes: Mutex<Vec<u8>>,
    }

    #[async_trait]
    impl WebDavClient for MemoryWebDav {
        async fn list(&self, _path: &str) -> Result<Vec<DavEntry>, WebDavError> {
            unimplemented!()
        }

        async fn get(
            &self,
            path: &str,
        ) -> Result<Box<dyn futures::io::AsyncRead + Unpin + Send>, WebDavError> {
            if !self.exists {
                return Err(WebDavError::NotFound(path.to_string()));
            }

            Ok(Box::new(Cursor::new(self.bytes.lock().await.clone())))
        }

        async fn put(&self, _path: &str, body: &[u8], _size: u64) -> Result<(), WebDavError> {
            let mut bytes = self.bytes.lock().await;
            bytes.clear();
            bytes.extend_from_slice(body);
            Ok(())
        }

        async fn delete(&self, _path: &str) -> Result<(), WebDavError> {
            unimplemented!()
        }

        async fn mkcol(&self, _path: &str) -> Result<(), WebDavError> {
            unimplemented!()
        }

        async fn exists(&self, _path: &str) -> Result<bool, WebDavError> {
            Ok(self.exists)
        }

        async fn stat(&self, _path: &str) -> Result<DavEntry, WebDavError> {
            unimplemented!()
        }

        async fn lock(
            &self,
            _path: &str,
            _timeout: std::time::Duration,
        ) -> Result<String, WebDavError> {
            unimplemented!()
        }

        async fn refresh_lock(&self, _lock_token: &str) -> Result<(), WebDavError> {
            unimplemented!()
        }

        async fn unlock(&self, _path: &str, _lock_token: &str) -> Result<(), WebDavError> {
            unimplemented!()
        }

        async fn mv(&self, _from: &str, _to: &str) -> Result<(), WebDavError> {
            unimplemented!()
        }

        async fn copy(&self, _from: &str, _to: &str) -> Result<(), WebDavError> {
            unimplemented!()
        }
    }

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

    #[tokio::test]
    async fn load_from_remote_treats_get_not_found_after_exists_as_missing() {
        let webdav = RaceyWebDav::default();

        let sync_info = SyncInfo::load_from_remote(&webdav, "/remote")
            .await
            .expect("load should not fail when info.json disappears after exists()");

        assert!(sync_info.is_none());
    }

    #[tokio::test]
    async fn save_to_remote_round_trips_joplin_shape() {
        let webdav = MemoryWebDav {
            exists: true,
            ..Default::default()
        };
        let mut sync_info = SyncInfo::new();
        sync_info.e2ee.value = true;
        sync_info.e2ee.updated_time = 1234;

        sync_info
            .save_to_remote(&webdav, "/remote")
            .await
            .expect("save should work");

        let saved = String::from_utf8(webdav.bytes.lock().await.clone()).expect("utf8");
        assert!(saved.contains("\"appMinVersion\""));
        assert!(saved.contains("\"e2ee\""));
    }

    #[tokio::test]
    async fn client_id_manager_persists_generated_id() {
        let tempdir = tempdir().expect("tempdir");
        let path = tempdir.path().join("nested").join("client_id");

        let first = ClientIdManager::get_or_generate(&path)
            .await
            .expect("first call should generate an ID");
        let second = ClientIdManager::get_or_generate(&path)
            .await
            .expect("second call should reuse the stored ID");

        assert!(first.starts_with("neojoplin-"));
        assert_eq!(first, second);
    }

    #[tokio::test]
    async fn client_id_manager_rejects_blank_ids() {
        let tempdir = tempdir().expect("tempdir");
        let path = tempdir.path().join("client_id");
        fs::write(&path, "   \n").await.expect("write blank ID");

        let err = ClientIdManager::get_or_generate(&path)
            .await
            .expect_err("blank client IDs must be rejected");

        assert!(matches!(err, ClientIdError::InvalidClientId(ref invalid) if invalid == &path));
    }
}
