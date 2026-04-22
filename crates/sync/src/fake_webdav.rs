use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::io::Cursor;
use tokio::sync::RwLock;

use joplin_domain::{DavEntry, WebDavClient, WebDavError};

#[derive(Debug, Default)]
struct FakeWebDavState {
    directories: BTreeSet<String>,
    files: HashMap<String, Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct FakeWebDavClient {
    state: Arc<RwLock<FakeWebDavState>>,
}

impl FakeWebDavClient {
    pub fn new() -> Self {
        let mut directories = BTreeSet::new();
        directories.insert("/".to_string());
        Self {
            state: Arc::new(RwLock::new(FakeWebDavState {
                directories,
                files: HashMap::new(),
            })),
        }
    }

    pub async fn get_all_files(&self) -> HashMap<String, Vec<u8>> {
        self.state.read().await.files.clone()
    }
}

#[async_trait]
impl WebDavClient for FakeWebDavClient {
    async fn list(&self, path: &str) -> Result<Vec<DavEntry>, WebDavError> {
        let normalized = normalize_dir_path(path);
        let state = self.state.read().await;
        let mut entries = Vec::new();

        for directory in &state.directories {
            if directory == &normalized || directory == "/" {
                continue;
            }
            if let Some(child) = direct_child_path(&normalized, directory) {
                entries.push(DavEntry {
                    path: child,
                    is_directory: true,
                    size: None,
                    modified: None,
                    etag: None,
                });
            }
        }

        for (file_path, contents) in &state.files {
            if let Some(child) = direct_child_path(&normalized, file_path) {
                entries.push(DavEntry {
                    path: child,
                    is_directory: false,
                    size: Some(contents.len() as u64),
                    modified: None,
                    etag: None,
                });
            }
        }

        entries.sort_by(|left, right| left.path.cmp(&right.path));
        entries.dedup_by(|left, right| left.path == right.path);
        Ok(entries)
    }

    async fn get(
        &self,
        path: &str,
    ) -> Result<Box<dyn futures::io::AsyncRead + Unpin + Send>, WebDavError> {
        let normalized = normalize_file_path(path);
        let state = self.state.read().await;
        let data = state
            .files
            .get(&normalized)
            .cloned()
            .ok_or_else(|| WebDavError::NotFound(normalized.clone()))?;
        Ok(Box::new(Cursor::new(data)))
    }

    async fn put(&self, path: &str, body: &[u8], _size: u64) -> Result<(), WebDavError> {
        let normalized = normalize_file_path(path);
        let parent = parent_dir(&normalized);
        let mut state = self.state.write().await;
        ensure_directory(&mut state.directories, &parent);
        state.files.insert(normalized, body.to_vec());
        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<(), WebDavError> {
        let normalized_dir = normalize_dir_path(path);
        let normalized_file = normalize_file_path(path);
        let mut state = self.state.write().await;

        if state.files.remove(&normalized_file).is_some() {
            return Ok(());
        }

        if normalized_dir == "/" {
            state.files.clear();
            state.directories.retain(|dir| dir == "/");
            return Ok(());
        }

        state
            .files
            .retain(|file_path, _| !file_path.starts_with(&(normalized_dir.clone() + "/")));
        state.directories.retain(|dir| {
            dir == "/"
                || !(dir == &normalized_dir || dir.starts_with(&(normalized_dir.clone() + "/")))
        });
        Ok(())
    }

    async fn mkcol(&self, path: &str) -> Result<(), WebDavError> {
        let normalized = normalize_dir_path(path);
        let mut state = self.state.write().await;
        ensure_directory(&mut state.directories, &normalized);
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool, WebDavError> {
        let normalized_dir = normalize_dir_path(path);
        let normalized_file = normalize_file_path(path);
        let state = self.state.read().await;
        Ok(state.files.contains_key(&normalized_file)
            || state.directories.contains(&normalized_dir))
    }

    async fn stat(&self, path: &str) -> Result<DavEntry, WebDavError> {
        let normalized_dir = normalize_dir_path(path);
        let normalized_file = normalize_file_path(path);
        let state = self.state.read().await;

        if let Some(contents) = state.files.get(&normalized_file) {
            return Ok(DavEntry {
                path: normalized_file,
                is_directory: false,
                size: Some(contents.len() as u64),
                modified: None,
                etag: None,
            });
        }

        if state.directories.contains(&normalized_dir) {
            return Ok(DavEntry {
                path: normalized_dir,
                is_directory: true,
                size: None,
                modified: None,
                etag: None,
            });
        }

        Err(WebDavError::NotFound(path.to_string()))
    }

    async fn lock(&self, path: &str, _timeout: Duration) -> Result<String, WebDavError> {
        let token = format!("fake-lock:{}", normalize_file_path(path));
        Ok(token)
    }

    async fn refresh_lock(&self, _lock_token: &str) -> Result<(), WebDavError> {
        Ok(())
    }

    async fn unlock(&self, _path: &str, _lock_token: &str) -> Result<(), WebDavError> {
        Ok(())
    }

    async fn mv(&self, from: &str, to: &str) -> Result<(), WebDavError> {
        let from = normalize_file_path(from);
        let to = normalize_file_path(to);
        let mut state = self.state.write().await;
        let data = state
            .files
            .remove(&from)
            .ok_or_else(|| WebDavError::NotFound(from.clone()))?;
        let parent = parent_dir(&to);
        ensure_directory(&mut state.directories, &parent);
        state.files.insert(to, data);
        Ok(())
    }

    async fn copy(&self, from: &str, to: &str) -> Result<(), WebDavError> {
        let from = normalize_file_path(from);
        let to = normalize_file_path(to);
        let mut state = self.state.write().await;
        let data = state
            .files
            .get(&from)
            .cloned()
            .ok_or_else(|| WebDavError::NotFound(from.clone()))?;
        let parent = parent_dir(&to);
        ensure_directory(&mut state.directories, &parent);
        state.files.insert(to, data);
        Ok(())
    }
}

fn normalize_dir_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        "/".to_string()
    } else {
        format!("/{}", trimmed.trim_matches('/'))
    }
}

fn normalize_file_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        "/".to_string()
    } else {
        format!("/{}", trimmed.trim_matches('/'))
    }
}

fn parent_dir(path: &str) -> String {
    let normalized = normalize_file_path(path);
    match normalized.rsplit_once('/') {
        Some(("", _)) | None => "/".to_string(),
        Some((parent, _)) if parent.is_empty() => "/".to_string(),
        Some((parent, _)) => parent.to_string(),
    }
}

fn ensure_directory(directories: &mut BTreeSet<String>, path: &str) {
    let normalized = normalize_dir_path(path);
    directories.insert("/".to_string());
    if normalized == "/" {
        return;
    }

    let mut current = String::new();
    for segment in normalized.trim_matches('/').split('/') {
        current.push('/');
        current.push_str(segment);
        directories.insert(current.clone());
    }
}

fn direct_child_path(parent: &str, candidate: &str) -> Option<String> {
    let parent = normalize_dir_path(parent);
    let candidate = if candidate == "/" {
        return None;
    } else if candidate.starts_with('/') {
        candidate.to_string()
    } else {
        format!("/{}", candidate)
    };

    let remainder = if parent == "/" {
        candidate.strip_prefix('/')?
    } else {
        candidate.strip_prefix(&(parent.clone() + "/"))?
    };

    if remainder.is_empty() || remainder.contains('/') {
        return None;
    }

    Some(candidate)
}
