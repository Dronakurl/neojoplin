#!/bin/bash
# Simple sync test for NeoJoplin
set -e

echo "=== NeoJoplin Simple Sync Test ==="

# Test configuration
TEST_PORT=8083
TEST_URL="http://localhost:$TEST_PORT"
TEST_PATH="/test-sync-simple"
DEFAULT_DB="$HOME/.local/share/neojoplin/joplin.db"
BACKUP_DB="/tmp/joplin-backup-$$"

# Cleanup function
cleanup() {
    echo "Cleaning up..."
    pkill -f "python.*$TEST_PORT" 2>/dev/null || true
    if [ -f "$BACKUP_DB" ]; then
        mv "$BACKUP_DB" "$DEFAULT_DB"
        echo "Restored original database"
    fi
    echo "Cleanup complete"
}

trap cleanup EXIT

# Step 0: Backup existing database
echo "Step 0: Backing up existing database..."
if [ -f "$DEFAULT_DB" ]; then
    cp "$DEFAULT_DB" "$BACKUP_DB"
    echo "Database backed up"
else
    touch "$BACKUP_DB"  # Mark that we need to restore
fi

# Step 1: Start minimal WebDAV server
echo "Step 1: Starting WebDAV server on port $TEST_PORT..."
mkdir -p "/tmp/webdav-simple-$$"
cd "/tmp/webdav-simple-$$"

cat > server.py << 'EOF'
#!/usr/bin/env python3
from http.server import HTTPServer, SimpleHTTPRequestHandler
import sys
import os

class DualStackServer(HTTPServer):
    def server_bind(self):
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
        self.send_response(207)
        self.send_header('Content-Type', 'application/xml')
        self.end_headers()

        path = self.translate_path(self.path)
        xml = '<?xml version="1.0" encoding="utf-8" ?>\n'
        xml += '<D:multistatus xmlns:D="DAV:">\n'
        xml += '  <D:response><D:href>/</D:href></D:response>\n'
        xml += '</D:multistatus>\n'
        self.wfile.write(xml.encode())

    def log_message(self, format, *args):
        pass  # Suppress logging

if __name__ == '__main__':
    import socketserver
    port = int(os.environ.get('PORT', 8083))
    server = DualStackServer(('localhost', port), WebDAVHandler)
    server.serve_forever()
EOF

chmod +x server.py
python3 server.py &
WEBDAV_PID=$!
sleep 2

if ! kill -0 $WEBDAV_PID 2>/dev/null; then
    echo "Failed to start WebDAV server"
    exit 1
fi
echo "WebDAV server started (PID: $WEBDAV_PID)"

# Step 2: Initialize fresh database
echo "Step 2: Initializing fresh database..."
rm -f "$DEFAULT_DB"
cargo run --bin neojoplin -- init > /dev/null 2>&1 || {
    echo "Failed to initialize database"
    exit 1
}
echo "Database initialized at $DEFAULT_DB"

# Step 3: Create test data
echo "Step 3: Creating test data..."
cargo run --bin neojoplin -- mk-book "Test Folder" > /dev/null 2>&1 || {
    echo "Failed to create folder"
    exit 1
}

cargo run --bin neojoplin -- mk-note "Test Note" "Test content from NeoJoplin" > /dev/null 2>&1 || {
    echo "Failed to create note"
    exit 1
}
echo "Test data created"

# Step 4: Sync to WebDAV
echo "Step 4: Syncing to WebDAV at $TEST_URL$TEST_PATH..."
cargo run --bin neojoplin -- sync --url "$TEST_URL" --username test --password test --remote "$TEST_PATH" || {
    echo "Sync failed"
    exit 1
}
echo "Sync completed"

# Step 5: Verify WebDAV contents
echo "Step 5: Verifying WebDAV contents..."
if [ -d "/tmp/webdav-simple-$$$TEST_PATH" ]; then
    ITEMS=$(find "/tmp/webdav-simple-$$$TEST_PATH" -type f 2>/dev/null | wc -l)
    echo "Found $ITEMS items on WebDAV"
    if [ "$ITEMS" -ge 1 ]; then
        echo "✅ Sync test PASSED"
        echo ""
        echo "Summary:"
        echo "  ✓ Database initialization"
        echo "  ✓ Note and folder creation"
        echo "  ✓ WebDAV sync (upload)"
        echo "  ✓ File verification on WebDAV"
        exit 0
    else
        echo "✗ No items found on WebDAV"
        exit 1
    fi
else
    echo "✗ Sync directory not found at /tmp/webdav-simple-$$$TEST_PATH"
    exit 1
fi
