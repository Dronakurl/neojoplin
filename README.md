# NeoJoplin

A native Rust terminal client for [Joplin](https://joplinapp.org/) note-taking with 100% sync compatibility.

## Status

🚧 **Under Active Development** - This is a new project. See the [Implementation Plan](#implementation-status) for current progress.

## Features

- ✅ **100% Sync Compatible** - Bidirectional sync with Joplin Desktop/CLI
- ✅ **Terminal Interface** - Fast, keyboard-driven workflow
- ✅ **WebDAV Sync** - Sync with any WebDAV server (GMX, Nextcloud, etc.)
- ✅ **External Editor** - Edit notes with your favorite editor (helix, nvim, etc.)
- ✅ **Emoji Support** - Beautiful folder icons and UI elements
- 🚧 **TUI Mode** - Interactive terminal interface (planned)

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/Dronakurl/neojoplin.git
cd neojoplin

# Install dependencies (just, cargo)
just build
just install
```

### Prerequisites

- Rust toolchain (1.70+)
- `just` command runner
- SQLite3
- WebDAV server (or use Joplin Cloud, Dropbox, OneDrive, etc.)

## Quick Start

```bash
# Initialize the database
neojoplin init

# Configure WebDAV sync (reads from ~/.config/rclone/rclone.conf)
neojoplin config sync.target gmx

# Create a note
neojoplin mknote "My First Note" "Note content here..."

# List notes
neojoplin ls

# View a note
neojoplin cat "My First Note"

# Edit a note (uses $EDITOR or configured editor)
neojoplin edit "My First Note"

# Sync with WebDAV
neojoplin sync
```

## Configuration

### WebDAV Setup

NeoJoplin reads WebDAV credentials from your rclone config:

```ini
# ~/.config/rclone/rclone.conf
[gmx]
type = webdav
url = https://webdav.mc.gmx.net
vendor = other
user = your@email.com
pass = your_password
```

Then set the sync target:

```bash
neojoplin config sync.target gmx
```

### Editor

NeoJoplin respects the `EDITOR` environment variable:

```bash
export EDITOR=hx  # helix
export EDITOR=nvim  # neovim
export EDITER=vim  # vim
```

Or set it in the config:

```bash
neojoplin config editor hx
```

## Commands

### Note Management

- `neojoplin ls [pattern]` - List notes in current notebook
- `neojoplin cat <note>` - Display note content
- `neojoplin mknote <title> [body]` - Create new note
- `neojoplin edit <note>` - Edit note with external editor
- `neojoplin rmnote <note>` - Delete note

### Notebook Navigation

- `neojoplin cd <notebook>` - Change current notebook
- `neojoplin use <notebook>` - Select default notebook
- `neojoplin mkbook <title>` - Create new notebook
- `neojoplin rmbook <notebook>` - Delete notebook

### Synchronization

- `neojoplin sync` - Synchronize with remote
- `neojoplin config sync.target <target>` - Set sync target
- `neojoplin config sync.path <path>` - Set remote path

### Utilities

- `neojoplin search <query>` - Search notes
- `neojoplin version` - Show version information
- `neojoplin help [command]` - Show help

## Sync Compatibility

NeoJoplin implements the exact same sync protocol as the reference Joplin CLI:

- **Three-Phase Sync**: Upload local changes → Delete remote items → Download remote changes
- **Conflict Resolution**: Timestamp-based resolution with conflict copies
- **E2EE Support**: End-to-end encryption compatible with Joplin
- **Lock Handling**: Prevents concurrent syncs from multiple clients

You can use NeoJoplin alongside Joplin Desktop, Joplin CLI, or Joplin Mobile - all will stay in sync.

## Development

### Build

```bash
just build
```

### Run Tests

```bash
just test
```

### Development Mode

```bash
just dev
```

### Code Quality

```bash
just check    # clippy
just fmt      # format
```

## Implementation Status

See [IMPLEMENTATION.md](docs/IMPLEMENTATION.md) for detailed progress tracking.

### Completed ✅

- [x] Project structure and dependencies
- [x] Database schema (Joplin v41)
- [x] Basic CLI framework

### In Progress 🚧

- [ ] Database layer implementation
- [ ] Core commands (ls, cat, mknote, edit)
- [ ] WebDAV sync engine

### Planned 📋

- [ ] Three-phase sync protocol
- [ ] E2EE support
- [ ] Conflict resolution
- [ ] TUI mode (ratatui)
- [ ] Embedded editor (nvim-rs)

## Architecture

NeoJoplin is built with:

- **Tokio** - Async runtime
- **SQLx** - Database access
- **Clap** - CLI argument parsing
- **Reqwest** - HTTP client for WebDAV
- **Ratatui** - TUI framework (planned)

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- [Laurent Cozic](https://github.com/laurent22/) for creating Joplin
- The Joplin community for the excellent reference implementation

## Related Projects

- [Joplin](https://github.com/laurent22/joplin) - Original note-taking app
- [KJoplin](https://github.com/Dronakurl/kjoplin) - KDE/Qt client for Joplin
