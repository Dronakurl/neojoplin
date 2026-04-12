// WebDavClient trait implementation for ReqwestWebDavClient

use crate::ReqwestWebDavClient;
use neojoplin_core::{WebDavClient, DavEntry, WebDavError};
use futures::io::{Cursor, AsyncRead};
use async_trait::async_trait;

#[async_trait]
impl WebDavClient for ReqwestWebDavClient {
    async fn list(&self, path: &str) -> std::result::Result<Vec<DavEntry>, WebDavError> {
        // Use the list_impl method and convert results to DavEntry
        let files = self.list_impl(path).await
            .map_err(|e| WebDavError::RequestFailed(format!("List failed: {:?}", e)))?;

        let entries = files.into_iter().map(|filename| {
            // list_impl returns just filenames, so we need to construct the full path
            let full_path = if path.ends_with('/') {
                format!("{}{}", path, filename)
            } else {
                format!("{}/{}", path, filename)
            };

            DavEntry {
                path: full_path,
                is_directory: false, // We can't easily determine this from just the filename
                size: None,           // We don't have size info from list_impl
                modified: None,       // We don't have timestamp info from list_impl
                etag: None,
            }
        }).collect();

        Ok(entries)
    }

    async fn get(&self, path: &str) -> std::result::Result<Box<dyn AsyncRead + Unpin + Send>, WebDavError> {
        match self.get_impl(path).await {
            Ok(data) => Ok(Box::new(Cursor::new(data))),
            Err(e) => Err(WebDavError::RequestFailed(format!("{:?}", e))),
        }
    }

    async fn put(&self, path: &str, body: &[u8], size: u64) -> std::result::Result<(), WebDavError> {
        match self.put_impl(path, body).await {
            Ok(_) => Ok(()),
            Err(e) => Err(WebDavError::RequestFailed(format!("{:?}", e))),
        }
    }

    async fn delete(&self, path: &str) -> std::result::Result<(), WebDavError> {
        match self.delete_impl(path).await {
            Ok(_) => Ok(()),
            Err(e) => Err(WebDavError::RequestFailed(format!("{:?}", e))),
        }
    }

    async fn mkcol(&self, path: &str) -> std::result::Result<(), WebDavError> {
        match self.mkdir_impl(path).await {
            Ok(_) => Ok(()),
            Err(e) => Err(WebDavError::RequestFailed(format!("{:?}", e))),
        }
    }

    async fn exists(&self, path: &str) -> std::result::Result<bool, WebDavError> {
        match self.exists_impl(path).await {
            Ok(result) => Ok(result),
            Err(e) => Err(WebDavError::RequestFailed(format!("{:?}", e))),
        }
    }

    async fn stat(&self, path: &str) -> std::result::Result<DavEntry, WebDavError> {
        let meta = match self.get_file_meta_impl(path).await {
            Ok(meta) => meta,
            Err(e) => return Err(WebDavError::RequestFailed(format!("{:?}", e))),
        };

        Ok(DavEntry {
            path: meta.path.clone(),
            is_directory: meta.is_dir,
            size: if meta.size >= 0 { Some(meta.size as u64) } else { None },
            modified: if meta.modified != 0 { Some(meta.modified) } else { None },
            etag: None,
        })
    }

    async fn lock(&self, path: &str, _timeout: std::time::Duration) -> std::result::Result<String, WebDavError> {
        // Simple lock implementation - just create a lock file
        let lock_token = format!("lock_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
        let lock_path = format!("{}.lock", path);

        // Create lock file with token
        match self.put_impl(&lock_path, lock_token.as_bytes()).await {
            Ok(_) => Ok(lock_token),
            Err(e) => Err(WebDavError::RequestFailed(format!("{:?}", e))),
        }
    }

    async fn refresh_lock(&self, _lock_token: &str) -> std::result::Result<(), WebDavError> {
        // Lock refresh is a no-op for simple implementation
        Ok(())
    }

    async fn unlock(&self, path: &str, _lock_token: &str) -> std::result::Result<(), WebDavError> {
        let lock_path = format!("{}.lock", path);
        match self.delete_impl(&lock_path).await {
            Ok(_) => Ok(()),
            Err(e) => Err(WebDavError::RequestFailed(format!("{:?}", e))),
        }
    }

    async fn mv(&self, from: &str, to: &str) -> std::result::Result<(), WebDavError> {
        // Download from source
        let data = match self.get_impl(from).await {
            Ok(data) => data,
            Err(e) => return Err(WebDavError::RequestFailed(format!("{:?}", e))),
        };

        // Upload to destination
        match self.put_impl(to, &data).await {
            Ok(_) => {},
            Err(e) => return Err(WebDavError::RequestFailed(format!("{:?}", e))),
        }

        // Delete source
        match self.delete_impl(from).await {
            Ok(_) => Ok(()),
            Err(e) => Err(WebDavError::RequestFailed(format!("{:?}", e))),
        }
    }

    async fn copy(&self, from: &str, to: &str) -> std::result::Result<(), WebDavError> {
        // Download from source
        let data = match self.get_impl(from).await {
            Ok(data) => data,
            Err(e) => return Err(WebDavError::RequestFailed(format!("{:?}", e))),
        };

        // Upload to destination
        match self.put_impl(to, &data).await {
            Ok(_) => Ok(()),
            Err(e) => Err(WebDavError::RequestFailed(format!("{:?}", e))),
        }
    }
}
