# Development Workflow

Simple workflow for Claude Python Guardrails development.

## Branch Structure

- `dev` - Development branch (default working branch)
- `main` - Release branch (triggers releases)

## Development Process

### 1. Work on Dev Branch

```bash
# Clone and switch to dev
git clone <repo-url>
cd claude-python-guardrails
git checkout dev  # or create if doesn't exist: git checkout -b dev

# Make your changes
# Edit code, add tests, etc.
```

### 2. Local Testing

Always test before pushing:

```bash
# Run all tests
cargo test

# Check code formatting
cargo fmt --all -- --check

# Run clippy for linting
cargo clippy --all-targets --all-features -- -D warnings

# Build release to verify
cargo build --release

# Test CLI functionality
./target/release/claude-python-guardrails --help
```

### 3. Push to Dev

```bash
git add .
git commit -m "Your commit message"
git push origin dev
```

**This triggers CI workflow** which runs:
- Tests on Linux and macOS
- Code formatting checks
- Clippy linting
- Coverage reporting

### 4. Release to Main

When ready for release:

```bash
# Update version in Cargo.toml if needed
# Then create PR to main
git checkout main
git pull origin main
git merge dev
git push origin main
```

**This triggers release workflow** which:
- Builds binaries for all platforms
- Creates GitHub release with version from Cargo.toml
- Uploads binaries and checksums
- Generates release notes

## Requirements

- All tests must pass before merging to main
- Code must be formatted (`cargo fmt`)
- No clippy warnings allowed
- Version in `Cargo.toml` should be updated for new releases

## Quick Commands

```bash
# Full local validation (run this before pushing)
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test

# Build and test CLI
cargo build --release && ./target/release/claude-python-guardrails --version
```

## Git Aliases (Optional)

Add to your `.gitconfig` for convenience:

```ini
[alias]
    dev = checkout dev
    main = checkout main
    test-all = !cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test
```