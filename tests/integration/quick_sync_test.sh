#!/bin/bash
# Quick sync test for NeoJoplin
set -e

echo "=== NeoJoplin Quick Sync Test ==="

# Test configuration
TEST_DB="/tmp/quick-test-$$.db"
TEST_PORT=8082
TEST_URL="http://localhost:$TEST_PORT"
TEST_PATH="/test-sync-$$"

# Cleanup function
cleanup() {
    echo "Cleaning up..."
    pkill -f "python.*$TEST_PORT" 2>/dev/null || true
    rm -f "$TEST_DB"
    echo "Cleanup complete"
}

trap cleanup EXIT

# Step 1: Start minimal WebDAV server
echo "Step 1: Starting WebDAV server on port $TEST_PORT..."
mkdir -p "/tmp/webdav-test-$$"
cd "/tmp/webdav-test-$$"

cat > server.py << 'EOF'
#!/usr/bin/env python3
from http.server import HTTPServer, SimpleHTTPRequestHandler
import sys
import os
import socketserver

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
    port = int(os.environ.get('PORT', 8082))
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

# Step 2: Initialize database
echo "Step 2: Initializing database..."
cargo run --bin neojoplin -- init --db "$TEST_DB" > /dev/null 2>&1 || {
    echo "Failed to initialize database"
    exit 1
}
echo "Database initialized"

# Step 3: Create test data
echo "Step 3: Creating test data..."
cargo run --bin neojoplin -- mkbook "Test Folder" --db "$TEST_DB" > /dev/null 2>&1 || {
    echo "Failed to create folder"
    exit 1
}

cargo run --bin neojoplin -- mknote "Test Note" "Test content from NeoJoplin" --db "$TEST_DB" > /dev/null 2>&1 || {
    echo "Failed to create note"
    exit 1
}
echo "Test data created"

# Step 4: Sync to WebDAV
echo "Step 4: Syncing to WebDAV..."
cargo run --bin neojoplin -- sync --url "$TEST_URL" --username test --password test --remote "$TEST_PATH" --db "$TEST_DB" || {
    echo "Sync failed"
    exit 1
}
echo "Sync completed"

# Step 5: Verify WebDAV contents
echo "Step 5: Verifying WebDAV contents..."
if [ -d "/tmp/webdav-test-$$$TEST_PATH" ]; then
    ITEMS=$(find "/tmp/webdav-test-$$$TEST_PATH" -type f 2>/dev/null | wc -l)
    echo "Found $ITEMS items on WebDAV"
    if [ "$ITEMS" -ge 1 ]; then
        echo "✓ Sync test PASSED"
        exit 0
    else
        echo "✗ No items found on WebDAV"
        exit 1
    fi
else
    echo "✗ Sync directory not found"
    exit 1
fi
