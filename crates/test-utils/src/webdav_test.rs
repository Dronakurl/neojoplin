// Simple WebDAV connectivity test
use neojoplin_sync::{ReqwestWebDavClient, WebDavConfig};
use neojoplin_core::WebDavClient;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 4 {
        eprintln!("Usage: {} <url> <username> <password>", args[0]);
        eprintln!("Example: {} https://webdav.example.com user pass", args[0]);
        std::process::exit(1);
    }

    let url = &args[1];
    let username = &args[2];
    let password = &args[3];

    println!("Testing WebDAV connection to: {}", url);
    println!("Username: {}", username);

    let config = WebDavConfig::new(url.clone(), username.clone(), password.clone());
    let client = ReqwestWebDavClient::new(config)?;

    // Test 1: List root directory
    println!("\n=== Test 1: List root directory ===");
    match client.list("/").await {
        Ok(files) => {
            println!("✓ Found {} files/directories:", files.len());
            for file in files.iter().take(10) {
                let size = file.size.map_or("?".to_string(), |s| s.to_string());
                println!("  - {} ({} bytes, is_dir: {})",
                    file.path, size, file.is_directory);
            }
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }

    // Test 2: Create test directory
    println!("\n=== Test 2: Create test directory ===");
    match client.mkcol("/neojoplin-test").await {
        Ok(_) => println!("✓ Created /neojoplin-test"),
        Err(e) => println!("✗ Failed: {}", e),
    }

    // Test 3: Upload a test file
    println!("\n=== Test 3: Upload test file ===");
    let test_content = b"Hello from NeoJoplin WebDAV test!";
    match client.put("/neojoplin-test/hello.txt", test_content, test_content.len() as u64).await {
        Ok(_) => println!("✓ Uploaded test file"),
        Err(e) => println!("✗ Failed: {}", e),
    }

    // Test 4: Download the test file
    println!("\n=== Test 4: Download test file ===");
    match client.get("/neojoplin-test/hello.txt").await {
        Ok(mut reader) => {
            use futures::io::AsyncReadExt;
            let mut buffer = Vec::new();
            reader.read_to_end(&mut buffer).await?;
            let text = String::from_utf8_lossy(&buffer);
            println!("✓ Downloaded: {}", text);
        }
        Err(e) => println!("✗ Failed: {}", e),
    }

    // Test 5: List test directory
    println!("\n=== Test 5: List test directory ===");
    match client.list("/neojoplin-test").await {
        Ok(files) => {
            println!("✓ Found {} files:", files.len());
            for file in files.iter() {
                println!("  - {}", file.path);
            }
        }
        Err(e) => println!("✗ Failed: {}", e),
    }

    // Test 6: Delete test file
    println!("\n=== Test 6: Cleanup ===");
    match client.delete("/neojoplin-test/hello.txt").await {
        Ok(_) => println!("✓ Deleted test file"),
        Err(e) => println!("✗ Failed: {}", e),
    }

    match client.delete("/neojoplin-test").await {
        Ok(_) => println!("✓ Deleted test directory"),
        Err(e) => println!("✗ Failed: {}", e),
    }

    println!("\n=== All tests completed ===");
    Ok(())
}
