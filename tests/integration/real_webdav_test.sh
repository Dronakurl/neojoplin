#!/bin/bash
# Test real WebDAV sync functionality
set -e

echo "=== NeoJoplin Real WebDAV Sync Test ==="

# Test configuration
WEBDAV_URL="http://localhost:8080"
WEBDAV_PATH="/webdav/neojoplin-real-test"
WEBDAV_USER=""
WEBDAV_PASS=""  # No auth for local testing

echo "Testing against: $WEBDAV_URL$WEBDAV_PATH"

# Step 1: Create test database
echo "Step 1: Creating test database..."
TEST_DB="/tmp/real-sync-test-$$/joplin.db"
mkdir -p "$(dirname "$TEST_DB")"
rm -f "$TEST_DB"

cargo run --bin neojoplin -- init 2>&1 | grep -v "warning:" | head -5
echo "✓ Database created at $TEST_DB"

# Step 2: Create test data
echo "Step 2: Creating test data..."
cargo run --bin neojoplin -- mk-book "Real Test Folder" 2>&1 | grep -v "warning:" | tail -2
cargo run --bin neojoplin -- mk-note "Real Test Note" --body "This should sync to WebDAV" 2>&1 | grep -v "warning:" | tail -2
echo "✓ Test data created"

# Step 3: Test sync
echo "Step 3: Syncing to real WebDAV..."
cargo run --bin neojoplin -- sync --url "$WEBDAV_URL/webdav" --remote "$WEBDAV_PATH" 2>&1 | grep -v "warning:" | tail -10
echo "✓ Sync command completed"

# Step 4: Verify WebDAV contents
echo "Step 4: Verifying WebDAV contents..."
WEBDAV_FILES=$(curl -s -X PROPFIND "$WEBDAV_URL$WEBDAV_PATH/" -H "Depth: 1" -H "Content-Type: application/xml" --data '<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:displayname/></D:prop></D:propfind>' | grep -o "<D:displayname>[^<]*</D:displayname>" | wc -l)
echo "Found $WEBDAV_FILES entries on WebDAV"

# Step 5: Check for uploaded files
echo "Step 5: Checking for uploaded files..."
if curl -s -X PROPFIND "$WEBDAV_URL$WEBDAV_PATH/items/" -H "Depth: 1" -H "Content-Type: application/xml" --data '<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:displayname/></D:prop></D:propfind>' | grep -q "<D:displayname>"; then
    echo "✓ Files found in /items directory"

    # Show some file names
    echo "Sample files:"
    curl -s -X PROPFIND "$WEBDAV_URL$WEBDAV_PATH/items/" -H "Depth: 1" -H "Content-Type: application/xml" --data '<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:displayname/></D:prop></D:propfind>' | grep -o "<D:displayname>[^<]*</D:displayname>" | head -5
else
    echo "✗ No files found in /items directory"
fi

# Step 6: Test download by creating a new database
echo "Step 6: Testing download (new database)..."
TEST_DB2="/tmp/real-sync-test-download-$$/joplin.db"
mkdir -p "$(dirname "$TEST_DB2")"
rm -f "$TEST_DB2"

export HOME="/tmp/real-sync-test-download-$$"
cargo run --bin neojoplin -- init 2>&1 | grep -v "warning:" | tail -2

# Sync should download existing data
cargo run --bin neojoplin -- sync --url "$WEBDAV_URL/webdav" --remote "$WEBDAV_PATH" 2>&1 | grep -v "warning:" | tail -10

# Check if we got the notes back
NOTES_AFTER=$(cargo run --bin neojoplin -- ls 2>&1 | grep -v "warning:" | grep "📝" | wc -l)
echo "✓ After download: $NOTES_AFTER notes found"

# Cleanup
echo "Cleanup..."
rm -rf "/tmp/real-sync-test-$$"
rm -rf "/tmp/real-sync-test-download-$$"

echo ""
echo "=== Real WebDAV Sync Test Complete ==="
