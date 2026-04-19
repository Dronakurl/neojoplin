// Robust WebDAV XML parsing using quick-xml
//
// This module provides production-grade XML parsing for WebDAV PROPFIND responses,
// replacing the previous manual string-based parsing which was fragile.

use quick_xml::events::Event;
use quick_xml::Reader;
use anyhow::Result;

/// Parse WebDAV PROPFIND response to extract file names
pub fn parse_propfind_files(xml_body: &str, _base_url: &str) -> Result<Vec<String>> {
    let mut reader = Reader::from_str(xml_body);
    reader.config_mut().trim_text(true);

    let mut files = Vec::new();
    let mut current_buffer = Vec::new();
    let mut in_href = false;

    loop {
        match reader.read_event_into(&mut current_buffer) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                match name.as_ref() {
                    b"D:href" | b"href" => {
                        in_href = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if in_href {
                    if let Ok(href) = e.unescape() {
                        let href_string = href.into_owned();
                        // Filter out directory entries (those ending with /)
                        if !href_string.ends_with('/') {
                            if let Some(filename) = href_string.rsplit('/').next() {
                                if !filename.is_empty() {
                                    files.push(filename.to_string());
                                }
                            }
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                match name.as_ref() {
                    b"D:href" | b"href" => {
                        in_href = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(anyhow::anyhow!("XML parsing error: {}", e));
            }
            _ => {}
        }
        current_buffer.clear();
    }

    Ok(files)
}

/// Parse file metadata from WebDAV PROPFIND response
pub fn parse_file_metadata(xml_body: &str, path: &str) -> Result<FileMetadata> {
    let mut reader = Reader::from_str(xml_body);
    reader.config_mut().trim_text(true);

    let mut metadata = FileMetadata::default();
    let mut current_buffer = Vec::new();
    let mut current_tag_name = String::new();
    let mut in_prop = false;

    loop {
        match reader.read_event_into(&mut current_buffer) {
            Ok(Event::Start(ref e)) => {
                let name = e.name().to_owned();
                let name_bytes = name.as_ref();
                current_tag_name = String::from_utf8_lossy(name_bytes).to_string();

                match current_tag_name.as_str() {
                    "D:prop" | "prop" => {
                        in_prop = true;
                    }
                    "D:collection" | "collection" | "D:folder" | "folder" => {
                        metadata.is_dir = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if in_prop {
                    if let Ok(text) = e.unescape() {
                        let text_string = text.into_owned();

                        match current_tag_name.as_str() {
                            "D:getcontentlength" | "getcontentlength" => {
                                metadata.size = text_string.parse::<i64>().unwrap_or(0);
                            }
                            "D:getlastmodified" | "getlastmodified" => {
                                // Parse HTTP date format
                                if let Ok(date) = chrono::DateTime::parse_from_rfc2822(&text_string) {
                                    metadata.modified = date.timestamp_millis();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name().to_owned();
                let name_bytes = name.as_ref();
                let name_str = String::from_utf8_lossy(name_bytes);
                if name_str == "D:prop" || name_str == "prop" {
                    in_prop = false;
                }
                current_tag_name.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(anyhow::anyhow!("XML parsing error: {}", e));
            }
            _ => {}
        }
        current_buffer.clear();
    }

    metadata.path = path.to_string();
    Ok(metadata)
}

#[derive(Debug, Clone, Default)]
pub struct FileMetadata {
    pub path: String,
    pub size: i64,
    pub modified: i64,
    pub is_dir: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROPFIND_RESPONSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:response>
        <D:href>/webdav/test/</D:href>
        <D:propstat>
            <D:prop>
                <D:displayname>test</D:displayname>
            </D:prop>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/webdav/test/note.md</D:href>
        <D:propstat>
            <D:prop>
                <D:displayname>note.md</D:displayname>
            </D:prop>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    #[test]
    fn test_parse_propfind_files() {
        let files = parse_propfind_files(PROPFIND_RESPONSE, "http://localhost:8080/webdav").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "note.md");
    }

    const FILE_METADATA_RESPONSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
    <D:response>
        <D:href>/webdav/test/note.md</D:href>
        <D:propstat>
            <D:prop>
                <D:getcontentlength>1024</D:getcontentlength>
                <D:getlastmodified>Mon, 19 Apr 2026 12:00:00 GMT</D:getlastmodified>
                <D:resourcetype></D:resourcetype>
            </D:prop>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

    #[test]
    fn test_parse_file_metadata() {
        let metadata = parse_file_metadata(FILE_METADATA_RESPONSE, "/test/note.md").unwrap();
        assert_eq!(metadata.size, 1024);
        assert!(!metadata.is_dir);
        assert_eq!(metadata.path, "/test/note.md");
    }
}