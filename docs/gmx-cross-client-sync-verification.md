# GMX cross-client sync verification

## Scope

This verification now covers two GMX targets:

1. **Productive `/joplin`**
2. **Playground `/joplin_test`**

All tests were performed with isolated Joplin CLI and NeoJoplin homes. The Joplin Desktop profile was left untouched.

## Current result

**NeoJoplin and Joplin CLI now interoperate successfully on both local WebDAV and GMX `/joplin_test`, in plaintext and with E2EE.**

What is **working today**:

- Joplin CLI → NeoJoplin plaintext sync
- NeoJoplin → Joplin CLI plaintext sync
- Joplin CLI → NeoJoplin encrypted sync
- NeoJoplin → Joplin CLI encrypted sync
- bidirectional note body edits
- matching note counts after each playground scenario

What is **still not ready**:

- attaching NeoJoplin to the old productive encrypted `/joplin` dataset that contains legacy master keys using method `4`

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

## Remaining limitation: productive `/joplin`

The old productive `/joplin` dataset is a different problem from the fresh playground folder.

Safe inspection showed that `/joplin` contains a mix of master key methods:

- **method `8`** (current format)
- **method `4`** (legacy SJCL-style format)

NeoJoplin now works with fresh method-`8` encrypted targets such as `/joplin_test`, but it still does **not** support the old method-`4` master keys that exist in the productive `/joplin` folder.

So:

- **`/joplin_test`** is ready for cross-client use
- **`/joplin`** still needs legacy key support before it is safe

## Practical setup for a fresh shared encrypted target

1. Keep Joplin Desktop untouched.
2. Use isolated or clean profiles for first verification.
3. Configure both clients to the same remote path, for example `/joplin_test`.
4. For NeoJoplin, always pass the intended password explicitly:
   - `--e2ee-password "$GMX_E2EE_PASSWORD"`
5. For Joplin CLI, after syncing incoming encrypted changes, run:
   - `joplin e2ee decrypt --retry-failed-items`
6. Verify:
   - note counts match
   - Joplin-authored edits reach NeoJoplin
   - NeoJoplin-authored edits reach Joplin CLI
   - newly created notes appear in both clients

## Next step for the productive folder

To make the existing productive `/joplin` folder work, NeoJoplin still needs legacy method-`4` master key support.
