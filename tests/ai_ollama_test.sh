#!/bin/bash
# Integration test for NeoJoplin AI chat with Ollama
# This test verifies that the AI generate command works with a local Ollama server

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

pass() {
    echo -e "${GREEN}✓${NC} $1"
}

fail() {
    echo -e "${RED}✗${NC} $1"
    cleanup
    exit 1
}

info() {
    echo -e "${YELLOW}ℹ${NC} $1"
}

header() {
    echo ""
    echo "=========================================="
    echo "$1"
    echo "=========================================="
    echo ""
}

cleanup() {
    info "Cleaning up test data..."
    rm -rf ~/.local/share/neojoplin-test/ 2>/dev/null || true
}

# Check if Ollama container is running
check_ollama() {
    if ! curl -s http://localhost:11434/api/tags > /dev/null 2>&1; then
        return 1
    fi
    return 0
}

# Start Ollama if not running
start_ollama() {
    if check_ollama; then
        info "Ollama is already running at http://localhost:11434"
    else
        info "Starting Ollama Docker container..."
        "$PROJECT_DIR/scripts/start_ollama.sh" || fail "Failed to start Ollama"
    fi
}

# Test 1: Basic AI generate command
# Uses NEOJOPLIN_TEST_MODE to isolate test data
test_ai_generate() {
    header "Test 1: AI Generate Command"
    
    info "Testing AI generate with a simple prompt..."
    
    local output
    output=$(NEOJOPLIN_TEST_MODE=1 NEOJOPLIN_AI_PROVIDER=ollama \
        OLLAMA_BASE_URL=http://127.0.0.1:11434 \
        OLLAMA_MODEL=gemma2:2b \
        cargo run --quiet --bin neojoplin -- ai generate "What is 2+2?" 2>&1)
    
    # Check if the command succeeded and produced output
    if echo "$output" | grep -q "4"; then
        pass "AI generate command works and produces expected output"
    else
        fail "AI generate command failed or produced unexpected output:\n$output"
    fi
    
    echo ""
}

# Test 2: AI generate with note search context
# Tests that AI can search note TITLES and find relevant notes
test_ai_with_notes() {
    header "Test 2: AI Generate with Note Title Search"
    
    info "Creating a test note with specific title..."
    
    # Create a test note with a unique title
    NEOJOPLIN_TEST_MODE=1 cargo run --quiet --bin neojoplin -- init > /dev/null 2>&1
    NEOJOPLIN_TEST_MODE=1 cargo run --quiet --bin neojoplin -- mknote "Rust Programming Guide" \
        --body "Rust is a systems programming language that runs blazingly fast, prevents segfaults, and guarantees thread safety." > /dev/null 2>&1
    
    info "Asking AI about Rust programming..."
    
    local output
    output=$(NEOJOPLIN_TEST_MODE=1 NEOJOPLIN_AI_PROVIDER=ollama \
        OLLAMA_BASE_URL=http://127.0.0.1:11434 \
        OLLAMA_MODEL=gemma2:2b \
        cargo run --quiet --bin neojoplin -- ai generate "What is Rust programming?" 2>&1)
    
    # Check if AI found information from the note
    if echo "$output" | grep -qi "systems\|fast\|segfault\|thread safety"; then
        pass "AI successfully found and included note context from title search"
    else
        fail "AI did not find the Rust note:\n$output"
    fi
    
    echo ""
}

# Test 3: AI generate with content search
# Tests that AI can search note CONTENT (not just titles) for relevant information
test_ai_content_search() {
    header "Test 3: AI Generate with Note Content Search"
    
    info "Creating a note with unique content..."
    
    # Create a note with unique content that's unlikely to appear in AI training data
    # Using a unique identifier in the body
    NEOJOPLIN_TEST_MODE=1 cargo run --quiet --bin neojoplin -- mknote "Meeting Minutes" \
        --body "The secret project code is ALPHA-BRAVO-CHARLIE. This is a confidential identifier." > /dev/null 2>&1
    
    info "Asking AI about the secret project code..."
    
    local output
    output=$(NEOJOPLIN_TEST_MODE=1 NEOJOPLIN_AI_PROVIDER=ollama \
        OLLAMA_BASE_URL=http://127.0.0.1:11434 \
        OLLAMA_MODEL=gemma2:2b \
        cargo run --quiet --bin neojoplin -- ai generate "What is the secret project code?" 2>&1)
    
    # Check if AI found the unique content from the note
    if echo "$output" | grep -qi "ALPHA-BRAVO-CHARLIE"; then
        pass "AI successfully found and included note content (ALPHA-BRAVO-CHARLIE)"
    else
        fail "AI did not find the secret code in note content:\n$output"
    fi
    
    echo ""
}

# Test 3: AI summarize command
test_ai_summarize() {
    header "Test 3: AI Summarize Command"
    
    info "Creating a note to summarize..."
    
    NEOJOPLIN_TEST_MODE=1 cargo run --quiet --bin neojoplin -- mknote "Long Note" \
        --body "This is a long note with multiple sentences. It contains several paragraphs. The main point is that AI should be able to summarize it. This tests the summarization capability." > /dev/null 2>&1
    
    info "Asking AI to summarize the note..."
    
    local output
    output=$(NEOJOPLIN_TEST_MODE=1 NEOJOPLIN_AI_PROVIDER=ollama \
        OLLAMA_BASE_URL=http://127.0.0.1:11434 \
        OLLAMA_MODEL=gemma2:2b \
        cargo run --quiet --bin neojoplin -- ai summarize "Long Note" 2>&1)
    
    # Check if summarization worked
    if echo "$output" | grep -qi "summar\|long note\|main point"; then
        pass "AI summarize command works"
    else
        fail "AI summarize command failed:\n$output"
    fi
    
    echo ""
}

# Main test execution
main() {
    echo ""
    header "NeoJoplin AI Ollama Integration Tests"
    
    # Ensure we're in the project directory
    cd "$PROJECT_DIR"
    
    # Start Ollama
    start_ollama
    
    # Run tests
    test_ai_generate
    test_ai_with_notes
    test_ai_content_search
    test_ai_summarize
    
    header "All AI Ollama tests passed!"
    
    # Clean up test data
    cleanup
    
    echo ""
    info "Note: Ollama container is still running. To stop it, run:"
    info "  ./scripts/stop_ollama.sh"
    echo ""
}

# Run main and cleanup on error
main
exit_code=$?
cleanup
exit $exit_code
