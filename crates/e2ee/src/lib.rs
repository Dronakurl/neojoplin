// End-to-end encryption (E2EE) support for NeoJoplin
// Joplin-compatible encryption implementation

pub mod encryption;
pub mod master_key;
pub mod jed_format;
pub mod crypto;

pub use encryption::EncryptionService;
pub use crypto::{EncryptionMethod, CryptoService, EncryptedData};
pub use master_key::{MasterKey, MasterKeyManager};
pub use jed_format::{JedFormat, JedEncoder, JedDecoder};

/// E2EE error types
#[derive(Debug, thiserror::Error)]
pub enum E2eeError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Invalid master key: {0}")]
    InvalidMasterKey(String),

    #[error("Master key not loaded")]
    MasterKeyNotLoaded,

    #[error("Invalid JED format: {0}")]
    InvalidJedFormat(String),

    #[error("Unsupported encryption method: {0}")]
    UnsupportedEncryptionMethod(i32),

    #[error("Crypto error: {0}")]
    Crypto(String),
}

/// Result type for E2EE operations
pub type E2eeResult<T> = std::result::Result<T, E2eeError>;

/// Encryption context that holds loaded master keys
pub struct EncryptionContext {
    master_keys: std::collections::HashMap<String, Vec<u8>>,
    active_master_key_id: Option<String>,
}

impl EncryptionContext {
    pub fn new() -> Self {
        Self {
            master_keys: std::collections::HashMap::new(),
            active_master_key_id: None,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            master_keys: std::collections::HashMap::with_capacity(capacity),
            active_master_key_id: None,
        }
    }

    /// Load a master key into the context
    pub fn load_master_key(&mut self, id: String, key: Vec<u8>) {
        self.master_keys.insert(id.clone(), key);
        if self.active_master_key_id.is_none() {
            self.active_master_key_id = Some(id);
        }
    }

    /// Get the active master key
    pub fn active_master_key(&self) -> E2eeResult<&Vec<u8>> {
        self.active_master_key_id
            .as_ref()
            .and_then(|id| self.master_keys.get(id))
            .ok_or(E2eeError::MasterKeyNotLoaded)
    }

    /// Get a specific master key by ID
    pub fn master_key(&self, id: &str) -> E2eeResult<&Vec<u8>> {
        self.master_keys
            .get(id)
            .ok_or_else(|| E2eeError::InvalidMasterKey(format!("Master key not found: {}", id)))
    }

    /// Set the active master key
    pub fn set_active_master_key(&mut self, id: String) {
        if self.master_keys.contains_key(&id) {
            self.active_master_key_id = Some(id);
        }
    }

    /// Check if a master key is loaded
    pub fn is_master_key_loaded(&self, id: &str) -> bool {
        self.master_keys.contains_key(id)
    }

    /// Get the number of loaded master keys
    pub fn loaded_keys_count(&self) -> usize {
        self.master_keys.len()
    }
}

impl Default for EncryptionContext {
    fn default() -> Self {
        Self::new()
    }
}

/// E2EE manager that provides high-level encryption operations
pub struct E2eeManager {
    context: EncryptionContext,
    encryption_service: EncryptionService,
}

impl E2eeManager {
    pub fn new() -> Self {
        Self {
            context: EncryptionContext::new(),
            encryption_service: EncryptionService::new(),
        }
    }

    /// Get the encryption context (for loading master keys)
    pub fn context(&mut self) -> &mut EncryptionContext {
        &mut self.context
    }

    /// Encrypt a note body
    pub fn encrypt_note(&mut self, plain_text: &str, master_key_id: &str) -> E2eeResult<String> {
        let master_key = self.context.master_key(master_key_id)?;
        self.encryption_service
            .encrypt_string(plain_text, master_key, EncryptionMethod::StringV1)
    }

    /// Decrypt a note body
    pub fn decrypt_note(&mut self, cipher_text: &str, master_key_id: &str) -> E2eeResult<String> {
        let master_key = self.context.master_key(master_key_id)?;
        self.encryption_service
            .decrypt_string(cipher_text, master_key, EncryptionMethod::StringV1)
    }

    /// Encrypt a master key with a password
    pub fn encrypt_master_key(&mut self, master_key: &[u8], password: &str) -> E2eeResult<String> {
        let password_key = self.derive_password_key(password)?;

        // Use CryptoService directly to get EncryptedData
        let encrypted = CryptoService::encrypt_bytes(
            &password_key,
            master_key,
            EncryptionMethod::KeyV1,
        )?;

        // Format as JSON
        let json = serde_json::json!({
            "salt": hex::encode(&encrypted.salt),
            "iv": hex::encode(&encrypted.iv),
            "ct": hex::encode(&encrypted.ciphertext)
        });

        Ok(json.to_string())
    }

    /// Decrypt a master key with a password
    pub fn decrypt_master_key(&mut self, encrypted_json: &str, password: &str) -> E2eeResult<Vec<u8>> {
        let password_key = self.derive_password_key(password)?;
        self.encryption_service
            .decrypt_bytes(encrypted_json, &password_key, EncryptionMethod::KeyV1)
    }

    /// Generate a new master key
    pub fn generate_master_key(&mut self) -> Vec<u8> {
        use rand::RngCore;
        let mut key = [0u8; 32]; // 256-bit key
        rand::thread_rng().fill_bytes(&mut key);
        key.to_vec()
    }

    /// Derive a key from password using PBKDF2
    fn derive_password_key(&self, password: &str) -> E2eeResult<Vec<u8>> {
        self.encryption_service.derive_key_from_password(
            password,
            b"neojoplin-master-key-salt", // In production, this should be random
            220000, // High iteration count for master keys
            32, // 256-bit key
        )
    }
}

impl Default for E2eeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_context() {
        let mut context = EncryptionContext::new();
        let key_id = "test-key-1".to_string();
        let key = vec![0u8; 32];

        context.load_master_key(key_id.clone(), key);
        assert!(context.is_master_key_loaded(&key_id));
        assert_eq!(context.loaded_keys_count(), 1);
        assert_eq!(context.active_master_key().unwrap(), &vec![0u8; 32]);
    }

    #[test]
    fn test_e2ee_manager() {
        let mut manager = E2eeManager::new();
        let master_key = manager.generate_master_key();

        manager.context().load_master_key("test-key".to_string(), master_key);

        let plain_text = "Hello, World!";
        let encrypted = manager.encrypt_note(plain_text, "test-key").unwrap();
        println!("Encrypted: {}", encrypted);

        let decrypted = manager.decrypt_note(&encrypted, "test-key").unwrap();
        assert_eq!(decrypted, plain_text);
    }
}
