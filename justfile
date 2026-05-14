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

# Build and run (launches TUI by default, or pass CLI commands)
run ARGV="":
    cargo run --release --bin neojoplin -- {{ARGV}}

# Build and install to ~/.cargo/bin
install-bare:
    cargo install --path crates/cli --force
    @echo "Installed neojoplin to ~/.cargo/bin"

# Check code with clippy
check:
    cargo clippy -- -D warnings

# Strict project validation (fmt, lint with no warnings, tests)
verify:
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test --all

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

# Launch TUI (same as running with no arguments)
tui:
    cargo run --release --bin neojoplin -- --tui

# Legacy compatibility (now just runs the main binary)
run-tui: tui

# Legacy compatibility (now just installs the main binary)
install-cli: install

# Legacy compatibility (now just installs the main binary)
install-tui: install

# Legacy compatibility (now installs binary + plugins)
install:
    just clean
    cargo build --release
    just install-plugins
    cargo install --path crates/cli --force
    @echo "Installed neojoplin (CLI + TUI + plugins)"

# Note: tui-bin crate removed - unified binary provides both interfaces

# Alias for install (installs binary + plugins)
install-all: install

# Test WebDAV connection
test-webdav URL USERNAME PASSWORD:
    cargo run --bin webdav-test -- {{URL}} {{USERNAME}} {{PASSWORD}}

# Start local WebDAV server for testing
webdav-server:
    docker compose up -d webdav
    @echo "Local WebDAV server started on http://localhost:8080"
    @echo "WebDAV path: http://localhost:8080/webdav"

# Stop local WebDAV server
webdav-stop:
    docker compose down

# Test with local WebDAV server
test-local-webdav:
    cargo run --bin webdav-test -- http://localhost:8080/webdav test test

# View WebDAV server logs
webdav-logs:
    docker compose logs -f webdav

# Test bidirectional sync with Joplin CLI
test-sync:
    ./tests/integration/sync_test.sh


# AI Plugin Development
build-ai-plugin:
    cargo build -p ai-ollama --release

start-ollama:
    ./scripts/start_ollama.sh

stop-ollama:
    ./scripts/stop_ollama.sh

# Build all plugins
build-plugins:
    cargo build -p jarvis --release

# Install plugins to plugin directories (both regular and test mode)
install-plugins:
    # Install to regular plugin directory
    mkdir -p ~/.config/neojoplin/plugins/available/jarvis/0.1.0
    mkdir -p ~/.config/neojoplin/plugins/enabled
    cp target/release/libjarvis.so ~/.config/neojoplin/plugins/available/jarvis/0.1.0/
    ln -sf ../available/jarvis/0.1.0/libjarvis.so ~/.config/neojoplin/plugins/enabled/
    # Install to test plugin directory
    mkdir -p ~/.config/neojoplin-test/plugins/available/jarvis/0.1.0
    mkdir -p ~/.config/neojoplin-test/plugins/enabled
    cp target/release/libjarvis.so ~/.config/neojoplin-test/plugins/available/jarvis/0.1.0/
    ln -sf ../available/jarvis/0.1.0/libjarvis.so ~/.config/neojoplin-test/plugins/enabled/
    @echo "✓ Plugins installed to ~/.config/neojoplin/plugins/ and ~/.config/neojoplin-test/plugins/"

    @echo "✓ NeoJoplin and plugins installed successfully"
