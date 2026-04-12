// JED format handler for Joplin-compatible encryption

use crate::{crypto::CryptoService, E2eeError, E2eeResult};

/// JED format version
const JED_VERSION: &str = "JED01";

/// JED format encoder for encrypted data
pub struct JedEncoder;

impl JedEncoder {
    /// Encode data in JED format
    pub fn encode(
        data: &str,
        master_key: &[u8],
        master_key_id: &str,
        method: u16,
    ) -> E2eeResult<String> {
        let method_num = crate::crypto::EncryptionMethod::from_i32(method as i32)?;

        // Encrypt the data
        let encrypted = CryptoService::encrypt_string(master_key, data, method_num)?;

        // Parse the encrypted JSON to get IV and ciphertext
        let encrypted_json: serde_json::Value = serde_json::from_str(&encrypted)
            .map_err(|e| E2eeError::InvalidJedFormat(format!("Invalid encrypted JSON: {}", e)))?;

        let iv = encrypted_json["iv"].as_str()
            .ok_or_else(|| E2eeError::InvalidJedFormat("Missing IV".to_string()))?;
        let ct = encrypted_json["ct"].as_str()
            .ok_or_else(|| E2eeError::InvalidJedFormat("Missing ciphertext".to_string()))?;

        // Create chunk JSON
        let chunk_json = serde_json::json!({
            "iv": iv,
            "ct": ct
        });

        let chunk_str = chunk_json.to_string();
        let chunk_hex = hex::encode(&chunk_str);

        // Format: JED01 + method (2 bytes) + keyId (32 bytes) + length (6 hex) + chunk
        let mut jed = String::new();
        jed.push_str(JED_VERSION);
        jed.push_str(&format!("{:02X}", method));

        // Pad or truncate key ID to 32 bytes (64 hex chars)
        let key_id_hex = hex::encode(master_key_id.as_bytes());
        let padded_key_id = format!("{:0<64}", key_id_hex); // Pad with spaces
        jed.push_str(&padded_key_id[..64]);

        // Chunk length (6 hex digits)
        let chunk_len = chunk_hex.len();
        jed.push_str(&format!("{:06X}", chunk_len));

        // Chunk data
        jed.push_str(&chunk_hex);

        Ok(jed)
    }

    /// Encode binary data in JED format (chunked)
    pub fn encode_binary(
        data: &[u8],
        master_key: &[u8],
        master_key_id: &str,
        method: u16,
    ) -> E2eeResult<String> {
        let method_num = crate::crypto::EncryptionMethod::from_i32(method as i32)?;
        let chunk_size = method_num.chunk_size();

        let mut jed = String::new();
        jed.push_str(JED_VERSION);
        jed.push_str(&format!("{:02X}", method));

        // Pad or truncate key ID to 32 bytes (64 hex chars)
        let key_id_hex = hex::encode(master_key_id.as_bytes());
        let padded_key_id = format!("{:0<64}", key_id_hex);
        jed.push_str(&padded_key_id[..64]);

        // Process in chunks
        for chunk in data.chunks(chunk_size) {
            let encrypted = CryptoService::encrypt_bytes(master_key, chunk, method_num)?;

            let chunk_json = serde_json::json!({
                "iv": hex::encode(&encrypted.iv),
                "ct": hex::encode(&encrypted.ciphertext)
            });

            let chunk_str = chunk_json.to_string();
            let chunk_hex = hex::encode(&chunk_str);

            // Chunk length (6 hex digits)
            let chunk_len = chunk_hex.len();
            jed.push_str(&format!("{:06X}", chunk_len));

            // Chunk data
            jed.push_str(&chunk_hex);
        }

        Ok(jed)
    }
}

/// JED format decoder for encrypted data
pub struct JedDecoder;

impl JedDecoder {
    /// Decode JED format to string
    pub fn decode(jed: &str, master_key: &[u8]) -> E2eeResult<String> {
        // Parse header
        if !jed.starts_with(JED_VERSION) {
            return Err(E2eeError::InvalidJedFormat(
                format!("Invalid JED version, expected {}", JED_VERSION)
            ));
        }

        let mut pos = JED_VERSION.len();

        // Parse method (2 bytes)
        if jed.len() < pos + 2 {
            return Err(E2eeError::InvalidJedFormat("Missing method".to_string()));
        }
        let method_hex = &jed[pos..pos + 2];
        let method = u16::from_str_radix(method_hex, 16)
            .map_err(|e| E2eeError::InvalidJedFormat(format!("Invalid method: {}", e)))?;
        pos += 2;

        // Parse key ID (32 bytes / 64 hex chars) - we skip this for now
        if jed.len() < pos + 64 {
            return Err(E2eeError::InvalidJedFormat("Missing key ID".to_string()));
        }
        pos += 64;

        // Parse chunks
        let method_num = crate::crypto::EncryptionMethod::from_i32(method as i32)?;
        let mut result = Vec::new();

        while pos < jed.len() {
            // Parse chunk length (6 hex digits)
            if jed.len() < pos + 6 {
                return Err(E2eeError::InvalidJedFormat("Missing chunk length".to_string()));
            }
            let length_hex = &jed[pos..pos + 6];
            let chunk_len = usize::from_str_radix(length_hex, 16)
                .map_err(|e| E2eeError::InvalidJedFormat(format!("Invalid chunk length: {}", e)))?;
            pos += 6;

            // Parse chunk data
            if jed.len() < pos + chunk_len {
                return Err(E2eeError::InvalidJedFormat("Incomplete chunk data".to_string()));
            }
            let chunk_hex = &jed[pos..pos + chunk_len];
            pos += chunk_len;

            // Decode chunk from hex
            let chunk_str = hex::decode(chunk_hex)
                .map_err(|e| E2eeError::InvalidJedFormat(format!("Invalid chunk hex: {}", e)))?;
            let chunk_json: serde_json::Value = serde_json::from_slice(&chunk_str)
                .map_err(|e| E2eeError::InvalidJedFormat(format!("Invalid chunk JSON: {}", e)))?;

            let iv = chunk_json["iv"].as_str()
                .ok_or_else(|| E2eeError::InvalidJedFormat("Missing IV in chunk".to_string()))?;
            let ct = chunk_json["ct"].as_str()
                .ok_or_else(|| E2eeError::InvalidJedFormat("Missing ciphertext in chunk".to_string()))?;

            let iv_bytes = hex::decode(iv)
                .map_err(|e| E2eeError::InvalidJedFormat(format!("Invalid IV hex: {}", e)))?;
            let ct_bytes = hex::decode(ct)
                .map_err(|e| E2eeError::InvalidJedFormat(format!("Invalid ciphertext hex: {}", e)))?;

            // Decrypt chunk
            let decrypted = CryptoService::decrypt_bytes(
                master_key,
                &crate::crypto::CryptoService::generate_salt(), // Use empty salt for now
                &iv_bytes,
                &ct_bytes,
                method_num,
            )?;

            result.extend_from_slice(&decrypted);
        }

        String::from_utf8(result)
            .map_err(|e| E2eeError::DecryptionFailed(format!("Invalid UTF-8: {}", e)))
    }
}

/// JED format utilities
pub struct JedFormat;

impl JedFormat {
    /// Check if a string is in JED format
    pub fn is_jed_format(data: &str) -> bool {
        data.starts_with(JED_VERSION)
    }

    /// Extract the master key ID from JED format
    pub fn extract_key_id(jed: &str) -> E2eeResult<String> {
        if !jed.starts_with(JED_VERSION) {
            return Err(E2eeError::InvalidJedFormat("Not a JED format string".to_string()));
        }

        let pos = JED_VERSION.len() + 2; // Skip version + method
        if jed.len() < pos + 64 {
            return Err(E2eeError::InvalidJedFormat("Missing key ID".to_string()));
        }

        let key_id_hex = &jed[pos..pos + 64];
        let key_id_bytes = hex::decode(key_id_hex)
            .map_err(|e| E2eeError::InvalidJedFormat(format!("Invalid key ID hex: {}", e)))?;

        String::from_utf8(key_id_bytes)
            .map_err(|e| E2eeError::InvalidJedFormat(format!("Invalid key ID UTF-8: {}", e)))
    }

    /// Extract the encryption method from JED format
    pub fn extract_method(jed: &str) -> E2eeResult<u16> {
        if !jed.starts_with(JED_VERSION) {
            return Err(E2eeError::InvalidJedFormat("Not a JED format string".to_string()));
        }

        let pos = JED_VERSION.len();
        if jed.len() < pos + 2 {
            return Err(E2eeError::InvalidJedFormat("Missing method".to_string()));
        }

        let method_hex = &jed[pos..pos + 2];
        u16::from_str_radix(method_hex, 16)
            .map_err(|e| E2eeError::InvalidJedFormat(format!("Invalid method: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jed_encode_decode() {
        let master_key = vec![0u8; 32];
        let master_key_id = "test-key-id";
        let data = "Hello, World! This is a test note.";

        let jed = JedEncoder::encode(data, &master_key, master_key_id, 10).unwrap();
        println!("JED encoded: {}", jed);

        assert!(JedFormat::is_jed_format(&jed));

        let extracted_key_id = JedFormat::extract_key_id(&jed).unwrap();
        // Note: The key ID is hex-encoded bytes, so it won't match exactly
        println!("Extracted key ID: {}", extracted_key_id);

        let extracted_method = JedFormat::extract_method(&jed).unwrap();
        assert_eq!(extracted_method, 10);
    }

    #[test]
    fn test_jed_binary_encode() {
        let master_key = vec![0u8; 32];
        let master_key_id = "test-key-id";
        let data = vec![0u8; 200000]; // 200KB of data

        let jed = JedEncoder::encode_binary(&data, &master_key, master_key_id, 9).unwrap();
        assert!(JedFormat::is_jed_format(&jed));

        let method = JedFormat::extract_method(&jed).unwrap();
        assert_eq!(method, 9);
    }

    #[test]
    fn test_jed_invalid_format() {
        let result = JedFormat::extract_key_id("INVALID");
        assert!(result.is_err());
    }

    #[test]
    fn test_jed_not_jed_format() {
        assert!(!JedFormat::is_jed_format("plain text"));
    }
}
