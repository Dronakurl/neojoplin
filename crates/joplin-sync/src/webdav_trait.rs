// WebDavClient trait implementation for ReqwestWebDavClient

use crate::ReqwestWebDavClient;
use async_trait::async_trait;
use futures::io::{AsyncRead, Cursor};
use joplin_domain::{DavEntry, WebDavClient, WebDavError};

#[async_trait]
impl WebDavClient for ReqwestWebDavClient {
    async fn list(&self, path: &str) -> std::result::Result<Vec<DavEntry>, WebDavError> {
        // Use list_with_timestamps to get modification times for delta detection
        let files = self
            .list_with_timestamps_impl(path)
            .await
            .map_err(|e| WebDavError::RequestFailed(format!("List failed: {:?}", e)))?;

        let entries = files
            .into_iter()
            .map(|(filename, modified)| {
                let full_path = if path.ends_with('/') {
                    format!("{}{}", path, filename)
                } else {
                    format!("{}/{}", path, filename)
                };

                DavEntry {
                    path: full_path,
                    is_directory: false,
                    size: None,
                    modified,
                    etag: None,
                }
            })
            .collect();

        Ok(entries)
    }

    async fn get(
        &self,
        path: &str,
    ) -> std::result::Result<Box<dyn AsyncRead + Unpin + Send>, WebDavError> {
        self.get_impl(path)
            .await
            .map(|data| Box::new(Cursor::new(data)) as Box<dyn AsyncRead + Unpin + Send>)
    }

    async fn put(
        &self,
        path: &str,
        body: &[u8],
        _size: u64,
    ) -> std::result::Result<(), WebDavError> {
        self.put_impl(path, body).await
    }

    async fn delete(&self, path: &str) -> std::result::Result<(), WebDavError> {
        self.delete_impl(path).await
    }

    async fn mkcol(&self, path: &str) -> std::result::Result<(), WebDavError> {
        self.mkdir_impl(path).await
    }

    async fn exists(&self, path: &str) -> std::result::Result<bool, WebDavError> {
        self.exists_impl(path).await
    }

    async fn stat(&self, path: &str) -> std::result::Result<DavEntry, WebDavError> {
        let meta = self.get_file_meta_impl(path).await?;

        Ok(DavEntry {
            path: meta.path.clone(),
            is_directory: meta.is_dir,
            size: if meta.size >= 0 {
                Some(meta.size as u64)
            } else {
                None
            },
            modified: if meta.modified != 0 {
                Some(meta.modified)
            } else {
                None
            },
            etag: None,
        })
    }

    async fn lock(
        &self,
        path: &str,
        _timeout: std::time::Duration,
    ) -> std::result::Result<String, WebDavError> {
        // Simple lock implementation - just create a lock file
        let lock_token = format!(
            "lock_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
        let lock_path = format!("{}.lock", path);

        // Create lock file with token
        self.put_impl(&lock_path, lock_token.as_bytes()).await?;
        Ok(lock_token)
    }

    async fn refresh_lock(&self, _lock_token: &str) -> std::result::Result<(), WebDavError> {
        // Lock refresh is a no-op for simple implementation
        Ok(())
    }

    async fn unlock(&self, path: &str, _lock_token: &str) -> std::result::Result<(), WebDavError> {
        let lock_path = format!("{}.lock", path);
        self.delete_impl(&lock_path).await
    }

    async fn mv(&self, from: &str, to: &str) -> std::result::Result<(), WebDavError> {
        // Download from source
        let data = self.get_impl(from).await?;

        // Upload to destination
        self.put_impl(to, &data).await?;

        // Delete source
        self.delete_impl(from).await
    }

    async fn copy(&self, from: &str, to: &str) -> std::result::Result<(), WebDavError> {
        // Download from source
        let data = self.get_impl(from).await?;

        // Upload to destination
        self.put_impl(to, &data).await
    }
}
