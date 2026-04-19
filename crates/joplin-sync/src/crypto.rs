// Cryptographic operations for E2EE
//
// This module provides AES-256-GCM encryption compatible with Joplin CLI's
// StringV1/FileV1/KeyV1 encryption methods. Uses PBKDF2-HMAC-SHA512 for
// key derivation, matching the reference TypeScript implementation.

use anyhow::{Result, anyhow};
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce, Key
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use pbkdf2::pbkdf2_hmac;
use sha2::{Sha256, Sha512, Digest};

/// Joplin-compatible chunk encryption result (JSON serializable)
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct EncryptionChunk {
    pub salt: String,  // base64-encoded
    pub iv: String,    // base64-encoded
    pub ct: String,    // base64-encoded (ciphertext + auth tag)
}

/// Derive a 256-bit key using PBKDF2-HMAC-SHA512 (Joplin compatible)
pub fn derive_key_pbkdf2(password: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha512>(password.as_bytes(), salt, iterations, &mut key);
    key
}

/// SHA-256 hash of data
pub fn sha256(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

/// Encrypt data using AES-256-GCM with PBKDF2 key derivation (Joplin compatible)
/// Returns a JSON chunk `{"salt":"...","iv":"...","ct":"..."}` with base64 values.
/// The `password` is the key material (hex master key for StringV1, user password for KeyV1).
/// The `salt` is random bytes used as PBKDF2 salt.
pub fn encrypt_chunk(password: &str, salt: &[u8], plaintext: &[u8], iterations: u32) -> Result<EncryptionChunk> {
    let derived_key = derive_key_pbkdf2(password, salt, iterations);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&derived_key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher.encrypt(&nonce, plaintext)
        .map_err(|e| anyhow!("Encryption failed: {}", e))?;

    Ok(EncryptionChunk {
        salt: BASE64.encode(salt),
        iv: BASE64.encode(&nonce),
        ct: BASE64.encode(&ciphertext),
    })
}

/// Decrypt a Joplin encryption chunk using AES-256-GCM with PBKDF2 key derivation.
/// The `password` is the key material (hex master key for StringV1, user password for KeyV1).
pub fn decrypt_chunk(password: &str, chunk: &EncryptionChunk, iterations: u32) -> Result<Vec<u8>> {
    let salt = BASE64.decode(&chunk.salt)
        .map_err(|e| anyhow!("Invalid base64 salt: {}", e))?;
    let iv = BASE64.decode(&chunk.iv)
        .map_err(|e| anyhow!("Invalid base64 iv: {}", e))?;
    let ct = BASE64.decode(&chunk.ct)
        .map_err(|e| anyhow!("Invalid base64 ct: {}", e))?;

    if iv.len() != 12 {
        return Err(anyhow!("Invalid IV length: {} (expected 12)", iv.len()));
    }

    let derived_key = derive_key_pbkdf2(password, &salt, iterations);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&derived_key));
    let nonce = Nonce::from_slice(&iv);

    cipher.decrypt(nonce, ct.as_ref())
        .map_err(|e| anyhow!("AES-256-GCM decryption failed: {}", e))
}

/// Generate a random encryption key (256-bit)
pub fn generate_key() -> [u8; 32] {
    Aes256Gcm::generate_key(&mut OsRng).into()
}

/// Generate a random salt (256-bit)
pub fn generate_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    use rand::Rng;
    rand::thread_rng().fill(&mut salt);
    salt
}

/// Generate a random 12-byte nonce for AES-GCM
pub fn generate_nonce() -> [u8; 12] {
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let mut result = [0u8; 12];
    result.copy_from_slice(&nonce);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_roundtrip() {
        let password = "test_master_key_hex_string";
        let salt = generate_salt();
        let plaintext = b"Hello, World!";

        let chunk = encrypt_chunk(password, &salt, plaintext, 3).unwrap();
        let decrypted = decrypt_chunk(password, &chunk, 3).unwrap();

        assert_eq!(plaintext.to_vec(), decrypted);
    }

    #[test]
    fn test_key_derivation_deterministic() {
        let password = "test_password";
        let salt = b"fixed_salt_for_testing_purposes!";

        let key1 = derive_key_pbkdf2(password, salt, 1000);
        let key2 = derive_key_pbkdf2(password, salt, 1000);
        assert_eq!(key1, key2);

        let key3 = derive_key_pbkdf2(password, salt, 100);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_different_keys() {
        let key1 = generate_key();
        let key2 = generate_key();
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_sha256() {
        let hash = sha256(b"test");
        assert_eq!(hash.len(), 32);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_joplin_master_key_decrypt() {
        // Actual Joplin master key from info.json
        let content = r#"{"salt":"okSe+f+4ElSnBgTkT/J/ngycAwmi/Cpd3mPYqeiWVb0=","iv":"vmh8UOTY91/LJJab","ct":"hLQay1bu2gdi1mYhGiGrQLeiaeabnLkauZ1vnbC9Gf1NYKgxEieEcgwGoJVlQHPCoY0FNEUAQ7dtGDzlySTC+bSQT6jix0WMQ8aTkuDrV80v2oFIKw6CsAcyFAWPWmXliFcfY4hLEv1k1Exm5a1N3rjoseZ06EqQxg6jwR4TWD0FnSw8Hq1GBS15pqy66APTf5wFPMfdd5vPXQQLYT9bXuTfZiyQE+hxnWCS2uLy0ERnrykWaQALvjh2TBTcuxBhV6UikMDnKrGv6ly4nXgADvSQvEd7bvlsmnyak4hCKx6Etb/buaAanOHjesL6T8TUoqSdJunhAWCAffah7xN/VoAy3BlfwnbjXjnZCL1FOAI="}"#;
        
        let chunk: EncryptionChunk = serde_json::from_str(content).unwrap();
        
        // Known-good derived key from Python
        let salt = BASE64.decode(&chunk.salt).unwrap();
        let key = derive_key_pbkdf2("Adidas", &salt, 220000);
        let expected_key = hex::decode("70353b93e91ece98e7b1cf1ec6fd86fbfd83aa652585250e708ba0714a2bb525").unwrap();
        assert_eq!(&key[..], &expected_key[..], "PBKDF2 key derivation mismatch");
        
        // Now decrypt
        let result = decrypt_chunk("Adidas", &chunk, 220000);
        assert!(result.is_ok(), "Decryption failed: {:?}", result.err());
        
        let plaintext = result.unwrap();
        assert_eq!(plaintext.len(), 256, "Expected 256-byte master key");
    }
}
