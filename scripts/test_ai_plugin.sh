#!/bin/bash
# Test script for NeoJoplin AI plugin with Ollama

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "=========================================="
echo "NeoJoplin AI Plugin Test"
echo "=========================================="
echo ""

# Step 1: Start Ollama
"$SCRIPT_DIR/start_ollama.sh"

echo ""
echo "Step 2: Building AI plugin..."
cd "$PROJECT_DIR"

# Build in release mode
cargo build -p ai-ollama --release 2>&1 | tail -3

echo ""
echo "Step 3: Setting up plugin directory..."

# Create plugin directories
mkdir -p ~/.config/neojoplin-test/plugins/available/ai-ollama/0.1.0
mkdir -p ~/.config/neojoplin-test/plugins/enabled

# Copy plugin library
PLUGIN_LIB="$PROJECT_DIR/target/release/libai_ollama.so"
if [ -f "$PLUGIN_LIB" ]; then
    cp "$PLUGIN_LIB" ~/.config/neojoplin-test/plugins/available/ai-ollama/0.1.0/
    ln -sf ../../available/ai-ollama/0.1.0/libai_ollama.so \
        ~/.config/neojoplin-test/plugins/enabled/
    echo "✓ Plugin installed"
else
    echo "❌ Plugin library not found at $PLUGIN_LIB"
    echo "Did the build succeed?"
    exit 1
fi

echo ""
echo "Step 4: Testing AI generate command..."
echo ""

# Test the AI generate command
NEOJOPLIN_TEST_MODE=1 cargo run --bin neojoplin -- ai generate "Write a haiku about Rust programming." 2>&1

echo ""
echo "=========================================="
echo "Test complete!"
echo "=========================================="
echo ""
echo "Cleanup:"
echo "  $SCRIPT_DIR/stop_ollama.sh"
