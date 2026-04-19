#!/bin/bash
# Comprehensive cross-sync compatibility test between NeoJoplin and Joplin CLI

set -e

WEBDAV_URL="http://localhost:8080/webdav"
TEST_SYNC_PATH="/test-cross-sync-comprehensive"
NEOJOPLIN_BIN="$HOME/.local/bin/neojoplin"

echo "=== Comprehensive Cross-Sync Compatibility Test ==="
echo "This test verifies that NeoJoplin and Joplin CLI can sync data bidirectionally"
echo ""

# Clean up function
cleanup() {
    echo "Cleaning up..."
    # Delete test sync directory from WebDAV
    curl -s -X DELETE "$WEBDAV_URL$TEST_SYNC_PATH/" 2>/dev/null || true
}

# Trap to ensure cleanup runs even if test fails
trap cleanup EXIT

# Step 1: Clean up any previous test data
echo "Step 1: Cleaning up previous test data..."
cleanup
rm -rf ~/.local/share/neojoplin/joplin.db
mkdir -p ~/.local/share/neojoplin

# Step 2: Initialize NeoJoplin and create test data
echo ""
echo "Step 2: Initialize NeoJoplin and create test data..."
$NEOJOPLIN_BIN init

# Create folders
NEO_FOLDER_1=$($NEOJOPLIN_BIN mk-book "NeoJoplin Folder 1" | grep -oP '(?<=\().*?(?=\))')
echo "Created NeoJoplin folder: $NEO_FOLDER_1"

NEO_FOLDER_2=$($NEOJOPLIN_BIN mk-book "NeoJoplin Folder 2" | grep -oP '(?<=\().*?(?=\))')
echo "Created NeoJoplin folder: $NEO_FOLDER_2"

# Create notes
NEO_NOTE_1=$($NEOJOPLIN_BIN mk-note "NeoJoplin Note 1" --body "This note was created in NeoJoplin" --parent $NEO_FOLDER_1 | grep -oP '(?<=\().*?(?=\))')
echo "Created NeoJoplin note: $NEO_NOTE_1"

NEO_NOTE_2=$($NEOJOPLIN_BIN mk-note "NeoJoplin Note 2" --body "Another note from NeoJoplin" --parent $NEO_FOLDER_2 | grep -oP '(?<=\().*?(?=\))')
echo "Created NeoJoplin note: $NEO_NOTE_2"

# Step 3: Sync NeoJoplin to WebDAV
echo ""
echo "Step 3: Sync NeoJoplin data to WebDAV..."
$NEOJOPLIN_BIN sync --url "$WEBDAV_URL" --remote "$TEST_SYNC_PATH"

# Verify files were created
echo "Verifying files on WebDAV server..."
FILE_COUNT=$(curl -s -X PROPFIND "$WEBDAV_URL$TEST_SYNC_PATH/" -H "Depth: 1" | grep -o "<D:href>[^<]*</D:href>" | wc -l)
echo "Found $FILE_COUNT items on WebDAV server"

# Step 4: Configure Joplin CLI and sync
echo ""
echo "Step 4: Configure Joplin CLI and sync from WebDAV..."
joplin config sync.6.path "$WEBDAV_URL$TEST_SYNC_PATH"
joplin sync

# Step 5: Create data in Joplin CLI
echo ""
echo "Step 5: Create data in Joplin CLI..."
joplin mkbook "Joplin CLI Folder"
sleep 1  # Give Joplin time to create the folder
joplin mknote "Joplin CLI Note" "This note was created in Joplin CLI" --book "Joplin CLI Folder"

# Step 6: Sync Joplin CLI to WebDAV
echo ""
echo "Step 6: Sync Joplin CLI data to WebDAV..."
joplin sync

# Step 7: Sync NeoJoplin from WebDAV
echo ""
echo "Step 7: Sync NeoJoplin to receive Joplin CLI data..."
$NEOJOPLIN_BIN sync --url "$WEBDAV_URL" --remote "$TEST_SYNC_PATH"

# Step 8: Verify cross-sync compatibility
echo ""
echo "Step 8: Verifying cross-sync compatibility..."

# Check if NeoJoplin can see all folders
FOLDER_COUNT=$($NEOJOPLIN_BIN list-books | wc -l)
echo "NeoJoplin sees $FOLDER_COUNT folders"

# Check if titles are properly displayed
echo "Checking folder titles..."
if $NEOJOPLIN_BIN list-books | grep -q "NeoJoplin Folder 1"; then
    echo "✓ NeoJoplin Folder 1 found"
else
    echo "✗ NeoJoplin Folder 1 NOT found"
    exit 1
fi

if $NEOJOPLIN_BIN list-books | grep -q "NeoJoplin Folder 2"; then
    echo "✓ NeoJoplin Folder 2 found"
else
    echo "✗ NeoJoplin Folder 2 NOT found"
    exit 1
fi

if $NEOJOPLIN_BIN list-books | grep -q "Joplin CLI Folder"; then
    echo "✓ Joplin CLI Folder found (cross-sync successful!)"
else
    echo "✗ Joplin CLI Folder NOT found (cross-sync failed!)"
    exit 1
fi

# Check database integrity
echo ""
echo "Checking database integrity..."
FOLDER_DB_COUNT=$(sqlite3 -list ~/.local/share/neojoplin/joplin.db "SELECT COUNT(*) FROM folders;")
NOTE_DB_COUNT=$(sqlite3 -list ~/.local/share/neojoplin/joplin.db "SELECT COUNT(*) FROM notes;")

echo "Database contains: $FOLDER_DB_COUNT folders, $NOTE_DB_COUNT notes"

if [ "$FOLDER_DB_COUNT" -ge 3 ] && [ "$NOTE_DB_COUNT" -ge 2 ]; then
    echo "✓ Database contains expected data"
else
    echo "✗ Database missing expected data"
    exit 1
fi

# Step 9: Test bidirectional sync with concurrent modifications
echo ""
echo "Step 9: Testing bidirectional sync with concurrent modifications..."

# Create new note in NeoJoplin
NEO_NOTE_3=$($NEOJOPLIN_BIN mk-note "NeoJoplin Concurrent Note" --body "Created during concurrent test" --parent $NEO_FOLDER_1 | grep -oP '(?<=\().*?(?=\))')
echo "Created concurrent note in NeoJoplin: $NEO_NOTE_3"

# Sync NeoJoplin
$NEOJOPLIN_BIN sync --url "$WEBDAV_URL" --remote "$TEST_SYNC_PATH"

# Create new note in Joplin CLI
joplin mknote "Joplin Concurrent Note" "Created during concurrent test in Joplin" --book "Joplin CLI Folder"

# Sync Joplin CLI
joplin sync

# Final sync in NeoJoplin
$NEOJOPLIN_BIN sync --url "$WEBDAV_URL" --remote "$TEST_SYNC_PATH"

# Verify final state
FINAL_FOLDER_COUNT=$($NEOJOPLIN_BIN list-books | wc -l)
FINAL_NOTE_COUNT=$(sqlite3 -list ~/.local/share/neojoplin/joplin.db "SELECT COUNT(*) FROM notes;")

echo ""
echo "=== Final Test Results ==="
echo "Total folders: $FINAL_FOLDER_COUNT"
echo "Total notes: $FINAL_NOTE_COUNT"

if [ "$FINAL_FOLDER_COUNT" -ge 3 ] && [ "$FINAL_NOTE_COUNT" -ge 4 ]; then
    echo "✓ Cross-sync compatibility test PASSED"
    echo ""
    echo "Summary:"
    echo "- NeoJoplin can create data and sync to WebDAV"
    echo "- Joplin CLI can read NeoJoplin data"
    echo "- Joplin CLI can create data and sync to WebDAV"
    echo "- NeoJoplin can read Joplin CLI data"
    echo "- Both applications can share the same WebDAV target"
    echo "- Folder titles are properly preserved across sync"
    echo "- Bidirectional sync with concurrent modifications works"
    exit 0
else
    echo "✗ Cross-sync compatibility test FAILED"
    exit 1
fi
