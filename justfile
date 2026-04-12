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
