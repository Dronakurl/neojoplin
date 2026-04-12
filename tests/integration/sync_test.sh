#!/bin/bash
# Bidirectional sync compatibility test between Joplin CLI and NeoJoplin
# This test verifies that both clients can sync notes and notebooks correctly

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
TEST_DIR="/tmp/neojoplin-sync-test-$$"
NEOJOPLIN_DB="$TEST_DIR/neojoplin.db"
JOPLIN_PROFILE="$TEST_DIR/joplin-profile"
WEBDAV_PORT="${PORT:-8081}"  # Use 8081 by default to avoid conflicts
WEBDAV_URL="http://localhost:$WEBDAV_PORT"
WEBDAV_PATH="/test-sync-$$"
SYNC_TARGET_ID="$(uuidgen)"

echo -e "${YELLOW}=== NeoJoplin Sync Compatibility Test ===${NC}"
echo "Test directory: $TEST_DIR"
echo ""

# Cleanup function
cleanup() {
    echo -e "${YELLOW}Cleaning up...${NC}"
    # Stop WebDAV server if running
    pgrep -f "python.*webdav" | xargs -r kill 2>/dev/null || true
    rm -rf "$TEST_DIR"
    echo "Cleanup complete"
}

trap cleanup EXIT

# Step 1: Set up local WebDAV server
echo -e "${YELLOW}Step 1: Setting up local WebDAV server...${NC}"
mkdir -p "$TEST_DIR/webdav"
cd "$TEST_DIR/webdav"

# Create a simple Python WebDAV server
cat > server.py << 'EOF'
#!/usr/bin/env python3
from http.server import HTTPServer, SimpleHTTPRequestHandler
import socketserver
import sys
import os

class DualStackServer(HTTPServer):
    def server_bind(self):
        # Allow address reuse to avoid "Address already in use" errors
        self.allow_reuse_address = True
        super().server_bind()

class WebDAVHandler(SimpleHTTPRequestHandler):
    def do_MKCOL(self):
        os.makedirs(self.translate_path(self.path), exist_ok=True)
        self.send_response(201)
        self.end_headers()

    def do_PUT(self):
        path = self.translate_path(self.path)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        content_length = int(self.headers.get('Content-Length', 0))
        with open(path, 'wb') as f:
            f.write(self.rfile.read(content_length))
        self.send_response(201)
        self.end_headers()

    def do_DELETE(self):
        path = self.translate_path(self.path)
        if os.path.exists(path):
            if os.path.isdir(path):
                os.rmdir(path)
            else:
                os.remove(path)
        self.send_response(204)
        self.end_headers()

    def do_PROPFIND(self):
        import xml.etree.ElementTree as ET
        self.send_response(207)
        self.send_header('Content-Type', 'application/xml')
        self.end_headers()

        path = self.translate_path(self.path)
        files = []
        if os.path.exists(path) and os.path.isdir(path):
            files = ['.', '..']
            try:
                files.extend(os.listdir(path))
            except:
                pass

        xml = '<?xml version="1.0" encoding="utf-8" ?>\n'
        xml += '<D:multistatus xmlns:D="DAV:">\n'
        for f in files:
            xml += f'  <D:response><D:href>{self.path}/{f}</D:href></D:response>\n'
        xml += '</D:multistatus>\n'
        self.wfile.write(xml.encode())

if __name__ == '__main__':
    port = int(os.environ.get('PORT', 8081))  # Use 8081 by default to avoid conflicts
    print(f"Starting WebDAV server on port {port}", file=sys.stderr)
    server = DualStackServer(('localhost', port), WebDAVHandler)
    server.serve_forever()
EOF

chmod +x server.py
python3 server.py &
WEBDAV_PID=$!
sleep 2

if ! kill -0 $WEBDAV_PID 2>/dev/null; then
    echo -e "${RED}Failed to start WebDAV server${NC}"
    exit 1
fi
echo -e "${GREEN}WebDAV server started (PID: $WEBDAV_PID)${NC}"
echo ""

# Step 2: Configure NeoJoplin for testing
echo -e "${YELLOW}Step 2: Configuring NeoJoplin...${NC}"
mkdir -p "$(dirname "$NEOJOPLIN_DB")"

cat > "$TEST_DIR/neojoplin-config.json" << EOF
{
  "database_path": "$NEOJOPLIN_DB",
  "webdav": {
    "base_url": "$WEBDAV_URL",
    "username": "test",
    "password": "test123"
  },
  "sync": {
    "remote_path": "$WEBDAV_PATH",
    "lock_timeout": 300
  }
}
EOF

echo -e "${GREEN}NeoJoplin configured${NC}"
echo ""

# Step 3: Initialize NeoJoplin database
echo -e "${YELLOW}Step 3: Initializing NeoJoplin database...${NC}"
cargo run --manifest-path=/home/konrad/gallery/neojoplin/Cargo.toml --bin neojoplin -- \
  --config "$TEST_DIR/neojoplin-config.json" \
  init || {
  echo -e "${RED}Failed to initialize NeoJoplin${NC}"
  exit 1
}
echo -e "${GREEN}Database initialized${NC}"
echo ""

# Step 4: Create test data in NeoJoplin
echo -e "${YELLOW}Step 4: Creating test data in NeoJoplin...${NC}"
TEST_NOTE_ID=$(uuidgen)
TEST_FOLDER_ID=$(uuidgen)

# Create test folder
sqlite3 "$NEOJOPLIN_DB" << EOF
INSERT INTO folders (id, title, created_time, updated_time)
VALUES ('$TEST_FOLDER_ID', 'Test Folder', $(date +%s)000, $(date +%s)000);
EOF

# Create test notes
sqlite3 "$NEOJOPLIN_DB" << EOF
INSERT INTO notes (id, title, body, parent_id, created_time, updated_time)
VALUES
  ('$TEST_NOTE_ID', 'Note from NeoJoplin', 'This note was created in NeoJoplin', '$TEST_FOLDER_ID', $(date +%s)000, $(date +%s)000),
  ('$(uuidgen)', 'Another Note', 'Second note for testing', '$TEST_FOLDER_ID', $(date +%s)000, $(date +%s)000);
EOF

echo -e "${GREEN}Created 2 notes in 1 folder${NC}"
echo ""

# Step 5: Sync from NeoJoplin
echo -e "${YELLOW}Step 5: Syncing from NeoJoplin...${NC}"
cargo run --manifest-path=/home/konrad/gallery/neojoplin/Cargo.toml --bin neojoplin -- \
  --config "$TEST_DIR/neojoplin-config.json" \
  sync || {
  echo -e "${RED}NeoJoplin sync failed${NC}"
  exit 1
}
echo -e "${GREEN}NeoJoplin sync completed${NC}"
echo ""

# Step 6: Verify WebDAV contents
echo -e "${YELLOW}Step 6: Verifying WebDAV contents...${NC}"
WEBDAV_ITEMS=$(curl -s "$WEBDAV_URL$WEBDAV_PATH/items/" | grep -o "<D:href>[^<]*</D:href>" | wc -l)
echo "Found $WEBDAV_ITEMS items on WebDAV"
if [ "$WEBDAV_ITEMS" -lt 2 ]; then
  echo -e "${RED}Not enough items found on WebDAV${NC}"
  exit 1
fi
echo -e "${GREEN}WebDAV contains expected items${NC}"
echo ""

# Step 7: Configure Joplin CLI for testing
echo -e "${YELLOW}Step 7: Setting up Joplin CLI for testing...${NC}"
if ! command -v joplin &> /dev/null; then
  echo -e "${YELLOW}Joplin CLI not found, skipping Joplin compatibility test${NC}"
  echo -e "${GREEN}=== NeoJoplin self-test PASSED ===${NC}"
  exit 0
fi

mkdir -p "$JOPLIN_PROFILE"
export JOPLIN_DATA_DIR="$JOPLIN_PROFILE"

# Configure Joplin to use the same WebDAV server
joplin config sync.target "$SYNC_TARGET_ID"
joplin config sync.$SYNC_TARGET_ID.type 3
joplin config sync.$SYNC_TARGET_ID.path "$WEBDAV_URL$WEBDAV_PATH"
joplin config sync.$SYNC_TARGET_ID.username test
joplin config sync.$SYNC_TARGET_ID.password test123

echo -e "${GREEN}Joplin configured${NC}"
echo ""

# Step 8: Sync with Joplin CLI
echo -e "${YELLOW}Step 8: Syncing with Joplin CLI...${NC}"
joplin sync || {
  echo -e "${RED}Joplin sync failed${NC}"
  exit 1
}
echo -e "${GREEN}Joplin sync completed${NC}"
echo ""

# Step 9: Verify notes in Joplin
echo -e "${YELLOW}Step 9: Verifying notes in Joplin...${NC}"
JOPLIN_NOTES=$(joplin ls | grep -i "test" || true)
if ! echo "$JOPLIN_NOTES" | grep -q "NeoJoplin"; then
  echo -e "${RED}Expected notes not found in Joplin${NC}"
  echo "Notes found: $JOPLIN_NOTES"
  exit 1
fi
echo -e "${GREEN}Notes found in Joplin${NC}"
echo ""

# Step 10: Create note in Joplin and sync back
echo -e "${YELLOW}Step 10: Creating note in Joplin and syncing back...${NC}"
joplin mknote "Note from Joplin" "This note was created in Joplin CLI" || {
  echo -e "${RED}Failed to create note in Joplin${NC}"
  exit 1
}
joplin sync || {
  echo -e "${RED}Joplin sync failed${NC}"
  exit 1
}
echo -e "${GREEN}Joplin note created and synced${NC}"
echo ""

# Step 11: Sync back to NeoJoplin and verify
echo -e "${YELLOW}Step 11: Syncing back to NeoJoplin...${NC}"
cargo run --manifest-path=/home/konrad/gallery/neojoplin/Cargo.toml --bin neojoplin -- \
  --config "$TEST_DIR/neojoplin-config.json" \
  sync || {
  echo -e "${RED}NeoJoplin sync back failed${NC}"
  exit 1
}
echo -e "${GREEN}NeoJoplin sync back completed${NC}"
echo ""

# Step 12: Verify bidirectional sync
echo -e "${YELLOW}Step 12: Verifying bidirectional sync...${NC}"
NEOJOPLIN_NOTES=$(sqlite3 "$NEOJOPLIN_DB" "SELECT COUNT(*) FROM notes;")
if [ "$NEOJOPLIN_NOTES" -lt 3 ]; then
  echo -e "${RED}Expected at least 3 notes in NeoJoplin, found $NEOJOPLIN_NOTES${NC}"
  exit 1
fi

JOPLIN_NOTE=$(sqlite3 "$NEOJOPLIN_DB" "SELECT title FROM notes WHERE title LIKE '%Joplin%' LIMIT 1;")
if ! echo "$JOPLIN_NOTE" | grep -q "Joplin"; then
  echo -e "${RED}Joplin-created note not found in NeoJoplin${NC}"
  exit 1
fi
echo -e "${GREEN}Bidirectional sync verified${NC}"
echo ""

# Step 13: Test deletion sync
echo -e "${YELLOW}Step 13: Testing deletion sync...${NC}"
# Delete a note in NeoJoplin
sqlite3 "$NEOJOPLIN_DB" "DELETE FROM notes WHERE title LIKE '%Another%';"

# Sync deletion
cargo run --manifest-path=/home/konrad/gallery/neojoplin/Cargo.toml --bin neojoplin -- \
  --config "$TEST_DIR/neojoplin-config.json" \
  sync || {
  echo -e "${RED}NeoJoplin sync after deletion failed${NC}"
  exit 1
}

# Verify in Joplin
joplin sync
if joplin ls | grep -q "Another Note"; then
  echo -e "${RED}Deleted note still exists in Joplin${NC}"
  exit 1
fi
echo -e "${GREEN}Deletion sync verified${NC}"
echo ""

# Success!
echo -e "${GREEN}=== ALL TESTS PASSED ===${NC}"
echo ""
echo "Summary:"
echo "  ✓ NeoJoplin database initialization"
echo "  ✓ Note and folder creation"
echo "  ✓ NeoJoplin → WebDAV sync"
echo "  ✓ WebDAV → Joplin sync"
echo "  ✓ Joplin → WebDAV sync"
echo "  ✓ WebDAV → NeoJoplin sync"
echo "  ✓ Bidirectional compatibility verified"
echo "  ✓ Deletion sync verified"
echo ""
echo "The sync implementation is compatible with Joplin CLI!"
