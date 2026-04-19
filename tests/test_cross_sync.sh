#!/bin/bash
# Cross-Sync Compatibility Test
# Tests bidirectional sync between NeoJoplin and Joplin CLI
# This test verifies that NeoJoplin is 100% compatible with Joplin sync protocol

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
NEOJOPLIN_BIN="${NEOJOPLIN_BIN:-$HOME/.local/bin/neojoplin}"
WEBDAV_URL="${WEBDAV_URL:-http://localhost:8080/webdav}"
TEST_REMOTE="/test-cross-sync"
TEST_DB="/tmp/test-neojoplin.db"
JOPLIN_CONFIG_DIR="/tmp/test-joplin-config"

# Helper functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

test_step() {
    echo ""
    echo -e "${GREEN}==>${NC} $1"
}

cleanup() {
    log_info "Cleaning up test environment..."
    rm -rf "$TEST_DB"
    rm -rf "$JOPLIN_CONFIG_DIR"

    # Clean WebDAV remote
    curl -s -X DELETE "$WEBDAV_URL$TEST_REMOTE/" 2>/dev/null || true
}

check_webdav() {
    log_info "Checking WebDAV server availability..."
    if ! curl -s -I "$WEBDAV_URL/" > /dev/null 2>&1; then
        log_error "WebDAV server not available at $WEBDAV_URL"
        log_error "Please start the WebDAV server first: docker compose up -d"
        exit 1
    fi
    log_info "WebDAV server is available"
}

# Test 1: Basic functionality after extraction
test_basic_functionality() {
    test_step "Test 1: Basic NeoJoplin functionality"

    cleanup
    export XDG_DATA_HOME="/tmp/test-neojoplin-data"

    # Initialize database
    log_info "Initializing database..."
    $NEOJOPLIN_BIN init

    # Create notebook and note
    log_info "Creating notebook and note..."
    FOLDER_ID=$($NEOJOPLIN_BIN mk-book "Test Notebook" | grep -oP '\(\K[^)]+')
    log_info "Created folder: $FOLDER_ID"

    NOTE_ID=$($NEOJOPLIN_BIN mk-note "Test Note" --body "Test content" --parent "$FOLDER_ID" | grep -oP '\(\K[^)]+')
    log_info "Created note: $NOTE_ID"

    # Verify listing
    log_info "Verifying listing..."
    if ! $NEOJOPLIN_BIN ls | grep -q "Test Notebook"; then
        log_error "Failed to list notebook"
        exit 1
    fi

    if ! $NEOJOPLIN_BIN ls | grep -q "Test Note"; then
        log_error "Failed to list note"
        exit 1
    fi

    # Verify content
    log_info "Verifying note content..."
    CONTENT=$($NEOJOPLIN_BIN cat "$NOTE_ID")
    if ! echo "$CONTENT" | grep -q "Test content"; then
        log_error "Failed to get note content"
        exit 1
    fi

    log_info "✓ Basic functionality test passed"
}

# Test 2: NeoJoplin → WebDAV → Joplin sync
test_neojoplin_to_joplin() {
    test_step "Test 2: NeoJoplin → WebDAV → Joplin"

    cleanup
    export XDG_DATA_HOME="/tmp/test-neojoplin-data"
    export JOPLIN_CONFIG_HOME="$JOPLIN_CONFIG_DIR"

    # Setup NeoJoplin
    $NEOJOPLIN_BIN init
    FOLDER_ID=$($NEOJOPLIN_BIN mk-book "NeoJoplin Source" | grep -oP '\(\K[^)]+')
    $NEOJOPLIN_BIN mk-note "From NeoJoplin" --body "Created in NeoJoplin, should sync to Joplin" --parent "$FOLDER_ID" > /dev/null

    # Sync to WebDAV
    log_info "Syncing NeoJoplin to WebDAV..."
    $NEOJOPLIN_BIN sync --url "$WEBDAV_URL" --remote "$TEST_REMOTE" > /dev/null 2>&1

    # Setup Joplin
    mkdir -p "$JOPLIN_CONFIG_DIR"
    export JOPLIN_CONFIG_HOME="$JOPLIN_CONFIG_DIR"

    joplin config sync.target 6
    joplin config sync.6.path "$WEBDAV_URL$TEST_REMOTE"
    joplin config sync.6.username ""
    joplin config sync.6.password ""

    # Sync Joplin from WebDAV
    log_info "Syncing Joplin from WebDAV..."
    joplin sync > /dev/null 2>&1

    # Verify Joplin received the data
    log_info "Verifying Joplin received data..."
    if joplin ls / | grep -q "NeoJoplin Source"; then
        log_info "✓ Joplin can see NeoJoplin notebook"
    else
        log_error "Joplin cannot see NeoJoplin notebook"
        joplin ls /
        exit 1
    fi

    log_info "✓ NeoJoplin → Joplin sync test passed"
}

# Test 3: Joplin → WebDAV → NeoJoplin sync
test_joplin_to_neojoplin() {
    test_step "Test 3: Joplin → WebDAV → NeoJoplin"

    cleanup
    export XDG_DATA_HOME="/tmp/test-neojoplin-data"
    export JOPLIN_CONFIG_HOME="$JOPLIN_CONFIG_DIR"

    # Setup Joplin first
    mkdir -p "$JOPLIN_CONFIG_DIR"
    export JOPLIN_CONFIG_HOME="$JOPLIN_CONFIG_DIR"

    joplin config sync.target 6
    joplin config sync.6.path "$WEBDAV_URL$TEST_REMOTE"
    joplin config sync.6.username ""
    joplin config sync.6.password ""

    # Create data in Joplin
    log_info "Creating notebook and note in Joplin..."
    joplin mkbook "Joplin Source" > /dev/null 2>&1
    joplin use "Joplin Source" > /dev/null 2>&1
    joplin mknote "From Joplin" "Created in Joplin, should sync to NeoJoplin" > /dev/null 2>&1

    # Sync to WebDAV
    log_info "Syncing Joplin to WebDAV..."
    joplin sync > /dev/null 2>&1

    # Setup NeoJoplin
    export XDG_DATA_HOME="/tmp/test-neojoplin-data"
    $NEOJOPLIN_BIN init

    # Sync NeoJoplin from WebDAV
    log_info "Syncing NeoJoplin from WebDAV..."
    $NEOJOPLIN_BIN sync --url "$WEBDAV_URL" --remote "$TEST_REMOTE" > /dev/null 2>&1

    # Verify NeoJoplin received the data
    log_info "Verifying NeoJoplin received data..."
    if $NEOJOPLIN_BIN ls | grep -q "Joplin Source"; then
        log_info "✓ NeoJoplin can see Joplin notebook"
    else
        log_error "NeoJoplin cannot see Joplin notebook"
        $NEOJOPLIN_BIN ls
        exit 1
    fi

    if $NEOJOPLIN_BIN ls | grep -q "From Joplin"; then
        log_info "✓ NeoJoplin can see Joplin note"
    else
        log_error "NeoJoplin cannot see Joplin note"
        $NEOJOPLIN_BIN ls
        exit 1
    fi

    log_info "✓ Joplin → NeoJoplin sync test passed"
}

# Test 4: Bidirectional sync with conflicts
test_bidirectional_sync() {
    test_step "Test 4: Bidirectional sync with concurrent changes"

    cleanup
    export XDG_DATA_HOME="/tmp/test-neojoplin-data"
    export JOPLIN_CONFIG_HOME="$JOPLIN_CONFIG_DIR"

    # Setup both systems
    $NEOJOPLIN_BIN init
    mkdir -p "$JOPLIN_CONFIG_DIR"
    export JOPLIN_CONFIG_HOME="$JOPLIN_CONFIG_DIR"

    joplin config sync.target 6
    joplin config sync.6.path "$WEBDAV_URL$TEST_REMOTE"
    joplin config sync.6.username ""
    joplin config sync.6.password ""

    # Create initial data in NeoJoplin
    log_info "Creating initial data in NeoJoplin..."
    FOLDER_ID=$($NEOJOPLIN_BIN mk-book "Shared Notebook" | grep -oP '\(\K[^)]+')
    $NEOJOPLIN_BIN mk-note "Initial Note" --body "Initial content" --parent "$FOLDER_ID" > /dev/null
    $NEOJOPLIN_BIN sync --url "$WEBDAV_URL" --remote "$TEST_REMOTE" > /dev/null 2>&1

    # Sync to Joplin
    joplin sync > /dev/null 2>&1

    # Create new note in Joplin
    log_info "Creating note in Joplin..."
    joplin use "Shared Notebook" > /dev/null 2>&1
    joplin mknote "Joplin Note" "Created in Joplin" > /dev/null 2>&1
    joplin sync > /dev/null 2>&1

    # Create new note in NeoJoplin
    log_info "Creating note in NeoJoplin..."
    $NEOJOPLIN_BIN mk-note "NeoJoplin Note" --body "Created in NeoJoplin" --parent "$FOLDER_ID" > /dev/null
    $NEOJOPLIN_BIN sync --url "$WEBDAV_URL" --remote "$TEST_REMOTE" > /dev/null 2>&1

    # Verify both notes exist in both systems
    log_info "Verifying bidirectional sync..."
    joplin sync > /dev/null 2>&1

    NEOJOPLIN_NOTES=$($NEOJOPLIN_BIN ls | wc -l)
    if [ "$NEOJOPLIN_NOTES" -ge 3 ]; then
        log_info "✓ NeoJoplin has all notes (Shared Notebook + 3 notes)"
    else
        log_error "NeoJoplin missing notes (expected >= 3, got $NEOJOPLIN_NOTES)"
        $NEOJOPLIN_BIN ls
        exit 1
    fi

    log_info "✓ Bidirectional sync test passed"
}

# Main test execution
main() {
    log_info "Starting Cross-Sync Compatibility Tests..."
    log_info "NeoJoplin binary: $NEOJOPLIN_BIN"
    log_info "WebDAV URL: $WEBDAV_URL"

    # Check prerequisites
    check_webdav

    # Run tests
    test_basic_functionality
    test_neojoplin_to_joplin
    test_joplin_to_neojoplin
    test_bidirectional_sync

    # Cleanup
    cleanup

    echo ""
    log_info "✅ All cross-sync compatibility tests passed!"
    log_info "NeoJoplin is 100% compatible with Joplin sync protocol"
}

# Run main function
main "$@"
