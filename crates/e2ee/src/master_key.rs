// Master key management for E2EE

use crate::{crypto::CryptoService, E2eeError, E2eeResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;

/// Master key for encrypting/decrypting notes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterKey {
    pub id: String,
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
    pub created_time: i64,
    pub updated_time: i64,
}

impl Default for MasterKey {
    fn default() -> Self {
        Self::new()
    }
}

impl MasterKey {
    /// Create a new master key
    pub fn new() -> Self {
        let mut key_data = vec![0u8; 32]; // 256-bit key
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut key_data);

        let now = chrono::Utc::now().timestamp_millis();

        Self {
            id: Uuid::new_v4().to_string(),
            data: key_data,
            created_time: now,
            updated_time: now,
        }
    }

    /// Create from existing key data
    pub fn from_data(data: Vec<u8>) -> E2eeResult<Self> {
        if data.len() != 32 {
            return Err(E2eeError::InvalidMasterKey(
                "Master key must be 32 bytes (256 bits)".to_string(),
            ));
        }

        let now = chrono::Utc::now().timestamp_millis();

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            data,
            created_time: now,
            updated_time: now,
        })
    }

    /// Encrypt the master key with a password
    pub fn encrypt_with_password(&self, password: &str) -> E2eeResult<String> {
        use aes_gcm::{
            aead::{Aead, KeyInit},
            Aes256Gcm, Nonce,
        };

        let salt = CryptoService::generate_salt();
        let iterations = 220000; // High iteration count for master keys

        // Derive encryption key from password
        let password_key =
            CryptoService::derive_key_from_password(password, &salt, iterations, 32)?;

        // Generate random IV
        let mut iv = [0u8; 12];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut iv);

        // Encrypt the master key data directly (no second PBKDF2)
        let cipher = Aes256Gcm::new_from_slice(&password_key)
            .map_err(|e| E2eeError::InvalidMasterKey(format!("Failed to create cipher: {}", e)))?;
        let nonce = Nonce::from_slice(&iv);

        let ciphertext = cipher
            .encrypt(nonce, &self.data[..])
            .map_err(|e| E2eeError::InvalidMasterKey(format!("Encryption failed: {}", e)))?;

        // Format as JSON
        let json = serde_json::json!({
            "id": self.id,
            "salt": hex::encode(salt),
            "iterations": iterations,
            "data": {
                "iv": hex::encode(iv),
                "ct": hex::encode(&ciphertext)
            }
        });

        Ok(json.to_string())
    }

    /// Decrypt a master key from encrypted format
    pub fn decrypt_from_password(encrypted_json: &str, password: &str) -> E2eeResult<Self> {
        use aes_gcm::{
            aead::{Aead, KeyInit},
            Aes256Gcm, Nonce,
        };

        let encrypted: EncryptedMasterKey = serde_json::from_str(encrypted_json)
            .map_err(|e| E2eeError::InvalidMasterKey(format!("Invalid encrypted format: {}", e)))?;

        // Derive decryption key from password
        let salt = hex::decode(&encrypted.salt)
            .map_err(|e| E2eeError::InvalidMasterKey(format!("Invalid salt: {}", e)))?;
        let password_key =
            CryptoService::derive_key_from_password(password, &salt, encrypted.iterations, 32)?;

        // Decrypt the master key data directly (no second PBKDF2)
        let data_iv = hex::decode(&encrypted.data.iv)
            .map_err(|e| E2eeError::InvalidMasterKey(format!("Invalid data IV: {}", e)))?;
        let data_ct = hex::decode(&encrypted.data.ct)
            .map_err(|e| E2eeError::InvalidMasterKey(format!("Invalid data ciphertext: {}", e)))?;

        let cipher = Aes256Gcm::new_from_slice(&password_key)
            .map_err(|e| E2eeError::InvalidMasterKey(format!("Failed to create cipher: {}", e)))?;
        let nonce = Nonce::from_slice(&data_iv);

        let key_data = cipher
            .decrypt(nonce, data_ct.as_ref())
            .map_err(|e| E2eeError::InvalidMasterKey(format!("Decryption failed: {}", e)))?;

        // Create MasterKey with the original ID
        Ok(Self {
            id: encrypted.id,
            data: key_data,
            created_time: chrono::Utc::now().timestamp_millis(),
            updated_time: chrono::Utc::now().timestamp_millis(),
        })
    }

    /// Get the key bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

/// Encrypted master key format (for JSON serialization)
#[derive(Debug, Deserialize)]
struct EncryptedMasterKey {
    id: String,
    salt: String,
    iterations: u32,
    data: EncryptedKeyData,
}

#[derive(Debug, Deserialize)]
struct EncryptedKeyData {
    iv: String,
    ct: String,
}

/// Manager for master key operations
pub struct MasterKeyManager {
    keys_dir: PathBuf,
}

impl MasterKeyManager {
    /// Create a new master key manager
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            keys_dir: data_dir.join("keys"),
        }
    }

    /// Ensure the keys directory exists
    async fn ensure_dir(&self) -> E2eeResult<()> {
        fs::create_dir_all(&self.keys_dir)
            .await
            .map_err(|e| E2eeError::Crypto(format!("Failed to create keys directory: {}", e)))?;
        Ok(())
    }

    /// Save a master key to disk (encrypted with password)
    pub async fn save_key(&self, key: &MasterKey, password: &str) -> E2eeResult<()> {
        self.ensure_dir().await?;

        let encrypted: String = key.encrypt_with_password(password)?;
        let key_path = self.keys_dir.join(format!("{}.json", key.id));

        fs::write(&key_path, encrypted.as_bytes())
            .await
            .map_err(|e| E2eeError::Crypto(format!("Failed to write master key: {}", e)))?;

        Ok(())
    }

    /// Load a master key from disk
    pub async fn load_key(&self, id: &str, password: &str) -> E2eeResult<MasterKey> {
        let key_path = self.keys_dir.join(format!("{}.json", id));

        let encrypted = fs::read_to_string(&key_path).await.map_err(|e| {
            E2eeError::InvalidMasterKey(format!("Failed to read master key: {}", e))
        })?;

        MasterKey::decrypt_from_password(&encrypted, password)
    }

    /// List all master key IDs
    pub async fn list_keys(&self) -> E2eeResult<Vec<String>> {
        if !self.keys_dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = fs::read_dir(&self.keys_dir)
            .await
            .map_err(|e| E2eeError::Crypto(format!("Failed to read keys directory: {}", e)))?;

        let mut key_ids = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| E2eeError::Crypto(format!("Failed to read directory entry: {}", e)))?
        {
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            if file_name_str.ends_with(".json") {
                let id = file_name_str
                    .strip_suffix(".json")
                    .unwrap_or(&file_name_str);
                key_ids.push(id.to_string());
            }
        }

        Ok(key_ids)
    }

    /// Delete a master key
    pub async fn delete_key(&self, id: &str) -> E2eeResult<()> {
        let key_path = self.keys_dir.join(format!("{}.json", id));

        fs::remove_file(&key_path)
            .await
            .map_err(|e| E2eeError::Crypto(format!("Failed to delete master key: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_master_key_new() {
        let key = MasterKey::new();
        assert_eq!(key.data.len(), 32);
        assert!(!key.id.is_empty());
    }

    #[test]
    fn test_master_key_encrypt_decrypt() {
        let key = MasterKey::new();
        let password = "test-password";

        let encrypted = key.encrypt_with_password(password).unwrap();
        println!("Encrypted master key: {}", encrypted);

        let decrypted = MasterKey::decrypt_from_password(&encrypted, password).unwrap();
        assert_eq!(decrypted.data, key.data);
        assert_eq!(decrypted.id, key.id);
    }

    #[test]
    fn test_master_key_wrong_password() {
        let key = MasterKey::new();
        let encrypted = key.encrypt_with_password("correct-password").unwrap();

        let result = MasterKey::decrypt_from_password(&encrypted, "wrong-password");
        assert!(result.is_err());
    }

    #[test]
    fn test_master_key_from_data() {
        let data = vec![0u8; 32];
        let key = MasterKey::from_data(data.clone()).unwrap();
        assert_eq!(key.data, data);
    }

    #[test]
    fn test_master_key_invalid_length() {
        let data = vec![0u8; 16]; // Too short
        let result = MasterKey::from_data(data);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_master_key_manager() {
        let temp_dir = std::env::temp_dir().join(format!("test-keys-{}", Uuid::new_v4()));
        let manager = MasterKeyManager::new(temp_dir.clone());

        let key = MasterKey::new();
        let password = "test-password";

        // Save and load
        manager.save_key(&key, password).await.unwrap();
        let loaded = manager.load_key(&key.id, password).await.unwrap();

        assert_eq!(loaded.data, key.data);
        assert_eq!(loaded.id, key.id);

        // List keys
        let key_ids = manager.list_keys().await.unwrap();
        assert_eq!(key_ids, vec![key.id]);

        // Cleanup
        tokio::fs::remove_dir_all(temp_dir).await.ok();
    }
}
