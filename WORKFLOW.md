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
- **Automatically updates Homebrew formula** in the `homebrew-dev-tooling` repository

## Homebrew Formula Automation

The release workflow automatically updates the Homebrew formula when a new release is created.

### Initial Setup (One-time)

To enable Homebrew formula updates, you need to configure a GitHub Personal Access Token:

#### 1. Create Personal Access Token

1. Go to **GitHub Settings** → **Developer settings** → **Personal access tokens** → **Tokens (classic)**
2. Click **"Generate new token (classic)"**
3. Set **Expiration**: `No expiration` (or long-term like 1 year)
4. Select **Scopes**:
   - ✅ `repo` (Full control of private repositories)
   - This allows the workflow to clone and push to your `homebrew-dev-tooling` repository

#### 2. Add Token to Repository Secrets

1. Go to your **claude-python-guardrails** repository
2. Navigate to **Settings** → **Secrets and variables** → **Actions**
3. Click **"New repository secret"**
4. **Name**: `HOMEBREW_TAP_TOKEN`
5. **Secret**: Paste the personal access token from step 1
6. Click **"Add secret"**

#### 3. How It Works

When you push to `main` (creating a new release), the workflow will:

1. **Extract version** from `Cargo.toml`
2. **Download source tarball** from the GitHub release
3. **Calculate SHA256** of the source code
4. **Clone your Homebrew tap** repository (`homebrew-dev-tooling`)
5. **Update the formula** (`Formula/claude-python-guardrails.rb`) with:
   - New version number
   - New download URL
   - New SHA256 checksum
6. **Commit and push** the changes automatically

#### 4. What Gets Updated

The formula file will be updated from:
```ruby
url "https://github.com/DigiBugCat/claude-python-guardrails/archive/refs/tags/v0.1.0.tar.gz"
sha256 "b4f9f7ceaad22288c5b7a3b8bb6198868869cf71c6366487b5e633f0f162555f"
version "0.1.0"
```

To (for example, v0.1.1):
```ruby
url "https://github.com/DigiBugCat/claude-python-guardrails/archive/refs/tags/v0.1.1.tar.gz"
sha256 "new_calculated_sha256_here"
version "0.1.1"
```

#### 5. Troubleshooting

- **Token expired**: Regenerate PAT and update the secret
- **Permission denied**: Ensure the PAT has `repo` scope
- **Formula not updating**: Check the workflow logs in the Actions tab
- **Wrong branch**: Ensure your Homebrew tap uses `main` as the default branch

## Requirements

- All tests must pass before merging to main
- Code must be formatted (`cargo fmt`)
- No clippy warnings allowed
- Version in `Cargo.toml` should be updated for new releases
- **GitHub PAT configured** for Homebrew automation (see above)

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