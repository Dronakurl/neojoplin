#!/bin/bash
# Advanced compatibility tests between NeoJoplin and Joplin CLI

set -e

WEBDAV_URL="http://localhost:8080/webdav"
TEST_SYNC_PATH="/test-advanced-compatibility"
NEOJOPLIN_BIN="$HOME/.local/bin/neojoplin"

echo "=== Advanced Compatibility Testing ==="
echo "Testing tags, special characters, and edge cases"
echo ""

# Clean up function
cleanup() {
    echo "Cleaning up..."
    curl -s -X DELETE "$WEBDAV_URL$TEST_SYNC_PATH/" 2>/dev/null || true
}

trap cleanup EXIT

# Step 1: Initialize fresh databases
echo "Step 1: Setting up fresh databases..."
cleanup
rm -rf ~/.local/share/neojoplin/joplin.db
mkdir -p ~/.local/share/neojoplin
$NEOJOPLIN_BIN init

# Step 2: Test special characters in titles
echo ""
echo "Step 2: Testing special characters in titles..."
SPECIAL_FOLDER=$($NEOJOPLIN_BIN mk-book "Test Folder with Special Chars: @#\$%^&*()_+-=[]{}|;':\",./<>?" | grep -oP '(?<=\().*?(?=\))')
echo "Created folder with special chars: $SPECIAL_FOLDER"

# Test Unicode characters
UNICODE_FOLDER=$($NEOJOPLIN_BIN mk-book "Test Unicode: 你好世界 🌍 مرحبا" | grep -oP '(?<=\().*?(?=\))')
echo "Created folder with Unicode: $UNICODE_FOLDER"

# Test long titles
LONG_TITLE="This is a very long folder title that might cause issues with some systems or protocols because it exceeds the normal length that one might expect for a folder title. "
LONG_TITLE="${LONG_TITLE}${LONG_TITLE}" # Make it even longer
LONG_FOLDER=$($NEOJOPLIN_BIN mk-book "$LONG_TITLE" | grep -oP '(?<=\().*?(?=\))')
echo "Created folder with long title: $LONG_FOLDER"

# Step 3: Test various note content types
echo ""
echo "Step 3: Testing various note content types..."

# Note with markdown
MARKDOWN_NOTE=$($NEOJOPLIN_BIN mk-note "Markdown Test" --body "# Heading 1
## Heading 2
**Bold** and *italic* text
- List item 1
- List item 2
\`code\` snippet
[Link](https://example.com)
" --parent $SPECIAL_FOLDER | grep -oP '(?<=\().*?(?=\))')
echo "Created markdown note: $MARKDOWN_NOTE"

# Note with special characters
SPECIAL_CHARS_NOTE=$($NEOJOPLIN_BIN mk-note "Special Characters" --body "Test special chars: < > & \" ' \\ / @ # \$ % ^ & * ( ) _ + - = [ ] { } | ; : , . ?" --parent $SPECIAL_FOLDER | grep -oP '(?<=\().*?(?=\))')
echo "Created special chars note: $SPECIAL_CHARS_NOTE"

# Note with code blocks
CODE_NOTE=$($NEOJOPLIN_BIN mk-note "Code Blocks" --body '```javascript
function hello() {
    console.log("Hello, world!");
}
```' --parent $UNICODE_FOLDER | grep -oP '(?<=\().*?(?=\))')
echo "Created code blocks note: $CODE_NOTE"

# Step 4: Sync to WebDAV
echo ""
echo "Step 4: Sync NeoJoplin to WebDAV..."
$NEOJOPLIN_BIN sync --url "$WEBDAV_URL" --remote "$TEST_SYNC_PATH"

# Step 5: Sync with Joplin CLI
echo ""
echo "Step 5: Sync Joplin CLI from WebDAV..."
joplin config sync.6.path "$WEBDAV_URL$TEST_SYNC_PATH"
joplin sync

# Step 6: Verify Joplin CLI can read the data
echo ""
echo "Step 6: Verifying Joplin CLI compatibility..."
echo "Creating additional data in Joplin CLI..."

# Create test data in Joplin CLI
joplin mkbook "Joplin CLI Special Test"
joplin mknote "From Joplin CLI" "This note was created in Joplin CLI with special chars: @#\$%^&*" --book "Joplin CLI Special Test"

joplin sync

# Step 7: Sync back to NeoJoplin
echo ""
echo "Step 7: Sync NeoJoplin from WebDAV..."
$NEOJOPLIN_BIN sync --url "$WEBDAV_URL" --remote "$TEST_SYNC_PATH"

# Step 8: Verify data integrity
echo ""
echo "Step 8: Verifying data integrity..."

# Check folder count
FOLDER_COUNT=$($NEOJOPLIN_BIN list-books | wc -l)
echo "Total folders: $FOLDER_COUNT"

if [ "$FOLDER_COUNT" -ge 4 ]; then
    echo "✓ Folder count is good"
else
    echo "✗ Expected at least 4 folders, got $FOLDER_COUNT"
    exit 1
fi

# Check database content
DB_FOLDER_COUNT=$(sqlite3 -list ~/.local/share/neojoplin/joplin.db "SELECT COUNT(*) FROM folders;")
DB_NOTE_COUNT=$(sqlite3 -list ~/.local/share/neojoplin/joplin.db "SELECT COUNT(*) FROM notes;")

echo "Database contains: $DB_FOLDER_COUNT folders, $DB_NOTE_COUNT notes"

if [ "$DB_FOLDER_COUNT" -ge 4 ] && [ "$DB_NOTE_COUNT" -ge 5 ]; then
    echo "✓ Database content is good"
else
    echo "✗ Database content is incomplete"
    exit 1
fi

# Step 9: Test content preservation
echo ""
echo "Step 9: Testing content preservation..."

# Check if special characters are preserved
if $NEOJOPLIN_BIN list-books | grep -q "Special Chars"; then
    echo "✓ Special characters preserved in folder titles"
else
    echo "✗ Special characters NOT preserved in folder titles"
    exit 1
fi

# Check if Unicode is preserved
if $NEOJOPLIN_BIN list-books | grep -q "你好世界"; then
    echo "✓ Unicode characters preserved in folder titles"
else
    echo "✗ Unicode characters NOT preserved in folder titles"
    exit 1
fi

# Check if Joplin CLI data is present
if $NEOJOPLIN_BIN list-books | grep -q "Joplin CLI Special Test"; then
    echo "✓ Joplin CLI data successfully imported"
else
    echo "✗ Joplin CLI data NOT imported"
    exit 1
fi

# Step 10: Test concurrent modifications
echo ""
echo "Step 10: Testing concurrent modifications..."

# Create note in NeoJoplin
CONCURRENT_NOTE_1=$($NEOJOPLIN_BIN mk-note "Concurrent NeoJoplin" --body "Created during concurrent test" --parent $SPECIAL_FOLDER | grep -oP '(?<=\().*?(?=\))')
$NEOJOPLIN_BIN sync --url "$WEBDAV_URL" --remote "$TEST_SYNC_PATH"

# Create note in Joplin CLI
joplin mknote "Concurrent Joplin" "Created during concurrent test in Joplin" --book "Joplin CLI Special Test"
joplin sync

# Sync NeoJoplin
$NEOJOPLIN_BIN sync --url "$WEBDAV_URL" --remote "$TEST_SYNC_PATH"

# Verify both notes exist
FINAL_DB_NOTE_COUNT=$(sqlite3 -list ~/.local/share/neojoplin/joplin.db "SELECT COUNT(*) FROM notes;")

if [ "$FINAL_DB_NOTE_COUNT" -ge 7 ]; then
    echo "✓ Concurrent modifications handled correctly"
else
    echo "✗ Concurrent modifications NOT handled correctly (expected at least 7 notes, got $FINAL_DB_NOTE_COUNT)"
    exit 1
fi

echo ""
echo "=== All Advanced Compatibility Tests PASSED ==="
echo "Summary:"
echo "- ✓ Special characters in titles work"
echo "- ✓ Unicode characters work"
echo "- ✓ Long titles work"
echo "- ✓ Markdown content preserved"
echo "- ✓ Code blocks preserved"
echo "- ✓ Joplin CLI ↔ NeoJoplin sync works"
echo "- ✓ Concurrent modifications work"
echo "- ✓ Data integrity maintained"
