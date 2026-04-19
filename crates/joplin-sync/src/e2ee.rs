// End-to-end encryption (E2EE) implementation for Joplin compatibility
//
// This module implements the JED (Joplin Encrypted Data) format and
// encryption/decryption operations compatible with Joplin CLI v3.5+.
//
// JED format:
//   [JED01][metadata_length:6hex][encryption_method:2hex][master_key_id:32hex]
//   [chunk1_length:6hex][chunk1_json]
//   [chunk2_length:6hex][chunk2_json]
//   ...
//
// Each chunk is JSON: {"salt":"base64","iv":"base64","ct":"base64"}
// Encrypted with PBKDF2-HMAC-SHA512 + AES-256-GCM.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const JED_IDENTIFIER: &str = "JED";
pub const JED_VERSION: &str = "01";

// PBKDF2 iteration counts matching Joplin reference implementation
const KEY_V1_ITERATIONS: u32 = 220_000;
const STRING_V1_ITERATIONS: u32 = 3;
const FILE_V1_ITERATIONS: u32 = 3;

// Chunk size for StringV1 encryption (64KB of UTF-16LE = 32K chars)
const STRING_V1_CHUNK_SIZE: usize = 65536;

/// Encryption methods supported by Joplin
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
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

    pub fn default_string() -> Self { EncryptionMethod::StringV1 }
    pub fn default_master_key() -> Self { EncryptionMethod::KeyV1 }
    pub fn default_file() -> Self { EncryptionMethod::FileV1 }

    fn iterations(self) -> u32 {
        match self {
            EncryptionMethod::KeyV1 => KEY_V1_ITERATIONS,
            EncryptionMethod::FileV1 => FILE_V1_ITERATIONS,
            EncryptionMethod::StringV1 => STRING_V1_ITERATIONS,
            _ => 0,
        }
    }
}

/// JED format header
#[derive(Debug, Clone)]
pub struct JedHeader {
    pub encryption_method: EncryptionMethod,
    pub master_key_id: String, // 32 hex chars (no dashes)
}

/// Master key information (compatible with info.json format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterKey {
    pub id: String,
    pub created_time: i64,
    pub updated_time: i64,
    pub source_application: String,
    pub encryption_method: i32,
    #[serde(default)]
    pub checksum: String,
    pub content: String,
    #[serde(default, rename = "hasBeenUsed")]
    pub has_been_used: bool,
    #[serde(default)]
    pub enabled: bool,
}

impl MasterKey {
    pub fn new(id: String, encrypted_content: String, encryption_method: EncryptionMethod) -> Self {
        Self {
            id,
            created_time: joplin_domain::now_ms(),
            updated_time: joplin_domain::now_ms(),
            source_application: "neojoplin".to_string(),
            encryption_method: encryption_method.as_u8() as i32,
            checksum: String::new(),
            content: encrypted_content,
            has_been_used: false,
            enabled: true,
        }
    }

    pub fn mark_as_used(&mut self) {
        self.has_been_used = true;
        self.updated_time = joplin_domain::now_ms();
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// E2EE service for encryption and decryption operations
pub struct E2eeService {
    master_keys: HashMap<String, String>, // key_id -> decrypted hex key
    active_master_key_id: Option<String>,
    master_password: Option<String>,
}

impl E2eeService {
    pub fn new() -> Self {
        Self {
            master_keys: HashMap::new(),
            active_master_key_id: None,
            master_password: None,
        }
    }

    pub fn set_master_password(&mut self, password: String) {
        self.master_password = Some(password);
    }

    pub fn get_master_password(&self) -> Option<&String> {
        self.master_password.as_ref()
    }

    pub fn add_master_key(&mut self, key_id: String, decrypted_key: String) {
        self.master_keys.insert(key_id, decrypted_key);
    }

    pub fn set_active_master_key(&mut self, key_id: String) {
        self.active_master_key_id = Some(key_id);
    }

    pub fn get_active_master_key_id(&self) -> Option<&String> {
        self.active_master_key_id.as_ref()
    }

    pub fn get_active_master_key(&self) -> Option<&String> {
        self.active_master_key_id.as_ref()
            .and_then(|id| self.master_keys.get(id))
    }

    /// Look up a master key by ID, trying both with and without UUID dashes
    fn find_master_key(&self, key_id: &str) -> Result<&String> {
        if let Some(key) = self.master_keys.get(key_id) {
            return Ok(key);
        }
        // Try adding dashes to make it a UUID format
        if key_id.len() == 32 {
            let with_dashes = format!(
                "{}-{}-{}-{}-{}",
                &key_id[0..8], &key_id[8..12], &key_id[12..16],
                &key_id[16..20], &key_id[20..32]
            );
            if let Some(key) = self.master_keys.get(&with_dashes) {
                return Ok(key);
            }
        }
        // Try removing dashes
        let without_dashes = key_id.replace('-', "");
        if let Some(key) = self.master_keys.get(&without_dashes) {
            return Ok(key);
        }
        Err(anyhow!("Master key not found: {} (loaded keys: {:?})",
            key_id, self.master_keys.keys().collect::<Vec<_>>()))
    }

    /// Generate a new master key encrypted with the user's password (KeyV1)
    pub fn generate_master_key(&self, password: &str) -> Result<(String, MasterKey)> {
        let key_id = uuid::Uuid::new_v4().to_string().replace('-', "");
        let master_key_bytes = crate::crypto::generate_key();
        let master_key_hex = hex::encode(master_key_bytes);

        // Encrypt with KeyV1: PBKDF2(password, random_salt, 220000, SHA-512) -> AES-256-GCM
        let salt = crate::crypto::generate_salt();
        // Joplin encodes the hex string as hex bytes for KeyV1
        let plaintext_bytes = hex::decode(&master_key_hex)
            .map_err(|e| anyhow!("Invalid hex: {}", e))?;
        let chunk = crate::crypto::encrypt_chunk(password, &salt, &plaintext_bytes, KEY_V1_ITERATIONS)?;
        let encrypted_content = serde_json::to_string(&chunk)?;

        let master_key_obj = MasterKey::new(key_id.clone(), encrypted_content, EncryptionMethod::KeyV1);
        Ok((key_id, master_key_obj))
    }

    /// Load and decrypt a master key from its encrypted form
    pub fn load_master_key(&mut self, master_key: &MasterKey) -> Result<()> {
        let password = self.master_password.as_ref()
            .ok_or_else(|| anyhow!("Master password not set"))?
            .clone();

        let method = EncryptionMethod::from_u8(master_key.encryption_method as u8)
            .unwrap_or(EncryptionMethod::KeyV1);

        let decrypted_key = self.decrypt_single_chunk(
            &password,
            &master_key.content,
            method,
        )?;

        tracing::info!("Loaded master key: {} (method: {:?}, decrypted key length: {})",
            master_key.id, method, decrypted_key.len());

        // Normalize key ID (remove dashes)
        let key_id = master_key.id.replace('-', "");
        self.master_keys.insert(key_id.clone(), decrypted_key);

        if self.active_master_key_id.is_none() {
            self.active_master_key_id = Some(key_id);
        }

        Ok(())
    }

    /// Decrypt a single encryption chunk (for master key decryption or single-chunk items)
    fn decrypt_single_chunk(&self, password: &str, content: &str, method: EncryptionMethod) -> Result<String> {
        let chunk: crate::crypto::EncryptionChunk = serde_json::from_str(content)
            .map_err(|e| anyhow!("Invalid encryption chunk JSON: {} (content starts with: {})",
                e, &content[..content.len().min(100)]))?;

        let decrypted_bytes = crate::crypto::decrypt_chunk(password, &chunk, method.iterations())?;

        // Decode based on method
        match method {
            EncryptionMethod::KeyV1 => {
                // KeyV1: result is hex-encoded master key
                Ok(hex::encode(&decrypted_bytes))
            }
            EncryptionMethod::StringV1 => {
                // StringV1: result is UTF-16LE encoded string
                decode_utf16le(&decrypted_bytes)
            }
            EncryptionMethod::FileV1 => {
                // FileV1: result is base64-encoded file data
                use base64::Engine;
                Ok(base64::engine::general_purpose::STANDARD.encode(&decrypted_bytes))
            }
            _ => {
                // Try UTF-8 as fallback
                String::from_utf8(decrypted_bytes.clone())
                    .or_else(|_| decode_utf16le(&decrypted_bytes))
            }
        }
    }

    /// Encrypt a string using the active master key (StringV1 with chunked JED format)
    pub fn encrypt_string(&self, plaintext: &str) -> Result<String> {
        let master_key_hex = self.get_active_master_key()
            .ok_or_else(|| anyhow!("No active master key"))?
            .clone();
        let key_id = self.active_master_key_id.as_ref()
            .ok_or_else(|| anyhow!("No active master key ID"))?
            .replace('-', "");

        let header = encode_jed_header(&JedHeader {
            encryption_method: EncryptionMethod::StringV1,
            master_key_id: key_id,
        });

        let mut result = header;

        // Encode plaintext as UTF-16LE, then encrypt in chunks
        let utf16le_bytes = encode_utf16le(plaintext);

        for chunk_data in utf16le_bytes.chunks(STRING_V1_CHUNK_SIZE) {
            let salt = crate::crypto::generate_salt();
            let chunk = crate::crypto::encrypt_chunk(
                &master_key_hex, &salt, chunk_data, STRING_V1_ITERATIONS,
            )?;
            let chunk_json = serde_json::to_string(&chunk)?;
            result.push_str(&format!("{:06x}{}", chunk_json.len(), chunk_json));
        }

        Ok(result)
    }

    /// Decrypt a JED-formatted encrypted string
    pub fn decrypt_string(&self, jed_data: &str) -> Result<String> {
        let (header, body) = parse_jed_header(jed_data)?;
        let master_key_hex = self.find_master_key(&header.master_key_id)?;

        // Read and decrypt chunks
        let mut plaintext = String::new();
        let mut pos = 0;

        while pos < body.len() {
            if pos + 6 > body.len() {
                break;
            }

            let chunk_len_hex = &body[pos..pos + 6];
            let chunk_len = usize::from_str_radix(chunk_len_hex, 16)
                .map_err(|_| anyhow!("Invalid chunk length hex: {}", chunk_len_hex))?;
            pos += 6;

            if chunk_len == 0 {
                continue;
            }
            if pos + chunk_len > body.len() {
                return Err(anyhow!("Chunk extends beyond data (need {} bytes at pos {}, have {})",
                    chunk_len, pos, body.len()));
            }

            let chunk_json = &body[pos..pos + chunk_len];
            pos += chunk_len;

            let decrypted = self.decrypt_single_chunk(
                master_key_hex,
                chunk_json,
                header.encryption_method,
            )?;
            plaintext.push_str(&decrypted);
        }

        Ok(plaintext)
    }

    pub fn is_enabled(&self) -> bool {
        self.active_master_key_id.is_some() && !self.master_keys.is_empty()
    }

    pub fn get_master_key_ids(&self) -> Vec<String> {
        self.master_keys.keys().cloned().collect()
    }
}

impl Default for E2eeService {
    fn default() -> Self {
        Self::new()
    }
}

/// Encode UTF-16LE bytes from a string (matching Joplin's Buffer.from(str, 'utf16le'))
fn encode_utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .flat_map(|c| c.to_le_bytes())
        .collect()
}

/// Decode UTF-16LE bytes to a string
fn decode_utf16le(bytes: &[u8]) -> Result<String> {
    if bytes.len() % 2 != 0 {
        return Err(anyhow!("UTF-16LE data has odd length: {}", bytes.len()));
    }
    let u16_values: Vec<u16> = bytes.chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    String::from_utf16(&u16_values)
        .map_err(|e| anyhow!("Invalid UTF-16LE data: {}", e))
}

/// Encode a JED header matching Joplin's encodeHeader_()
fn encode_jed_header(header: &JedHeader) -> String {
    assert_eq!(header.master_key_id.len(), 32, "Master key ID must be 32 hex chars");
    let metadata = format!("{:02x}{}", header.encryption_method.as_u8(), header.master_key_id);
    format!("JED01{:06x}{}", metadata.len(), metadata)
}

/// Parse a JED header, returning (header, remaining_body)
fn parse_jed_header(data: &str) -> Result<(JedHeader, &str)> {
    if data.len() < 5 || &data[0..3] != "JED" {
        return Err(anyhow!("Invalid JED identifier"));
    }

    // JED01 + 6-hex metadata_length
    if data.len() < 11 {
        return Err(anyhow!("JED data too short for header"));
    }

    let md_size = usize::from_str_radix(&data[5..11], 16)
        .map_err(|_| anyhow!("Invalid metadata size hex: {}", &data[5..11]))?;

    let header_end = 11 + md_size;
    if data.len() < header_end {
        return Err(anyhow!("JED data too short for metadata (need {}, have {})", header_end, data.len()));
    }

    let metadata = &data[11..header_end];

    // Metadata format: encryption_method (2 hex) + master_key_id (32 hex)
    if metadata.len() < 34 {
        return Err(anyhow!("Metadata too short: {} (expected >= 34)", metadata.len()));
    }

    let method_val = u8::from_str_radix(&metadata[0..2], 16)
        .map_err(|_| anyhow!("Invalid encryption method: {}", &metadata[0..2]))?;
    let encryption_method = EncryptionMethod::from_u8(method_val)?;
    let master_key_id = metadata[2..34].to_string();

    Ok((
        JedHeader { encryption_method, master_key_id },
        &data[header_end..],
    ))
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
    fn test_jed_header_roundtrip() {
        let header = JedHeader {
            encryption_method: EncryptionMethod::StringV1,
            master_key_id: "0123456789abcdef0123456789abcdef".to_string(),
        };

        let encoded = encode_jed_header(&header);
        assert!(encoded.starts_with("JED01"));

        let (parsed, body) = parse_jed_header(&encoded).unwrap();
        assert_eq!(parsed.master_key_id, "0123456789abcdef0123456789abcdef");
        assert_eq!(parsed.encryption_method, EncryptionMethod::StringV1);
        assert!(body.is_empty());
    }

    #[test]
    fn test_jed_header_matches_joplin_format() {
        // Verify our header matches the format from actual Joplin data:
        // JED01 000022 0a b892c8028cb246c5b124ac5880478be9
        let header = JedHeader {
            encryption_method: EncryptionMethod::StringV1,
            master_key_id: "b892c8028cb246c5b124ac5880478be9".to_string(),
        };
        let encoded = encode_jed_header(&header);
        assert_eq!(&encoded, "JED010000220ab892c8028cb246c5b124ac5880478be9");
    }

    #[test]
    fn test_parse_real_joplin_header() {
        let data = "JED010000220ab892c8028cb246c5b124ac5880478be9000433{\"rest\"}";
        let (header, body) = parse_jed_header(data).unwrap();
        assert_eq!(header.encryption_method, EncryptionMethod::StringV1);
        assert_eq!(header.master_key_id, "b892c8028cb246c5b124ac5880478be9");
        assert!(body.starts_with("000433"));
    }

    #[test]
    fn test_utf16le_roundtrip() {
        let original = "Hello, World! 🌍";
        let encoded = encode_utf16le(original);
        let decoded = decode_utf16le(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_master_key_generation_and_loading() {
        let mut service = E2eeService::new();
        service.set_master_password("test_password".to_string());

        let (key_id, master_key) = service.generate_master_key("test_password").unwrap();
        service.load_master_key(&master_key).unwrap();

        assert!(service.is_enabled());
        assert!(service.get_active_master_key().is_some());
    }

    #[test]
    fn test_string_encryption_decryption_roundtrip() {
        let mut service = E2eeService::new();
        service.set_master_password("test_password".to_string());

        let (key_id, master_key) = service.generate_master_key("test_password").unwrap();
        service.load_master_key(&master_key).unwrap();
        service.set_active_master_key(key_id);

        let original = "Hello, World! This is a test of NeoJoplin E2EE.";
        let encrypted = service.encrypt_string(original).unwrap();
        assert!(encrypted.starts_with("JED01"));

        let decrypted = service.decrypt_string(&encrypted).unwrap();
        assert_eq!(original, decrypted);
    }
}
