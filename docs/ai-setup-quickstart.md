# AI Support Setup - Quick Start

This guide helps you get started with AI development in NeoJoplin on the `ai-support` branch.

## Prerequisites

1. You're on the `ai-support` branch
2. Rust toolchain is installed

## Using Test Mode

To prevent AI development from affecting your actual notes, use test mode:

### Option 1: CLI Flag
```bash
neojoplin --test-mode
```

### Option 2: Environment Variable
```bash
export NEOJOPLIN_TEST_MODE=1
neojoplin
```

### Option 3: Custom Directories
```bash
# For fine-grained control
export NEOJOPLIN_CONFIG_DIR=/path/to/test/config
export NEOJOPLIN_DATA_DIR=/path/to/test/data
neojoplin
```

## What Test Mode Does

When test mode is enabled:
- **Config directory**: `~/.config/neojoplin-test/` (instead of `~/.config/neojoplin/`)
- **Data directory**: `~/.local/share/neojoplin-test/` (instead of `~/.local/share/neojoplin/`)

This means:
- Database: `~/.local/share/neojoplin-test/joplin.db`
- Encryption config: `~/.local/share/neojoplin-test/encryption.json`
- Sync targets: `~/.local/share/neojoplin-test/sync-targets.json`
- TUI config: `~/.config/neojoplin-test/config.toml`

All isolated from your production data!

## Creating the AI Module

To create the AI module structure:

```bash
# Create the new crate
cargo new --lib crates/ai

# Add it to workspace
# Edit Cargo.toml and add "crates/ai" to the members list
```

## Recommended Development Workflow

1. **Start in test mode**: Always use `--test-mode` or `NEOJOPLIN_TEST_MODE=1`
2. **Create test data**: Add some test notes to verify AI features work
3. **Iterate**: Test freely without worrying about your real notes
4. **Reset**: Simply delete `~/.local/share/neojoplin-test/` to start fresh

## Clean Slate

To completely reset your test environment:

```bash
# Remove test data
rm -rf ~/.local/share/neojoplin-test/
rm -rf ~/.config/neojoplin-test/

# Or for custom directories
rm -rf $NEOJOPLIN_DATA_DIR
rm -rf $NEOJOPLIN_CONFIG_DIR
```

## Verifying Isolation

To confirm test mode is working:

```bash
# Check which data directory is being used
NEOJOPLIN_TEST_MODE=1 cargo run --bin neojoplin -- init 2>&1 | grep -i "database\|initialized"

# The path should contain "neojoplin-test"
```

## Next Steps

1. Read the full integration guide in `docs/ai-integration-guide.md`
2. Create the AI crate structure
3. Implement your first AI provider (Ollama is recommended for local testing)
4. Add AI commands to the CLI
5. Integrate AI into the TUI

## Troubleshooting

### "Database already exists"
This means you're NOT in test mode. Check:
- Did you use `--test-mode` or set `NEOJOPLIN_TEST_MODE=1`?
- Run `neojoplin --help` to verify the flag exists

### Configuration not found
In test mode, configs are in different locations. Either:
- Run in test mode to create test configs
- Copy configs from production (not recommended for AI development)

### Switching Back to Production

Simply omit the test mode flag/variable:
```bash
neojoplin  # Uses production directories
```

## Environment Variables Reference

| Variable | Purpose | Default (without test mode) |
|----------|---------|----------------------------|
| `NEOJOPLIN_TEST_MODE` | Enable test mode | Not set |
| `NEOJOPLIN_CONFIG_DIR` | Custom config directory | `~/.config/neojoplin` |
| `NEOJOPLIN_DATA_DIR` | Custom data directory | `~/.local/share/neojoplin` |
| `AI_PROVIDER` | AI provider type | `None` |
| `AI_API_KEY` | API key for AI provider | Not set |
| `AI_API_URL` | API endpoint URL | Provider-specific |
| `AI_MODEL` | Model to use | `llama3` |

## Useful Commands

```bash
# Build in test mode
NEOJOPLIN_TEST_MODE=1 cargo build

# Run tests in test mode  
NEOJOPLIN_TEST_MODE=1 cargo test

# Initialize test database
NEOJOPLIN_TEST_MODE=1 cargo run --bin neojoplin -- init

# Add a test note
NEOJOPLIN_TEST_MODE=1 cargo run --bin neojoplin -- mknote --title "Test Note" --body "This is a test"

# Launch TUI in test mode
NEOJOPLIN_TEST_MODE=1 cargo run --bin neojoplin -- --tui
```

## Branch Management

The `ai-support` branch contains:
- Configuration isolation via `NEOJOPLIN_TEST_MODE`
- Documentation for AI integration
- CLI flag `--test-mode` for easy switching

To update from main:
```bash
git checkout ai-support
git merge main  # Or rebase, depending on your workflow
```

## Contributing Back

When your AI features are ready for production:
1. Ensure they work in both test and production modes
2. Document any new configuration options
3. Add appropriate error handling
4. Consider making AI features optional (feature flags)
5. Submit PR to main branch
