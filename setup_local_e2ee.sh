#!/bin/bash
# Setup script for local WebDAV E2EE testing
# This configures both neojoplin and joplin CLI to use the local WebDAV server with E2EE

set -e

# Load E2EE password from .env
if [ -f .env ]; then
    source .env
else
    echo "Error: .env file not found"
    exit 1
fi

if [ -z "$E2EE_PASSWORD" ]; then
    echo "Error: E2EE_PASSWORD not set in .env file"
    exit 1
fi

WEBDAV_URL="http://localhost:8080/webdav/"
REMOTE_PATH="/local-e2ee-test"
NEOJOPLIN_BIN="$HOME/.local/bin/neojoplin"

echo "=== Setting up local WebDAV E2EE testing ==="
echo "WebDAV URL: $WEBDAV_URL"
echo "Remote path: $REMOTE_PATH"
echo "E2EE password: $E2EE_PASSWORD"
echo ""

# Check if Docker WebDAV server is running
echo "Checking if Docker WebDAV server is running..."
if ! docker ps | grep -q "neojoplin-webdav-1"; then
    echo "Starting Docker WebDAV server..."
    docker compose up -d
    sleep 3
else
    echo "✅ Docker WebDAV server is already running"
fi

# Test WebDAV connection
echo ""
echo "Testing WebDAV connection..."
if curl -s -X PROPFIND "$WEBDAV_URL" -H "Depth: 1" > /dev/null; then
    echo "✅ WebDAV server is accessible"
else
    echo "❌ WebDAV server is not accessible"
    exit 1
fi

# Setup NeoJoplin
echo ""
echo "=== Setting up NeoJoplin ==="
rm -rf ~/.local/share/neojoplin/joplin.db
mkdir -p ~/.local/share/neojoplin

# Initialize database
echo "Initializing NeoJoplin database..."
$NEOJOPLIN_BIN init > /dev/null 2>&1

# Create sync configuration
echo "Configuring NeoJoplin sync..."
NEOJOPLIN_DATA_DIR="$HOME/.local/share/neojoplin"
cat > "$NEOJOPLIN_DATA_DIR/sync-config.json" << EOF
{
  "type": "webdav",
  "url": "$WEBDAV_URL",
  "remote_path": "$REMOTE_PATH"
}
EOF

echo "✅ NeoJoplin configured for WebDAV sync"

# Setup Joplin CLI
echo ""
echo "=== Setting up Joplin CLI ==="
# Configure Joplin CLI (remove trailing slash for Joplin CLI)
JOPLIN_WEBDAV_URL="${WEBDAV_URL%/}"  # Remove trailing slash
echo "Configuring Joplin CLI sync..."
joplin config sync.target 6
joplin config sync.6.path "$JOPLIN_WEBDAV_URL$REMOTE_PATH"
joplin config sync.6.username ""
joplin config sync.6.password ""

echo "✅ Joplin CLI configured for WebDAV sync"

# Create test data in NeoJoplin
echo ""
echo "=== Creating test data in NeoJoplin ==="
FOLDER_ID=$($NEOJOPLIN_BIN mk-book "E2EE Test Notebook" | grep -oP '(?<=\().*?(?=\))')
echo "Created folder: $FOLDER_ID"

NOTE_ID=$($NEOJOPLIN_BIN mk-note "E2EE Test Note" --body "This is a test note for E2EE functionality" --parent $FOLDER_ID | grep -oP '(?<=\().*?(?=\))')
echo "Created note: $NOTE_ID"

# First sync without E2EE to establish baseline
echo ""
echo "=== Initial sync without E2EE ==="
echo "Syncing NeoJoplin to WebDAV..."
$NEOJOPLIN_BIN sync --url "$WEBDAV_URL" --remote "$REMOTE_PATH" 2>&1 | grep -E "(Sync|Phase)" || true

echo "Syncing Joplin CLI..."
joplin sync 2>&1 | grep -E "(Synchronization|Completed)" || true

# Enable E2EE in Joplin CLI (using the password from .env)
echo ""
echo "=== Enabling E2EE in Joplin CLI ==="
echo "Using password from .env file..."

# Note: This would normally require interactive input
# For automated testing, we're setting up the infrastructure
echo "⚠️  E2EE requires interactive password setup in Joplin CLI"
echo "⚠️  To manually enable E2EE in Joplin CLI, run:"
echo "   joplin e2ee:enable"
echo "   (and enter password: $E2EE_PASSWORD)"

# For now, let's test that the sync works without E2EE
echo ""
echo "=== Testing sync compatibility ==="
echo "Creating additional note in Joplin CLI..."
joplin mkbook "Joplin E2EE Test"
joplin mknote "From Joplin CLI" "This note was created in Joplin CLI" --book "Joplin E2EE Test"

echo "Syncing Joplin CLI to WebDAV..."
joplin sync 2>&1 | grep -E "(Synchronization|Completed)" || true

echo "Syncing NeoJoplin from WebDAV..."
$NEOJOPLIN_BIN sync --url "$WEBDAV_URL" --remote "$REMOTE_PATH" 2>&1 | grep -E "(Sync|Phase)" || true

# Verify data integrity
echo ""
echo "=== Verifying data integrity ==="
FOLDER_COUNT=$($NEOJOPLIN_BIN list-books | wc -l)
NOTE_COUNT=$(sqlite3 -list ~/.local/share/neojoplin/joplin.db "SELECT COUNT(*) FROM notes;")

echo "NeoJoplin folders: $FOLDER_COUNT"
echo "NeoJoplin notes: $NOTE_COUNT"

if [ "$FOLDER_COUNT" -ge 2 ] && [ "$NOTE_COUNT" -ge 2 ]; then
    echo "✅ Data exchange working correctly"
else
    echo "❌ Data exchange failed"
    exit 1
fi

echo ""
echo "=== Setup Complete ==="
echo "✅ Both NeoJoplin and Joplin CLI are configured for local WebDAV sync"
echo "✅ E2EE password is set in .env file: $E2EE_PASSWORD"
echo "✅ Initial sync test passed"
echo ""
echo "To manually enable E2EE:"
echo "1. NeoJoplin: Use the TUI (press 'S' for settings, then Encryption tab)"
echo "2. Joplin CLI: Run 'joplin e2ee:enable' and enter password"
echo ""
echo "Test commands:"
echo "- NeoJoplin: $NEOJOPLIN_BIN sync --url $WEBDAV_URL --remote $REMOTE_PATH"
echo "- Joplin CLI: joplin sync"
