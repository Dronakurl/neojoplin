# Joplin CLI Compatibility Investigation

This note is kept only as historical context. Its main conclusions are now outdated.

## What changed

The earlier investigation captured real issues at the time:

- NeoJoplin needed proper `info.json` handling
- encrypted attach behavior was wrong on fresh profiles
- productive GMX data used legacy master-key formats that NeoJoplin did not yet support
- remote deletions were not applied back into NeoJoplin

Those gaps have since been addressed in the codebase and re-verified against both local WebDAV and GMX targets.

## Current status

For the current verified state, use:

- `docs/gmx-webdav-readiness-assessment.md`
- `docs/gmx-cross-client-sync-verification.md`

Those documents supersede this one and describe the current working cross-client behavior, productive-target findings, and remaining operational cautions.
