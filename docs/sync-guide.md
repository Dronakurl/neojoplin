# NeoJoplin Sync Guide

## Overview

NeoJoplin supports bidirectional synchronization with Joplin CLI via WebDAV, including end-to-end encryption (E2EE). Both applications can share the same sync target and maintain identical data.

## Quick Start

```bash
# Initialize NeoJoplin
neojoplin init

# Create some data
neojoplin mk-book "My Notebook"
neojoplin mk-note "My Note" --body "Hello" --parent <folder-id>
neojoplin mk-todo "Buy milk" --parent <folder-id>

# Sync to WebDAV with E2EE
neojoplin sync --url http://localhost:8080/webdav --remote /my-sync --e2ee-password MyPassword
```

## Cross-Application Compatibility

NeoJoplin is designed to work alongside the Joplin CLI. Both apps can sync to the same WebDAV target and see each other's changes.

### Expected Behavior

1. **Create in Joplin → Sync both → See in NeoJoplin**: Notes, folders, tags, and todos created in Joplin appear in NeoJoplin after syncing.
2. **Create in NeoJoplin → Sync both → See in Joplin**: Items created in NeoJoplin appear in Joplin after syncing.
3. **Idempotent sync**: Syncing twice with no changes produces zero uploads/downloads.
4. **Todo sync**: Todo completion state syncs bidirectionally.

### Setup for Cross-App Sync

**Scenario A: NeoJoplin is already set up, adding Joplin CLI**

```bash
# 1. Configure Joplin CLI to use the same WebDAV target
joplin config sync.target 6
joplin config sync.6.path "http://localhost:8080/webdav/shared-sync"
joplin config sync.6.username ""
joplin config sync.6.password ""

# 2. IMPORTANT: Set the master password BEFORE syncing
#    Use the same password NeoJoplin is using for E2EE
joplin e2ee enable --password "YourPassword"

# 3. Sync to download NeoJoplin's data
joplin sync

# 4. Decrypt downloaded items (Joplin CLI does not auto-decrypt)
joplin e2ee decrypt --password "YourPassword" -f

# 5. Verify Joplin has the data
joplin ls
```

**Scenario B: Starting fresh with both apps**

```bash
# 1. Configure Joplin CLI
joplin config sync.target 6
joplin config sync.6.path "http://localhost:8080/webdav/shared-sync"
joplin e2ee enable --password "YourPassword"

# 2. Create data and sync Joplin
joplin mkbook "Shared Notebook"
joplin use "Shared Notebook"
joplin mknote "From Joplin"
joplin sync

# 3. Sync NeoJoplin with same target and password
neojoplin sync --url http://localhost:8080/webdav --remote /shared-sync --e2ee-password "YourPassword"

# 4. Verify data
neojoplin ls
```

**Why `joplin e2ee enable` must come before `joplin sync`**

Joplin CLI does not auto-decrypt items after sync. It needs the master password set in its database before it can decrypt the master key downloaded from the WebDAV server. Running `joplin e2ee enable --password <password>` sets the master password in Joplin's local settings. After that, `joplin sync` can download encrypted items, and `joplin e2ee decrypt` decrypts them.


## End-to-End Encryption (E2EE)

NeoJoplin supports Joplin's E2EE format:
- **Master key encryption**: Method 8 (KeyV1) — PBKDF2-HMAC-SHA512 with 220,000 iterations
- **String encryption**: Method 10 (StringV1) — PBKDF2-HMAC-SHA512 with 3 iterations, UTF-16LE encoding
- **Format**: JED01 (Joplin Encrypted Data) with chunked AES-256-GCM

### E2EE Password Priority

The E2EE password is resolved in this order:
1. `--e2ee-password` CLI flag (highest priority)
2. `E2EE_PASSWORD` environment variable
3. `.env` file in the current directory
4. No default — encryption is skipped if no password is available

### Encrypted Data on WebDAV

When E2EE is enabled, files on the WebDAV server contain:
- Plaintext metadata: `id`, `parent_id`, `updated_time`, `deleted_time`, `type_`, `encryption_applied`
- Encrypted content: `encryption_cipher_text` field contains JED01-formatted encrypted data
- The actual note title, body, and sensitive fields are inside the encrypted payload

## Todo Management

NeoJoplin supports Joplin-compatible todos (notes with `is_todo=1`).

### CLI Commands

```bash
# Create a todo
neojoplin mk-todo "Buy groceries" --parent <folder-id>

# Create a todo with due date
neojoplin mk-todo "Submit report" --due "2026-04-20T12:00:00Z" --parent <folder-id>

# Toggle completion
neojoplin todo-toggle "Buy groceries"

# List shows todo status with nerd font icons
neojoplin ls
# 📁 My Notebook (id)
# 📝 Regular Note (id)
# 󰄱 Unchecked Todo (id)
# 󰄲 Completed Todo (id)
```

### TUI Keybindings

| Key | Action |
|-----|--------|
| `T` | Create new todo |
| `t` | Toggle todo completion |

### Icons (Nerd Fonts)

- `📝` — Regular note
- `󰄱` — Unchecked todo
- `󰄲` — Completed todo
- `📁` — Folder/notebook

## WebDAV Configuration

### Local Docker Server

```bash
docker compose up -d
neojoplin sync --url http://localhost:8080/webdav --remote /my-sync
```

### GMX WebDAV

```bash
neojoplin sync --url https://webdav.mc.gmx.net -U user@gmx.de -P password --remote /kjoplin_test
```

### Encryption State Changes

NeoJoplin automatically handles transitions between encrypted and unencrypted states:

**Enabling E2EE on existing data:**
```bash
# Previously synced without encryption
neojoplin sync --url http://localhost:8080/webdav --remote /my-sync

# Now sync with encryption — all items re-uploaded encrypted
neojoplin sync --url http://localhost:8080/webdav --remote /my-sync --e2ee-password MyPassword
```
When E2EE is enabled on a previously unencrypted sync target, all items are automatically re-uploaded in encrypted format.

**Disabling E2EE:**
```bash
neojoplin e2ee disable --force
neojoplin sync --url http://localhost:8080/webdav --remote /my-sync
```
All items are re-uploaded unencrypted. Note: this does not delete the master key from the server.

**Auto-enable**: If `--e2ee-password` is provided but no local encryption config exists, NeoJoplin automatically generates a master key and enables E2EE.

## Crate Architecture

The sync functionality is organized into reusable crates:

- **`joplin-domain`**: Core data models (Note, Folder, Tag, etc.) — no NeoJoplin-specific code
- **`joplin-sync`**: WebDAV client, sync engine, E2EE — reusable in other Joplin-compatible projects
- **`neojoplin-storage`**: SQLite storage implementation
- **`neojoplin-cli`**: CLI application
- **`neojoplin-tui`**: Terminal UI

The `joplin-domain` and `joplin-sync` crates can be used independently in other projects that need to interact with Joplin's sync protocol or data format.

## Troubleshooting

### "Missing required property: type_"
This error from Joplin means the serialized item format is incorrect. Ensure items don't end with a trailing newline — Joplin's parser interprets trailing `\n` as a body separator.

### Decryption fails
- Verify both apps use the same E2EE password
- Check that master keys are synced (stored in `info.json` on WebDAV)
- Run `joplin e2ee decrypt --password <pass>` to manually trigger decryption in Joplin

### Items not appearing after sync
- Items may need decryption: run `joplin e2ee decrypt --password <pass>`
- Check that `parent_id` is set correctly for notes to appear in the right notebook
