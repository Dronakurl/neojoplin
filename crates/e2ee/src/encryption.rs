// High-level encryption service for notes and data

use crate::{crypto::{CryptoService, EncryptionMethod}, E2eeError, E2eeResult};

/// High-level encryption service for notes and data
pub struct EncryptionService;

impl EncryptionService {
    pub fn new() -> Self {
        Self
    }

    /// Encrypt a string using the specified method
    pub fn encrypt_string(
        &self,
        plain_text: &str,
        key: &[u8],
        method: EncryptionMethod,
    ) -> E2eeResult<String> {
        CryptoService::encrypt_string(key, plain_text, method)
    }

    /// Decrypt a string using the specified method
    pub fn decrypt_string(
        &self,
        cipher_text: &str,
        key: &[u8],
        method: EncryptionMethod,
    ) -> E2eeResult<String> {
        CryptoService::decrypt_string(key, cipher_text, method)
    }

    /// Encrypt bytes and return as JSON string (Joplin format)
    pub fn encrypt_bytes(
        &self,
        plain_text: &[u8],
        key: &[u8],
        method: EncryptionMethod,
    ) -> E2eeResult<String> {
        let encrypted = CryptoService::encrypt_bytes(key, plain_text, method)?;

        // Format as JSON (matching Joplin format)
        let json = serde_json::json!({
            "salt": hex::encode(&encrypted.salt),
            "iv": hex::encode(&encrypted.iv),
            "ct": hex::encode(&encrypted.ciphertext)
        });

        Ok(json.to_string())
    }

    /// Decrypt bytes from JSON string format
    pub fn decrypt_bytes(
        &self,
        encrypted_json: &str,
        key: &[u8],
        method: EncryptionMethod,
    ) -> E2eeResult<Vec<u8>> {
        let encrypted: EncryptedJson = serde_json::from_str(encrypted_json)
            .map_err(|e| E2eeError::DecryptionFailed(format!("Invalid encrypted format: {}", e)))?;

        let salt = hex::decode(&encrypted.salt)
            .map_err(|e| E2eeError::DecryptionFailed(format!("Invalid salt: {}", e)))?;
        let iv = hex::decode(&encrypted.iv)
            .map_err(|e| E2eeError::DecryptionFailed(format!("Invalid IV: {}", e)))?;
        let ct = hex::decode(&encrypted.ct)
            .map_err(|e| E2eeError::DecryptionFailed(format!("Invalid ciphertext: {}", e)))?;

        CryptoService::decrypt_bytes(key, &salt, &iv, &ct, method)
    }

    /// Encrypt data in chunks (for large data like resources)
    pub fn encrypt_chunked(
        &self,
        data: &[u8],
        key: &[u8],
        method: EncryptionMethod,
    ) -> E2eeResult<Vec<String>> {
        let chunk_size = method.chunk_size();
        let mut chunks = Vec::new();

        for chunk in data.chunks(chunk_size) {
            let encrypted = self.encrypt_bytes(chunk, key, method)?;
            chunks.push(encrypted);
        }

        Ok(chunks)
    }

    /// Decrypt chunked data
    pub fn decrypt_chunked(
        &self,
        chunks: &[String],
        key: &[u8],
        method: EncryptionMethod,
    ) -> E2eeResult<Vec<u8>> {
        let mut result = Vec::new();

        for chunk in chunks {
            let decrypted = self.decrypt_bytes(chunk, key, method)?;
            result.extend_from_slice(&decrypted);
        }

        Ok(result)
    }

    /// Derive a key from password using PBKDF2
    pub fn derive_key_from_password(
        &self,
        password: &str,
        salt: &[u8],
        iterations: u32,
        key_length: usize,
    ) -> E2eeResult<Vec<u8>> {
        CryptoService::derive_key_from_password(password, salt, iterations, key_length)
    }
}

impl Default for EncryptionService {
    fn default() -> Self {
        Self::new()
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
    fn test_encrypt_decrypt_string() {
        let service = EncryptionService::new();
        let key = vec![0u8; 32];
        let plain_text = "Hello, World!";

        let encrypted = service.encrypt_string(plain_text, &key, EncryptionMethod::StringV1).unwrap();
        println!("Encrypted: {}", encrypted);

        let decrypted = service.decrypt_string(&encrypted, &key, EncryptionMethod::StringV1).unwrap();
        assert_eq!(decrypted, plain_text);
    }

    #[test]
    fn test_encrypt_decrypt_bytes() {
        let service = EncryptionService::new();
        let key = vec![0u8; 32];
        let plain_data = b"Binary data";

        let encrypted = service.encrypt_bytes(plain_data, &key, EncryptionMethod::KeyV1).unwrap();
        let decrypted = service.decrypt_bytes(&encrypted, &key, EncryptionMethod::KeyV1).unwrap();

        assert_eq!(decrypted, plain_data);
    }

    #[test]
    fn test_encrypt_chunked() {
        let service = EncryptionService::new();
        let key = vec![0u8; 32];

        // Create data larger than chunk size
        let large_data = vec![0u8; 200000]; // 200KB

        let chunks = service.encrypt_chunked(&large_data, &key, EncryptionMethod::FileV1).unwrap();
        assert!(chunks.len() > 1, "Should have multiple chunks");

        let decrypted = service.decrypt_chunked(&chunks, &key, EncryptionMethod::FileV1).unwrap();
        assert_eq!(decrypted, large_data);
    }

    #[test]
    fn test_derive_key() {
        let service = EncryptionService::new();
        let password = "test-password";
        let salt = vec![1u8; 32];

        let key1 = service.derive_key_from_password(password, &salt, 3, 32).unwrap();
        let key2 = service.derive_key_from_password(password, &salt, 3, 32).unwrap();

        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 32);
    }
}
