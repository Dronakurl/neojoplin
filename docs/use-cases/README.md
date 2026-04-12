# NeoJoplin ↔ Joplin CLI Sync Test Cases

This directory contains test cases for verifying bidirectional sync compatibility between NeoJoplin and the reference Joplin CLI implementation.

## Prerequisites

1. **Local WebDAV Server**
   ```bash
   just webdav-server
   ```

2. **Joplin CLI Setup**
   - Located at: `~/kjoplin/joplin`
   - Must be compiled and configured
   - Test profile configured to use local WebDAV

3. **NeoJoplin Setup**
   - Located at: `~/gallery/neojoplin`
   - Database at: `~/.local/share/neojoplin/joplin.db`

## Test Cases

### Use Case 1: NeoJoplin → Joplin CLI
[./01-neojoplin-to-joplin.md](./01-neojoplin-to-joplin.md)

Test creating content in NeoJoplin and syncing to Joplin CLI.

### Use Case 2: Joplin CLI → NeoJoplin
[./02-joplin-to-neojoplin.md](./02-joplin-to-neojoplin.md)

Test creating content in Joplin CLI and syncing to NeoJoplin.

### Use Case 3: Bidirectional Sync
[./03-bidirectional-sync.md](./03-bidirectional-sync.md)

Test both applications creating content and syncing bidirectionally.

### Use Case 4: Edit and Sync
[./04-edit-and-sync.md](./04-edit-and-sync.md)

Test editing content and syncing changes between applications.

## Running Tests

Execute each test case in order, verifying success criteria before proceeding.

## Cleanup Commands

```bash
# Reset NeoJoplin database
rm ~/.local/share/neojoplin/joplin.db
cargo run -- init

# Reset Joplin CLI database
rm ~/.config/joplin/test/profile.sqlite
cd ~/kjoplin/joplin && npm run sync -- --profile test

# Clear WebDAV server
docker exec neojoplin-webdav-1 rm -rf /srv/webdav/*
```

## Success Metrics

- All test cases pass
- No data loss or corruption
- Consistent state across applications
- Proper conflict resolution
- Timestamp-based sync working correctly
