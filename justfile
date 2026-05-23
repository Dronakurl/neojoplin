# NeoJoplin Build Automation

# Import test recipes
import "scripts/just/test.just"

# Default recipe
default:
    @just --list

# Build the project
build:
    cargo build --release

# Build and run (launches TUI by default, or pass CLI commands)
run ARGV="":
    cargo run --release --bin neojoplin -- {{ARGV}}

# Check code with clippy
check:
    cargo clippy -- -D warnings

# Strict project validation (fmt, lint with no warnings, tests)
verify:
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings
    just test

# Format code
fmt:
    cargo fmt

# Clean build artifacts
clean:
    cargo clean

# Run sync command
sync:
    cargo run -- sync

# Show help
help:
    cargo run -- --help

# Reset database (careful!)
reset-db:
    @read -p "Are you sure you want to delete the database? This cannot be undone. [y/N] " -n 1 -r && \
    if [[ $REPLY =~ ^[Yy]$ ]]; then \
        rm -f ~/.local/share/neojoplin/joplin.db && \
        echo "Database deleted. It will be recreated automatically on next TUI start." ; \
    else \
        echo "Database deletion cancelled." ; \
    fi

# Run development binary
dev-run:
    cargo run --

# Show database schema
db-schema:
    @sqlite3 ~/.local/share/neojoplin/joplin.db ".schema"

# List all notes in database
db-list-notes:
    @sqlite3 ~/.local/share/neojoplin/joplin.db "SELECT id, title FROM notes LIMIT 10"

# Launch TUI (same as running with no arguments)
tui:
    cargo run --release --bin neojoplin -- --tui

# Install the system
install:
    cargo build --release
    cargo install --path crates/cli --force
    @echo "Installed neojoplin (CLI + TUI)"

# Start local WebDAV server for testing
webdav-server:
    cd docker && docker compose up -d webdav
    @echo "Local WebDAV server started on http://localhost:8080"
    @echo "WebDAV path: http://localhost:8080/webdav"

# Stop local WebDAV server
webdav-stop:
    cd docker && docker compose down

# View WebDAV server logs
webdav-logs:
    cd docker && docker compose logs -f webdav

# Ollama Docker container management
start-ollama:
    cd docker && docker compose -f docker-compose.ollama.yml up -d

stop-ollama:
    cd docker && docker compose -f docker-compose.ollama.yml down

ollama-logs:
    cd docker && docker compose -f docker-compose.ollama.yml logs -f ollama

ollama-restart:
    cd docker && docker compose -f docker-compose.ollama.yml restart ollama
