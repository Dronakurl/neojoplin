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
- To reset database: `rm ~/.local/share/neojoplin/joplin.db && neojoplin init`
- To inspect database: `sqlite3 ~/.local/share/neojoplin/joplin.db`

**Note:** Always use `~/.local/bin/neojoplin` for testing after `just install`, as the system PATH may find older versions in `~/.cargo/bin/`.

## Architecture Overview

NeoJoplin is a **Joplin-compatible terminal client** that must maintain **100% sync protocol compatibility** with the reference TypeScript implementation. This is the critical constraint that drives the architecture.

The joplin terminal application is also installed on this system. The code of the joplin application can be found under ~/gallery/kjoplin/joplin
 
### Core Challenge

The entire application is built around **exact replication of Joplin's sync protocol and database schema**. Any deviation will break compatibility with existing Joplin installations. I want that the user can use the joplin command line along side the neojoplin application, sharing the same database and syncing to it. 

### Key Architectural Layers I want that the user can use the joplin command line along side the neojoplin application, sharing the same database and syncing to it. 

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

**Joplin Reference Implementation:**
- **Location:** `/home/konrad/gallery/kjoplin/joplin/` - Contains the complete Joplin application source code
- **CLI Tool:** The `joplin` command-line application is already installed on this system
- **Compatibility Goal:** NeoJoplin is designed to be 100% compatible with Joplin, allowing both applications to use the same database simultaneously
- **Testing:** Use the Joplin CLI to test compatibility after implementing sync features
- **E2EE Syncing:** Both applications should be able to sync with the same WebDAV target using end-to-end encryption

**TUI Emoji Display:**
- **Folder Icons:** The Joplin database stores emoji configurations for each notebook in the `folders.icon` column
- **Format:** JSON string like `{"emoji":"📝"}` 
- **TUI Implementation:** NeoJoplin TUI extracts and displays these emojis in the notebooks panel
- **Fallback:** Uses default "📁" emoji if no icon is configured or JSON parsing fails
- **Location:** `crates/tui/src/ui.rs::extract_folder_emoji()` handles emoji extraction

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

**Local WebDAV Server (Docker-based):**
The project includes a Docker-based local WebDAV server for testing and development.

**Starting the local WebDAV server:**
```bash
# Using docker compose
docker compose up -d

# Or manually
docker start neojoplin-webdav-1
```

**Local WebDAV server details:**
- URL: `http://localhost:8080/webdav/` (note trailing slash)
- No authentication required
- Auto-starts on port 8080
- Persists data in Docker volume
- Supports PROPFIND, GET, PUT, DELETE, MKCOL operations

**Automated E2EE Setup:**
```bash
# Run the setup script to configure both neojoplin and joplin CLI
./setup_local_e2ee.sh
```

This script will:
- Check and start the Docker WebDAV server if needed
- Configure NeoJoplin to use the local WebDAV server
- Configure Joplin CLI to use the same WebDAV target
- Create test data and verify cross-compatibility
- Set up E2EE password from .env file for automated testing

**Using local WebDAV for testing:**
```bash
# Create test data
rm -rf ~/.local/share/neojoplin/joplin.db && ~/.local/bin/neojoplin init
~/.local/bin/neojoplin mk-book "Test Notebook"
~/.local/bin/neojoplin mk-note "Test Note" --body "Content" --parent <folder-id>

# Sync to local WebDAV
~/.local/bin/neojoplin sync --url http://localhost:8080/webdav/ --remote /test-sync

# Enable E2EE in NeoJoplin TUI (press 'S' for settings, then Encryption tab)
# Enable E2EE in Joplin CLI: joplin e2ee:enable

# Test E2EE sync
~/.local/bin/neojoplin sync --url http://localhost:8080/webdav/ --remote /test-sync

# Verify contents
curl -s -X PROPFIND "http://localhost:8080/webdav/test-sync/" -H "Depth: 1"
```

**Sync Path Configuration:**
The sync remote path is configurable via `SyncEngine::with_remote_path()`. Default is `/neojoplin`.
- **Production**: `SyncEngine::new(...).with_remote_path("/neojoplin".to_string())`
- **Testing**: Use unique paths per test to avoid conflicts: `"/test-sync-123"`
- **Local WebDAV**: `--remote /test-sync` (for local testing)
- **Compatibility**: Joplin can sync to any path, not just root level

**Production credentials from rclone:**
- Config location: `~/.config/rclone/rclone.conf`
- Target section: `[gmx]`
- URL: `https://webdav.mc.gmx.net`
- Remote path: `/neojoplin/`

The rclone password is "obscured". The real password is: `MUsWu2kVB9tgxGM`
**Note:** Never commit the password to git. Use .env file for local development only.

### Module Organization

**Current Crate Structure:**
- `crates/core/` - Core domain models and database interfaces
- `crates/storage/` - SQLite database implementation with Joplin v41 schema
- `crates/sync/` - WebDAV sync engine with three-phase protocol
- `crates/e2ee/` - End-to-end encryption implementation (JED format)
- `crates/cli/` - Command-line interface using Clap
- `crates/tui/` - Terminal user interface using Ratatui
- `crates/test-utils/` - Testing utilities and fake implementations

**CLI Architecture:**
- Uses direct command dispatch (no framework overhead)
- Commands map to database operations
- TUI and CLI modes available
- All core functionality implemented

### Testing Strategy

**CRITICAL: Test sync compatibility after every sync-related change**

**Quick test with local Docker WebDAV:**
```bash
# Ensure local WebDAV server is running
docker compose up -d

# Test basic functionality
rm -rf ~/.local/share/neojoplin/joplin.db && ~/.local/bin/neojoplin init
~/.local/bin/neojoplin mk-book "Test Notebook"
~/.local/bin/neojoplin mk-note "Test Note" --body "Content" --parent <folder-id>

# Test sync
~/.local/bin/neojoplin sync --url http://localhost:8080/webdav --remote /test-sync

# Verify database integrity
sqlite3 ~/.local/share/neojoplin/joplin.db "SELECT 'Folders:' as type, COUNT(*) FROM folders UNION ALL SELECT 'Notes:', COUNT(*) FROM notes;"
```

**Cross-client compatibility test:**
```bash
# 1. Create data in NeoJoplin and sync to WebDAV
~/.local/bin/neojoplin sync --url http://localhost:8080/webdav --remote /compat-test

# 2. Configure Joplin CLI to use same WebDAV target
joplin config sync.target 6
joplin config sync.6.path http://localhost:8080/webdav/compat-test
joplin config sync.6.username ""
joplin config sync.6.password ""

# 3. Sync Joplin CLI and verify it can read NeoJoplin data
joplin sync
joplin ls

# 4. Create data in Joplin and sync back to NeoJoplin
joplin mkbook "Joplin Notebook"
joplin mknote "From Joplin" "Content"
joplin sync

# 5. Sync NeoJoplin and verify it can read Joplin data
~/.local/bin/neojoplin sync --url http://localhost:8080/webdav --remote /compat-test
~/.local/bin/neojoplin ls
```

**Requirements:**
- Docker for local WebDAV server
- Joplin CLI installed (for compatibility testing)
- Port 8080 available

**Unit tests:** In module files throughout the codebase
**Integration tests:** `tests/integration/`

### Current Implementation Status

**✅ Phase 1 Complete:**
- Database schema v41 (exact Joplin compatibility)
- Core models (Note, Folder, Tag, Resource, etc.)
- Basic CLI commands (init, mk-note, mk-book, ls, cat, list-books)
- SQLite with FTS5 full-text search
- CRUD operations for notes and folders

**✅ Phase 2-3 Complete:**
- WebDAV client implementation
- Three-phase sync protocol (UPLOAD → DELETE_REMOTE → DELTA)
- Multi-type item sync (notes, folders, tags, resources)
- Lock handling and conflict resolution
- Full Joplin CLI compatibility achieved

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

### Recent Major Fixes

**Multi-Type Item Download Implementation (April 2026):**
- **Issue:** NeoJoplin could upload folders but couldn't download them during sync
- **Root Cause:** Delta phase only scanned `/items/` directory, ignored `/folders/`, `/tags/`, `/resources/`
- **Solution:** Implemented type-safe item handling with `ItemType` enum
- **Result:** Full Joplin CLI compatibility achieved, all item types sync correctly
- **Files Modified:** `crates/sync/src/sync_engine.rs`
- **Testing:** Cross-client sync now works bidirectionally with Joplin CLI

### Future Technology Choices

**✅ Completed:**
- **TUI**: Ratatui for interactive terminal UI
- **Sync**: Custom WebDAV client using reqwest
- **Multi-type sync**: All item types (notes, folders, tags, resources) supported

**🔄 Future Enhancements:**
- **Editor**: nvim-rs for embedded Neovim (not helix as initially planned)
- **Enhanced E2EE**: Full JED format implementation for encrypted sync
- **Performance**: Async optimization for large note collections
