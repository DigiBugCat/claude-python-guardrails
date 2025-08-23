# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Core Architecture

**Claude Python Guardrails** is a Rust CLI tool for intelligent file exclusion in Python projects, with AI-powered analysis using Cerebras LLM. Features a two-layer architecture:

### Main Components
- `src/main.rs` - CLI interface using clap with 6 commands: `check`, `lint`, `test`, `init`, `validate`, `analyze`
- `src/lib.rs` - Core logic with `GuardrailsChecker` struct that compiles glob patterns using `globset` crate
- `src/cerebras.rs` - AI-powered analysis using Cerebras LLM API for intelligent exclusion recommendations
- Configuration system using serde + serde_yaml for YAML parsing

### Key Data Structures
- `GuardrailsConfig` - Root configuration with exclude patterns and rules
- `ExclusionConfig` - Global patterns + Python-specific exclusions
- `PythonExclusions` - Context-aware patterns (lint_skip, test_skip)
- `GuardrailsChecker` - Main processor that pre-compiles glob patterns for performance
- `CerebrasConfig` - Configuration for Cerebras API integration (API key, model, endpoint)
- `SmartExclusionAnalyzer` - AI-powered analyzer using Cerebras LLM
- `ExclusionAnalysis` - Structured output with exclusion recommendations and reasoning

### Exclusion Contexts
The tool supports **context-aware exclusions**:
- **General** (`check`) - File excluded from any processing
- **Lint** (`lint`) - File excluded only from linting
- **Test** (`test`) - File excluded only from testing

Global patterns apply to all contexts; Python-specific patterns only apply to their respective contexts.

## AI-Powered Analysis

### Cerebras Integration
The tool includes optional Cerebras LLM integration for intelligent file analysis that goes beyond pattern matching:

- **File purpose detection**: Understands if files contain business logic, type definitions, configs, or generated code
- **Context-aware recommendations**: Different analysis for linting vs testing vs general processing
- **Smart reasoning**: Provides clear explanations for exclusion decisions
- **Conservative fallback**: When API fails, assumes files need full processing (safe default)

### Environment Variables
- `CEREBRAS_API_KEY` - Required for AI analysis functionality
- `RUST_LOG` - Controls logging level (debug, info, warn, error)

### Analysis Modes
1. **AI-powered** (with `CEREBRAS_API_KEY`): Uses Cerebras LLM for intelligent analysis
2. **Heuristic** (no API key): Falls back to pattern-based rules  
3. **Conservative** (API failure): Assumes all files need processing

### Usage
```bash
# Set API key
export CEREBRAS_API_KEY="your-api-key-here"

# Analyze file with AI
./target/release/claude-python-guardrails analyze src/models.py

# Get JSON output for programmatic use  
./target/release/claude-python-guardrails analyze src/config.py --format json
```

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
./target/release/claude-python-guardrails analyze src/main.py --format json
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
- **reqwest** - HTTP client for Cerebras API calls
- **tokio** - Async runtime for API requests
- **uuid** - Unique identifiers for API requests

## CI/CD Integration

CI runs on Linux and macOS, validates formatting, clippy, and all tests. The release workflow builds binaries for multiple platforms when pushing to main. Integration tests are skipped in CI due to compatibility issues but core functionality is verified directly.