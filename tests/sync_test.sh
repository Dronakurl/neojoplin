#!/bin/bash
# Reproducible bidirectional sync test between NeoJoplin and Joplin CLI
# This test should pass after every code change

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
NEOJOPLIN_DB="/tmp/neojoplin-test.db"
JOPLIN_DB="/tmp/joplin-test-profile/profile.sqlite"
WEBDAV_URL="http://localhost:8080/webdav"
WEBDAV_USER="test"
WEBDAV_PASS="test"
SYNC_PATH="/sync-test"

echo -e "${YELLOW}=== NeoJoplin ↔ Joplin CLI Sync Test ===${NC}"
echo ""

# Cleanup function
cleanup() {
    echo -e "${YELLOW}Cleaning up...${NC}"
    rm -f "$NEOJOPLIN_DB"
    rm -rf "/tmp/joplin-test-profile"
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

check_neojoplin_note() {
    local title="$1"
    local body="$2"
    if cargo run --quiet --bin neojoplin -- cat "$title" 2>/dev/null | grep -q "$body"; then
        success "NeoJoplin has note: $title"
    else
        fail "NeoJoplin missing note: $title"
    fi
}

check_joplin_note() {
    local title="$1"
    local body="$2"
    cd ~/gallery/kjoplin/joplin
    if npm run --silent cat -- "$title" 2>/dev/null | grep -q "$body"; then
        success "Joplin CLI has note: $title"
    else
        fail "Joplin CLI missing note: $title"
    fi
}

# Start tests
step "Step 1: Check WebDAV server"
if curl -s -f "$WEBDAV_URL/" > /dev/null 2>&1 || curl -s -f "http://localhost:8080/health" > /dev/null 2>&1; then
    success "WebDAV server is running"
else
    fail "WebDAV server not running. Start it with: just webdav-server"
fi

step "Step 2: Initialize NeoJoplin test database"
export NEOJOPLIN_DB_PATH="$NEOJOPLIN_DB"
rm -f "$NEOJOPLIN_DB"
cargo run --quiet --bin neojoplin -- init || fail "Failed to initialize NeoJoplin"
success "NeoJoplin initialized"

step "Step 3: Initialize Joplin CLI test profile"
rm -rf "$JOPLIN_DB"
mkdir -p "$(dirname "$JOPLIN_DB")"
cd ~/gallery/kjoplin/joplin
cat > ~/.config/joplin/test-temp.json << EOF
{
  "profileId": "test-temp",
  "directory": "/tmp/joplin-test-profile",
  "sync.target": 5,
  "sync.5.path": "$WEBDAV_URL$SYNC_PATH",
  "sync.5.username": "$WEBDAV_USER",
  "sync.5.password": "$WEBDAV_PASS"
}
EOF
success "Joplin CLI configured"

step "Step 4: Create test content in NeoJoplin"
cd ~/gallery/neojoplin
cargo run --quiet --bin neojoplin -- mk-book "Test Folder" || fail "Failed to create folder"
cargo run --quiet --bin neojoplin -- mk-note "NeoJoplin Note 1" --body "This note was created in NeoJoplin" || fail "Failed to create note"
cargo run --quiet --bin neojoplin -- mk-note "NeoJoplin Note 2" --body "Another note from NeoJoplin" || fail "Failed to create note"
success "Created 1 folder and 2 notes in NeoJoplin"

step "Step 5: Sync NeoJoplin to WebDAV"
cargo run --quiet --bin neojoplin -- sync --url "$WEBDAV_URL" --username "$WEBDAV_USER" --password "$WEBDAV_PASS" --remote "$SYNC_PATH" || fail "NeoJoplin sync failed"
success "NeoJoplin synced to WebDAV"

step "Step 6: Sync Joplin CLI from WebDAV"
cd ~/gallery/kjoplin/joplin
npm run --silent sync -- --profile test-temp || fail "Joplin CLI sync failed"
success "Joplin CLI synced from WebDAV"

step "Step 7: Verify Joplin CLI has NeoJoplin content"
check_joplin_note "NeoJoplin Note 1" "This note was created in NeoJoplin"
check_joplin_note "NeoJoplin Note 2" "Another note from NeoJoplin"

step "Step 8: Create test content in Joplin CLI"
npm run --silent mknote -- --title "Joplin Note 1" --body "This note was created in Joplin CLI" || fail "Failed to create Joplin note"
npm run --silent mknote -- --title "Joplin Note 2" --body "Another note from Joplin CLI" || fail "Failed to create Joplin note"
success "Created 2 notes in Joplin CLI"

step "Step 9: Sync Joplin CLI to WebDAV"
npm run --silent sync -- --profile test-temp || fail "Joplin CLI sync failed"
success "Joplin CLI synced to WebDAV"

step "Step 10: Sync NeoJoplin from WebDAV"
cd ~/gallery/neojoplin
cargo run --quiet --bin neojoplin -- sync --url "$WEBDAV_URL" --username "$WEBDAV_USER" --password "$WEBDAV_PASS" --remote "$SYNC_PATH" || fail "NeoJoplin sync failed"
success "NeoJoplin synced from WebDAV"

step "Step 11: Verify NeoJoplin has Joplin CLI content"
check_neojoplin_note "Joplin Note 1" "This note was created in Joplin CLI"
check_neojoplin_note "Joplin Note 2" "Another note from Joplin CLI"

step "Step 12: Verify bidirectional sync (both should have all 4 notes)"
cd ~/gallery/neojoplin
NOTE_COUNT=$(cargo run --quiet --bin neojoplin -- ls 2>/dev/null | grep -c "📝" || echo "0")
if [ "$NOTE_COUNT" -eq 4 ]; then
    success "NeoJoplin has all 4 notes"
else
    fail "NeoJoplin has $NOTE_COUNT notes, expected 4"
fi

cd ~/gallery/kjoplin/joplin
JOPLIN_COUNT=$(npm run --silent ls 2>/dev/null | grep -c "Test" || echo "0")
if [ "$JOPLIN_COUNT" -ge 2 ]; then
    success "Joplin CLI has notes from NeoJoplin"
else
    fail "Joplin CLI missing notes from NeoJoplin"
fi

echo ""
echo -e "${GREEN}=== All sync tests passed! ===${NC}"
echo ""
echo "Summary:"
echo "  ✓ NeoJoplin → WebDAV → Joplin CLI"
echo "  ✓ Joplin CLI → WebDAV → NeoJoplin"
echo "  ✓ Bidirectional sync working"
echo ""
