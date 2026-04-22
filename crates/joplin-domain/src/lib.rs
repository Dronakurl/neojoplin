// joplin-domain - Pure Joplin domain types and traits
//
// This crate contains domain models, traits, and types that are
// directly derived from the Joplin specification and database schema v41.
// It is designed to be reusable across different Joplin-compatible applications.

pub mod domain;
pub mod error;
pub mod sync;
pub mod traits;

// Re-exports for convenience
pub use domain::*;
pub use error::*;
pub use sync::*;
pub use traits::*;

/// Convenience Result type for domain operations
pub type Result<T> = std::result::Result<T, DomainError>;

/// File metadata for sync operations
#[derive(Debug, Clone)]
pub struct FileMeta {
    pub path: String,
    pub size: i64,
    pub modified: i64,
    pub is_dir: bool,
}
