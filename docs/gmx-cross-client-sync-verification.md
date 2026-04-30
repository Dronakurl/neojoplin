# GMX cross-client sync verification

## Scope

This verification covers two GMX targets:

1. **Productive `/joplin`**
2. **Playground `/joplin_test`**

All tests were performed with isolated Joplin CLI and NeoJoplin homes where appropriate. The Joplin Desktop profile was left untouched.

## Current result

**NeoJoplin and Joplin CLI now interoperate successfully on local WebDAV, GMX `/joplin_test`, and the productive GMX `/joplin` target.**

What is **working today**:

- Joplin CLI → NeoJoplin plaintext sync
- NeoJoplin → Joplin CLI plaintext sync
- Joplin CLI → NeoJoplin encrypted sync
- NeoJoplin → Joplin CLI encrypted sync
- bidirectional note body edits
- cross-client note creation
- remote deletion propagation back into NeoJoplin
- matching note counts after the verified scenarios

What is **still worth watching**:

- the productive `/joplin` dataset still contains older disabled legacy master keys, so sync logs may contain compatibility warnings for historical keys that are no longer active
- Joplin CLI must have the active productive key present in `encryption.passwordCache` locally or it cannot encrypt newly created notes for upload

## Verified working scenarios

### 1. Local WebDAV plaintext

- Joplin CLI created a note
- NeoJoplin imported it
- NeoJoplin edited that note
- Joplin CLI received the edit
- NeoJoplin created a second note
- Joplin CLI imported it
- Joplin CLI edited that second note
- NeoJoplin received the edit
- Final counts matched: **2 notes / 2 notes**

### 2. Local WebDAV encrypted

- Same roundtrip as above
- NeoJoplin attached to the encrypted target using the remote master key
- Joplin CLI required an explicit `joplin e2ee decrypt --retry-failed-items` pass after incoming encrypted changes
- Final counts matched: **2 notes / 2 notes**

### 3. GMX `/joplin_test` plaintext

- Joplin CLI created `GmxPlain`
- NeoJoplin imported it
- NeoJoplin edited it to `Neo edited GMX plain body v2`
- Joplin CLI received that edit
- NeoJoplin created `GmxNeoPlain`
- Joplin CLI imported it
- Joplin CLI edited it to `Edited by Joplin on GMX plain v3`
- NeoJoplin received that edit
- Final counts matched: **2 notes / 2 notes**

### 4. GMX `/joplin_test` encrypted

- Joplin CLI created `GmxEnc`
- NeoJoplin imported it with `--e2ee-password "$GMX_E2EE_PASSWORD"`
- NeoJoplin edited it to `Neo edited GMX encrypted body v2`
- Joplin CLI received the encrypted change
- Joplin CLI decrypted with `joplin e2ee decrypt --retry-failed-items`
- NeoJoplin created `GmxNeoEnc`
- Joplin CLI imported and decrypted it
- Joplin CLI edited it to `Edited by Joplin on GMX encrypted v3`
- NeoJoplin received that edit
- Final counts matched: **2 notes / 2 notes**

### 5. Productive GMX `/joplin`

The productive target is encrypted and contains mixed master-key generations:

- **method `8`** keys in the current format
- **method `4`** keys in the older SJCL-style format

The productive verification covered:

1. fresh NeoJoplin import from GMX `/joplin`
2. encrypted note decryption against real remote note content
3. note-count parity with Joplin CLI after import: **301 / 301**
4. Joplin CLI creating a new encrypted probe note and syncing it to GMX
5. NeoJoplin importing that probe note
6. Joplin CLI deleting the same probe note and syncing again
7. NeoJoplin removing the probe note on the next sync and returning to **301 / 301**

That establishes working create, edit, decrypt, and delete convergence on the productive shared target.

## Code fixes that made this work

### 1. Stop auto-generating a fresh local master key on first sync attach

Previously, passing `--e2ee-password` on a fresh NeoJoplin profile would auto-enable local encryption and generate a new local key before sync. That caused key drift and broke encrypted interoperability.

Now, a fresh profile with only a password can attach to an already-encrypted target and use the target's master key.

### 2. Pass the E2EE service into the sync engine as soon as a password is available

Previously, NeoJoplin only attached the E2EE service to the sync engine if keys were already loaded locally. That prevented a fresh profile from loading remote keys during sync.

Now, a password-only E2EE service is enough for the sync engine to fetch and load remote keys.

### 3. Load remote keys before evaluating encryption state changes

Previously, the sync engine decided "local encryption is disabled" before loading remote keys, which triggered the wrong re-upload behavior for encrypted targets.

Now, a fresh profile attaches to the remote encrypted state correctly before deciding whether any re-upload is needed.

### 4. Support legacy productive-key decryption

The productive GMX target contains a mixture of newer KeyV1-style keys and older SJCL-based keys. NeoJoplin now supports the legacy SJCL AES-CCM path needed to unlock productive remote data that Joplin had already encrypted years ago.

This was verified against both the Joplin reference implementation and real encrypted GMX note payloads.

### 5. Apply remote deletions during delta sync

Previously, NeoJoplin only downloaded new or updated remote items. If Joplin deleted a note and synced, the remote file disappeared but NeoJoplin kept the local note forever.

NeoJoplin now purges locally tracked items that disappeared from the remote target, so deletions converge in the same way as creations and edits.

## Practical setup for a productive shared target

1. Configure both clients to `https://webdav.mc.gmx.net/joplin/`.
2. Keep Joplin Desktop on its own untouched profile.
3. Ensure Joplin CLI has the active productive master key unlocked in its local `encryption.passwordCache`.
4. Start NeoJoplin with the productive E2EE password available so it can load remote master keys during sync.
5. After incoming encrypted changes in Joplin CLI, run `joplin e2ee decrypt --retry-failed-items` if any items remain pending decryption.
6. Verify parity by comparing note counts in both local databases after major migration steps.
