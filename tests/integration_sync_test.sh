#!/usr/bin/env bash
# Integration test: Bidirectional sync between NeoJoplin and Joplin CLI
# Prerequisites: Docker WebDAV running on port 8080, joplin CLI installed, neojoplin installed
set -euo pipefail

WEBDAV_URL="http://localhost:8080/webdav"
TEST_PATH="/integration-test-$(date +%s)"
E2EE_PASS="TestPassword123"
NEOJOPLIN="${NEOJOPLIN:-$HOME/.local/bin/neojoplin}"
PASS=0
FAIL=0

log() { echo -e "\033[1;34m[TEST]\033[0m $*"; }
pass() { echo -e "\033[1;32m[PASS]\033[0m $*"; PASS=$((PASS + 1)); }
fail() { echo -e "\033[1;31m[FAIL]\033[0m $*"; FAIL=$((FAIL + 1)); }
cleanup() {
    log "Cleaning up test path: $TEST_PATH"
    curl -s -X DELETE "${WEBDAV_URL}${TEST_PATH}/" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# --- Setup ---
log "Setting up test environment..."
curl -s -X MKCOL "${WEBDAV_URL}${TEST_PATH}/" >/dev/null 2>&1 || true

# Reset Joplin CLI (NOT joplin-desktop!)
rm -rf ~/.config/joplin/database.sqlite ~/.config/joplin/*.sqlite-*

# Reset NeoJoplin
rm -f ~/.local/share/neojoplin/joplin.db
$NEOJOPLIN init >/dev/null 2>&1

# Configure Joplin CLI
joplin config sync.target 6 >/dev/null 2>&1
joplin config sync.6.path "${WEBDAV_URL}${TEST_PATH}" >/dev/null 2>&1
joplin config sync.6.username "" >/dev/null 2>&1
joplin config sync.6.password "" >/dev/null 2>&1
joplin e2ee enable --password "$E2EE_PASS" >/dev/null 2>&1

# --- Test 1: Joplin → NeoJoplin ---
log "Test 1: Joplin → NeoJoplin sync"
joplin mkbook "Test Notebook" >/dev/null 2>&1
joplin use "Test Notebook" >/dev/null 2>&1
joplin mknote "Joplin Note" >/dev/null 2>&1
joplin mktodo "Joplin Todo" >/dev/null 2>&1
joplin sync >/dev/null 2>&1

$NEOJOPLIN sync --url "$WEBDAV_URL" --remote "$TEST_PATH" --e2ee-password "$E2EE_PASS" >/dev/null 2>&1

NEO_OUTPUT=$($NEOJOPLIN ls 2>&1)
if echo "$NEO_OUTPUT" | grep -q "Joplin Note"; then
    pass "Note synced from Joplin to NeoJoplin"
else
    fail "Note not found in NeoJoplin: $NEO_OUTPUT"
fi

if echo "$NEO_OUTPUT" | grep -q "Joplin Todo"; then
    pass "Todo synced from Joplin to NeoJoplin"
else
    fail "Todo not found in NeoJoplin: $NEO_OUTPUT"
fi

if echo "$NEO_OUTPUT" | grep -q "Test Notebook"; then
    pass "Folder synced from Joplin to NeoJoplin"
else
    fail "Folder not found in NeoJoplin: $NEO_OUTPUT"
fi

# --- Test 2: Idempotent sync ---
log "Test 2: Idempotent sync (second sync = no changes)"
SYNC_OUTPUT=$($NEOJOPLIN sync --url "$WEBDAV_URL" --remote "$TEST_PATH" --e2ee-password "$E2EE_PASS" 2>&1)
if echo "$SYNC_OUTPUT" | grep -q "No changes"; then
    pass "Second sync reports no changes"
else
    fail "Second sync reported changes: $SYNC_OUTPUT"
fi

# --- Test 3: NeoJoplin → Joplin ---
log "Test 3: NeoJoplin → Joplin sync"
FOLDER_ID=$(sqlite3 ~/.local/share/neojoplin/joplin.db "SELECT id FROM folders LIMIT 1;" 2>/dev/null | grep -oP '[0-9a-f]{32}')
$NEOJOPLIN mk-note "NeoJoplin Note" --body "Created in NeoJoplin" --parent "$FOLDER_ID" >/dev/null 2>&1
$NEOJOPLIN mk-todo "NeoJoplin Todo" --parent "$FOLDER_ID" >/dev/null 2>&1

$NEOJOPLIN sync --url "$WEBDAV_URL" --remote "$TEST_PATH" --e2ee-password "$E2EE_PASS" >/dev/null 2>&1
joplin sync >/dev/null 2>&1
joplin e2ee decrypt --password "$E2EE_PASS" >/dev/null 2>&1

JOPLIN_OUTPUT=$(joplin ls 2>&1)
if echo "$JOPLIN_OUTPUT" | grep -q "NeoJoplin Note"; then
    pass "Note synced from NeoJoplin to Joplin"
else
    fail "Note not found in Joplin: $JOPLIN_OUTPUT"
fi

if echo "$JOPLIN_OUTPUT" | grep -q "NeoJoplin Todo"; then
    pass "Todo synced from NeoJoplin to Joplin"
else
    fail "Todo not found in Joplin: $JOPLIN_OUTPUT"
fi

# --- Test 4: Encryption verification ---
log "Test 4: Data encrypted on WebDAV"
FILE_PATH=$(curl -s -X PROPFIND "${WEBDAV_URL}${TEST_PATH}/" -H "Depth: 1" | grep -oP '(?<=<d:href>|<D:href>|<href>)[^<]*\.md' | head -1)
if [ -z "$FILE_PATH" ]; then
    # Try alternate XML format
    FILE_PATH=$(curl -s -X PROPFIND "${WEBDAV_URL}${TEST_PATH}/" -H "Depth: 1" | grep -oP '/webdav[^"<]*\.md' | head -1)
fi
if [ -n "$FILE_PATH" ]; then
    CONTENT=$(curl -s "http://localhost:8080${FILE_PATH}")
    if echo "$CONTENT" | grep -q "encryption_applied: 1"; then
        pass "Files are encrypted on WebDAV"
    else
        fail "Files not encrypted"
    fi
    if echo "$CONTENT" | grep -q "JED01"; then
        pass "Files use JED encryption format"
    else
        fail "JED format not found"
    fi
else
    fail "No .md files found on WebDAV"
fi

# --- Test 5: Todo toggle sync ---
log "Test 5: Todo toggle syncs bidirectionally"
$NEOJOPLIN todo-toggle "NeoJoplin Todo" >/dev/null 2>&1
$NEOJOPLIN sync --url "$WEBDAV_URL" --remote "$TEST_PATH" --e2ee-password "$E2EE_PASS" >/dev/null 2>&1
joplin sync >/dev/null 2>&1
joplin e2ee decrypt --password "$E2EE_PASS" >/dev/null 2>&1

TODO_STATUS=$(sqlite3 ~/.config/joplin/database.sqlite ".mode list" ".headers off" "SELECT todo_completed FROM notes WHERE title='NeoJoplin Todo';" 2>/dev/null)
if [ -n "$TODO_STATUS" ] && [ "$TODO_STATUS" -gt 0 ]; then
    pass "Todo completion synced from NeoJoplin to Joplin"
else
    fail "Todo completion not synced (status: $TODO_STATUS)"
fi

# --- Test 6: Final idempotent check ---
log "Test 6: Final idempotent sync"
SYNC_OUTPUT=$($NEOJOPLIN sync --url "$WEBDAV_URL" --remote "$TEST_PATH" --e2ee-password "$E2EE_PASS" 2>&1)
if echo "$SYNC_OUTPUT" | grep -q "No changes"; then
    pass "Final sync reports no changes"
else
    fail "Final sync reported changes: $SYNC_OUTPUT"
fi

# --- Summary ---
echo ""
echo "================================"
echo "Results: $PASS passed, $FAIL failed"
echo "================================"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
