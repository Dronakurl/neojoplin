# GMX WebDAV readiness assessment (no sync executed)

## Scope

This assessment was performed without running `sync` against the productive GMX target.
Only code inspection and non-destructive local/manual checks were executed.

## Findings

1. CLI sync supports explicit GMX targeting:
   - `--url`
   - `--username`
   - `--password`
   - `--remote`
   - `--e2ee-password`
2. Remote path handling is correctly wired:
   - The CLI passes `--remote` to `SyncEngine::with_remote_path(...)`.
   - The engine uses that path for directory existence checks, lock dir creation, item list/upload/download, and `info.json`.
3. E2EE handling is implemented in sync:
   - Downloaded encrypted items (`encryption_applied: 1`) require a loaded E2EE service.
   - If decryption is not possible, sync returns an explicit error.
4. Important environment detail:
   - Current CLI logic auto-reads `E2EE_PASSWORD` from environment/`.env`.
   - It does **not** auto-read `GMX_E2EE_PASSWORD` by name.
   - Therefore, relying only on `GMX_E2EE_PASSWORD` would not be sufficient unless mapped to `E2EE_PASSWORD` or passed via `--e2ee-password`.
5. Remote mount check (`~/mnt/joplin`) indicates encrypted target:
   - `info.json` exists with `version: 3`.
   - `info.json` reports `e2ee: true`.
   - `activeMasterKeyId` is present.
   - `masterKeys` contains entries.
   - Sample/count checks on remote `.md` items show encryption markers consistent with encrypted Joplin payloads.

## Build/check/test status during assessment

- `just build`: passed
- `just test`: passed
- `just check`: failed on pre-existing clippy-deny issues outside GMX setup flow (`crates/core/src/autosync.rs`, `crates/core/src/jex.rs`)

These clippy findings are unrelated to the remote-path/E2EE sync wiring but should be cleaned up separately.

## Readiness verdict

**Conditionally ready** for GMX `/joplin` sync from a mechanism perspective:
- ✅ Path routing and encrypted-item sync flow are in place.
- ✅ Remote target appears to be an encrypted Joplin dataset.
- ⚠️ You must provide the E2EE password in a way the CLI actually consumes (`--e2ee-password` or `E2EE_PASSWORD`).

## Execution plan to set up NeoJoplin for GMX `/joplin` (safe rollout)

1. Snapshot productive DB before first production sync:
   - copy `~/.local/share/neojoplin/joplin.db` to a timestamped backup.
2. Export credentials from `.env` into the variable names expected by the CLI invocation:
   - use `GMX_URL`, `GMX_USER`, `GMX_PASS` for URL/user/pass arguments.
   - map the GMX E2EE password to `E2EE_PASSWORD` (or use `--e2ee-password` explicitly).
3. Run a **readiness dry command only** (help/argument sanity), no sync:
   - verify command shape includes `--remote /joplin`.
4. First production sync run (when explicitly approved) with verbose caution:
   - run one single sync invocation against `--remote /joplin`.
   - avoid parallel clients during the first run.
5. Post-sync verification:
   - inspect note/folder counts locally.
   - check logs for decrypt errors or key mismatch.
   - confirm no unexpected mass deletes.
6. If anything looks wrong:
   - stop further sync runs.
   - restore DB backup.
   - re-check E2EE password/key alignment before retry.
