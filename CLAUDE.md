# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Core Architecture

**Claude Python Guardrails** is a Rust CLI tool for intelligent file exclusion in Python projects, with a two-layer architecture:

### Main Components
- `src/main.rs` - CLI interface using clap with 5 commands: `check`, `lint`, `test`, `init`, `validate`
- `src/lib.rs` - Core logic with `GuardrailsChecker` struct that compiles glob patterns using `globset` crate
- Configuration system using serde + serde_yaml for YAML parsing

### Key Data Structures
- `GuardrailsConfig` - Root configuration with exclude patterns and rules
- `ExclusionConfig` - Global patterns + Python-specific exclusions
- `PythonExclusions` - Context-aware patterns (lint_skip, test_skip)
- `GuardrailsChecker` - Main processor that pre-compiles glob patterns for performance

### Exclusion Contexts
The tool supports **context-aware exclusions**:
- **General** (`check`) - File excluded from any processing
- **Lint** (`lint`) - File excluded only from linting
- **Test** (`test`) - File excluded only from testing

Global patterns apply to all contexts; Python-specific patterns only apply to their respective contexts.

## Development Commands

### Build and Test
```bash
# Full local validation (run before pushing)
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test

# Build release binary
cargo build --release

# Run only unit tests (in lib.rs)
cargo test --lib

# Run only integration tests
cargo test --test integration_test

# Test CLI functionality
./target/release/claude-python-guardrails --help
./target/release/claude-python-guardrails init
./target/release/claude-python-guardrails check src/main.py
```

### Individual Commands
```bash
# Code formatting
cargo fmt --all -- --check

# Linting (no warnings allowed)
cargo clippy --all-targets --all-features -- -D warnings

# Run specific test
cargo test test_basic_exclusion
```

## Branch Workflow

- **`dev`** - Development branch, triggers CI (tests, formatting, clippy)
- **`main`** - Release branch, triggers automated binary builds and GitHub releases

Always work on `dev` branch. Pushing to `main` creates a new release with version from `Cargo.toml`.

## Testing Architecture

- **Unit tests** - Embedded in `src/lib.rs` using `#[cfg(test)]`, test core logic with temp files
- **Integration tests** - `tests/integration_test.rs` uses `cargo run --` to test CLI commands end-to-end
- **Test patterns** - Uses `tempfile` crate for filesystem operations, tests both success and failure cases

### Exit Code Semantics
- `0` - File should be **included** (not excluded)
- `1` - File should be **excluded** 
- `2` - Error occurred

## Configuration System

**YAML-based configuration** (`guardrails.yaml`) with hierarchical exclusions:

```yaml
exclude:
  patterns: []           # Global exclusions (apply everywhere)
  python:
    lint_skip: []        # Skip during linting only
    test_skip: []        # Skip during testing only
rules:
  max_file_size: "10MB"  # File size limit
  skip_binary_files: true
  skip_generated_files: true
```

The `GuardrailsChecker` pre-compiles all patterns into `GlobSet` objects for fast matching.

## Key Dependencies

- **globset** - Fast glob pattern matching (core functionality)
- **clap** - CLI parsing with derive macros
- **serde/serde_yaml** - Configuration deserialization
- **anyhow** - Error handling with context
- **tempfile** - Testing with temporary directories

## CI/CD Integration

CI runs on Linux and macOS, validates formatting, clippy, and all tests. The release workflow builds binaries for multiple platforms when pushing to main. Integration tests are skipped in CI due to compatibility issues but core functionality is verified directly.