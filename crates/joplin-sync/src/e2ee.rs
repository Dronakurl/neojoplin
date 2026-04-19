// End-to-end encryption (E2EE) implementation for Joplin compatibility
//
// This module implements the JED (Joplin Encrypted Data) format and
// encryption/decryption operations compatible with Joplin CLI.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// JED header identifier
pub const JED_IDENTIFIER: &str = "JED";

/// Current JED format version
pub const JED_VERSION: &str = "01";

/// Encryption methods supported by Joplin
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum EncryptionMethod {
    // Legacy SJCL methods (deprecated but still supported for decryption)
    SJCL = 1,
    SJCL2 = 2,
    SJCL3 = 3,
    SJCL4 = 4,
    SJCL1a = 5,
    Custom = 6,
    SJCL1b = 7,

    // Current methods
    KeyV1 = 8,   // For master key encryption
    FileV1 = 9,  // For file encryption
    StringV1 = 10, // For string encryption (default)
}

impl EncryptionMethod {
    pub fn from_u8(value: u8) -> Result<Self> {
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
            _ => Err(anyhow!("Invalid encryption method: {}", value)),
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Get the default encryption method for strings
    pub fn default_string() -> Self {
        EncryptionMethod::StringV1
    }

    /// Get the default encryption method for master keys
    pub fn default_master_key() -> Self {
        EncryptionMethod::KeyV1
    }

    /// Get the default encryption method for files
    pub fn default_file() -> Self {
        EncryptionMethod::FileV1
    }
}

/// JED format header
#[derive(Debug, Clone)]
pub struct JedHeader {
    pub identifier: String,
    pub version: String,
    pub metadata: JedMetadata,
}

/// JED metadata
#[derive(Debug, Clone)]
pub struct JedMetadata {
    pub length: u32,
    pub encryption_method: EncryptionMethod,
    pub master_key_id: String,
}

/// Master key information (compatible with sync.json format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterKey {
    pub id: String,
    pub created_time: i64,
    pub updated_time: i64,
    pub source_application: String,
    pub encryption_method: i32,
    pub checksum: String,
    pub content: String,

    #[serde(default)]
    pub has_been_used: bool,

    #[serde(default)]
    pub enabled: bool,
}

impl MasterKey {
    /// Create a new master key
    pub fn new(id: String, encrypted_content: String, encryption_method: EncryptionMethod) -> Self {
        Self {
            id,
            created_time: joplin_domain::now_ms(),
            updated_time: joplin_domain::now_ms(),
            source_application: "neojoplin".to_string(),
            encryption_method: encryption_method.as_u8() as i32,
            checksum: String::new(), // Not used for modern encryption methods
            content: encrypted_content,
            has_been_used: false,
            enabled: true,
        }
    }

    /// Mark the master key as used
    pub fn mark_as_used(&mut self) {
        self.has_been_used = true;
        self.updated_time = joplin_domain::now_ms();
    }

    /// Check if the master key is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// E2EE service for encryption and decryption operations
pub struct E2eeService {
    master_keys: HashMap<String, String>, // key_id -> decrypted_key
    active_master_key_id: Option<String>,
    master_password: Option<String>,
}

impl E2eeService {
    /// Create a new E2EE service
    pub fn new() -> Self {
        Self {
            master_keys: HashMap::new(),
            active_master_key_id: None,
            master_password: None,
        }
    }

    /// Set the master password
    pub fn set_master_password(&mut self, password: String) {
        self.master_password = Some(password);
    }

    /// Get the master password
    pub fn get_master_password(&self) -> Option<&String> {
        self.master_password.as_ref()
    }

    /// Add a decrypted master key
    pub fn add_master_key(&mut self, key_id: String, decrypted_key: String) {
        self.master_keys.insert(key_id, decrypted_key);
    }

    /// Set the active master key ID
    pub fn set_active_master_key(&mut self, key_id: String) {
        self.active_master_key_id = Some(key_id);
    }

    /// Get the active master key ID
    pub fn get_active_master_key_id(&self) -> Option<&String> {
        self.active_master_key_id.as_ref()
    }

    /// Get the active master key (decrypted)
    pub fn get_active_master_key(&self) -> Option<&String> {
        if let Some(key_id) = &self.active_master_key_id {
            self.master_keys.get(key_id)
        } else {
            None
        }
    }

    /// Generate a new master key
    pub fn generate_master_key(&self, password: &str) -> Result<(String, MasterKey)> {
        // Generate a random 256-bit key
        let key_id = uuid::Uuid::new_v4().to_string();
        let master_key_bytes = crate::crypto::generate_key();
        let master_key_hex = hex::encode(master_key_bytes);

        // Derive encryption key from password using the same fixed salt
        let salt = b"neojoplin-e2ee-salt-32bytes-"; // Fixed salt for simplicity
        let derived_key = crate::crypto::derive_key(password, salt, 100_000);

        // Encrypt the master key with the derived key using AES-256-GCM
        let encrypted_content = self.encrypt_master_key_aes(&master_key_hex, &derived_key)?;

        let master_key_obj = MasterKey::new(key_id.clone(), encrypted_content, EncryptionMethod::KeyV1);

        Ok((key_id, master_key_obj))
    }

    /// Encrypt a master key with a derived key using AES-256-GCM
    fn encrypt_master_key_aes(&self, plain_key: &str, derived_key: &[u8; 32]) -> Result<String> {
        let master_key_bytes = plain_key.as_bytes();
        let encrypted = crate::crypto::encrypt_aes256_gcm(derived_key, master_key_bytes)?;
        Ok(hex::encode(encrypted))
    }

    /// Decrypt a master key with a derived key using AES-256-GCM
    fn decrypt_master_key_aes(&self, encrypted_content: &str, derived_key: &[u8; 32]) -> Result<String> {
        let encrypted = hex::decode(encrypted_content)
            .map_err(|e| anyhow!("Invalid hex in encrypted content: {}", e))?;

        let decrypted = crate::crypto::decrypt_aes256_gcm(derived_key, &encrypted)?;

        String::from_utf8(decrypted)
            .map_err(|_| anyhow!("Invalid UTF-8 in decrypted content"))
    }

    /// Load and decrypt a master key
    pub fn load_master_key(&mut self, master_key: &MasterKey) -> Result<()> {
        let password = self.master_password.as_ref()
            .ok_or_else(|| anyhow!("Master password not set"))?;

        // Derive a 256-bit key from the password
        let salt = b"neojoplin-e2ee-salt-32bytes-"; // Fixed salt for simplicity
        let derived_key = crate::crypto::derive_key(password, salt, 100_000);

        let decrypted_key = self.decrypt_master_key_aes(&master_key.content, &derived_key)?;
        self.master_keys.insert(master_key.id.clone(), decrypted_key);

        // Set as active if it's the first key or if it's newer
        if self.active_master_key_id.is_none() || master_key.is_enabled() {
            self.active_master_key_id = Some(master_key.id.clone());
        }

        Ok(())
    }

    /// Generate a random hex key
    /// Encrypt a string using the active master key
    pub fn encrypt_string(&self, plaintext: &str) -> Result<String> {
        let master_key = self.get_active_master_key()
            .ok_or_else(|| anyhow!("No active master key"))?;

        self.encrypt_string_with_key(plaintext, master_key)
    }

    /// Encrypt a string with a specific key
    fn encrypt_string_with_key(&self, plaintext: &str, key: &str) -> Result<String> {
        // Create JED format
        let key_id = self.active_master_key_id.as_ref()
            .ok_or_else(|| anyhow!("No active master key ID"))?;

        // Remove dashes from UUID to get 32-char hex string
        let master_key_id_hex = key_id.replace('-', "");

        // Convert hex key to bytes
        let key_bytes = hex::decode(key)
            .map_err(|_| anyhow!("Invalid hex key"))?;

        // Ensure key is exactly 32 bytes
        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes[..32.min(key_bytes.len())]);

        let metadata = JedMetadata {
            length: plaintext.len() as u32,
            encryption_method: EncryptionMethod::StringV1,
            master_key_id: master_key_id_hex,
        };

        // Use AES-256-GCM encryption
        let encrypted_data = crate::crypto::encrypt_aes256_gcm(&key_array, plaintext.as_bytes())?;

        // Encode encrypted data as hex
        let encrypted_hex = hex::encode(encrypted_data);

        // Serialize JED format
        Ok(format_jed_header(&JedHeader {
            identifier: JED_IDENTIFIER.to_string(),
            version: JED_VERSION.to_string(),
            metadata,
        }) + &encrypted_hex)
    }

    /// Decrypt a JED-formatted string
    pub fn decrypt_string(&self, jed_data: &str) -> Result<String> {
        // Parse JED header
        let (header, encrypted_hex) = parse_jed_header(jed_data)?;

        // Try to find the master key - first try direct lookup, then try with dashes added
        let master_key = if let Some(key) = self.master_keys.get(&header.metadata.master_key_id) {
            key
        } else {
            // Try adding dashes to make it a UUID
            let key_id_with_dashes = format!(
                "{}-{}-{}-{}-{}",
                &header.metadata.master_key_id[0..8],
                &header.metadata.master_key_id[8..12],
                &header.metadata.master_key_id[12..16],
                &header.metadata.master_key_id[16..20],
                &header.metadata.master_key_id[20..32]
            );

            self.master_keys.get(&key_id_with_dashes)
                .ok_or_else(|| anyhow!("Master key not found: {}", header.metadata.master_key_id))?
        };

        // Decrypt using the appropriate method
        match header.metadata.encryption_method {
            EncryptionMethod::StringV1 => {
                // Decode hex encrypted data
                let encrypted_data = hex::decode(encrypted_hex)
                    .map_err(|e| anyhow!("Invalid hex in encrypted data: {}", e))?;

                // Convert hex master key to bytes
                let key_bytes = hex::decode(master_key)
                    .map_err(|_| anyhow!("Invalid hex master key"))?;

                // Ensure key is exactly 32 bytes
                let mut key_array = [0u8; 32];
                key_array.copy_from_slice(&key_bytes[..32.min(key_bytes.len())]);

                // Decrypt using AES-256-GCM
                let decrypted = crate::crypto::decrypt_aes256_gcm(&key_array, &encrypted_data)?;

                String::from_utf8(decrypted)
                    .map_err(|_| anyhow!("Invalid UTF-8 in decrypted data"))
            },
            _ => Err(anyhow!("Unsupported encryption method: {:?}", header.metadata.encryption_method)),
        }
    }

    /// Check if E2EE is enabled
    pub fn is_enabled(&self) -> bool {
        self.active_master_key_id.is_some() && !self.master_keys.is_empty()
    }

    /// Get all loaded master key IDs
    pub fn get_master_key_ids(&self) -> Vec<String> {
        self.master_keys.keys().cloned().collect()
    }
}

impl Default for E2eeService {
    fn default() -> Self {
        Self::new()
    }
}

/// Format a JED header
fn format_jed_header(header: &JedHeader) -> String {
    let metadata_str = format!(
        "{:06x}{:02x}{}",
        header.metadata.length,
        header.metadata.encryption_method.as_u8(),
        header.metadata.master_key_id
    );

    format!("{}{}{:06x}{}",
        header.identifier,
        header.version,
        metadata_str.len(), // Just the metadata string length
        metadata_str
    )
}

/// Parse a JED header
fn parse_jed_header(data: &str) -> Result<(JedHeader, &str)> {
    if !data.starts_with(JED_IDENTIFIER) {
        return Err(anyhow!("Invalid JED identifier"));
    }

    if data.len() < 5 {
        return Err(anyhow!("JED data too short"));
    }

    let identifier = data[0..3].to_string();
    let version = data[3..5].to_string();

    // Parse metadata length (6 hex chars)
    if data.len() < 11 {
        return Err(anyhow!("JED data too short for metadata length"));
    }

    let metadata_length_str = &data[5..11];
    let metadata_length = u32::from_str_radix(metadata_length_str, 16)
        .map_err(|_| anyhow!("Invalid metadata length"))?;

    if data.len() < 11 + metadata_length as usize {
        return Err(anyhow!("JED data too short for metadata"));
    }

    let metadata_data = &data[11..11 + metadata_length as usize];

    // Parse encryption method (2 hex chars)
    let encryption_method_str = &metadata_data[6..8];
    let encryption_method_val = u8::from_str_radix(encryption_method_str, 16)
        .map_err(|_| anyhow!("Invalid encryption method"))?;
    let encryption_method = EncryptionMethod::from_u8(encryption_method_val)?;

    // Parse master key ID (32 hex chars)
    let master_key_id = if metadata_data.len() >= 40 {
        metadata_data[8..40].to_string()
    } else {
        return Err(anyhow!("Metadata too short for master key ID"));
    };

    let metadata = JedMetadata {
        length: 0, // Not used in current implementation
        encryption_method,
        master_key_id,
    };

    let header = JedHeader {
        identifier,
        version,
        metadata,
    };

    let encrypted_data = &data[11 + metadata_length as usize..];

    Ok((header, encrypted_data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_method_conversion() {
        assert_eq!(EncryptionMethod::StringV1.as_u8(), 10);
        assert_eq!(EncryptionMethod::KeyV1.as_u8(), 8);
        assert_eq!(EncryptionMethod::from_u8(10).unwrap(), EncryptionMethod::StringV1);
    }

    #[test]
    fn test_jed_header_formatting() {
        let header = JedHeader {
            identifier: JED_IDENTIFIER.to_string(),
            version: "01".to_string(),
            metadata: JedMetadata {
                length: 100,
                encryption_method: EncryptionMethod::StringV1,
                master_key_id: "0123456789abcdef0123456789abcdef".to_string(),
            },
        };

        let formatted = format_jed_header(&header);
        assert!(formatted.starts_with("JED01"));

        // Parse it back
        let (parsed_header, _) = parse_jed_header(&formatted).unwrap();
        assert_eq!(parsed_header.metadata.master_key_id, "0123456789abcdef0123456789abcdef");
        assert_eq!(parsed_header.metadata.encryption_method, EncryptionMethod::StringV1);
    }

    #[test]
    fn test_master_key_generation_and_loading() {
        let mut service = E2eeService::new();
        let password = "test_password";
        service.set_master_password(password.to_string());

        // Generate a master key
        let (key_id, master_key) = service.generate_master_key(password).unwrap();

        // Load the master key
        service.load_master_key(&master_key).unwrap();

        // Verify it's loaded
        assert_eq!(service.get_active_master_key_id(), Some(&key_id));
        assert!(service.is_enabled());
    }

    #[test]
    fn test_string_encryption_decryption() {
        let mut service = E2eeService::new();
        let password = "test_password";
        service.set_master_password(password.to_string());

        // Generate and load a master key
        let (key_id, master_key) = service.generate_master_key(password).unwrap();
        service.load_master_key(&master_key).unwrap();
        service.set_active_master_key(key_id.clone());

        // Make sure we have the right key ID set
        assert_eq!(service.get_active_master_key_id(), Some(&key_id));

        let original = "Hello, World!";
        let encrypted = service.encrypt_string(original).unwrap();
        let decrypted = service.decrypt_string(&encrypted).unwrap();

        assert_eq!(original, decrypted);
    }
}
