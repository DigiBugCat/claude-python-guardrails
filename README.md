# Claude Python Guardrails

**Python-specific automation system for Claude Code** - intelligent file exclusion + automated linting & testing. A drop-in replacement for generic smart-lint/smart-test hooks with Python-optimized patterns and built-in tool discovery.

## ⚡ What's New: Smart Automation

**Claude Python Guardrails v0.1.0** adds full automation capabilities equivalent to smart-lint.sh/smart-test.sh hooks but **Python-specific** and **faster**:

- **🎯 Auto-discovery**: Finds `ruff`, `flake8`, `pylint`, `pytest` automatically  
- **🔒 Concurrency control**: PID-based locking with configurable cooldowns
- **⚡ Zero-config**: Works out of the box with Python projects
- **📋 Claude Code integration**: Drop-in replacement for generic hooks
- **🧠 Smart exclusions**: Context-aware patterns (lint vs test vs general)

### Claude Code Hook Usage

Replace your existing hooks with:

```bash
# Instead of ~/.claude/hooks/smart-lint.sh
claude-python-guardrails smart-lint

# Instead of ~/.claude/hooks/smart-test.sh  
claude-python-guardrails smart-test
```

**Exit codes match hooks exactly**: `0` = silent, `2` = show message (success or error)

> **📖 Full Claude Code Integration Guide**: See [CLAUDE_CODE_HOOKS.md](./CLAUDE_CODE_HOOKS.md) for complete hook configuration, troubleshooting, and advanced usage.

## 🚀 Quick Start

### Installation

```bash
# Clone and build
git clone https://github.com/DigiBugCat/claude-python-guardrails
cd claude-python-guardrails
cargo build --release

# Or install directly from source
cargo install --path .
```

### File Exclusion Usage

```bash
# Generate default configuration
claude-python-guardrails init

# Check if a file should be excluded
claude-python-guardrails check src/main.py

# Check for linting specifically
claude-python-guardrails lint migrations/001_initial.py

# Check for testing specifically  
claude-python-guardrails test conftest.py

# Validate configuration
claude-python-guardrails validate
```

## 📝 Configuration

The tool uses `guardrails.yaml` for configuration. Generate a default one with:

```bash
claude-python-guardrails init
```

### Example Configuration

```yaml
exclude:
  patterns:
    - "*.pyc"
    - "__pycache__/"
    - ".venv/**"
    - "target/**"
    
  python:
    lint_skip:
      - "migrations/**"
      - "*_pb2.py"
      - "*.generated.py"
    test_skip:
      - "conftest.py"
      - "test_*.py"
      - "tests/fixtures/**"

rules:
  max_file_size: "10MB"
  skip_binary_files: true
  skip_generated_files: true

automation:
  lint:
    enabled: true
    cooldown_seconds: 2
    timeout_seconds: 20
    preferred_tool: "ruff"  # or "flake8", "pylint"
  test:
    enabled: true
    cooldown_seconds: 2
    timeout_seconds: 20
    preferred_tool: "pytest"  # or "unittest"
```

## 🔧 CLI Commands

### `check <file>`
Check if a file should be excluded from any processing.

```bash
claude-python-guardrails check src/main.py
# Exit code: 0 = include, 1 = exclude
```

### `lint <file>`
Check if a file should be excluded from linting.

```bash
claude-python-guardrails lint migrations/001_initial.py
# Exit code: 1 (excluded from linting)
```

### `test <file>`
Check if a file should be excluded from testing.

```bash
claude-python-guardrails test conftest.py
# Exit code: 1 (excluded from testing)
```

### `init`
Generate default configuration file.

```bash
claude-python-guardrails init
# Creates: guardrails.yaml

claude-python-guardrails init -o custom-config.yaml  
# Creates: custom-config.yaml
```

### `validate`
Validate configuration file syntax.

```bash
claude-python-guardrails validate
# ✅ Valid configuration

claude-python-guardrails validate custom-config.yaml
# Validates specific file
```

### `smart-lint` ⚡ **NEW**
Smart linting automation for Claude Code hooks.

```bash
# Automatically discovers and runs Python linters (ruff → flake8 → pylint)
claude-python-guardrails smart-lint

# Reads JSON from stdin, walks up to find project root
# Exit codes: 0 = silent, 2 = show message (success/error)
```

### `smart-test` ⚡ **NEW**
Smart testing automation for Claude Code hooks.

```bash
# Automatically discovers and runs Python test runners (pytest → unittest)
claude-python-guardrails smart-test  

# Reads JSON from stdin, walks up to find project root
# Exit codes: 0 = silent, 2 = show message (success/error)
```

## 🎯 Use Cases

### Integration with Scripts

```bash
#!/bin/bash
# Smart linting script

for file in $(find . -name "*.py"); do
    if claude-python-guardrails lint "$file"; then
        echo "✅ Including $file for linting"
        ruff check "$file"
    else
        echo "⏭️  Skipping $file (excluded)"
    fi
done
```

### Claude Code Integration

Replace the existing exclusion logic in `.claude/hooks/smart-lint.sh`:

```bash
# Check if file should be excluded
if claude-python-guardrails lint "$file_path"; then
    # File is included, proceed with linting
    run_linting_tools "$file_path"
else
    # File is excluded, skip
    log_debug "Skipping $file_path (excluded by guardrails)"
    exit 0
fi
```

### Exit Codes

- `0`: File should be **included** (not excluded)
- `1`: File should be **excluded**
- `2`: Error occurred

### Verbose Output

Use `-v` for detailed information:

```bash
claude-python-guardrails -v check src/main.py
# ✅ src/main.py: INCLUDED

claude-python-guardrails -v lint migrations/001_initial.py  
# ❌ migrations/001_initial.py: EXCLUDED from linting
```

## 📂 Project Structure

```
claude-python-guardrails/
├── Cargo.toml           # Rust dependencies
├── README.md            # This file  
├── guardrails.yaml      # Default configuration
└── src/
    ├── main.rs          # CLI interface
    └── lib.rs           # Core exclusion logic
```

## 🛠️ Configuration Details

### Global Patterns
Apply to all operations (lint, test, general):

```yaml
exclude:
  patterns:
    - "*.pyc"           # Compiled Python
    - "__pycache__/"    # Python cache dirs
    - ".venv/"          # Virtual environments
    - "target/"         # Rust build artifacts
```

### Python-Specific Exclusions

#### Lint Skip
Files to exclude from linting only:

```yaml
exclude:
  python:
    lint_skip:
      - "migrations/"     # Database migrations
      - "*_pb2.py"        # Protocol buffers
      - "*.generated.py"  # Generated code
```

#### Test Skip  
Files to exclude from testing only:

```yaml
exclude:
  python:
    test_skip:
      - "conftest.py"     # Test configuration
      - "test_*.py"       # Test files themselves
```

### Rules

Additional file-based exclusions:

```yaml
rules:
  max_file_size: "10MB"        # Skip large files
  skip_binary_files: true      # Skip files with null bytes
  skip_generated_files: true   # Skip detected generated files
```

## 🔍 Generated File Detection

The tool automatically detects generated files based on common patterns:

- `*_pb2.py` (Protocol buffers)
- `*_pb2_grpc.py` (gRPC)  
- `*.generated.py`
- `*_generated.py`
- Files in `generated/` directories
- `*.gen.*` patterns

## 🚢 Examples

### Real-World Integration

```bash
# In your Makefile
lint:
	@for file in $$(find . -name "*.py"); do \
		if claude-python-guardrails lint "$$file"; then \
			ruff check "$$file" || exit 1; \
		fi; \
	done

test:  
	@for file in $$(find . -name "*.py"); do \
		if claude-python-guardrails test "$$file"; then \
			echo "Testing $$file..."; \
			pytest "tests/test_$$(basename $$file)" || exit 1; \
		fi; \
	done
```

### Custom Configuration Path

```bash
claude-python-guardrails -c /path/to/custom.yaml check file.py
```

## 📊 Performance

- **Fast**: Compiled Rust binary with minimal dependencies
- **Efficient**: Glob pattern matching with `globset` crate
- **Smart**: Context-aware exclusions (lint vs test vs general)

## 📚 Documentation

- **[Claude Code Hooks Integration](./CLAUDE_CODE_HOOKS.md)** - Complete guide to setting up automation hooks
- **[Configuration Reference](./guardrails.yaml)** - Example YAML configuration with all options
- **[Architecture Guide](./CLAUDE.md)** - Development and contribution guide

## 🙏 Acknowledgments

This project was inspired by the Claude Code hook patterns from [Veraticus/nix-config](https://github.com/Veraticus/nix-config/tree/main/home-manager/claude-code/hooks). Their approach to intelligent file filtering in Claude Code workflows provided the foundation for this standalone tool.

## 🤝 Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

MIT License - see LICENSE file for details

---

**Claude Python Guardrails** - Simple, fast, and focused exclusion checking for Python projects.