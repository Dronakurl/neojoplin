// Cryptographic operations for E2EE
// Implements AES-256-GCM encryption with PBKDF2 key derivation

use crate::{E2eeError, E2eeResult};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::Sha512;

/// Encryption method identifiers (matching Joplin)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionMethod {
    SJCL = 1,
    SJCL2 = 2,
    SJCL3 = 3,
    SJCL4 = 4,
    SJCL1a = 5,
    Custom = 6,
    SJCL1b = 7,
    KeyV1 = 8,
    FileV1 = 9,
    StringV1 = 10,
}

impl EncryptionMethod {
    /// Get the chunk size for this encryption method
    pub fn chunk_size(&self) -> usize {
        match self {
            EncryptionMethod::StringV1 => 65536, // 64KB
            EncryptionMethod::FileV1 => 131072,  // 128KB
            EncryptionMethod::KeyV1 => 5000,
            _ => 5000, // Default for older methods
        }
    }

    /// Get PBKDF2 iteration count
    pub fn iteration_count(&self) -> u32 {
        match self {
            EncryptionMethod::KeyV1 => 220000,
            EncryptionMethod::StringV1 | EncryptionMethod::FileV1 => 3,
            _ => 1000,
        }
    }

    /// Parse from i32 (for database storage)
    pub fn from_i32(value: i32) -> E2eeResult<Self> {
        match value {
            1 => Ok(EncryptionMethod::SJCL),
            2 => Ok(EncryptionMethod::SJCL2),
            3 => Ok(EncryptionMethod::SJCL3),
            4 => Ok(EncryptionMethod::SJCL4),
            5 => Ok(EncryptionMethod::SJCL1a),
            6 => Ok(EncryptionMethod::Custom),
            7 => Ok(EncryptionMethod::SJCL1b),
            8 => Ok(EncryptionMethod::KeyV1),
            9 => Ok(EncryptionMethod::FileV1),
            10 => Ok(EncryptionMethod::StringV1),
            _ => Err(E2eeError::UnsupportedEncryptionMethod(value)),
        }
    }

    /// Convert to i32 (for database storage)
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

/// Encryption result containing the encrypted data
#[derive(Debug, Clone)]
pub struct EncryptedData {
    pub salt: Vec<u8>,
    pub iv: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

/// Crypto service for encryption operations
pub struct CryptoService;

impl CryptoService {
    /// Encrypt a string using the specified method and key
    pub fn encrypt_string(
        key: &[u8],
        plain_text: &str,
        method: EncryptionMethod,
    ) -> E2eeResult<String> {
        let plain_bytes = plain_text.as_bytes();
        let encrypted = Self::encrypt_bytes(key, plain_bytes, method)?;

        // Format as JSON (matching Joplin format)
        let json = serde_json::json!({
            "salt": hex::encode(&encrypted.salt),
            "iv": hex::encode(&encrypted.iv),
            "ct": hex::encode(&encrypted.ciphertext)
        });

        Ok(json.to_string())
    }

    /// Encrypt bytes using the specified method and key
    pub fn encrypt_bytes(
        key: &[u8],
        plain_text: &[u8],
        method: EncryptionMethod,
    ) -> E2eeResult<EncryptedData> {
        // Generate random salt (32 bytes)
        let mut salt = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut salt);

        // Generate random IV (12 bytes for GCM)
        let mut iv = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut iv);

        // Derive data key from master key using PBKDF2
        let data_key = Self::derive_key(key, &salt, method.iteration_count())?;

        // Perform AES-256-GCM encryption
        let cipher = Aes256Gcm::new_from_slice(&data_key)
            .map_err(|e| E2eeError::Crypto(format!("Failed to create cipher: {}", e)))?;
        let nonce = Nonce::from_slice(&iv);

        let ciphertext = cipher
            .encrypt(nonce, plain_text)
            .map_err(|e| E2eeError::Crypto(format!("Encryption failed: {}", e)))?;

        // Ciphertext already includes the auth tag
        Ok(EncryptedData {
            salt: salt.to_vec(),
            iv: iv.to_vec(),
            ciphertext,
        })
    }

    /// Decrypt a string using the specified method and key
    pub fn decrypt_string(
        key: &[u8],
        cipher_text: &str,
        method: EncryptionMethod,
    ) -> E2eeResult<String> {
        // Parse JSON format
        let encrypted: EncryptedJson = serde_json::from_str(cipher_text)
            .map_err(|e| E2eeError::DecryptionFailed(format!("Invalid encrypted format: {}", e)))?;

        let salt = hex::decode(&encrypted.salt)
            .map_err(|e| E2eeError::DecryptionFailed(format!("Invalid salt: {}", e)))?;
        let iv = hex::decode(&encrypted.iv)
            .map_err(|e| E2eeError::DecryptionFailed(format!("Invalid IV: {}", e)))?;
        let ct = hex::decode(&encrypted.ct)
            .map_err(|e| E2eeError::DecryptionFailed(format!("Invalid ciphertext: {}", e)))?;

        let decrypted_bytes = Self::decrypt_bytes(key, &salt, &iv, &ct, method)?;

        String::from_utf8(decrypted_bytes)
            .map_err(|e| E2eeError::DecryptionFailed(format!("Invalid UTF-8: {}", e)))
    }

    /// Decrypt bytes using the specified method and key
    pub fn decrypt_bytes(
        key: &[u8],
        salt: &[u8],
        iv: &[u8],
        ciphertext: &[u8],
        method: EncryptionMethod,
    ) -> E2eeResult<Vec<u8>> {
        // Derive data key from master key using PBKDF2
        let data_key = Self::derive_key(key, salt, method.iteration_count())?;

        // Perform AES-256-GCM decryption
        let cipher = Aes256Gcm::new_from_slice(&data_key)
            .map_err(|e| E2eeError::Crypto(format!("Failed to create cipher: {}", e)))?;
        let nonce = Nonce::from_slice(iv);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| E2eeError::DecryptionFailed(format!("Decryption failed: {}", e)))?;

        Ok(plaintext)
    }

    /// Derive a key from password using PBKDF2
    pub fn derive_key_from_password(
        password: &str,
        salt: &[u8],
        iterations: u32,
        key_length: usize,
    ) -> E2eeResult<Vec<u8>> {
        let mut key = vec![0u8; key_length];
        pbkdf2_hmac::<Sha512>(password.as_bytes(), salt, iterations, &mut key);
        Ok(key)
    }

    /// Derive a data key from master key using PBKDF2
    fn derive_key(master_key: &[u8], salt: &[u8], iterations: u32) -> E2eeResult<Vec<u8>> {
        let mut data_key = vec![0u8; 32]; // 256-bit key
        pbkdf2_hmac::<Sha512>(master_key, salt, iterations, &mut data_key);
        Ok(data_key)
    }

    /// Generate a random nonce for encryption
    pub fn generate_nonce() -> [u8; 12] {
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        nonce
    }

    /// Generate a random salt (32 bytes)
    pub fn generate_salt() -> [u8; 32] {
        let mut salt = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut salt);
        salt
    }
}

/// JSON format for encrypted data (matching Joplin)
#[derive(serde::Deserialize, serde::Serialize)]
struct EncryptedJson {
    salt: String,
    iv: String,
    ct: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_round_trip() {
        let key = [0u8; 32]; // Test key
        let plain_text = "Hello, World!";

        let encrypted =
            CryptoService::encrypt_string(&key, plain_text, EncryptionMethod::StringV1).unwrap();
        println!("Encrypted: {}", encrypted);

        let decrypted =
            CryptoService::decrypt_string(&key, &encrypted, EncryptionMethod::StringV1).unwrap();
        assert_eq!(decrypted, plain_text);
    }

    #[test]
    fn test_encryption_method_chunk_sizes() {
        assert_eq!(EncryptionMethod::StringV1.chunk_size(), 65536);
        assert_eq!(EncryptionMethod::FileV1.chunk_size(), 131072);
        assert_eq!(EncryptionMethod::KeyV1.chunk_size(), 5000);
    }

    #[test]
    fn test_encryption_method_iterations() {
        assert_eq!(EncryptionMethod::KeyV1.iteration_count(), 220000);
        assert_eq!(EncryptionMethod::StringV1.iteration_count(), 3);
        assert_eq!(EncryptionMethod::FileV1.iteration_count(), 3);
    }

    #[test]
    fn test_encryption_method_conversion() {
        assert_eq!(
            EncryptionMethod::from_i32(10).unwrap(),
            EncryptionMethod::StringV1
        );
        assert_eq!(EncryptionMethod::StringV1.to_i32(), 10);
    }

    #[test]
    fn test_key_derivation() {
        let password = "test-password";
        let salt = CryptoService::generate_salt();

        let key1 = CryptoService::derive_key_from_password(password, &salt, 3, 32).unwrap();
        let key2 = CryptoService::derive_key_from_password(password, &salt, 3, 32).unwrap();

        assert_eq!(key1, key2, "Key derivation should be deterministic");
        assert_eq!(key1.len(), 32);
    }

    #[test]
    fn test_random_generation() {
        let nonce1 = CryptoService::generate_nonce();
        let nonce2 = CryptoService::generate_nonce();

        assert_ne!(nonce1, nonce2, "Nonces should be unique");
    }
}
