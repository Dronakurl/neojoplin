# Use Case 2: Joplin CLI → NeoJoplin Sync

**Description**: Create content in Joplin CLI, sync to WebDAV, download with NeoJoplin

## Steps

1. **Reset Environment**
   - Clear WebDAV server data
   - Reset NeoJoplin database
   - Reset Joplin CLI sync state

2. **Create Content in Joplin CLI**
   ```bash
   cd ~/kjoplin/joplin
   npm run mkbook -- "Joplin Notebook"
   npm run mknote -- "Joplin Note" --body "Created in Joplin CLI"
   ```

3. **Sync from Joplin CLI**
   ```bash
   npm run sync -- --profile test
   ```

4. **Verify WebDAV Content**
   - Check that files exist on WebDAV server
   - Verify JSON structure matches Joplin format

5. **Download with NeoJoplin**
   ```bash
   cd ~/gallery/neojoplin
   cargo run -- sync --url http://localhost:8080/webdav --username test --password test
   cargo run -- ls
   cargo run -- cat "Joplin Note"
   ```

## Expected Results

- ✅ Notebook and note created in Joplin CLI
- ✅ Files uploaded to WebDAV server
- ✅ NeoJoplin can download and display the content
- ✅ Content matches between Joplin CLI and NeoJoplin

## Success Criteria

- Note appears in NeoJoplin list
- Note content is identical
- No sync errors in either application
- Folder structure matches
