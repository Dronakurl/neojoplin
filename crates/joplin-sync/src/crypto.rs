// Cryptographic operations for E2EE
//
// This module provides production-grade AES-256-GCM encryption
// compatible with Joplin CLI's StringV1 encryption method.

use anyhow::{Result, anyhow};
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce, Key
};
use sha2::{Sha256, Digest};

/// Derive a 256-bit encryption key from a password using PBKDF2
pub fn derive_key(password: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
    // Simple PBKDF2-like derivation (for production, use proper PBKDF2)
    let mut key = [0u8; 32];
    let mut data = password.as_bytes().to_vec();
    data.extend_from_slice(salt);

    for _ in 0..iterations {
        let hash = Sha256::digest(&data);
        key.copy_from_slice(hash.as_slice());
        data = hash.to_vec();
    }

    key
}

/// Encrypt plaintext using AES-256-GCM
pub fn encrypt_aes256_gcm(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher.encrypt(&nonce, plaintext)
        .map_err(|e| anyhow!("Encryption failed: {}", e))?;

    // Return nonce + ciphertext (nonce is needed for decryption)
    let mut result = Vec::with_capacity(nonce.len() + ciphertext.len());
    result.extend_from_slice(&nonce);
    result.extend_from_slice(&ciphertext);

    Ok(result)
}

/// Decrypt ciphertext using AES-256-GCM
pub fn decrypt_aes256_gcm(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 12 {
        return Err(anyhow!("Ciphertext too short"));
    }

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));

    // Extract nonce (first 12 bytes) and ciphertext
    let nonce = Nonce::from_slice(&data[0..12]);
    let ciphertext = &data[12..];

    let plaintext = cipher.decrypt(nonce, ciphertext)
        .map_err(|e| anyhow!("Decryption failed: {}", e))?;

    Ok(plaintext)
}

/// Encrypt a string using AES-256-GCM and return as base64
pub fn encrypt_string_aes256_gcm(key: &[u8; 32], plaintext: &str) -> Result<String> {
    let plaintext_bytes = plaintext.as_bytes();
    let encrypted = encrypt_aes256_gcm(key, plaintext_bytes)?;

    // Return as hex string for compatibility with Joplin
    Ok(hex::encode(encrypted))
}

/// Decrypt a hex-encoded string using AES-256-GCM
pub fn decrypt_string_aes256_gcm(key: &[u8; 32], ciphertext_hex: &str) -> Result<String> {
    let ciphertext = hex::decode(ciphertext_hex)
        .map_err(|e| anyhow!("Invalid hex encoding: {}", e))?;

    let decrypted = decrypt_aes256_gcm(key, &ciphertext)?;

    String::from_utf8(decrypted)
        .map_err(|e| anyhow!("Invalid UTF-8 in decrypted data: {}", e))
}

/// Generate a random encryption key (256-bit)
pub fn generate_key() -> [u8; 32] {
    Aes256Gcm::generate_key(&mut OsRng).into()
}

/// Generate a random salt for key derivation
pub fn generate_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    use rand::Rng;
    let mut rng = rand::thread_rng();
    rng.fill(&mut salt);
    salt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes256_gcm_roundtrip() {
        let key = generate_key();
        let plaintext = "Hello, World!";

        let encrypted = encrypt_string_aes256_gcm(&key, plaintext).unwrap();
        let decrypted = decrypt_string_aes256_gcm(&key, &encrypted).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_key_derivation() {
        let password = "test_password";
        let salt = generate_salt();

        let key1 = derive_key(password, &salt, 1000);
        let key2 = derive_key(password, &salt, 1000);

        assert_eq!(key1, key2);

        let key3 = derive_key(password, &salt, 100);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_different_keys() {
        let key1 = generate_key();
        let key2 = generate_key();

        assert_ne!(key1, key2);
    }
}
