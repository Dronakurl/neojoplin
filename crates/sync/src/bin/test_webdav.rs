// Test WebDAV connection with GMX

use neojoplin_sync::{ReqwestWebDavClient, WebDavConfig};
use neojoplin_core::WebDavClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
        // Create WebDAV config from rclone GMX settings
        // NOTE: Replace with actual GMX password
        let config = WebDavConfig::new(
            "https://webdav.mc.gmx.net".to_string(),
            "konrad.urlichs@gmx.de".to_string(),
            std::env::var("GMX_PASSWORD").unwrap_or_else(|_| "YOUR_PASSWORD_HERE".to_string()),
        );

        println!("Creating WebDAV client for GMX...");
        let client = ReqwestWebDavClient::new(config)?;

        // Test 1: Create a test directory in /neojoplin/
        println!("\n=== Test 1: Creating /neojoplin/test directory ===");
        match client.mkcol("/neojoplin").await {
            Ok(_) => println!("✓ Created or verified /neojoplin directory"),
            Err(e) => println!("✗ Failed to create /neojoplin: {:?}", e),
        }

        match client.mkcol("/neojoplin/test").await {
            Ok(_) => println!("✓ Created or verified /neojoplin/test directory"),
            Err(e) => println!("✗ Failed to create /neojoplin/test: {:?}", e),
        }

        // Test 2: List directory contents
        println!("\n=== Test 2: Listing /neojoplin directory ===");
        match client.list("/neojoplin").await {
            Ok(entries) => {
                println!("✓ Found {} entries:", entries.len());
                for entry in entries {
                    println!("  - {} ({})", entry.path, if entry.is_directory { "dir" } else { "file" });
                }
            }
            Err(e) => println!("✗ Failed to list directory: {:?}", e),
        }

        // Test 3: Create a test file
        println!("\n=== Test 3: Creating test file ===");
        let test_data = b"Hello from NeoJoplin WebDAV client!";
        match client.put("/neojoplin/test/hello.txt", test_data.as_ref(), test_data.len() as u64).await {
            Ok(_) => println!("✓ Created /neojoplin/test/hello.txt"),
            Err(e) => println!("✗ Failed to create file: {:?}", e),
        }

        // Test 4: Read the test file
        println!("\n=== Test 4: Reading test file ===");
        match client.get("/neojoplin/test/hello.txt").await {
            Ok(mut reader) => {
                use futures::AsyncReadExt;
                let mut buffer = Vec::new();
                match reader.read_to_end(&mut buffer).await {
                    Ok(n) => {
                        let content = String::from_utf8_lossy(&buffer);
                        println!("✓ Read {} bytes: {}", n, content);
                    }
                    Err(e) => println!("✗ Failed to read file content: {:?}", e),
                }
            }
            Err(e) => println!("✗ Failed to get file: {:?}", e),
        }

        // Test 5: Check file existence
        println!("\n=== Test 5: Checking file existence ===");
        match client.exists("/neojoplin/test/hello.txt").await {
            Ok(exists) => println!("✓ File exists: {}", exists),
            Err(e) => println!("✗ Failed to check existence: {:?}", e),
        }

        // Test 6: Get file metadata
        println!("\n=== Test 6: Getting file metadata ===");
        match client.stat("/neojoplin/test/hello.txt").await {
            Ok(metadata) => {
                println!("✓ File metadata:");
                println!("  - Path: {}", metadata.path);
                println!("  - Size: {} bytes", metadata.size.unwrap_or(0));
                println!("  - Directory: {}", metadata.is_directory);
                println!("  - Modified: {:?}", metadata.modified);
            }
            Err(e) => println!("✗ Failed to get metadata: {:?}", e),
        }

        // Test 7: List test directory
        println!("\n=== Test 7: Listing test directory ===");
        match client.list("/neojoplin/test").await {
            Ok(entries) => {
                println!("✓ Found {} entries in test directory:", entries.len());
                for entry in entries {
                    println!("  - {} ({} bytes)", entry.path, entry.size.unwrap_or(0));
                }
            }
            Err(e) => println!("✗ Failed to list test directory: {:?}", e),
        }

        // Test 8: Copy file
        println!("\n=== Test 8: Copying file ===");
        match client.copy("/neojoplin/test/hello.txt", "/neojoplin/test/hello_copy.txt").await {
            Ok(_) => println!("✓ Copied file to hello_copy.txt"),
            Err(e) => println!("✗ Failed to copy file: {:?}", e),
        }

        // Test 9: Move/rename file
        println!("\n=== Test 9: Moving file ===");
        match client.mv("/neojoplin/test/hello_copy.txt", "/neojoplin/test/hello_moved.txt").await {
            Ok(_) => println!("✓ Moved file to hello_moved.txt"),
            Err(e) => println!("✗ Failed to move file: {:?}", e),
        }

        // Test 10: Delete file
        println!("\n=== Test 10: Deleting files ===");
        match client.delete("/neojoplin/test/hello_moved.txt").await {
            Ok(_) => println!("✓ Deleted hello_moved.txt"),
            Err(e) => println!("✗ Failed to delete file: {:?}", e),
        }

        match client.delete("/neojoplin/test/hello.txt").await {
            Ok(_) => println!("✓ Deleted hello.txt"),
            Err(e) => println!("✗ Failed to delete file: {:?}", e),
        }

        // Test 11: Delete test directory
        println!("\n=== Test 11: Cleanup - deleting test directory ===");
        match client.delete("/neojoplin/test").await {
            Ok(_) => println!("✓ Deleted test directory"),
            Err(e) => println!("✗ Failed to delete test directory: {:?}", e),
        }

        println!("\n=== All tests completed ===");
        Ok::<(), Box<dyn std::error::Error>>(())
}
