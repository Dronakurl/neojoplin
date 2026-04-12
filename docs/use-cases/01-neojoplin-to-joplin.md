# Use Case 1: NeoJoplin → Joplin CLI Sync

**Description**: Create content in NeoJoplin, sync to WebDAV, download with Joplin CLI

## Steps

1. **Reset Environment**
   - Clear WebDAV server data
   - Reset NeoJoplin database
   - Reset Joplin CLI sync state

2. **Create Content in NeoJoplin**
   ```bash
   cargo run -- mk-book "Test Notebook"
   cargo run -- mk-note "Test Note" --body "Created in NeoJoplin"
   ```

3. **Sync from NeoJoplin**
   ```bash
   cargo run -- sync --url http://localhost:8080/webdav --username test --password test
   ```

4. **Verify WebDAV Content**
   - Check that files exist on WebDAV server
   - Verify JSON structure

5. **Download with Joplin CLI**
   ```bash
   cd ~/kjoplin/joplin
   npm run sync -- --profile test
   npm run ls
   ```

## Expected Results

- ✅ Notebook and note created in NeoJoplin database
- ✅ Files uploaded to WebDAV server (/neojoplin/folders/ and /neojoplin/items/)
- ✅ Joplin CLI can download and display the content
- ✅ Content matches between NeoJoplin and Joplin CLI

## Success Criteria

- Note appears in Joplin CLI list
- Note content is identical
- No sync errors in either application
