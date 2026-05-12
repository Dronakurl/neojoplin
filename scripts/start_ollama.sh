#!/bin/bash
# Start Ollama Docker container for NeoJoplin AI testing
# Uses existing downloaded models from realtypecoach setup

set -e

# Configuration
OLLAMA_VERSION="latest"
CONTAINER_NAME="neojoplin-ollama"
HOST_PORT=11434
MODEL="gemma2:2b"  # Model already downloaded in ollama_data volume

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
    echo "❌ Docker is not running. Please start Docker first."
    exit 1
fi

echo "Starting Ollama Docker container for NeoJoplin..."
echo "Model: $MODEL"
echo "Port: $HOST_PORT"
echo ""

# Check if container exists
if docker ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
    echo "Container ${CONTAINER_NAME} already exists."
    
    # Check if it's running
    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        echo "✓ Container is already running!"
        echo ""
        echo "Ollama API is available at: http://localhost:${HOST_PORT}"
        exit 0
    else
        echo "Starting existing container..."
        docker start "${CONTAINER_NAME}" > /dev/null
    fi
else
    echo "Creating new container..."
    docker run -d \
        --name "${CONTAINER_NAME}" \
        -p "${HOST_PORT}:11434" \
        -v ollama:/root/.ollama \
        -v ollama_data:/data \
        ollama:\${OLLAMA_VERSION} \
        tail -f /dev/null
fi

# Wait for Ollama to be ready
MAX_RETRIES=30
RETRY_DELAY=2
retry_count=0

while ! curl -s http://localhost:${HOST_PORT}/api/tags > /dev/null 2>&1; do
    retry_count=$((retry_count + 1))
    if [ $retry_count -ge $MAX_RETRIES ]; then
        echo "❌ Ollama API not responding after ${MAX_RETRIES} retries"
        echo "Check container logs:"
        echo "  docker logs ${CONTAINER_NAME}"
        exit 1
    fi
    echo "Waiting for Ollama API... (attempt ${retry_count}/${MAX_RETRIES})"
    sleep $RETRY_DELAY
done

echo "✓ Ollama API is ready!"
echo ""

# Check if model is available
if curl -s http://localhost:${HOST_PORT}/api/tags | grep -q "\"name\":\"${MODEL}\""; then
    echo "✓ Model '${MODEL}' is loaded and ready"
else
    echo "Pulling model '${MODEL}'..."
    curl -s -X POST http://localhost:${HOST_PORT}/api/pull \
        -H "Content-Type: application/json" \
        -d "{\"name\": \"${MODEL}\"}" \
        > /dev/null
    
    echo "✓ Model '${MODEL}' pulled successfully"
fi

echo ""
echo "=========================================="
echo "Ollama is running for NeoJoplin AI testing!"
echo "=========================================="
echo ""
echo "API Endpoint: http://localhost:${HOST_PORT}"
echo "Model: ${MODEL}"
echo ""
echo "Test with NeoJoplin:"
echo "  NEOJOPLIN_TEST_MODE=1 cargo run --bin neojoplin -- ai generate \"Hello, world!\""
echo ""
echo "Manage container:"
echo "  docker stop ${CONTAINER_NAME}    - Stop container"
echo "  docker start ${CONTAINER_NAME}   - Start container"
echo "  docker logs ${CONTAINER_NAME}    - View logs"
echo "  docker rm -f ${CONTAINER_NAME}   - Remove container"
echo ""
