# Local WebDAV Server for Testing

This directory contains Docker setup for running a local WebDAV server for testing NeoJoplin sync functionality.

## Quick Start

```bash
# Start the WebDAV server
just webdav-server

# Or manually with docker-compose
docker-compose up -d webdav

# Test the connection
just test-local-webdav

# View logs
just webdav-logs

# Stop the server
just webdav-stop
```

## Server Details

- **URL**: http://localhost:8080/webdav
- **Test Credentials**: test / test
- **Data Directory**: Docker volume `webdav-data`

## Usage with NeoJoplin

```bash
# Sync with local WebDAV server
cargo run -- sync --url http://localhost:8080/webdav --username test --password test
```

## WebDAV Testing

Use the `webdav-test` binary to test any WebDAV server:

```bash
# Test local server
just test-local-webdav

# Test remote server
just test-webdav https://webdav.example.com username password
```

## Docker Compose

The `docker-compose.yml` file defines a simple WebDAV server using Caddy with the WebDAV module.

The server:
- Listens on port 8080
- Serves WebDAV at `/webdav` path
- Stores data in a named volume
- No authentication (for testing only!)
