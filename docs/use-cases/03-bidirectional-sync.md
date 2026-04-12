# Use Case 3: Bidirectional Sync

**Description**: Both applications create content, sync bidirectionally without conflicts

## Steps

1. **Reset Environment**
   - Clear WebDAV server data
   - Reset both applications' databases

2. **Create Content in Both Applications**
   ```bash
   # NeoJoplin
   cargo run -- mk-book "NeoJoplin Book"
   cargo run -- mk-note "NeoJoplin Note" --body "From NeoJoplin"

   # Joplin CLI
   cd ~/kjoplin/joplin
   npm run mkbook -- "Joplin Book"
   npm run mknote -- "Joplin Note" --body "From Joplin CLI"
   ```

3. **Sync from Both Applications**
   ```bash
   # Sync NeoJoplin first
   cd ~/gallery/neojoplin
   cargo run -- sync --url http://localhost:8080/webdav --username test --password test

   # Sync Joplin CLI
   cd ~/kjoplin/joplin
   npm run sync -- --profile test

   # Sync NeoJoplin again to get Joplin's changes
   cd ~/gallery/neojoplin
   cargo run -- sync --url http://localhost:8080/webdav --username test --password test
   ```

4. **Verify Both Applications**
   ```bash
   # Check NeoJoplin
   cargo run -- ls

   # Check Joplin CLI
   cd ~/kjoplin/joplin
   npm run ls
   ```

## Expected Results

- ✅ Both notebook sets appear in both applications
- ✅ Both note sets appear in both applications
- ✅ No data loss or corruption
- ✅ Proper conflict resolution (if any)

## Success Criteria

- 2 notebooks in each application
- 2 notes in each application
- Content integrity maintained
- No sync errors or conflicts
