# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

**Always use `just` for build operations:**
- `just build` - Build release binary
- `just test` - Run all tests
- `just check` - Run clippy with warnings as errors
- `just fmt` - Format code with rustfmt
- `just run` - Build and run with arguments
- `just install` - Install to ~/.local/bin
- `just clean` - Clean build artifacts

**Running specific tests:**
- `cargo test --all` - Run all tests
- `cargo test database::` - Run only database tests
- `cargo test -- --nocapture` - Show test output

**Database management:**
- Database location: `~/.local/share/neojoplin/joplin.db`
- To reset database: `rm ~/.local/share/neojoplin/joplin.db && cargo run -- init`
- To inspect database: `sqlite3 ~/.local/share/neojoplin/joplin.db`

## Architecture Overview

NeoJoplin is a **Joplin-compatible terminal client** that must maintain **100% sync protocol compatibility** with the reference TypeScript implementation. This is the critical constraint that drives the architecture.

### Core Challenge

The entire application is built around **exact replication of Joplin's sync protocol and database schema**. Any deviation will break compatibility with existing Joplin installations.

### Key Architectural Layers

1. **Database Layer** (`src/core/`) - Must match Joplin schema v41 exactly
   - **Models**: Complete Joplin data structures (Note, Folder, Tag, Resource, etc.)
   - **Database**: SQLite with WAL mode, FTS5 full-text search
   - **Compatibility**: Every field type and table structure must match the reference implementation

2. **CLI Layer** (`src/main.rs`) - Simple command dispatch
   - Uses Clap for argument parsing
   - Direct command execution (no framework overhead)
   - Commands map to database operations

3. **Sync Layer** (planned) - Three-phase protocol implementation
   - UPLOAD → DELETE_REMOTE → DELTA
   - WebDAV client for GMX server
   - Lock handling for multi-client safety

### Critical Reference Implementations

These files **must** be consulted when implementing sync or database features:

**Schema Definition:**
- `/home/konrad/gallery/kjoplin/docs/database.md` - Complete SQLite schema specification
- `/home/konrad/gallery/kjoplin/src/core/syncmanager.cpp` - C++ reference with working sync

**Sync Protocol:**
- `/home/konrad/gallery/kjoplin/joplin/packages/lib/Synchronizer.ts` - Three-phase sync algorithm
- `/home/konrad/gallery/kjoplin/joplin/packages/app-cli/app/command-edit.ts` - External editor pattern

**Encryption Format:**
- `/home/konrad/gallery/kjoplin/docs/E2EE.md` - JED format specification for E2EE

### Data Model Constraints

**Important**: The Joplin database uses `is_todo` (0/1 integer) to distinguish notes from todos, **not** a `type` field. The TypeScript reference uses `type_` at the application layer, but this is **not in the database**.

**Key model relationships:**
- Notes belong to folders via `parent_id` (can be empty for root)
- Tags link to notes via `note_tags` junction table
- Resources attach to notes via `note_resources` junction table
- All entities use UUID v4 for IDs
- Timestamps are **milliseconds since epoch** (not seconds)

### Type Mismatches to Watch

The database schema differs from typical Rust types:
- `order` is `INTEGER` (not `NUMERIC`) despite the schema documentation
- Coordinates (`latitude`, `longitude`, `altitude`) are stored as `INTEGER`
- Use `i64` for these fields in Rust models

### WebDAV Configuration

The application reads WebDAV credentials from rclone config:
- Config location: `~/.config/rclone/rclone.conf`
- Target section: `[gmx]`
- URL: `https://webdav.mc.gmx.net`
- Remote path: `/neojoplin/`

The rclone password is "obscured" and must be decrypted using rclone's algorithm.

### Module Organization

**Currently Implemented:**
- `src/core/database.rs` - SQLite connection, schema creation, CRUD operations
- `src/core/models.rs` - All Joplin data structures with serde support
- `src/main.rs` - CLI entry point with command dispatch

**Planned (Empty Modules):**
- `src/cli/` - CLI framework (currently inline in main.rs)
- `src/commands/` - Command implementations (currently inline in main.rs)
- `src/utils/` - Utilities for editor, emoji, progress reporting

### Testing Strategy

- **Unit tests**: In module files (e.g., `src/core/models.rs`)
- **Integration tests**: `tests/integration/` and `tests/unit/`
- **Sync compatibility**: Must test bidirectional sync with reference Joplin CLI

Critical test: Create notes in NeoJoplin → sync → verify in Joplin CLI → modify → sync back → verify changes.

### Current Implementation Status

**✅ Phase 1 Complete:**
- Database schema v41 (exact Joplin compatibility)
- Core models (Note, Folder, Tag, Resource, etc.)
- Basic CLI commands (init, mknote, mkbook, ls, cat, list-books)
- SQLite with FTS5 full-text search
- CRUD operations for notes and folders

**🚧 Phase 2-3 In Progress:**
- WebDAV client implementation
- Three-phase sync protocol
- JED format parser/generator
- Lock handling and conflict resolution

### Important Implementation Notes

1. **Never modify the database schema** - it must match Joplin v41 exactly
2. **Always test sync compatibility** when changing database or sync code
3. **Use milliseconds for timestamps** - Joplin uses ms, not seconds
4. **FTS5 limitations** - Virtual tables don't support UPSERT, use DELETE + INSERT
5. **Type coercion** - SQLite stores some numbers as INTEGER despite schema saying NUMERIC

### Configuration Files

- **WebDAV credentials**: `~/.config/rclone/rclone.conf` (gmx section)
- **App config**: `~/.config/neojoplin/config.json` (settings storage)
- **Database**: `~/.local/share/neojoplin/joplin.db`
- **Cache**: `~/.local/share/neojoplin/` (resources, temp files)

### Future Technology Choices

**Phase 4+ (not yet implemented):**
- **TUI**: Ratatui for interactive terminal UI
- **Editor**: nvim-rs for embedded Neovim (not helix as initially planned)
- **Sync**: Custom WebDAV client using reqwest (async-webdav crate was insufficient)
