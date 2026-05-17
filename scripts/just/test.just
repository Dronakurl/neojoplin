# Test recipes for NeoJoplin

# Run tests
test:
    cargo test --all

# Run single test by name
test-one FILTER:
    cargo test -- --exact {{FILTER}}

# Run database tests only
test-db:
    cargo test database::

# Run tests with output
test-verbose:
    cargo test -- --nocapture

# Run AI Ollama integration tests
test-ai:
    ./tests/ai_ollama_test.sh

# Run AI Ollama integration tests with verbose output
test-ai-verbose:
    bash -x ./tests/ai_ollama_test.sh

# Test WebDAV connection
test-webdav URL USERNAME PASSWORD:
    cargo run --bin webdav-test -- {{URL}} {{USERNAME}} {{PASSWORD}}

# Test with local WebDAV server
test-local-webdav:
    cargo run --bin webdav-test -- http://localhost:8080/webdav test test

# Test bidirectional sync with Joplin CLI
test-sync:
    ./tests/sync_test.sh

# Create a test note
test-note:
    cargo run -- mknote "Test Note" "This is a test note created via justfile"

# Create comprehensive test data
test-data:
    cargo run -- mkbook "Development" && \
    cargo run -- mkbook "Personal" && \
    cargo run -- mk-note "Welcome" "Welcome to NeoJoplin!" && \
    echo "Test data created successfully"
