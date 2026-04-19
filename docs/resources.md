# Resource Support Implementation Plan

## Overview

Resources are file attachments (images, PDFs, documents) that can be attached to notes in Joplin. NeoJoplin currently does not support resources, but implementing them is essential for full compatibility with the Joplin CLI.

## What are Resources?

**Resources** are binary file attachments associated with notes:
- Images (`.jpg`, `.png`, `.gif`, etc.)
- Documents (`.pdf`, `.docx`, etc.)
- Audio files (`.mp3`, `.wav`, etc.)
- Any other file types users want to attach

**Joplin CLI usage:**
```bash
joplin attach <note> <file>     # Attach a file to a note
joplin ls                        # Shows notes with attachments
```

## Current State

### ✅ Implemented
- Resource data model exists in `crates/core/src/domain.rs`
- Database schema includes `resources` table
- Sync engine has resource type enum (`ItemType::Resource`)
- Resource handling stub in `store_downloaded_item()` (returns "not yet implemented" error)

### ❌ Missing
- No storage methods (create_resource, get_resource, etc.)
- no binary file handling
- No sync serialization/deserialization
- No CLI commands for attachments
- Resources are skipped during sync

## Database Schema

### Resources Table
```sql
CREATE TABLE resources (
    id TEXT PRIMARY KEY,
    title TEXT,
    filename TEXT,
    file_extension TEXT,
    mime TEXT,
    size INTEGER DEFAULT -1,
    created_time INTEGER NOT NULL,
    updated_time INTEGER NOT NULL,
    user_created_time INTEGER DEFAULT 0,
    user_updated_time INTEGER DEFAULT 0,
    blob_updated_time INTEGER DEFAULT 0,
    encryption_cipher_text TEXT DEFAULT "",
    encryption_applied INTEGER DEFAULT 0,
    encryption_blob_encrypted INTEGER DEFAULT 0,
    share_id TEXT DEFAULT "",
    master_key_id TEXT DEFAULT "",
    is_shared INTEGER DEFAULT 0
);
```

### Note Resources Junction Table
```sql
CREATE TABLE note_resources (
    id TEXT PRIMARY KEY,
    user_updated_time INTEGER DEFAULT 0,
    note_id TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    is_associated INTEGER DEFAULT 0
);
```

## Resource Model

```rust
pub struct Resource {
    pub id: String,
    pub title: String,
    pub filename: String,
    pub file_extension: String,
    pub mime: String,
    pub size: i64,
    pub created_time: i64,
    pub updated_time: i64,
    pub user_created_time: i64,
    pub user_updated_time: i64,
    pub blob_updated_time: i64,
    pub encryption_cipher_text: String,
    pub encryption_applied: i32,
    pub encryption_blob_encrypted: i32,
    pub share_id: String,
    pub master_key_id: String,
    pub is_shared: i32,
}
```

## Implementation Plan

### Phase 1: Storage Layer
**File:** `crates/storage/src/lib.rs`

Add to `Storage` trait:
```rust
// Resource operations
async fn create_resource(&self, resource: &Resource) -> Result<(), DatabaseError>;
async fn get_resource(&self, id: &str) -> Result<Option<Resource>, DatabaseError>;
async fn update_resource(&self, resource: &Resource) -> Result<(), DatabaseError>;
async fn delete_resource(&self, id: &str) -> Result<(), DatabaseError>;
async fn list_resources(&self) -> Result<Vec<Resource>, DatabaseError>;
async fn get_resources_by_note(&self, note_id: &str) -> Result<Vec<Resource>, DatabaseError>;

// Note-Resource associations
async fn add_note_resource(&self, note_resource: &NoteResource) -> Result<(), DatabaseError>;
async fn remove_note_resource(&self, note_id: &str, resource_id: &str) -> Result<(), DatabaseError>;
async fn get_note_resources(&self, note_id: &str) -> Result<Vec<Resource>, DatabaseError>;
```

Implement in `SqliteStorage`:
- Standard CRUD operations for resources table
- Junction table operations for note_resources
- Handle resource metadata separately from binary blob

### Phase 2: Binary File Storage
**Directory:** `~/.local/share/neojoplin/resources/`

Store binary blobs separately from database:
- Path structure: `resources/<resource_id>.<file_extension>`
- Database stores metadata only
- Binary files stored on filesystem
- Handle large file uploads/downloads

### Phase 3: Sync Layer
**File:** `crates/sync/src/sync_engine.rs`

Add resource serialization:
```rust
fn serialize_resource(&self, resource: &Resource) -> Result<String> {
    // Convert resource to JSON format for WebDAV upload
    // Include metadata only, not binary blob
}

async fn serialize_resource_blob(&self, resource_id: &str) -> Result<Vec<u8>> {
    // Read binary blob from filesystem
    // Return bytes for WebDAV upload
}
```

Add resource deserialization:
```rust
fn deserialize_resource(&self, id: &str, content: &str) -> Result<Resource> {
    // Parse JSON metadata from WebDAV
    // Return Resource struct
}

async fn download_resource_blob(&self, resource_id: &str, extension: &str) -> Result<Vec<u8>> {
    // Download binary blob from WebDAV
    // Save to resources directory
}
```

Update sync engine:
- Include resources in UPLOAD phase
- Include resources in DELTA phase (download)
- Handle `/resources/` directory in WebDAV
- Manage resource blob files separately

### Phase 4: CLI Commands
**File:** `crates/cli/src/main.rs`

Add new commands:
```rust
// Attach file to note
Attach {
    note_id: String,
    file_path: String,
}

// List resources for a note
ListResources {
    note_id: String,
}

// Detach resource from note
Detach {
    note_id: String,
    resource_id: String,
}

// Download resource blob
DownloadResource {
    resource_id: String,
    output_path: String,
}
```

### Phase 5: WebDAV Integration
**Paths:**
- Metadata: `https://webdav.server/neojoplin/resources/<id>.md`
- Binary blob: `https://webdav.server/neojoplin/resources/<id>.<ext>`

**Flow:**
1. Upload metadata JSON to `<id>.md`
2. Upload binary blob to `<id>.<ext>`
3. Download both during sync
4. Handle large files with progress reporting

## WebDAV Resource Structure

### Upload Format
**Metadata file** (`resources/<resource_id>.md`):
```json
{
    "id": "abc123...",
    "title": "My Image",
    "filename": "photo.jpg",
    "file_extension": ".jpg",
    "mime": "image/jpeg",
    "size": 102400,
    "created_time": 1234567890000,
    "updated_time": 1234567890000,
    ...
}
```

**Binary file** (`resources/<resource_id>.jpg`):
- Raw binary data from the original file

### Download Process
1. List `/resources/` directory during DELTA phase
2. Download `.md` files for metadata
3. Download binary files separately
4. Store metadata in database
5. Store binary blobs in `resources/` directory

## Technical Considerations

### Large File Handling
- Use streaming for large file uploads/downloads
- Report progress for binary transfers
- Handle network timeouts gracefully
- Support resume functionality for large files

### Encryption
- Resources support E2EE (End-to-End Encryption)
- Need to handle `encryption_blob_encrypted` flag
- Encrypt/decrypt binary blobs when E2EE enabled
- Use JED format (same as notes)

### Performance
- Lazy load resource blobs (only when needed)
- Cache frequently accessed resources
- Batch resource operations during sync
- Use async I/O for file operations

### Compatibility
- Must match Joplin's resource handling exactly
- Support same file types and MIME types
- Handle resource references in note body
- Maintain note-resource associations

## Testing Plan

### Unit Tests
- Resource CRUD operations
- Note-resource associations
- Serialization/deserialization
- File I/O operations

### Integration Tests
- Upload resources to WebDAV
- Download resources from WebDAV
- Cross-client sync with Joplin CLI
- Large file handling

### Manual Tests
```bash
# Test attachment
~/.local/bin/neojoplin attach <note-id> /path/to/image.jpg

# Test listing
~/.local/bin/neojoplin ls-resources <note-id>

# Test sync with resources
~/.local/bin/neojoplin sync --url http://localhost:8080/webdav --remote /test

# Test cross-client compatibility
joplin attach <note-id> /path/to/file.pdf
joplin sync
~/.local/bin/neojoplin sync  # Should download the resource
```

## Dependencies

### Rust Crates
- `mime` - MIME type detection
- `mime_guess` - Guess MIME types from file extensions
- `walkdir` - Traverse resource directories
- `tempfile` - Temporary file handling

### System Requirements
- Disk space for resource storage
- Memory for large file operations
- Network bandwidth for sync

## Success Criteria

✅ Resources can be attached to notes via CLI
✅ Resources sync correctly with WebDAV
✅ Binary files are stored correctly
✅ Cross-client compatibility with Joplin CLI
✅ Large files (10MB+) handled smoothly
✅ E2EE support for encrypted resources

## References

- Joplin database schema: `~/gallery/kjoplin/docs/database.md`
- Resource model: `crates/core/src/domain.rs`
- Sync engine: `crates/sync/src/sync_engine.rs`
- Storage trait: `crates/core/src/traits.rs`

## Next Steps

1. Start with Phase 1 (Storage Layer)
2. Test resource CRUD operations
3. Implement Phase 2 (Binary Storage)
4. Add sync support (Phase 3)
5. Finally add CLI commands (Phase 4)

**Priority:** Medium - Core sync functionality works without resources, but full Joplin compatibility requires them.
