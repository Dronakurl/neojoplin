# NeoJoplin Settings Dialog Implementation Plan

## Phase 1: Configuration Architecture (Foundation)

### 1.1 Consolidate Config Systems
**Problem:** NeoJoplin has two different config systems:
- `crates/core/src/config.rs` - Comprehensive, JSON-based, has `SyncTarget` enum
- `crates/tui/src/config.rs` - Simpler, TOML-based, WebDAV-focused

**Solution:**
1. **Use `crates/core/src/config.rs` as the base** - it already has `SyncTarget` enum
2. **Add WebDAV-specific settings** following Joplin's pattern:
   ```rust
   sync.6.path: String,     // WebDAV URL
   sync.6.username: String,
   sync.6.password: String, // secure
   ```
3. **Maintain Joplin compatibility** - use same setting names and format
4. **Remove `crates/tui/src/config.rs`** after migration

### 1.2 Settings Storage Architecture
```rust
// ~/.config/neojoplin/settings.json (Joplin-compatible format)
{
  "$schema": "https://joplinapp.org/schema/settings.json",
  "sync.target": 6,                    // Active sync target ID
  "sync.6.path": "https://webdav.example.com/",
  "sync.6.username": "user@example.com",
  "sync.6.password": "encrypted_password",
  "sync.6.ignore_tls_errors": false,
  "sync.interval": 0,                   // 0 = manual sync
  "encryption.enabled": false,
  "encryption.activeMasterKeyId": ""
}
```

## Phase 2: Settings Dialog Structure

### 2.1 Settings Tab Layout
```rust
pub enum SettingsTab {
    Sync,        // Sync target configuration
    Encryption,  // Already exists - keep
    Advanced,    // Advanced settings
    About,       // Already exists - keep
}
```

### 2.2 Sync Tab UI Design
**Multi-window layout:**
```
┌─ Sync Settings ─────────────────────────────────────┐
│                                                     │
│  Active Target: [WebDAV ▼]                         │
│                                                     │
│  ┌─ WebDAV Configuration ───────────────────────┐  │
│  │                                              │  │
│  │  Server URL:     [https://webdav.example.com/] │
│  │  Username:      [user@example.com]           │  │
│  │  Password:      [••••••••]                   │  │
│  │  Remote Path:   [/neojoplin]                │  │
│  │                                              │  │
│  │  [ ] Ignore TLS Errors                      │  │
│  │                                              │  │
│  │  [Test Connection]  [Save]  [Cancel]        │  │
│  └──────────────────────────────────────────────┘  │
│                                                     │
│  Available Targets:                                 │
│  ● WebDAV         ( configured )                   │
│  ○ OneDrive       ( not implemented )              │
│  ○ Dropbox        ( not implemented )              │
│  ○ Joplin Server  ( not implemented )              │
└─────────────────────────────────────────────────────┘
```

### 2.3 Target Type State Management
```rust
pub enum SyncTargetType {
    None,           // ID 0
    Memory,         // ID 1
    FileSystem,     // ID 2
    OneDrive,       // ID 3
    Nextcloud,      // ID 5
    WebDAV,         // ID 6
    Dropbox,        // ID 7
    AmazonS3,       // ID 8
    JoplinServer,   // ID 9
    JoplinCloud,    // ID 10
}

pub struct SyncTargetSettings {
    pub target_type: SyncTargetType,
    pub webdav: WebDAVSettings,
    // Future: onedrive, dropbox, etc.
}

pub struct WebDAVSettings {
    pub url: String,
    pub username: String,
    pub password: String,
    pub remote_path: String,
    pub ignore_tls_errors: bool,
}
```

## Phase 3: Input Handling & Validation

### 3.1 Text Input Fields
```rust
pub struct SettingsInputState {
    pub active_field: Option<SettingsField>,
    pub url_input: String,
    pub username_input: String,
    pub password_input: String,
    pub path_input: String,
    pub validation_errors: HashMap<SettingsField, String>,
}

pub enum SettingsField {
    WebDAVUrl,
    WebDAVUsername,
    WebDAVPassword,
    WebDAVPath,
}
```

### 3.2 Input Validation
- **URL validation:** Must be valid HTTP/HTTPS URL
- **Path validation:** Must start with `/`
- **Connection testing:** Async WebDAV PROPFIND test
- **Password masking:** Display `••••` but store actual password

## Phase 4: Integration with TUI

### 4.1 Settings Dialog Event Handling
```rust
// Key bindings in settings dialog
// - Tab/Shift+Tab: Navigate between fields
// - Enter: Edit field / Save settings
// - Esc: Close settings / Cancel
// - Ctrl+s: Test connection
// - Arrow keys: Navigate target list
```

### 4.2 State Management Updates
```rust
// In AppState
pub struct AppState {
    // ... existing fields ...

    // Settings dialog state
    pub show_settings: bool,
    pub settings: SettingsState,
    pub settings_modified: bool,
}

// In SettingsState
pub struct SettingsState {
    pub current_tab: SettingsTab,
    pub sync_settings: SyncTargetSettings,
    pub encryption: EncryptionSettings,
    pub input_state: SettingsInputState,
    pub connection_status: Option<ConnectionStatus>,
}
```

## Phase 5: Configuration Persistence

### 5.1 Settings Save/Load
```rust
impl SettingsState {
    pub async fn save_settings(&self) -> Result<()> {
        // Save to ~/.config/neojoplin/settings.json
        // Maintain Joplin compatibility
    }

    pub async fn load_settings(&mut self) -> Result<()> {
        // Load from ~/.config/neojoplin/settings.json
        // Handle migration from old config formats
    }
}
```

### 5.2 Password Security
- **Store passwords in plaintext** (Joplin compatibility)
- **Consider password encryption** for future enhancement
- **File permissions:** `0600` (user read/write only)

## Phase 6: Testing & Validation

### 6.1 Unit Tests
- Configuration serialization/deserialization
- URL validation
- Path validation
- Settings field navigation

### 6.2 Integration Tests
- Settings save/load cycle
- WebDAV connection testing
- Cross-compatibility with Joplin CLI

## Implementation Order

**Step 1:** Consolidate config systems (Phase 1)
**Step 2:** Add sync target types and settings structures (Phase 2.2-2.3)
**Step 3:** Implement settings dialog UI rendering (Phase 2.1-2.2)
**Step 4:** Add input handling and validation (Phase 3)
**Step 5:** Integrate with TUI event system (Phase 4)
**Step 6:** Add configuration persistence (Phase 5)
**Step 7:** Testing and validation (Phase 6)

## Files to Modify

### New Files:
- `crates/tui/src/sync_settings.rs` - Sync target settings module
- `crates/tui/src/settings_dialog.rs` - Settings dialog rendering

### Modified Files:
- `crates/core/src/config.rs` - Add WebDAV settings, Joplin compatibility
- `crates/tui/src/settings.rs` - Add sync settings, input handling
- `crates/tui/src/ui.rs` - Add settings dialog rendering
- `crates/tui/src/app.rs` - Add settings dialog event handling
- `crates/tui/src/state.rs` - Add settings state

### Remove Files:
- `crates/tui/src/config.rs` - Consolidate into core config

## Compatibility Notes

1. **Joplin CLI compatibility:** Settings file format must match Joplin's structure
2. **Migration path:** Handle existing TOML config → JSON config migration
3. **Default values:** Match Joplin's defaults for seamless interoperability