# NeoJoplin - Rust Terminal Joplin Client

## Project Overview

NeoJoplin is a native Rust terminal client for the Joplin note-taking application with 100% sync compatibility to the reference TypeScript CLI implementation.

## Key Facts

- **Location**: `/home/konrad/gallery/neojoplin`
- **GitHub**: https://github.com/Dronakurl/neojoplin
- **Reference Implementation**: `/home/konrad/gallery/kjoplin/joplin` (TypeScript CLI)
- **KDE Reference**: `/home/konrad/gallery/kjoplin` (C++/Qt implementation)

## Core Requirements

1. **100% Sync Compatibility**: Must sync bidirectionally with reference Joplin CLI
2. **WebDAV Sync**: Uses GMX WebDAV server (credentials from `~/.config/rclone/rclone.conf`)
3. **SQLite Schema**: Must match Joplin schema v41 exactly
4. **Editor Integration**: External editor support (helix/Neovim) with embedded display
5. **CLI-First**: Command-line interface first, TUI later (ratatui)

## Architecture

### Technology Stack

- **Runtime**: Tokio for async orchestration
- **CLI**: Clap for argument parsing
- **TUI**: Ratatui for terminal UI (Phase 4)
- **Database**: SQLx with SQLite
- **HTTP/WebDAV**: Reqwest + async-webdav
- **Editor**: nvim-rs for embedded Neovim (Phase 3)
- **Serialization**: Serde + serde_json

### Project Structure

```
neojoplin/
├── src/
│   ├── main.rs              # Entry point
│   ├── cli/                 # CLI framework
│   ├── commands/            # Command implementations
│   ├── core/                # Core functionality (database, sync, webdav)
│   └── utils/               # Utilities (editor, emoji, progress)
├── tests/                   # Integration and unit tests
├── docs/                    # Documentation
├── Cargo.toml              # Dependencies
├── justfile                # Build automation
└── CLAUDE.md               # This file
```

## Development Guidelines

### Build Commands

Always use `just` for build operations:
- `just build` - Build release binary
- `just test` - Run tests
- `just install` - Install to ~/.local/bin
- `just run` - Build and run
- `just dev` - Hot reload development

### Code Style

- Use `cargo fmt` for formatting
- Use `cargo clippy` for linting
- Follow Rust naming conventions
- Add doc comments to public APIs

### Database

- Location: `~/.local/share/neojoplin/joplin.db`
- Schema: Must match Joplin v41 exactly
- Reference: `/home/konrad/gallery/kjoplin/docs/database.md`

### Sync Protocol

- Three-phase: UPLOAD → DELETE_REMOTE → DELTA
- WebDAV endpoint: GMX server
- Lock file: `/neojoplin/lock.json`
- Reference: `/home/konrad/gallery/kjoplin/joplin/packages/lib/Synchronizer.ts`

### Editor Integration

- Phase 1: Spawn separate terminal session (simpler)
- Phase 2: Embedded with nvim-rs (complex, better UX)

## Critical Reference Files

1. `/home/konrad/gallery/kjoplin/docs/database.md` - SQLite schema
2. `/home/konrad/gallery/kjoplin/joplin/packages/lib/Synchronizer.ts` - Sync protocol
3. `/home/konrad/gallery/kjoplin/src/core/syncmanager.cpp` - WebDAV implementation
4. `/home/konrad/gallery/kjoplin/joplin/packages/app-cli/app/command-edit.ts` - Editor integration
5. `/home/konrad/gallery/kjoplin/docs/E2EE.md` - JED format specification

## Configuration

- WebDAV credentials: `~/.config/rclone/rclone.conf` (gmx section)
- App config: `~/.config/neojoplin/config.json`
- Data directory: `~/.local/share/neojoplin/`

## Implementation Phases

1. **Phase 1**: Project foundation + Database layer
2. **Phase 2**: WebDAV sync engine
3. **Phase 3**: Core commands (ls, cat, edit, mknote, etc.)
4. **Phase 4**: Sync compatibility testing
5. **Phase 5**: Polish and documentation
6. **Phase 6**: TUI mode (ratatui)

## Notes

- Joplin is installed on this system and can be used for testing
- Sync target is `neojoplin` folder on GMX WebDAV server
- Prioritize core sync compatibility over feature completeness
- Start with simple external editor launch, add embedded support later
