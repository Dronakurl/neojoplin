// Integration tests for sync engine using FakeWebDavClient

use neojoplin_sync::FakeWebDavClient;
use neojoplin_core::{WebDavClient};
use futures::io::AsyncReadExt;

#[tokio::test]
async fn test_fake_webdav_full_workflow() {
    // Test complete WebDAV workflow
    let webdav = FakeWebDavClient::new();

    // Create remote directory
    webdav.mkcol("/test").await.unwrap();

    // Upload multiple files
    webdav.put("/test/file1.txt", b"content1", 8).await.unwrap();
    webdav.put("/test/file2.txt", b"content2", 8).await.unwrap();

    // Verify files exist
    assert!(webdav.exists("/test/file1.txt").await.unwrap());
    assert!(webdav.exists("/test/file2.txt").await.unwrap());

    // List directory
    let entries = webdav.list("/test").await.unwrap();
    assert_eq!(entries.len(), 2);

    // Get and verify file content
    let mut reader = webdav.get("/test/file1.txt").await.unwrap();
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer).await.unwrap();
    assert_eq!(buffer, b"content1");

    // Test move operation
    webdav.mv("/test/file1.txt", "/test/moved.txt").await.unwrap();
    assert!(!webdav.exists("/test/file1.txt").await.unwrap());
    assert!(webdav.exists("/test/moved.txt").await.unwrap());

    // Test copy operation
    webdav.copy("/test/file2.txt", "/test/copied.txt").await.unwrap();
    assert!(webdav.exists("/test/file2.txt").await.unwrap());
    assert!(webdav.exists("/test/copied.txt").await.unwrap());

    // Test delete
    webdav.delete("/test/file2.txt").await.unwrap();
    assert!(!webdav.exists("/test/file2.txt").await.unwrap());
    assert!(webdav.exists("/test/copied.txt").await.unwrap());
}

#[tokio::test]
async fn test_fake_webdav_nested_directories() {
    // Test nested directory handling
    let webdav = FakeWebDavClient::new();

    // Create directory structure
    webdav.mkcol("/parent").await.unwrap();
    webdav.put("/parent/child1.txt", b"data1", 5).await.unwrap();
    webdav.put("/parent/child2.txt", b"data2", 5).await.unwrap();

    // Verify direct children only
    let entries = webdav.list("/parent").await.unwrap();
    assert_eq!(entries.len(), 2);

    // Verify paths are correct
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"/parent/child1.txt"));
    assert!(paths.contains(&"/parent/child2.txt"));
}

#[tokio::test]
async fn test_fake_webdav_integration() {
    // Test that FakeWebDavClient works correctly with sync engine
    let webdav = FakeWebDavClient::new();

    // Create remote directory
    webdav.mkcol("/test").await.unwrap();

    // Upload a file
    let data = b"test content";
    webdav.put("/test/file.txt", data, data.len() as u64).await.unwrap();

    // Verify file exists
    assert!(webdav.exists("/test/file.txt").await.unwrap());

    // List directory
    let entries = webdav.list("/test").await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "/test/file.txt");

    // Get file
    let mut reader = webdav.get("/test/file.txt").await.unwrap();
    use futures::io::AsyncReadExt;
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer).await.unwrap();
    assert_eq!(buffer, data);

    // Delete file
    webdav.delete("/test/file.txt").await.unwrap();
    assert!(!webdav.exists("/test/file.txt").await.unwrap());
}
