# NeoJoplin Build Automation

# Default recipe
default:
    @just --list

# Build the project
build:
    cargo build --release

# Run tests
test:
    cargo test --all

# Build and run
run:
    cargo run --

# Build and install to ~/.local/bin
install: build
    @cargo install --path .

# Development mode with hot reload
dev:
    @echo "Install cargo-watch for this command: cargo install cargo-watch"
    cargo watch -x run

# Check code with clippy
check:
    cargo clippy -- -D warnings

# Format code
fmt:
    cargo fmt

# Clean build artifacts
clean:
    cargo clean

# Initialize database
init-db:
    cargo run -- init-db

# Run with debug logging
debug:
    cargo run -- --log-level debug

# Create a test note
test-note:
    cargo run -- mknote "Test Note" "This is a test note created via justfile"

# Run sync command
sync:
    cargo run -- sync

# Show help
help:
    cargo run -- --help

# Run single test by name
test-one FILTER:
    cargo test -- --exact {{FILTER}}

# Run database tests only
test-db:
    cargo test database::

# Run tests with output
test-verbose:
    cargo test -- --nocapture

# Watch and rebuild on changes
watch:
    @echo "Install cargo-watch for this command: cargo install cargo-watch"
    cargo watch -x build

# Reset database (careful!)
reset-db:
    rm -f ~/.local/share/neojoplin/joplin.db
    @echo "Database deleted. Run 'just run -- init' to recreate."

# Development build (faster than release)
dev-build:
    cargo build

# Run development binary
dev-run:
    cargo run --

# Open database in sqlite3
db-shell:
    sqlite3 ~/.local/share/neojoplin/joplin.db

# Show database schema
db-schema:
    @sqlite3 ~/.local/share/neojoplin/joplin.db ".schema"

# List all notes in database
db-list-notes:
    @sqlite3 ~/.local/share/neojoplin/joplin.db "SELECT id, title FROM notes LIMIT 10"

# Create comprehensive test data
test-data:
    cargo run -- init && \
    cargo run -- mkbook "Development" && \
    cargo run -- mkbook "Personal" && \
    cargo run -- mk-note "Welcome" "Welcome to NeoJoplin!" && \
    echo "Test data created successfully"

# Build TUI binary
build-tui:
    cargo build --release -p neojoplin-tui

# Run TUI
run-tui:
    cargo run -p neojoplin-tui

# Install TUI to ~/.local/bin
install-tui: build-tui
    @cargo install --path crates/tui-bin

# Install CLI to ~/.local/bin
install-cli: build
    @cargo install --path crates/cli

# Install both binaries
install-all: install-cli install-tui
    @echo "Installed neojoplin (CLI) and neojoplin-tui (TUI)"
