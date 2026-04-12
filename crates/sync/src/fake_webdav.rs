// Fake WebDAV client for testing

use neojoplin_core::{WebDavClient, DavEntry, WebDavError};
use futures::io::{Cursor, AsyncRead};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory WebDAV implementation for testing
#[derive(Debug, Clone)]
pub struct FakeWebDavClient {
    files: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    locks: Arc<RwLock<HashMap<String, String>>>,
}

impl FakeWebDavClient {
    pub fn new() -> Self {
        Self {
            files: Arc::new(RwLock::new(HashMap::new())),
            locks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get all files for testing
    pub async fn get_all_files(&self) -> HashMap<String, Vec<u8>> {
        self.files.read().await.clone()
    }

    /// Check if a file exists for testing
    pub async fn contains_file(&self, path: &str) -> bool {
        self.files.read().await.contains_key(path)
    }

    /// Get file content for testing
    pub async fn get_file_content(&self, path: &str) -> Option<Vec<u8>> {
        self.files.read().await.get(path).cloned()
    }

    /// Clear all files for testing
    pub async fn clear(&self) {
        self.files.write().await.clear();
        self.locks.write().await.clear();
    }

    /// List all files in a directory for testing
    pub async fn list_directory(&self, dir: &str) -> Vec<String> {
        let files = self.files.read().await;
        let prefix = dir.trim_end_matches('/');
        files.keys()
            .filter(|path| path.starts_with(prefix) || path.starts_with(&format!("{}/", prefix)))
            .map(|s| s.clone())
            .collect()
    }
}

impl Default for FakeWebDavClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WebDavClient for FakeWebDavClient {
    async fn list(&self, path: &str) -> std::result::Result<Vec<DavEntry>, WebDavError> {
        let files = self.files.read().await;
        let prefix = format!("{}/", path.trim_end_matches('/'));

        let entries: Vec<DavEntry> = files
            .keys()
            .filter(|key| {
                // Only include direct children (not nested paths)
                if key.starts_with(&prefix) {
                    let relative = key.strip_prefix(&prefix).unwrap_or("");
                    // Include if it doesn't contain another slash (direct child) and is not empty
                    !relative.is_empty() && !relative.contains('/')
                } else {
                    false
                }
            })
            .map(|path| DavEntry {
                path: path.clone(),
                is_directory: false,
                size: Some(files.get(path).map(|d| d.len()).unwrap_or(0) as u64),
                modified: Some(0),
                etag: None,
            })
            .collect();

        Ok(entries)
    }

    async fn get(&self, path: &str) -> std::result::Result<Box<dyn AsyncRead + Unpin + Send>, WebDavError> {
        let files = self.files.read().await;
        match files.get(path) {
            Some(data) => Ok(Box::new(Cursor::new(data.clone()))),
            None => Err(WebDavError::NotFound(path.to_string())),
        }
    }

    async fn put(&self, path: &str, body: &[u8], _size: u64) -> std::result::Result<(), WebDavError> {
        let mut files = self.files.write().await;
        files.insert(path.to_string(), body.to_vec());
        Ok(())
    }

    async fn delete(&self, path: &str) -> std::result::Result<(), WebDavError> {
        let mut files = self.files.write().await;
        match files.remove(path) {
            Some(_) => Ok(()),
            None => Err(WebDavError::NotFound(path.to_string())),
        }
    }

    async fn mkcol(&self, path: &str) -> std::result::Result<(), WebDavError> {
        // Create a directory by adding an empty entry
        let dir_path = format!("{}/", path.trim_end_matches('/'));
        let mut files = self.files.write().await;
        files.insert(dir_path, Vec::new());
        Ok(())
    }

    async fn exists(&self, path: &str) -> std::result::Result<bool, WebDavError> {
        let files = self.files.read().await;
        Ok(files.contains_key(path) || files.contains_key(&format!("{}/", path.trim_end_matches('/'))))
    }

    async fn stat(&self, path: &str) -> std::result::Result<DavEntry, WebDavError> {
        let files = self.files.read().await;
        match files.get(path) {
            Some(data) => Ok(DavEntry {
                path: path.to_string(),
                is_directory: false,
                size: Some(data.len() as u64),
                modified: Some(0),
                etag: None,
            }),
            None => Err(WebDavError::NotFound(path.to_string())),
        }
    }

    async fn lock(&self, path: &str, _timeout: std::time::Duration) -> std::result::Result<String, WebDavError> {
        let lock_token = format!("lock_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs());
        let mut locks = self.locks.write().await;
        locks.insert(path.to_string(), lock_token.clone());
        Ok(lock_token)
    }

    async fn refresh_lock(&self, _lock_token: &str) -> std::result::Result<(), WebDavError> {
        // Lock refresh is a no-op for fake implementation
        Ok(())
    }

    async fn unlock(&self, path: &str, _lock_token: &str) -> std::result::Result<(), WebDavError> {
        let mut locks = self.locks.write().await;
        locks.remove(path);
        Ok(())
    }

    async fn mv(&self, from: &str, to: &str) -> std::result::Result<(), WebDavError> {
        let mut files = self.files.write().await;
        match files.remove(from) {
            Some(data) => {
                files.insert(to.to_string(), data);
                Ok(())
            },
            None => Err(WebDavError::NotFound(from.to_string())),
        }
    }

    async fn copy(&self, from: &str, to: &str) -> std::result::Result<(), WebDavError> {
        let files = self.files.read().await;
        match files.get(from) {
            Some(data) => {
                let data_clone = data.clone();
                drop(files);
                let mut files = self.files.write().await;
                files.insert(to.to_string(), data_clone);
                Ok(())
            },
            None => Err(WebDavError::NotFound(from.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fake_webdav_put_and_get() {
        use futures::io::AsyncReadExt;

        let client = FakeWebDavClient::new();

        // Put a file
        let data = b"Hello, World!";
        let result = client.put("/test.txt", data, data.len() as u64).await;
        assert!(result.is_ok());

        // Get the file back
        let mut reader = client.get("/test.txt").await.unwrap();
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer).await.unwrap();
        assert_eq!(String::from_utf8(buffer).unwrap(), "Hello, World!");
    }

    #[tokio::test]
    async fn test_fake_webdav_delete() {
        let client = FakeWebDavClient::new();

        // Put a file
        let data = b"Test data";
        client.put("/test.txt", data, data.len() as u64).await.unwrap();

        // Delete it
        assert!(client.delete("/test.txt").await.is_ok());

        // Should not exist anymore
        assert!(!client.contains_file("/test.txt").await);
    }

    #[tokio::test]
    async fn test_fake_webdav_exists() {
        let client = FakeWebDavClient::new();

        assert!(!client.exists("/test.txt").await.unwrap());

        let data = b"Test";
        client.put("/test.txt", data, 4).await.unwrap();

        assert!(client.exists("/test.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_fake_webdav_mkcol() {
        let client = FakeWebDavClient::new();

        assert!(client.mkcol("/testdir").await.is_ok());
        assert!(client.exists("/testdir").await.unwrap());
    }

    #[tokio::test]
    async fn test_fake_webdav_list() {
        let client = FakeWebDavClient::new();

        // Add some files
        client.put("/dir/file1.txt", b"data1", 5).await.unwrap();
        client.put("/dir/file2.txt", b"data2", 5).await.unwrap();
        client.put("/other/file3.txt", b"data3", 5).await.unwrap();

        // List /dir
        let entries = client.list("/dir").await.unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn test_fake_webdav_lock() {
        let client = FakeWebDavClient::new();

        let token = client.lock("/test.txt", std::time::Duration::from_secs(30)).await.unwrap();
        assert!(token.starts_with("lock_"));

        assert!(client.unlock("/test.txt", &token).await.is_ok());
    }

    #[tokio::test]
    async fn test_fake_webdav_move() {
        let client = FakeWebDavClient::new();

        let data = b"Test data";
        client.put("/from.txt", data, data.len() as u64).await.unwrap();

        assert!(client.mv("/from.txt", "/to.txt").await.is_ok());
        assert!(!client.contains_file("/from.txt").await);
        assert!(client.contains_file("/to.txt").await);
    }

    #[tokio::test]
    async fn test_fake_webdav_copy() {
        let client = FakeWebDavClient::new();

        let data = b"Test data";
        client.put("/from.txt", data, data.len() as u64).await.unwrap();

        assert!(client.copy("/from.txt", "/to.txt").await.is_ok());
        assert!(client.contains_file("/from.txt").await);
        assert!(client.contains_file("/to.txt").await);
    }
}
