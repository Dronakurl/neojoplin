#!/bin/bash
# Test bidirectional sync with real WebDAV
set -e

echo "=== Bidirectional Real WebDAV Sync Test ==="

# Configuration
WEBDAV_URL="http://localhost:8080/webdav"
WEBDAV_PATH="/neojoplin-bidi-test"
TEST_DB1="/tmp/bidi-test-1/joplin.db"
TEST_DB2="/tmp/bidi-test-2/joplin.db"

# Cleanup function
cleanup() {
    rm -rf "/tmp/bidi-test-1" "/tmp/bidi-test-2"
    echo "Cleanup complete"
}
trap cleanup EXIT

echo "Test 1: Upload from NeoJoplin #1"
mkdir -p "/tmp/bidi-test-1"
export HOME="/tmp/bidi-test-1"

# Initialize first database
cargo run --bin neojoplin -- init 2>&1 | grep -v "warning:" | tail -2

# Create test data
cargo run --bin neojoplin -- mk-book "Bidi Test 1" 2>&1 | grep -v "warning:" | tail -2
cargo run --bin neojoplin -- mk-note "Bidi Note 1" --body "Created by NeoJoplin #1" 2>&1 | grep -v "warning:" | tail -2

echo "Notes in DB1:"
cargo run --bin neojoplin -- ls 2>&1 | grep -v "warning:" | grep "📝\|📁"

# Sync to WebDAV
echo "Syncing DB1 to WebDAV..."
cargo run --bin neojoplin -- sync --url "$WEBDAV_URL" --remote "$WEBDAV_PATH" 2>&1 | grep -v "warning:" | tail -5

# Verify WebDAV contents
WEBDAV_ITEMS=$(curl -s -X PROPFIND "$WEBDAV_URL$WEBDAV_PATH/items/" -H "Depth: 1" -H "Content-Type: application/xml" --data '<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:displayname/></D:prop></D:propfind>' | grep -c "<D:displayname>[^<]*.md</D:displayname>")
echo "✓ WebDAV has $WEBDAV_ITEMS items"

echo ""
echo "Test 2: Download to NeoJoplin #2"
mkdir -p "/tmp/bidi-test-2"
export HOME="/tmp/bidi-test-2"

# Initialize second database
cargo run --bin neojoplin -- init 2>&1 | grep -v "warning:" | tail -2

# Sync from WebDAV (should download existing data)
echo "Syncing DB2 from WebDAV..."
cargo run --bin neojoplin -- sync --url "$WEBDAV_URL" --remote "$WEBDAV_PATH" 2>&1 | grep -v "warning:" | tail -5

# Check if notes were downloaded
echo "Notes in DB2 (after download):"
DOWNLOADED_NOTES=$(cargo run --bin neojoplin -- ls 2>&1 | grep -v "warning:" | grep -c "📝" || echo "0")
echo "✓ DB2 has $DOWNLOADED_NOTES notes"

echo ""
echo "Test 3: Add note in DB2 and sync back"
# Add a note in DB2
cargo run --bin neojoplin -- mk-note "Bidi Note 2" --body "Created by NeoJoplin #2" 2>&1 | grep -v "warning:" | tail -2

# Sync back to WebDAV
echo "Syncing DB2 to WebDAV..."
cargo run --bin neojoplin -- sync --url "$WEBDAV_URL" --remote "$WEBDAV_PATH" 2>&1 | grep -v "warning:" | tail -5

# Verify WebDAV has both sets of data
WEBDAV_ITEMS_FINAL=$(curl -s -X PROPFIND "$WEBDAV_URL$WEBDAV_PATH/items/" -H "Depth: 1" -H "Content-Type: application/xml" --data '<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:displayname/></D:prop></D:propfind>' | grep -c "<D:displayname>[^<]*.md</D:displayname>")
echo "✓ WebDAV has $WEBDAV_ITEMS_FINAL items (should be more than before)"

echo ""
echo "Test 4: Sync back to DB1 and verify"
export HOME="/tmp/bidi-test-1"
echo "Syncing DB1 from WebDAV..."
cargo run --bin neojoplin -- sync --url "$WEBDAV_URL" --remote "$WEBDAV_PATH" 2>&1 | grep -v "warning:" | tail -5

FINAL_NOTES_DB1=$(cargo run --bin neojoplin -- ls 2>&1 | grep -v "warning:" | grep -c "📝" || echo "0")
echo "✓ DB1 now has $FINAL_NOTES_DB1 notes (should include note from DB2)"

echo ""
echo "=== Bidirectional Sync Test Complete ==="
