// Core error types for Joplin operations

use thiserror::Error;
use serde::{Serialize, Deserialize};

/// Core domain error type for Joplin operations
#[derive(Error, Debug)]
pub enum DomainError {
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("Sync error: {0}")]
    Sync(#[from] SyncError),

    #[error("WebDAV error: {0}")]
    WebDav(#[from] WebDavError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("E2EE error: {0}")]
    E2EE(#[from] E2eeError),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

/// E2EE-related errors
#[derive(Error, Debug)]
pub enum E2eeError {
    #[error("Master key not found: {0}")]
    MasterKeyNotFound(String),

    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Invalid JED format: {0}")]
    InvalidJedFormat(String),

    #[error("Invalid master password")]
    InvalidMasterPassword,

    #[error("Key derivation failed: {0}")]
    KeyDerivationFailed(String),

    #[error("Master key generation failed: {0}")]
    KeyGenerationFailed(String),

    #[error("Invalid encryption method: {0}")]
    InvalidEncryptionMethod(String),

    #[error("Encryption not enabled")]
    EncryptionNotEnabled,

    #[error("Crypto error: {0}")]
    CryptoError(String),

    #[error("Invalid master key format: {0}")]
    InvalidMasterKeyFormat(String),

    #[error("Master key already exists: {0}")]
    MasterKeyAlreadyExists(String),

    #[error("No active master key")]
    NoActiveMasterKey,

    #[error("Crypto operation failed: {0}")]
    CryptoOperationFailed(String),
}

/// Database-related errors
#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Database connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Query failed: {0}")]
    QueryFailed(String),

    #[error("Migration failed: {0}")]
    MigrationFailed(String),

    #[error("Item not found: {0}")]
    NotFound(String),

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Database is locked")]
    DatabaseLocked,

    #[error("Schema version mismatch: expected {expected}, found {found}")]
    SchemaMismatch { expected: i32, found: i32 },
}

/// Sync-related errors
#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Network error: {0}")]
    Network(#[from] NetworkError),

    #[error("Authentication error: {0}")]
    Auth(#[from] AuthError),

    #[error("Conflict detected: {0}")]
    Conflict(String),

    #[error("Server error: {0}")]
    Server(String),

    #[error("Local error: {0}")]
    Local(#[from] DatabaseError),

    #[error("Lock acquisition failed: {0}")]
    LockFailed(String),

    #[error("Sync phase failed: {phase} - {reason}")]
    PhaseFailed { phase: SyncPhase, reason: String },

    #[error("Too many retries")]
    TooManyRetries,

    #[error("Sync cancelled")]
    Cancelled,

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Sync phases
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncPhase {
    Upload,
    DeleteRemote,
    Delta,
}

impl std::fmt::Display for SyncPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncPhase::Upload => write!(f, "Upload"),
            SyncPhase::DeleteRemote => write!(f, "DeleteRemote"),
            SyncPhase::Delta => write!(f, "Delta"),
        }
    }
}

/// Network-related errors
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Connection timeout")]
    Timeout,

    #[error("Connection refused")]
    ConnectionRefused,

    #[error("DNS resolution failed: {0}")]
    DnsFailed(String),

    #[error("Network unreachable")]
    Unreachable,

    #[error("Connection reset by peer")]
    ConnectionReset,

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("HTTP error: {0} {1}")]
    Http(u16, String),

    #[error("Request timeout")]
    RequestTimeout,
}

impl NetworkError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            NetworkError::Timeout
                | NetworkError::ConnectionRefused
                | NetworkError::Unreachable
                | NetworkError::ConnectionReset
                | NetworkError::RequestTimeout
        )
    }
}

/// Authentication-related errors
#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Authentication method not supported")]
    MethodNotSupported,

    #[error("Token expired")]
    TokenExpired,

    #[error("Authentication failed: {0}")]
    Failed(String),
}

/// WebDAV-specific errors
#[derive(Error, Debug)]
pub enum WebDavError {
    #[error("Network error: {0}")]
    Network(#[from] NetworkError),

    #[error("Authentication error: {0}")]
    Auth(#[from] AuthError),

    #[error("Server error: {0}")]
    Server(String),

    #[error("Path not found: {0}")]
    NotFound(String),

    #[error("Path already exists: {0}")]
    AlreadyExists(String),

    #[error("Lock error: {0}")]
    Lock(String),

    #[error("Unsupported WebDAV feature: {0}")]
    Unsupported(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("XML parsing error: {0}")]
    XmlParsingError(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Timeout")]
    Timeout,

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

impl SyncError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            SyncError::Network(err) => err.is_retryable(),
            SyncError::Server(_) => true, // Server errors might be transient
            SyncError::LockFailed(_) => true, // Lock failures might be transient
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_error_retryable() {
        assert!(NetworkError::Timeout.is_retryable());
        assert!(NetworkError::ConnectionRefused.is_retryable());
        assert!(!NetworkError::Http(404, "Not Found".to_string()).is_retryable());
    }

    #[test]
    fn test_sync_error_retryable() {
        let sync_err = SyncError::Network(NetworkError::Timeout);
        assert!(sync_err.is_retryable());

        let sync_err = SyncError::Auth(AuthError::InvalidCredentials);
        assert!(!sync_err.is_retryable());
    }

    #[test]
    fn test_sync_phase_display() {
        assert_eq!(SyncPhase::Upload.to_string(), "Upload");
        assert_eq!(SyncPhase::DeleteRemote.to_string(), "DeleteRemote");
        assert_eq!(SyncPhase::Delta.to_string(), "Delta");
    }
}
