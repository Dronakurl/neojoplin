# Use Case 4: Edit Content and Sync Changes

**Description**: Edit existing note in one application, sync changes to other application

## Steps

1. **Setup Initial State**
   - Create test content in NeoJoplin
   - Sync to WebDAV
   - Download in Joplin CLI

2. **Edit Content in NeoJoplin**
   ```bash
   cd ~/gallery/neojoplin
   cargo run -- edit "Test Note" --body "Updated content from NeoJoplin"
   cargo run -- sync --url http://localhost:8080/webdav --username test --password test
   ```

3. **Download Changes in Joplin CLI**
   ```bash
   cd ~/kjoplin/joplin
   npm run sync -- --profile test
   npm run cat -- "Test Note"
   ```

4. **Edit Content in Joplin CLI**
   ```bash
   cd ~/kjoplin/joplin
   npm run edit -- "Test Note" --body "Updated from Joplin CLI"
   npm run sync -- --profile test
   ```

5. **Download Changes in NeoJoplin**
   ```bash
   cd ~/gallery/neojoplin
   cargo run -- sync --url http://localhost:8080/webdav --username test --password test
   cargo run -- cat "Test Note"
   ```

## Expected Results

- ✅ Edits in NeoJoplin appear in Joplin CLI
- ✅ Edits in Joplin CLI appear in NeoJoplin
- ✅ updated_time timestamps properly updated
- ✅ No duplicate notes created
- ✅ Correct conflict resolution if both edit same note

## Success Criteria

- Content changes propagate correctly
- Timestamps determine which version wins
- No data loss
- No duplicate entries
