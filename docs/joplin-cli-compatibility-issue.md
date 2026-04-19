# Joplin CLI Compatibility Investigation

## Critical Discovery: E2EE Mismatch (FIXED)

### Problem Identified
There was a **critical mismatch** between the encryption settings in the two metadata files:

- **`sync.json`**: `"e2ee": { "value": false }` (E2EE disabled)
- **`info.json`**: `"e2ee": { "value": true }` (E2EE enabled)

### Impact
Joplin CLI uses `info.json` to determine if E2EE is enabled, while NeoJoplin was only updating `sync.json`. This caused:

1. Joplin CLI to expect encrypted items
2. When it found unencrypted NeoJoplin items, it failed to decrypt them silently
3. Items were never downloaded to Joplin's local database

### Fix Applied
Manually updated `info.json` to disable E2EE:
```bash
curl -s http://localhost:8080/webdav/real-e2ee-encryption-test/info.json | \
  jq '.e2ee.value = false' | \
  curl -X PUT "http://localhost:8080/webdav/real-e2ee-encryption-test/info.json" --data-binary @-
```

### Result
After fixing the E2EE mismatch, Joplin CLI sync behavior changed:
- **Before**: "Created remote items: 1." (minimal activity)
- **After**: "Created remote items: 8. Fetched items: 10/10." (much more activity)

## Remaining Issue: Items Still Not Downloading

Even after fixing the E2EE mismatch, **Joplin CLI still does not download NeoJoplin items**.

### Symptoms
- Items are correctly uploaded to WebDAV by NeoJoplin
- Items are properly formatted in Joplin's markdown format
- Joplin CLI reports sync activity but items don't appear in local database
- `sqlite3 ~/.config/joplin/database.sqlite "SELECT * FROM notes"` still shows only original Joplin notes

### Test Results

#### Test 1: Direct WebDAV Item Creation
Created test items directly on WebDAV with various formats:
- Plain text items
- Items with/without parent_id
- Items with different timestamps
- Items matching exact format of working Joplin items

**Result**: None were downloaded by Joplin CLI

#### Test 2: Database Reset
Deleted all `sync_items` from Joplin's database to force full resync:
```bash
sqlite3 ~/.config/joplin/database.sqlite "DELETE FROM sync_items;"
joplin sync
```

**Result**: Sync completed but no NeoJoplin items were downloaded

#### Test 3: Timestamp Analysis
Checked if timestamp mismatch was preventing downloads:
- NeoJoplin item: `updated_time: 2026-04-19T14:04:13.299Z` (1776606253299 ms)
- Joplin sync_time: `1776606680324` (more recent)
- **Analysis**: Joplin has more recent sync_time than NeoJoplin items, so it doesn't download them

### Potential Root Causes

1. **Sync Direction Confusion**: Joplin might be using sync timestamps incorrectly, looking for items AFTER its last sync instead of items that need to be downloaded

2. **Item Type Filtering**: Joplin might be filtering items based on some criteria we're not meeting (item_type, source_application, etc.)

3. **ID Validation**: Joplin might be rejecting items with certain ID patterns or formats

4. **Required Metadata**: Joplin might require additional fields or metadata that we're not providing

5. **Delta Algorithm Bug**: There might be a bug in how Joplin implements the delta sync algorithm when dealing with items from multiple clients

## NeoJoplin Implementation Gaps

### Missing `info.json` Management
NeoJoplin updates `sync.json` but does **not** update `info.json`. This needs to be fixed:

**Required Changes**:
1. Add `info.json` reading/writing to NeoJoplin's sync engine
2. Ensure both `sync.json` and `info.json` have consistent E2EE settings
3. Update both files when sync configuration changes

### Missing Delta Timestamp Context
The current implementation might not be properly handling the sync context for cross-client synchronization.

## Recommended Next Steps

1. **Implement `info.json` Management**
   - Add code to read/write `info.json` in sync operations
   - Ensure E2EE settings are consistent between `sync.json` and `info.json`
   - Test with fresh WebDAV targets

2. **Investigate Joplin CLI Source Code**
   - Study how Joplin CLI determines which items to download
   - Understand the delta sync algorithm implementation
   - Identify what makes an item "downloadable" from Joplin's perspective

3. **Alternative Approach: Shared Database**
   - Since both apps support the same database schema, test if they can share the same database file
   - This would bypass the sync protocol entirely for testing purposes

4. **Protocol Level Debugging**
   - Add extensive logging to NeoJoplin's sync engine
   - Compare exact HTTP requests/responses between Joplin and NeoJoplin
   - Use Wireshark or similar to analyze the sync protocol at network level

## Test Commands

### Verify E2EE Status
```bash
# Check sync.json
curl -s http://localhost:8080/webdav/TARGET/sync.json | jq .e2ee

# Check info.json  
curl -s http://localhost:8080/webdav/TARGET/info.json | jq .e2ee

# They should match!
```

### Force Full Resync in Joplin CLI
```bash
sqlite3 ~/.config/joplin/database.sqlite "DELETE FROM sync_items;"
joplin sync
```

### Create Test Items
```bash
# Create test note directly on WebDAV
cat > /tmp/test.md << 'EOF'
Test Note
id: test-id-123
parent_id: PARENT_FOLDER_ID
created_time: 2026-04-19T14:00:00.000Z
updated_time: 2026-04-19T14:00:00.000Z
is_conflict: 0
latitude: 0.00000000
longitude: 0.00000000
altitude: 0.0000
author: 
source_url: 
is_todo: 0
todo_due: 0
todo_completed: 0
source: 
source_application: 
application_data: 
order: 0
user_created_time: 0
user_updated_time: 0
encryption_cipher_text: 
encryption_applied: 0
markup_language: 1
is_shared: 0
share_id: 
conflict_original_id: 
master_key_id: 
user_data: 
deleted_time: 0
type_: 1

Test body content
EOF

curl -X PUT "http://localhost:8080/webdav/TARGET/items/test-id-123.md" --data-binary @/tmp/test.md
```

## Conclusion

The **E2EE mismatch issue has been identified and fixed**, but **Joplin CLI still doesn't download NeoJoplin items**. The root cause appears to be deeper in the sync algorithm or how Joplin validates items for download.

Further investigation of Joplin CLI's source code or network-level protocol analysis is needed to fully resolve this compatibility issue.
