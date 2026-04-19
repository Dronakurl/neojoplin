# NeoJoplin Architecture Summary

## Overview

NeoJoplin is a Rust-based terminal client for Joplin, a note-taking application. The architecture is designed for modularity, allowing individual components to be reused in other projects while maintaining 100% compatibility with Joplin's sync protocol and E2EE implementation.

## Design Principles

- **Domain-Driven Design**: Clear separation between business logic and infrastructure
- **Trait-Based Abstraction**: Interfaces over implementations for testability and flexibility
- **Zero-Cost Abstractions**: Rust's ownership system enables memory safety without performance overhead
- **Joplin Compatibility**: Primary focus on protocol-level compatibility with the reference implementation
- **Independent Components**: Each crate can be used standalone in other projects

## Crate Architecture

```
neojoplin/
├── Cargo.toml                    # Workspace metadata
├── crates/
│   ├── joplin-domain/           # Core domain traits and models
│   ├── joplin-sync/             # Sync engine and E2EE implementation
│   ├── neojoplin-core/          # Application configuration
│   ├── neojoplin-storage/       # SQLite database implementation
│   ├── neojoplin-cli/           # Command-line interface
│   ├── neojoplin-tui/           # Terminal user interface
│   └── neojoplin-test-utils/    # Testing utilities
```

## Core Crates

### 1. `joplin-domain` - Foundation Layer

**Purpose**: Defines core data models and trait interfaces

**Key Exports**:
- Domain models: `Note`, `Folder`, `Tag`, `Resource`, `NoteTag`
- Trait interfaces: `Storage`, `WebDavClient`
- Error types: `SyncError`, `WebDavError`, `SyncPhase`, `SyncEvent`
- Utilities: UUID generation, timestamp functions

**Independent Usage**: Can be used as a dependency for other Joplin-compatible tools

**Dependencies**: Minimal (only `uuid`, `chrono`, `serde`)

### 2. `joplin-sync` - Sync & E2EE Engine

**Purpose**: Complete sync protocol and encryption implementation

**Key Components**:
- **Three-Phase Sync**: UPLOAD → DELETE_REMOTE → DELTA protocol
- **WebDAV Client**: HTTP client with Basic Auth and PROPFIND support
- **E2EE Implementation**: AES-256-GCM encryption with JED format
- **Master Key Management**: PBKDF2 key derivation and secure key storage
- **Sync Info**: Joplin-compatible sync.json handling

**Independent Usage**: Can be used as a library for implementing Joplin sync in other applications

**Key Technologies**:
- `reqwest` for HTTP operations
- `aes-gcm` for AES-256-GCM encryption
- `pbkdf2` for key derivation
- `hex` for encoding
- `serde` for serialization

**Test Coverage**: 19 comprehensive unit tests covering all major components

### 3. `neojoplin-core` - Application Configuration

**Purpose**: Cross-platform configuration and data directory management

**Key Features**:
- Platform-specific data directories (XDG on Linux, AppData on Windows, etc.)
- Configuration file management
- Editor integration

**Independent Usage**: Useful for any terminal application needing cross-platform config

### 4. `neojoplin-storage` - Database Layer

**Purpose**: SQLite implementation with Joplin v41 schema compatibility

**Key Features**:
- Full Joplin database schema (notes, folders, tags, resources, etc.)
- Full-text search (FTS5)
- Async operations with SQLx
- Migration support for schema updates

**Independent Usage**: Can be used as a standalone Joplin database library

**Technologies**: `sqlx` for async database operations, `sqlite3` as embedded database

### 5. `neojoplin-cli` - Command-Line Interface

**Purpose**: User-facing CLI application

**Key Features**:
- Note and folder management
- Sync operations
- E2EE management
- External editor integration

**Technologies**: `clap` for argument parsing, `dialoguer` for interactive prompts

### 6. `neojoplin-tui` - Terminal User Interface

**Purpose**: Interactive terminal interface with keyboard-driven navigation

**Key Features**:
- Ratatui-based UI
- Note browsing and editing
- Sync progress indicators
- E2EE status display
- Modal dialogs for configuration

**Technologies**: `ratatui` for TUI framework, `crossterm` for terminal operations

## Technology Choices

### Core Technologies

1. **Async Runtime**: `tokio` - Industry standard for async Rust
2. **HTTP Client**: `reqwest` - Mature, well-maintained HTTP library
3. **Database**: `sqlx` + `sqlite3` - Compile-time checked SQL queries
4. **Serialization**: `serde` - De facto standard for Rust serialization
5. **CLI**: `clap` - Modern argument parsing with derive macros
6. **TUI**: `ratatui` - Active fork of tui-rs with better maintenance

### Cryptography

1. **Encryption**: `aes-gcm` - AES-256-GCM authenticated encryption
2. **Key Derivation**: `pbkdf2` - Password-based key derivation with 100,000 iterations
3. **Encoding**: `hex` - Hex encoding for encrypted data
4. **Random**: `getrandom` - Secure random number generation

### Potential Architecture Questions for Review

1. **WebDAV Client**: Currently implements simple XML parsing. Should this use a dedicated XML parser library for better robustness?

2. **Error Handling**: Uses `anyhow` for error propagation. Should domain-specific error types be more granular?

3. **Dependency Management**: Some crates have overlapping dependencies. Could there be better consolidation?

4. **Testing**: Good unit test coverage, but limited integration tests. Should more integration testing be added?

5. **Performance**: SQLite operations are async, but sync engine could benefit from parallel processing. Is this a priority?

6. **Security**: E2EE implementation uses PBKDF2 with 100,000 iterations. Should this be configurable or use Argon2?

7. **Modularity**: Some functionality in `joplin-sync` could be split into separate crates (crypto, webdav, sync). Would this improve reusability?

8. **Compatibility**: Heavy focus on Joplin compatibility. Should there be a migration path for users wanting to move away from Joplin-specific formats?

## Component Reusability

Each crate is designed for standalone usage:

```rust
// Example: Using joplin-sync in another project
use joplin_sync::{SyncEngine, ReqwestWebDavClient, WebDavConfig, E2eeService};

let config = WebDavConfig::new(url, username, password);
let webdav = ReqwestWebDavClient::new(config)?;
let e2ee = E2eeService::new();
let sync_engine = SyncEngine::new(storage, webdav, event_tx)
    .with_e2ee(e2ee)
    .with_remote_path("/myapp".to_string());
sync_engine.sync().await?;
```

## Future Considerations

1. **Plugin System**: Could the architecture support user extensions?
2. **Alternative Backends**: Could storage layer support PostgreSQL or cloud databases?
3. **Sync Targets**: Could sync engine support protocols beyond WebDAV?
4. **Mobile**: Could this architecture work on mobile platforms (iOS/Android)?
5. **Web Assembly**: Could components be compiled to WASM for web usage?

## Conclusion

NeoJoplin's architecture prioritizes correctness, compatibility, and modularity. The use of trait-based abstractions and clear separation of concerns makes components reusable while maintaining Joplin compatibility. The technology choices favor mature, well-maintained libraries over bleeding-edge solutions for production reliability.

**Key Strength**: Each crate can be independently used in other projects, making this a valuable set of libraries for anyone building Joplin-compatible tools or similar note-taking applications.