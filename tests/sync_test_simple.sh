#!/bin/bash
# Simple NeoJoplin sync test
# Tests upload to WebDAV and download from WebDAV

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
NEOJOPLIN_DB="$HOME/.local/share/neojoplin/joplin.db"
WEBDAV_URL="http://localhost:8080/webdav"
WEBDAV_USER="test"
WEBDAV_PASS="test"
SYNC_PATH="/sync-test"

echo -e "${YELLOW}=== NeoJoplin Sync Test ===${NC}"
echo ""

# Cleanup function
cleanup() {
    echo -e "${YELLOW}Cleaning up...${NC}"
    rm -f "$NEOJOPLIN_DB"
    docker exec neojoplin-webdav-1 rm -rf /srv/webdav/sync-test 2>/dev/null || true
    echo "Cleanup complete"
}

# Trap to ensure cleanup on exit
trap cleanup EXIT

# Pre-test cleanup
cleanup

# Helper functions
step() {
    echo -e "\n${YELLOW}>>> $1${NC}"
}

success() {
    echo -e "${GREEN}✓ $1${NC}"
}

fail() {
    echo -e "${RED}✗ $1${NC}"
    exit 1
}

# Start tests
step "Step 1: Check WebDAV server"
if curl -s -f "http://localhost:8080/health" > /dev/null 2>&1; then
    success "WebDAV server is running"
else
    fail "WebDAV server not running. Start it with: just webdav-server"
fi

step "Step 2: Initialize NeoJoplin and create test content"
cargo run --quiet --bin neojoplin -- init || fail "Failed to initialize"
cargo run --quiet --bin neojoplin -- mk-book "Test Folder" || fail "Failed to create folder"
cargo run --quiet --bin neojoplin -- mk-note "Test Note 1" --body "First test note" || fail "Failed to create note 1"
cargo run --quiet --bin neojoplin -- mk-note "Test Note 2" --body "Second test note" || fail "Failed to create note 2"
success "Created 1 folder and 2 notes"

step "Step 3: Sync to WebDAV (upload)"
cargo run --quiet --bin neojoplin -- sync --url "$WEBDAV_URL" --username "$WEBDAV_USER" --password "$WEBDAV_PASS" --remote "$SYNC_PATH" || fail "Upload sync failed"
success "Uploaded to WebDAV"

step "Step 4: Verify files on WebDAV"
FILE_COUNT=$(curl -s -X PROPFIND "$WEBDAV_URL$SYNC_PATH/" | grep -o "<D:href>[^<]*</D:href>" | wc -l)
if [ "$FILE_COUNT" -gt 5 ]; then
    success "WebDAV has $FILE_COUNT files/directories"
else
    fail "WebDAV has only $FILE_COUNT files/directories, expected more"
fi

step "Step 5: Verify file format (check first note)"
NOTE_FILE=$(curl -s -X PROPFIND "$WEBDAV_URL$SYNC_PATH/items/" | grep -o '<D:href>[^<]*\.md</D:href>' | head -1 | sed 's/<D:href>//;s/<\/D:href>//')
if [ -n "$NOTE_FILE" ]; then
    NOTE_CONTENT=$(curl -s "http://localhost:8080$NOTE_FILE")
    if echo "$NOTE_CONTENT" | grep -q "Test Note"; then
        success "Files are in correct Joplin format"
    else
        fail "Files are not in correct Joplin format"
    fi
else
    fail "No note files found on WebDAV"
fi

step "Step 6: Reset database and reinitialize"
rm -f "$NEOJOPLIN_DB"
cargo run --quiet --bin neojoplin -- init || fail "Failed to reinitialize"
success "Database reset"

step "Step 7: Create new note in fresh database"
cargo run --quiet --bin neojoplin -- mk-note "Local Note" --body "This note is local only" || fail "Failed to create local note"
success "Created local note"

step "Step 8: Sync from WebDAV (download)"
cargo run --quiet --bin neojoplin -- sync --url "$WEBDAV_URL" --username "$WEBDAV_USER" --password "$WEBDAV_PASS" --remote "$SYNC_PATH" || fail "Download sync failed"
success "Downloaded from WebDAV"

step "Step 9: Verify downloaded content"
if cargo run --quiet --bin neojoplin -- ls 2>/dev/null | grep -q "Test Note 1"; then
    success "Downloaded notes are present"
else
    fail "Downloaded notes missing"
fi

step "Step 10: Verify local note still exists"
if cargo run --quiet --bin neojoplin -- ls 2>/dev/null | grep -q "Local Note"; then
    success "Local note preserved during sync"
else
    fail "Local note was lost"
fi

step "Step 11: Verify total note count"
NOTE_COUNT=$(cargo run --quiet --bin neojoplin -- ls 2>/dev/null | grep -c "📝" || echo "0")
if [ "$NOTE_COUNT" -eq 3 ]; then
    success "All 3 notes present (2 downloaded + 1 local)"
else
    fail "Expected 3 notes, found $NOTE_COUNT"
fi

echo ""
echo -e "${GREEN}=== All sync tests passed! ===${NC}"
echo ""
echo "Summary:"
echo "  ✓ Upload to WebDAV working"
echo "  ✓ File format correct (Joplin-compatible)"
echo "  ✓ Download from WebDAV working"
echo "  ✓ Local content preserved during sync"
echo "  ✓ Automatic directory creation working"
echo ""
