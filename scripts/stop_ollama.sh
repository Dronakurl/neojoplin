#!/bin/bash
# Stop Ollama Docker container for NeoJoplin

CONTAINER_NAME="neojoplin-ollama"

if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
    echo "Stopping container ${CONTAINER_NAME}..."
    docker stop "${CONTAINER_NAME}" > /dev/null
    echo "✓ Container stopped"
else
    echo "Container ${CONTAINER_NAME} is not running"
fi
