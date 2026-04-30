# KJoplin comparison notes

This note is a cleaned-up version of the original root-level `hint.md` from the `kjoplin` comparison work.

## Implement now

1. **Keep compatibility docs current**

   The old `docs/joplin-cli-compatibility-issue.md` had gone stale. It should remain clearly marked as historical context only, with current sync status tracked in the GMX verification docs.

2. **Clarify schema/version semantics**

   `neojoplin-storage` currently writes version `42`. That should be documented unambiguously as either a true Joplin schema version or a NeoJoplin-internal storage version so future compatibility work starts from the right assumption.

3. **Be conservative with shared sync metadata**

   Remote `info.json` files are shared protocol state. Any NeoJoplin-specific fields there should be treated carefully and minimized where possible.

4. **Recheck master-key wrapping assumptions**

   Any helper that uses a fixed salt or other simplified key-derivation shortcut should be reviewed to ensure it is not part of persisted or user-facing E2EE flows.

## Defer for now

1. **General backend embeddability**

   Making storage, sync, and E2EE fully host-configurable is a good direction, but it is not required to ship working Joplin-compatible sync today.

2. **Stable FFI or Python boundary**

   A `cdylib`/C ABI or `pyo3` layer is promising for future frontends, but it is a productization step rather than a sync-blocker.

3. **Broader multi-frontend architecture work**

   NeoJoplin is already the cleaner backend foundation compared with the current `kjoplin` split. That larger consolidation effort should happen after the sync protocol path is stable.
