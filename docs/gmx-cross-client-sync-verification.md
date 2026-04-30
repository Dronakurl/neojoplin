# GMX `/joplin` cross-client sync verification

## Scope

This verification used the productive GMX WebDAV target at `/joplin` and was performed carefully against:

- **Joplin CLI**
- **NeoJoplin**

The Joplin Desktop profile was left untouched.

## What was changed locally

1. The old Joplin CLI profile was backed up and replaced with a fresh profile before pointing it to GMX. This was necessary because the previous CLI profile had unrelated local data and hundreds of conflicts, which would have been unsafe to sync to production.
2. The NeoJoplin local database/data directory was removed to allow a clean import from the encrypted remote.

## What worked

### Joplin CLI

Joplin CLI was configured to use the GMX WebDAV target and synced successfully.

Observed result after the clean sync/decrypt flow:

- Sync target: WebDAV (`sync.target = 6`)
- Remote path: `https://webdav.mc.gmx.net/joplin/`
- Visible note count: **301**
- Sync status: **466 / 466** items
- Conflicts: **0**

Important finding: the productive GMX target did **not** decrypt with the generic `E2EE_PASSWORD` value from `.env`. It decrypted only with `GMX_E2EE_PASSWORD`.

## What failed

### NeoJoplin against the same encrypted target

NeoJoplin did **not** successfully sync the productive encrypted GMX target.

Observed result:

- NeoJoplin completed the command without a hard process failure, but emitted many sync warnings.
- NeoJoplin imported only **2** visible notes.
- Joplin CLI imported **301** visible notes from the same remote.
- NeoJoplin reported repeated failures such as:
  - `Failed to decrypt master key ... Invalid IV length: 16 (expected 12)`
  - `Failed to decrypt item ... Master key not found: ...`

This means cross-client parity is currently **not achieved**.

## Root cause assessment

The failure is not a bad password issue.

Evidence:

1. Joplin CLI successfully decrypted the same remote with `GMX_E2EE_PASSWORD`.
2. A safe inspection of remote `info.json` showed:
   - `e2ee = true`
   - `masterKeyCount = 12`
   - remote master key content uses a format whose first entry has:
     - `salt_len = 12`
     - `iv_len = 24`
     - no nested `data` object
3. NeoJoplin's master key loader expects a different JSON shape and encoding for encrypted master keys:
   - hex-encoded salt
   - nested `data.iv` / `data.ct`
   - AES-GCM nonce handling that does not match the remote format

Because of that mismatch, NeoJoplin loads only its own newly generated local key and cannot load most remote master keys from GMX/Joplin. Once encrypted notes reference those remote keys, note decryption fails and sync completeness collapses.

## Reliability verdict

**NeoJoplin is not currently reliable for the productive encrypted GMX `/joplin` target.**

At the time of verification:

| Client | Result |
| --- | --- |
| Joplin CLI | Working |
| NeoJoplin | Not working reliably |

So the system is **not ready** for dependable same-remote cross-client operation between NeoJoplin and Joplin CLI on this encrypted dataset.

## Consequence for the requested cross-client checks

The following checks could **not** be validated end-to-end because NeoJoplin never reached a correct full import state:

- same final note count between both applications
- note body edit propagation between both applications
- new note propagation between both applications
- trustworthy simultaneous use against the same encrypted remote

Those tests would not be meaningful until the NeoJoplin E2EE master-key compatibility bug is fixed.

## Practical setup plan for NeoJoplin with the GMX `/joplin` folder

This is the safe order to follow once the E2EE compatibility issue is fixed:

1. Keep Joplin Desktop untouched.
2. Use a clean NeoJoplin data directory before first production sync.
3. Configure NeoJoplin with:
   - `--url $GMX_URL`
   - `-U $GMX_USER`
   - `-P $GMX_PASS`
   - `-r /joplin`
4. Pass the productive encryption password explicitly as:
   - `--e2ee-password "$GMX_E2EE_PASSWORD"`
5. Do **not** rely on `E2EE_PASSWORD` if it differs from `GMX_E2EE_PASSWORD`.
6. Run one initial full sync and verify:
   - no master-key decryption warnings
   - note/folder/resource counts match Joplin CLI
   - random note bodies open correctly
7. Only after count parity is reached, test bidirectional create/edit propagation.
8. Only after that succeeds, assess concurrent sync behavior with both clients.

## Required code fixes before retrying

1. Make NeoJoplin load remote Joplin master keys in the actual format used by GMX/Joplin `info.json`.
2. Ensure IV/salt decoding matches the real remote encoding.
3. Avoid auto-generating a fresh active local key on first sync when the intent is to attach to an already-encrypted remote target.
4. Make environment handling explicit so the productive GMX E2EE password is not confused with unrelated local/test passwords.
