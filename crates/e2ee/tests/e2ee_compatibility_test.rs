// E2EE Compatibility Tests for NeoJoplin
// Tests encryption/decryption compatibility with Joplin's encryption format

use neojoplin_e2ee::{
    crypto::EncryptionMethod,
    encryption::EncryptionService,
    jed_format::{JedEncoder, JedFormat},
    master_key::{MasterKey, MasterKeyManager},
    E2eeManager, EncryptionContext,
};
use tokio::runtime::Runtime;
use uuid::Uuid;

// Test data from actual Joplin encrypted notes
const JOPLIN_ENCRYPTED_SAMPLE: &str = r#"{"iv":"7bba4f5f276553cc7cecf231","ct":"fcb835407d255bf6e14b7e6f96e135b80df9d7ab27d2e1dd8f27e3332b8448e670b98b19f797006704353025ec34bf53","salt":"89bfd70f216bd421cf53fc1f5cd8c75209f66a96425e6d00d13cba29c60be22a"}"#;

#[test]
fn test_encryption_method_string_v1_parameters() {
    // StringV1 is used for encrypting notes
    let method = EncryptionMethod::StringV1;

    assert_eq!(method.chunk_size(), 65536, "StringV1 should use 64KB chunks");
    assert_eq!(method.iteration_count(), 3, "StringV1 should use 3 PBKDF2 iterations");
    assert_eq!(method.to_i32(), 10, "StringV1 method ID should be 10");
}

#[test]
fn test_encryption_method_file_v1_parameters() {
    // FileV1 is used for encrypting resources
    let method = EncryptionMethod::FileV1;

    assert_eq!(method.chunk_size(), 131072, "FileV1 should use 128KB chunks");
    assert_eq!(method.iteration_count(), 3, "FileV1 should use 3 PBKDF2 iterations");
    assert_eq!(method.to_i32(), 9, "FileV1 method ID should be 9");
}

#[test]
fn test_encryption_method_key_v1_parameters() {
    // KeyV1 is used for encrypting master keys
    let method = EncryptionMethod::KeyV1;

    assert_eq!(method.chunk_size(), 5000, "KeyV1 should use 5000 byte chunks");
    assert_eq!(method.iteration_count(), 220000, "KeyV1 should use 220,000 PBKDF2 iterations");
    assert_eq!(method.to_i32(), 8, "KeyV1 method ID should be 8");
}

#[test]
fn test_aes_gcm_encryption_decryption_round_trip() {
    let service = EncryptionService::new();
    let key = vec![0u8; 32]; // Test key (all zeros for reproducibility)
    let plain_text = "Hello, World! This is a test note with some content.";

    let encrypted = service
        .encrypt_string(plain_text, &key, EncryptionMethod::StringV1)
        .expect("Encryption should succeed");

    println!("Encrypted JSON: {}", encrypted);

    // Verify it's valid JSON with required fields
    let json: serde_json::Value =
        serde_json::from_str(&encrypted).expect("Encrypted data should be valid JSON");
    assert!(json.get("salt").is_some(), "Should have salt field");
    assert!(json.get("iv").is_some(), "Should have IV field");
    assert!(json.get("ct").is_some(), "Should have ciphertext field");

    // Verify we can decrypt it back
    let decrypted = service
        .decrypt_string(&encrypted, &key, EncryptionMethod::StringV1)
        .expect("Decryption should succeed");

    assert_eq!(decrypted, plain_text, "Decrypted text should match original");
}

#[test]
fn test_chunked_encryption_large_file() {
    let service = EncryptionService::new();
    let key = vec![1u8; 32];

    // Create data larger than chunk size (200KB vs 64KB chunk size)
    let large_data = vec![2u8; 200_000];

    let chunks = service
        .encrypt_chunked(&large_data, &key, EncryptionMethod::StringV1)
        .expect("Chunked encryption should succeed");

    assert!(
        chunks.len() > 1,
        "Should create multiple chunks for large data"
    );
    println!("Created {} chunks for 200KB data", chunks.len());

    // Verify decryption
    let decrypted = service
        .decrypt_chunked(&chunks, &key, EncryptionMethod::StringV1)
        .expect("Chunked decryption should succeed");

    assert_eq!(
        decrypted, large_data,
        "Decrypted chunked data should match original"
    );
}

#[test]
fn test_master_key_encryption_with_password() {
    let master_key = MasterKey::new();
    let password = "test-password-12345";
    let original_data = master_key.data.clone();
    let original_id = master_key.id.clone();

    let encrypted = master_key
        .encrypt_with_password(password)
        .expect("Master key encryption should succeed");

    println!("Encrypted master key: {}", encrypted);

    // Verify it's valid JSON
    let json: serde_json::Value =
        serde_json::from_str(&encrypted).expect("Encrypted master key should be valid JSON");
    assert_eq!(
        json["id"].as_str(),
        Some(original_id.as_str()),
        "Should preserve master key ID"
    );
    assert_eq!(
        json["iterations"].as_u64(),
        Some(220000),
        "Should use 220,000 PBKDF2 iterations"
    );

    // Verify decryption
    let decrypted = MasterKey::decrypt_from_password(&encrypted, password)
        .expect("Master key decryption should succeed");

    assert_eq!(
        decrypted.data, original_data,
        "Decrypted master key data should match original"
    );
    assert_eq!(
        decrypted.id, original_id,
        "Decrypted master key ID should match original"
    );
}

#[test]
fn test_master_key_wrong_password_fails() {
    let master_key = MasterKey::new();
    let password = "correct-password";
    let wrong_password = "wrong-password";

    let encrypted = master_key
        .encrypt_with_password(password)
        .expect("Encryption should succeed");

    let result = MasterKey::decrypt_from_password(&encrypted, wrong_password);
    assert!(
        result.is_err(),
        "Decryption with wrong password should fail"
    );
    println!(
        "Correctly rejected wrong password: {:?}",
        result.unwrap_err()
    );
}

#[test]
fn test_jed_format_encoding() {
    let master_key = vec![0u8; 32];
    let master_key_id = "test-master-key-id";
    let data = "This is a test note for JED format encoding.";

    let jed = JedEncoder::encode(data, &master_key, master_key_id, 10)
        .expect("JED encoding should succeed");

    println!("JED encoded: {}", jed);

    // Verify JED format
    assert!(
        jed.starts_with("JED01"),
        "JED should start with JED01 version header"
    );

    // Extract method
    let method = JedFormat::extract_method(&jed).expect("Should extract method");
    assert_eq!(method, 10, "Method should be 10 (StringV1)");

    // Extract key ID
    let key_id = JedFormat::extract_key_id(&jed).expect("Should extract key ID");
    println!("Extracted key ID: {}", key_id);
}

#[test]
fn test_jed_format_detection() {
    let jed_string = "JED010A...";
    let plain_string = "plain text";

    assert!(
        JedFormat::is_jed_format(jed_string),
        "Should detect JED format"
    );
    assert!(
        !JedFormat::is_jed_format(plain_string),
        "Should not detect JED format in plain text"
    );
}

#[test]
fn test_encryption_context() {
    let mut context = EncryptionContext::new();
    let key_id = "test-key-1".to_string();
    let key = vec![3u8; 32];

    context.load_master_key(key_id.clone(), key);

    assert!(
        context.is_master_key_loaded(&key_id),
        "Key should be loaded"
    );
    assert_eq!(context.loaded_keys_count(), 1, "Should have 1 key loaded");

    let retrieved = context
        .master_key(&key_id)
        .expect("Should retrieve loaded key");
    assert_eq!(retrieved.len(), 32, "Retrieved key should be 32 bytes");
}

#[test]
fn test_e2ee_manager_encrypt_decrypt_note() {
    let mut manager = E2eeManager::new();
    let master_key = manager.generate_master_key();
    let key_id = "test-key".to_string();

    manager
        .context()
        .load_master_key(key_id.clone(), master_key);

    let plain_text = "Secret note content";
    let encrypted = manager
        .encrypt_note(plain_text, &key_id)
        .expect("Note encryption should succeed");

    println!("Encrypted note: {}", encrypted);

    let decrypted = manager
        .decrypt_note(&encrypted, &key_id)
        .expect("Note decryption should succeed");

    assert_eq!(decrypted, plain_text, "Decrypted note should match original");
}

#[test]
fn test_master_key_manager_save_load() {
    let rt = Runtime::new().expect("Should create runtime");
    let temp_dir = std::env::temp_dir().join(format!("test-e2ee-{}", Uuid::new_v4()));
    let manager = MasterKeyManager::new(temp_dir.clone());

    rt.block_on(async {
        let key = MasterKey::new();
        let password = "test-password";
        let key_id = key.id.clone();

        // Save key
        manager
            .save_key(&key, password)
            .await
            .expect("Should save master key");

        // List keys
        let key_ids = manager
            .list_keys()
            .await
            .expect("Should list keys");
        assert_eq!(key_ids, vec![key_id.clone()], "Should list saved key");

        // Load key
        let loaded = manager
            .load_key(&key_id, password)
            .await
            .expect("Should load master key");

        assert_eq!(loaded.data, key.data, "Loaded key data should match");
        assert_eq!(loaded.id, key.id, "Loaded key ID should match");

        // Cleanup
        tokio::fs::remove_dir_all(temp_dir).await.ok();
    });
}

#[test]
fn test_key_derivation_deterministic() {
    let service = EncryptionService::new();
    let password = "test-password";
    let salt = vec![4u8; 32];
    let iterations = 3;

    let key1 = service
        .derive_key_from_password(password, &salt, iterations, 32)
        .expect("First derivation should succeed");
    let key2 = service
        .derive_key_from_password(password, &salt, iterations, 32)
        .expect("Second derivation should succeed");

    assert_eq!(key1, key2, "Key derivation should be deterministic");
    assert_eq!(key1.len(), 32, "Derived key should be 32 bytes");
}

#[test]
fn test_encryption_different_keys_different_results() {
    let service = EncryptionService::new();
    let key1 = vec![5u8; 32];
    let key2 = vec![6u8; 32];
    let plain_text = "Test data";

    let encrypted1 = service
        .encrypt_string(plain_text, &key1, EncryptionMethod::StringV1)
        .expect("First encryption should succeed");
    let encrypted2 = service
        .encrypt_string(plain_text, &key2, EncryptionMethod::StringV1)
        .expect("Second encryption should succeed");

    assert_ne!(
        encrypted1, encrypted2,
        "Encryption with different keys should produce different results"
    );
}

#[test]
fn test_encryption_same_data_different_results() {
    let service = EncryptionService::new();
    let key = vec![7u8; 32];
    let plain_text = "Test data";

    let encrypted1 = service
        .encrypt_string(plain_text, &key, EncryptionMethod::StringV1)
        .expect("First encryption should succeed");
    let encrypted2 = service
        .encrypt_string(plain_text, &key, EncryptionMethod::StringV1)
        .expect("Second encryption should succeed");

    assert_ne!(
        encrypted1, encrypted2,
        "Encryption should use random IV/salt, producing different results"
    );

    // But both should decrypt to the same original
    let decrypted1 = service
        .decrypt_string(&encrypted1, &key, EncryptionMethod::StringV1)
        .expect("First decryption should succeed");
    let decrypted2 = service
        .decrypt_string(&encrypted2, &key, EncryptionMethod::StringV1)
        .expect("Second decryption should succeed");

    assert_eq!(decrypted1, plain_text, "First decryption should match");
    assert_eq!(decrypted2, plain_text, "Second decryption should match");
}

#[test]
fn test_file_v1_chunked_encryption() {
    let service = EncryptionService::new();
    let key = vec![8u8; 32];

    // Create data that spans exactly 3 chunks (3 * 128KB = 384KB)
    let large_data = vec![9u8; 400_000];

    let chunks = service
        .encrypt_chunked(&large_data, &key, EncryptionMethod::FileV1)
        .expect("FileV1 chunked encryption should succeed");

    assert_eq!(
        chunks.len(),
        4,
        "400KB data with 128KB chunks should produce 4 chunks"
    );

    let decrypted = service
        .decrypt_chunked(&chunks, &key, EncryptionMethod::FileV1)
        .expect("FileV1 chunked decryption should succeed");

    assert_eq!(
        decrypted, large_data,
        "Decrypted FileV1 data should match original"
    );
}
