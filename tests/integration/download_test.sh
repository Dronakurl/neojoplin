#!/bin/bash
# Test download functionality from real WebDAV
set -e

echo "=== NeoJoplin Download Sync Test ==="

# Configuration
WEBDAV_URL="http://localhost:8080/webdav"
WEBDAV_PATH="/neojoplin-real-sync-test"

echo "Current WebDAV contents:"
curl -s -X PROPFIND "$WEBDAV_URL$WEBDAV_PATH/items/" -H "Depth: 1" -H "Content-Type: application/xml" --data '<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:displayname/></D:prop></D:propfind>' | grep -o "<D:displayname>[^<]*</D:displayname>" | head -5

# Count current items
CURRENT_ITEMS=$(curl -s -X PROPFIND "$WEBDAV_URL$WEBDAV_PATH/items/" -H "Depth: 1" -H "Content-Type: application/xml" --data '<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:displayname/></D:prop></D:propfind>' | grep -c "<D:displayname>[^<]*.md</D:displayname>" || echo "0")
echo "Total items on WebDAV: $CURRENT_ITEMS"

# Test 1: Add new item to current database and sync
echo "Test 1: Add new note and sync up"
cargo run --bin neojoplin -- mk-note "Download Test Note" --body "Testing download sync" 2>&1 | grep -v "warning:" | tail -2

echo "Current notes in DB:"
CURRENT_NOTES=$(cargo run --bin neojoplin -- ls 2>&1 | grep -v "warning:" | grep -c "📝" || echo "0")
echo "Current notes: $CURRENT_NOTES"

echo "Syncing to WebDAV..."
cargo run --bin neojoplin -- sync --url "$WEBDAV_URL" --remote "$WEBDAV_PATH" 2>&1 | grep -v "warning:" | tail -5

# Verify WebDAV has the new item
NEW_ITEMS=$(curl -s -X PROPFIND "$WEBDAV_URL$WEBDAV_PATH/items/" -H "Depth: 1" -H "Content-Type: application/xml" --data '<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:displayname/></D:prop></D:propfind>' | grep -c "<D:displayname>[^<]*.md</D:displayname>" || echo "0")
echo "Items on WebDAV after sync: $NEW_ITEMS (should be $((CURRENT_ITEMS + 1)))"

# Test 2: Clear database and try to download
echo ""
echo "Test 2: Download existing WebDAV data"
echo "Deleting local database and reinitializing..."
rm -f ~/.local/share/neojoplin/joplin.db

cargo run --bin neojoplin -- init 2>&1 | grep -v "warning:" | tail -2

echo "Notes after fresh init (should be 0):"
FRESH_NOTES=$(cargo run --bin neojoplin -- ls 2>&1 | grep -v "warning:" | grep -c "📝" || echo "0")
echo "Fresh notes: $FRESH_NOTES"

echo "Syncing from WebDAV (should download existing data)..."
cargo run --bin neojoplin -- sync --url "$WEBDAV_URL" --remote "$WEBDAV_PATH" 2>&1 | grep -v "warning:" | tail -5

echo "Notes after download (should have previous items):"
DOWNLOADED_NOTES=$(cargo run --bin neojoplin -- ls 2>&1 | grep -v "warning:" | grep -c "📝" || echo "0")
echo "Downloaded notes: $DOWNLOADED_NOTES"

echo ""
echo "=== Download Test Results ==="
if [ "$DOWNLOADED_NOTES" -gt 0 ]; then
    echo "✅ SUCCESS: Downloaded $DOWNLOADED_NOTES notes from WebDAV"
else
    echo "❌ FAIL: No notes were downloaded from WebDAV"
fi

if [ "$NEW_ITEMS" -gt "$CURRENT_ITEMS" ]; then
    echo "✅ SUCCESS: Upload added new note to WebDAV"
else
    echo "❌ FAIL: Upload didn't increase WebDAV item count"
fi
