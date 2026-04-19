// WebDAV client implementation using reqwest

use reqwest::{Client, header};
use std::time::Duration;
use joplin_domain::{WebDavError, FileMeta};
use std::net::{IpAddr, Ipv4Addr};

// Use WebDavError directly in this module
type WebDavResult<T> = std::result::Result<T, WebDavError>;
// Create a Result alias for convenience in this module
type Result<T> = WebDavResult<T>;

// Custom Depth header for WebDAV
static DEPTH: header::HeaderName = header::HeaderName::from_static("depth");

// Custom Engine trait import for base64
use base64::Engine;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// WebDAV configuration
#[derive(Debug, Clone)]
pub struct WebDavConfig {
    pub base_url: String,
    pub username: String,
    pub password: String,
}

impl WebDavConfig {
    pub fn new(base_url: String, username: String, password: String) -> Self {
        Self {
            base_url,
            username,
            password,
        }
    }
}

/// Reqwest-based WebDAV client
#[derive(Debug, Clone)]
pub struct ReqwestWebDavClient {
    client: Client,
    config: WebDavConfig,
}

impl ReqwestWebDavClient {
    pub fn new(config: WebDavConfig) -> WebDavResult<Self> {
        // Force IPv4 by setting local address to IPv4 unspecified
        // This ensures the client only uses IPv4 for connections
        let ipv4 = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
        let client = Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .local_address(ipv4)
            .build()
            .map_err(|e| WebDavError::ConnectionFailed(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { client, config })
    }

    fn build_url(&self, path: &str) -> String {
        let path = path.trim_start_matches('/');
        format!("{}/{}", self.config.base_url.trim_end_matches('/'), path)
    }

    fn build_auth(&self) -> header::HeaderValue {
        let auth = format!("{}:{}", self.config.username, self.config.password);
        let encoded = base64::engine::general_purpose::STANDARD.encode(auth);
        header::HeaderValue::from_str(&format!("Basic {}", encoded))
            .expect("Failed to create auth header")
    }

    async fn request(&self, method: reqwest::Method, path: &str, body: Option<Vec<u8>>) -> Result<reqwest::Response> {
        let url = self.build_url(path);
        let mut request = self.client.request(method, &url);

        request = request.header(
            header::AUTHORIZATION,
            self.build_auth()
        );

        if let Some(body) = body {
            request = request.body(body);
        }

        request.send().await
            .map_err(|e| WebDavError::ConnectionFailed(format!("Request failed: {}", e)))
    }

    pub async fn get_impl(&self, path: &str) -> WebDavResult<Vec<u8>> {
        let response = self.request(reqwest::Method::GET, path, None).await?;

        if response.status().is_success() {
            Ok(response.bytes().await
                .map_err(|e| WebDavError::RequestFailed(format!("Failed to read response: {}", e)))?
                .to_vec())
        } else if response.status().as_u16() == 404 {
            Err(WebDavError::NotFound(path.to_string()))
        } else {
            Err(WebDavError::RequestFailed(format!("GET failed with status: {}", response.status())))
        }
    }

    pub async fn put_impl(&self, path: &str, data: &[u8]) -> Result<()> {
        let response = self.request(reqwest::Method::PUT, path, Some(data.to_vec())).await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(WebDavError::RequestFailed(format!("PUT failed with status: {}", response.status())))
        }
    }

    pub async fn delete_impl(&self, path: &str) -> Result<()> {
        let response = self.request(reqwest::Method::DELETE, path, None).await?;

        if response.status().is_success() || response.status().as_u16() == 404 {
            Ok(())
        } else {
            Err(WebDavError::RequestFailed(format!("DELETE failed with status: {}", response.status())))
        }
    }

    pub async fn mkdir_impl(&self, path: &str) -> Result<()> {
        let method = reqwest::Method::from_bytes(b"MKCOL")
            .map_err(|e| WebDavError::RequestFailed(format!("Invalid method: {}", e)))?;
        let response = self.request(method, path, None).await?;

        if response.status().is_success() || response.status().as_u16() == 405 { // 405 Method Not Allowed = directory exists
            Ok(())
        } else {
            Err(WebDavError::RequestFailed(format!("MKCOL failed with status: {}", response.status())))
        }
    }

    pub async fn list_impl(&self, path: &str) -> Result<Vec<String>> {
        let url = self.build_url(path);
        let method = reqwest::Method::from_bytes(b"PROPFIND")
            .map_err(|e| WebDavError::RequestFailed(format!("Invalid method: {}", e)))?;
        let response = self.client
            .request(method, &url)
            .header(header::AUTHORIZATION, self.build_auth())
            .header(&DEPTH, "1")
            .body(r#"<?xml version="1.0" encoding="utf-8" ?>
                <D:propfind xmlns:D="DAV:">
                    <D:prop>
                        <D:displayname/>
                    </D:prop>
                </D:propfind>"#)
            .send()
            .await
            .map_err(|e| WebDavError::RequestFailed(format!("PROPFIND failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(WebDavError::RequestFailed(format!("PROPFIND failed with status: {}", response.status())));
        }

        let body = response.text().await
            .map_err(|e| WebDavError::RequestFailed(format!("Failed to read response: {}", e)))?;

        // Parse PROPFIND response to extract file names
        let files = parse_propfind_response(&body, &self.config.base_url)?;
        Ok(files)
    }

    pub async fn exists_impl(&self, path: &str) -> Result<bool> {
        match self.get_file_meta_impl(path).await {
            Ok(_) => Ok(true),
            Err(WebDavError::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub async fn get_file_meta_impl(&self, path: &str) -> Result<FileMeta> {
        let url = self.build_url(path);
        let method = reqwest::Method::from_bytes(b"PROPFIND")
            .map_err(|e| WebDavError::RequestFailed(format!("Invalid method: {}", e)))?;
        let response = self.client
            .request(method, &url)
            .header(header::AUTHORIZATION, self.build_auth())
            .header(&DEPTH, "0")
            .body(r#"<?xml version="1.0" encoding="utf-8" ?>
                <D:propfind xmlns:D="DAV:">
                    <D:prop>
                        <D:getcontentlength/>
                        <D:getlastmodified/>
                        <D:resourcetype/>
                    </D:prop>
                </D:propfind>"#)
            .send()
            .await
            .map_err(|e| WebDavError::RequestFailed(format!("PROPFIND failed: {}", e)))?;

        if response.status().as_u16() == 404 {
            return Err(WebDavError::NotFound(path.to_string()));
        }

        if !response.status().is_success() {
            return Err(WebDavError::RequestFailed(format!("PROPFIND failed with status: {}", response.status())));
        }

        let body = response.text().await
            .map_err(|e| WebDavError::RequestFailed(format!("Failed to read response: {}", e)))?;

        parse_file_meta(&body, path)
    }
}

/// Simple PROPFIND response parser
fn parse_propfind_response(body: &str, _base_url: &str) -> WebDavResult<Vec<String>> {
    let mut files = Vec::new();

    // Simple XML parsing - extract all href tags
    let mut search_start = 0;
    while let Some(href_start) = body[search_start..].find("<D:href>") {
        let actual_start = search_start + href_start;
        if let Some(href_end) = body[actual_start + 8..].find("</D:href>") {
            let actual_end = actual_start + 8 + href_end;
            let href = &body[actual_start + 8..actual_end];

            // Filter out directory entries (those ending with /)
            if !href.ends_with('/') {
                if let Some(filename) = href.rsplit('/').next() {
                    if !filename.is_empty() {
                        files.push(filename.to_string());
                    }
                }
            }

            search_start = actual_end + 9; // Move past this href tag
        } else {
            break;
        }
    }

    Ok(files)
}

/// Parse file metadata from PROPFIND response
fn parse_file_meta(body: &str, path: &str) -> WebDavResult<FileMeta> {
    let mut size = None;
    let mut modified = None;
    let mut is_dir = false;

    for line in body.lines() {
        if line.contains("<D:getcontentlength>") {
            if let Some(start) = line.find("<D:getcontentlength>") {
                if let Some(end) = line.find("</D:getcontentlength>") {
                    let size_str = &line[start + 20..end];
                    size = Some(size_str.parse::<i64>().unwrap_or(0));
                }
            }
        } else if line.contains("<D:getlastmodified>") {
            if let Some(start) = line.find("<D:getlastmodified>") {
                if let Some(end) = line.find("</D:getlastmodified>") {
                    let modified_str = &line[start + 21..end];
                    // Parse HTTP date format
                    if let Ok(date) = chrono::DateTime::parse_from_rfc2822(modified_str) {
                        modified = Some(date.timestamp_millis());
                    }
                }
            }
        } else if line.contains("<D:collection/>") || line.contains("<D:folder/>") {
            is_dir = true;
        }
    }

    Ok(FileMeta {
        path: path.to_string(),
        size: size.unwrap_or(0),
        modified: modified.unwrap_or(0),
        is_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_new() {
        let config = WebDavConfig::new(
            "https://example.com/webdav".to_string(),
            "user".to_string(),
            "pass".to_string(),
        );
        assert_eq!(config.base_url, "https://example.com/webdav");
        assert_eq!(config.username, "user");
        assert_eq!(config.password, "pass");
    }

    #[test]
    fn test_build_url() {
        let config = WebDavConfig::new(
            "https://example.com/webdav/".to_string(),
            "user".to_string(),
            "pass".to_string(),
        );
        let client = ReqwestWebDavClient::new(config).unwrap();

        assert_eq!(client.build_url("test.txt"), "https://example.com/webdav/test.txt");
        assert_eq!(client.build_url("/test.txt"), "https://example.com/webdav/test.txt");
        assert_eq!(client.build_url("folder/test.txt"), "https://example.com/webdav/folder/test.txt");
    }
}
