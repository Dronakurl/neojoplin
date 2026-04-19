#!/bin/bash
# Run integration tests for NeoJoplin sync engine
# Requires local WebDAV server to be running

set -e

echo "=== NeoJoplin Integration Tests ==="
echo ""

# Check if WebDAV server is running
if ! curl -s "http://localhost:8080/webdav/" > /dev/null; then
    echo "❌ WebDAV server not running. Starting Docker WebDAV server..."
    docker compose up -d
    sleep 3
    echo "✅ WebDAV server started"
fi

echo "Running integration tests..."
echo ""

# Run each integration test individually with clear output
cd crates/joplin-sync

echo "Test 1: Basic sync operations"
cargo test -- --ignored integration_basic_sync
if [ $? -eq 0 ]; then
    echo "✅ Basic sync test PASSED"
else
    echo "❌ Basic sync test FAILED"
    exit 1
fi

echo ""
echo "Test 2: Data roundtrip"
cargo test -- --ignored integration_roundtrip
if [ $? -eq 0 ]; then
    echo "✅ Roundtrip test PASSED"
else
    echo "❌ Roundtrip test FAILED"
    exit 1
fi

echo ""
echo "Test 3: WebDAV operations"
cargo test -- --ignored integration_webdav_operations
if [ $? -eq 0 ]; then
    echo "✅ WebDAV operations test PASSED"
else
    echo "❌ WebDAV operations test FAILED"
    exit 1
fi

echo ""
echo "Test 4: Error handling"
cargo test -- --ignored integration_error_handling
if [ $? -eq 0 ]; then
    echo "✅ Error handling test PASSED"
else
    echo "❌ Error handling test FAILED"
    exit 1
fi

echo ""
echo "=== All Integration Tests PASSED ==="