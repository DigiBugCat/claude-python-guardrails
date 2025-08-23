# Claude Python Guardrails

**Claude Code Python automation hooks** - AI-powered linting and testing automation with Cerebras intelligence. Drop-in replacement for generic smart-lint/smart-test hooks with Python-optimized patterns and intelligent file analysis.

## ⚡ What This Is

**Claude Python Guardrails** is designed exclusively for Claude Code hooks. It provides:

- **🎯 Linting automation**: `lint` command for Claude Code PostToolUse hooks
- **🧪 Testing automation**: `test` command for Claude Code PostToolUse hooks  
- **🤖 AI-powered file analysis**: `analyze` command with Cerebras integration
- **⚡ Zero standalone usage**: Only works as Claude Code hooks (by design)

## 🚀 Installation

#### Option 1: Install from Source (Recommended)

```bash
# Clone and install system-wide
git clone https://github.com/DigiBugCat/claude-python-guardrails
cd claude-python-guardrails
cargo install --path .
```

This installs the binary to `~/.cargo/bin/claude-python-guardrails`.

#### Option 2: Manual Build

```bash
# Clone and build locally
git clone https://github.com/DigiBugCat/claude-python-guardrails
cd claude-python-guardrails
cargo build --release

# Binary will be at: ./target/release/claude-python-guardrails
```

#### macOS PATH Setup

If `claude-python-guardrails` command is not found after installation:

```bash
# Add Cargo bin directory to your PATH
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc

# Or for bash users
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bash_profile
source ~/.bash_profile
```

#### Verify Installation

```bash
# Check if installed correctly
claude-python-guardrails --version

# Should output: claude-python-guardrails 1.0.0
```

## 🔧 Claude Code Hook Setup

### Basic Hook Configuration

Add to your `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write|NotebookEdit",
        "hooks": [
          {
            "type": "command",
            "command": "claude-python-guardrails lint",
            "timeout": 30
          },
          {
            "type": "command", 
            "command": "claude-python-guardrails test",
            "timeout": 60
          }
        ]
      }
    ]
  }
}
```

### Advanced Hook Configuration with AI Analysis

For AI-powered exclusion decisions, first set your Cerebras API key:

```bash
# Add to your shell profile (~/.bashrc, ~/.zshrc, etc.)
export CEREBRAS_API_KEY="your-api-key-here"
```

Then add AI analysis to your hooks:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write|NotebookEdit",
        "hooks": [
          {
            "type": "command",
            "command": "claude-python-guardrails analyze --format json",
            "timeout": 15
          },
          {
            "type": "command",
            "command": "claude-python-guardrails lint",
            "timeout": 30
          },
          {
            "type": "command",
            "command": "claude-python-guardrails test",
            "timeout": 60
          }
        ]
      }
    ]
  }
}
```

## 🔍 Commands

All commands are designed to work exclusively with Claude Code hook JSON input via stdin. They do not accept file path arguments.

### `lint`

Automatically discovers and runs Python linters (ruff → flake8 → pylint):

```bash
# Used in Claude Code hooks only - reads JSON from stdin
claude-python-guardrails lint
```

**Exit codes**: `0` = silent success, `2` = show message (success or error)

### `test`

Automatically discovers and runs Python test runners (pytest → unittest):

```bash
# Used in Claude Code hooks only - reads JSON from stdin  
claude-python-guardrails test
```

**Exit codes**: `0` = silent success, `2` = show message (success or error)

### `analyze`

AI-powered file analysis using Cerebras for intelligent exclusion recommendations:

```bash
# Used in Claude Code hooks only - reads JSON from stdin
claude-python-guardrails analyze

# JSON output for programmatic use
claude-python-guardrails analyze --format json
```

#### Example Analysis Output

```bash
📁 File Analysis: src/user_model.py
════════════════════════════════════════════════════════════

📋 File Type: Pydantic Data Model
🎯 Purpose: User data validation and serialization

🚫 Exclusion Recommendations:
  • General Processing: ✅ INCLUDE
  • Linting: ❌ EXCLUDE  
  • Testing: ✅ INCLUDE

🤔 Reasoning:
This file contains Pydantic model definitions with runtime type validation. 
Linting may produce false positives for TYPE_CHECKING imports that are 
actually needed at runtime for Pydantic field validation.

💡 Configuration Recommendation:
Add to lint_skip: ["**/models.py", "*_model.py"] to avoid TC003/TC004 false positives while keeping general processing and testing enabled.
```

## 🛠️ How It Works

### Hook Integration

1. **PostToolUse Event**: Claude Code sends JSON to stdin when you edit files
2. **File Analysis**: The tool extracts the file path from the JSON
3. **Smart Processing**: Runs appropriate linters/tests based on file context
4. **Exit Codes**: Returns `0` (continue) or `2` (show message) to Claude Code

### Example Hook JSON Input

```json
{
  "hook_event_name": "PostToolUse",
  "tool_name": "Edit", 
  "tool_input": {
    "file_path": "src/models.py"
  }
}
```

### Automation Features

- **🔍 Auto-discovery**: Finds `ruff`, `flake8`, `pylint`, `pytest`, `unittest`
- **📂 Project root detection**: Walks up to find `pyproject.toml`, `setup.py`, etc.
- **🔒 PID-based locking**: Prevents concurrent runs with configurable cooldowns
- **🧠 Smart exclusions**: Context-aware patterns for different file types
- **⏱️ Timeout protection**: Configurable command timeouts

## 🎯 Success Messages

- **Lint success**: "👉 Lints pass. Continue with your task."
- **Test success**: "👉 Tests pass. Continue with your task."

## ⛔ Error Messages

- **Lint failure**: "⛔ BLOCKING: Run 'cd /path/to/project && ruff check .' to fix lint failures"
- **Test failure**: "⛔ BLOCKING: Run 'cd /path/to/project && pytest' to fix test failures"

## 🤖 AI Analysis with Cerebras

Set your API key to enable intelligent file analysis:

```bash
export CEREBRAS_API_KEY="your-api-key-here"
```

### What AI Analysis Provides

- **File type detection**: Distinguishes between business logic, models, configs, tests
- **Context-aware recommendations**: Different advice for linting vs testing
- **Smart reasoning**: Clear explanations for exclusion decisions
- **Configuration suggestions**: Actionable advice for guardrails.yaml patterns

### Conservative Fallback

When the Cerebras API is unavailable, the tool assumes files need full processing (safe default behavior).

## 🔧 Configuration

The tool uses hardcoded sensible defaults optimized for Python projects. No external configuration files are required.

### Built-in Exclusion Patterns

**Global exclusions** (all operations):
- `*.pyc`, `__pycache__/`
- `.venv/**`, `venv/**` 
- `target/**`, `node_modules/**`
- `.git/**`, `.pytest_cache/**`

**Lint exclusions**:
- `migrations/**`
- `*_pb2.py`, `*_pb2_grpc.py`
- `*.generated.py`, `*_generated.py`

**Test exclusions**:
- `conftest.py`
- `test_*.py`, `*_test.py`
- `tests/fixtures/**`

## 🔧 Troubleshooting

### Hooks Not Running

1. **Check registration**: Run `/hooks` in Claude Code
2. **Verify binary path**: Ensure `claude-python-guardrails` is in PATH
3. **Test manually**: Create test JSON and pipe to the command
4. **Check permissions**: Ensure binary is executable
5. **Restart Claude Code**: Hooks load at startup

### Debug Mode

```bash
# Test with sample hook JSON
echo '{"hook_event_name":"PostToolUse","tool_name":"Edit","tool_input":{"file_path":"test.py"}}' | claude-python-guardrails lint

# Use verbose mode
echo '{"hook_event_name":"PostToolUse","tool_name":"Edit","tool_input":{"file_path":"test.py"}}' | claude-python-guardrails -v lint
```

## 📚 Related Documentation

- **[Claude Code Hooks Integration](./CLAUDE_CODE_HOOKS.md)** - Complete setup guide
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

**Claude Python Guardrails** - Purpose-built automation hooks for Claude Code Python projects.