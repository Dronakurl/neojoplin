# NeoJoplin

A native Rust terminal client for [Joplin](https://joplinapp.org/) note-taking with sync compatibility.

## Features

- ✅ **Sync Compatible** - Bidirectional sync with Joplin Desktop/CLI
- ✅ **CLI Interface** - Fast, scriptable command-line interface
- ✅ **TUI** - Interactive terminal user interface with vim-style navigation
- ✅ **WebDAV Sync** - Three-phase sync With any WebDAV server (GMX, Nextcloud, etc.)
- ❌ **Other sync targets** - Filesystem, OneDrive, Dropbox, Amazon S3, WebDAV, Joplin Server 
- ✅ **External Editor** - Edit notes with your favorite editor (helix, nvim, etc.)
- ✅ **Emoji Support** - Folder icons and UI elements

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/Dronakurl/neojoplin.git
cd neojoplin

cargo install --path crates/cli --force
```

### Prerequisites

- Rust toolchain (1.70+)
- SQLite
- WebDAV server 

> [!TIP]
> There is a configuration for a local WebDAV server in the `docker` folder, which you can use for testing. It is based on `caddy`

## Quick Start

NeoJoplin provides a binary that works as both a CLI tool and launches the TUI by default.

### TUI (Default)

```bash
# Launch the TUI interface (default when no commands specified)
neojoplin
# or explicitly
neojoplin --tui

# Keybindings:
# q     - Quit
# ?     - Help
# Tab   - Switch panels (notebooks | notes | content)
# j/k   - Navigate (vim-style)
# Enter - Edit selected note
# n     - New note
# N     - New folder
# d     - Delete selected
# s     - Sync
# P     - Open AI chat overlay (Jarvis plugin)
```

### CLI Commands

```bash
# Initialize the database
neojoplin init

# Create a folder and note
neojoplin mkbook "Development"
neojoplin mknote "Rust Tips" --body "Use cargo for everything!"

# List notes
neojoplin ls

# Edit a note (uses $EDITOR or configured editor)
neojoplin edit "Rust Tips"

# Sync with WebDAV
neojoplin sync --url https://webdav.example.com --username user --password pass

# Plugin management + AI chat
neojoplin plugin list
neojoplin plugin enable jarvis
neojoplin ask "What notes mention Pflege?"
neojoplin plugin chat "Summarize my latest todo notes"
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

### AI Provider Configuration

NeoJoplin AI chat supports both Ollama and OpenAI-compatible APIs.

```bash
# Provider selection: ollama (default) or openai
export NEOJOPLIN_AI_PROVIDER=ollama

# Ollama defaults
export OLLAMA_BASE_URL=http://127.0.0.1:11434
export OLLAMA_MODEL=llama3.2

# OpenAI-compatible
export NEOJOPLIN_AI_PROVIDER=openai
export OPENAI_API_KEY=...
export OPENAI_BASE_URL=https://api.openai.com
export OPENAI_MODEL=gpt-4.1-mini
```

The CLI also reads `.env` in the current directory and `~/.env`.

## Commands

### Note Management

- `neojoplin ls [pattern]` - List notes in current notebook
- `neojoplin cat <note>` - Display note content
- `neojoplin mknote <title> [body]` - Create a new note
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

You can use the recipes defined in the [justfile](justfile).
To use them, you need the [just command runner](https://github.com/casey/just).

### Build

```bash
just build
```

### Run Tests

```bash
just test
```

### Install

```bash
just install
```

### Code Quality

```bash
just check    # clippy
just fmt      # format
```

### Available Features ✅

- **CLI**: `init`, `mknote`, `mkbook`, `mktodo`, `ls`, `cat`, `edit`, `sync`, `import`, `import-desktop`, `rm-note`, `rm-book`
- **TUI**: Three-panel layout, vim navigation, interactive editing
- **Sync**: Three-phase protocol, configurable remote path, WebDAV support
- **Editor**: External editor integration with terminal handling
- **Database**: SQLite with FTS5 search, exact Joplin schema

## Contributing

Contributions are welcome! 

## License

MIT License - see [LICENSE](LICENSE) for details.
