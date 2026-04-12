// NeoJoplin Core - Domain types and traits

pub mod domain;
pub mod error;
pub mod traits;
pub mod sync;
pub mod config;

// Re-exports for convenience
pub use domain::*;
pub use error::*;
pub use traits::*;
pub use sync::*;
pub use config::*;

/// Convenience Result type
pub type Result<T> = std::result::Result<T, NeoJoplinError>;

/// File metadata
#[derive(Debug, Clone)]
pub struct FileMeta {
    pub path: String,
    pub size: i64,
    pub modified: i64,
    pub is_dir: bool,
}
