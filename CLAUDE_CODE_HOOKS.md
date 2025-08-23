# Claude Code Hooks Integration

This document explains how to integrate **claude-python-guardrails** with Claude Code hooks for automatic linting and testing.

## 🚀 Quick Setup

Replace your existing generic smart-lint/smart-test hooks with our Python-optimized automation system.

### 1. Install the Binary

```bash
# From project root
cargo build --release

# Install system-wide (recommended)
cargo install --path .

# Or use the binary directly
./target/release/claude-python-guardrails
```

### 2. Configure Claude Code Hooks

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
            "command": "claude-python-guardrails smart-lint",
            "timeout": 30
          },
          {
            "type": "command",
            "command": "claude-python-guardrails smart-test",
            "timeout": 60
          }
        ]
      }
    ]
  }
}
```

### 3. Restart Claude Code

Hooks are loaded at startup, so restart Claude Code to activate the new configuration.

### 4. Verify Installation

1. Run `/hooks` in Claude Code to see registered hooks
2. Edit a Python file to trigger automatic linting/testing
3. Check for success messages: "👉 Lints pass. Continue with your task."

## 📋 Configuration Options

### User-Level (Global)
**File**: `~/.claude/settings.json`
**Scope**: All Claude Code sessions

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write|NotebookEdit",
        "hooks": [
          {
            "type": "command",
            "command": "claude-python-guardrails smart-lint"
          },
          {
            "type": "command", 
            "command": "claude-python-guardrails smart-test"
          }
        ]
      }
    ]
  }
}
```

### Project-Level (Per Project)
**File**: `<project>/.claude/settings.json`
**Scope**: Specific project only

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write|NotebookEdit",
        "hooks": [
          {
            "type": "command",
            "command": "$CLAUDE_PROJECT_DIR/.claude/bin/claude-python-guardrails smart-lint"
          },
          {
            "type": "command",
            "command": "$CLAUDE_PROJECT_DIR/.claude/bin/claude-python-guardrails smart-test"
          }
        ]
      }
    ]
  }
}
```

### Granular Control
**Separate hooks for different operations:**

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|MultiEdit",
        "hooks": [
          {
            "type": "command",
            "command": "claude-python-guardrails smart-lint",
            "timeout": 30
          }
        ]
      },
      {
        "matcher": "Write",
        "hooks": [
          {
            "type": "command",
            "command": "claude-python-guardrails smart-test",
            "timeout": 60
          }
        ]
      }
    ]
  }
}
```

## 🎛️ Advanced Configuration

### Python Files Only
**Only run hooks on Python files:**

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write|NotebookEdit",
        "hooks": [
          {
            "type": "command",
            "command": "echo \"$1\" | jq -r '.tool_input.file_path // .tool_input.notebook_path' | grep -q '\\.py$' && claude-python-guardrails smart-lint || true"
          }
        ]
      }
    ]
  }
}
```

### Custom Configuration Path
**Use project-specific guardrails.yaml:**

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write|NotebookEdit",
        "hooks": [
          {
            "type": "command",
            "command": "claude-python-guardrails -c $CLAUDE_PROJECT_DIR/.claude/guardrails.yaml smart-lint"
          }
        ]
      }
    ]
  }
}
```

### Conditional Execution
**Skip certain file types:**

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write|NotebookEdit",
        "hooks": [
          {
            "type": "command",
            "command": "file_path=$(echo \"$1\" | jq -r '.tool_input.file_path // .tool_input.notebook_path'); [[ ! \"$file_path\" =~ \\.(md|txt|json|yaml)$ ]] && claude-python-guardrails smart-lint || exit 0"
          }
        ]
      }
    ]
  }
}
```

## 🔧 Hook Behavior

### Exit Codes (Matches Claude Code Protocol)
- **Exit 0**: Silent success (no linter found, file excluded, etc.)
- **Exit 2**: Show message to user (success or blocking error)

### Success Messages
- **Lint success**: "👉 Lints pass. Continue with your task."
- **Test success**: "👉 Tests pass. Continue with your task."

### Error Messages
- **Lint failure**: "⛔ BLOCKING: Run 'cd /path/to/project && ruff check .' to fix lint failures"
- **Test failure**: "⛔ BLOCKING: Run 'cd /path/to/project && pytest' to fix test failures"

### Automation Features
✅ **Auto-discovery**: Finds `ruff`, `flake8`, `pylint`, `pytest`, `unittest`  
✅ **Project root detection**: Walks up to find `pyproject.toml`, `setup.py`, etc.  
✅ **PID-based locking**: Prevents concurrent runs (configurable cooldown)  
✅ **Smart exclusions**: Respects `guardrails.yaml` patterns  
✅ **Timeout protection**: Configurable command timeouts  

## 🔍 Troubleshooting

### Hooks Not Running
1. **Check registration**: Run `/hooks` in Claude Code
2. **Verify binary path**: Ensure `claude-python-guardrails` is in PATH
3. **Test manually**: Run the hook command directly
4. **Check permissions**: Ensure binary is executable
5. **Restart Claude Code**: Hooks load at startup

### Debug Mode
**Enable debug output:**
```bash
# Start Claude Code with debug logging
claude --debug

# Or set environment variable
export CLAUDE_HOOKS_DEBUG=1
```

### Manual Testing
**Test hook commands directly:**
```bash
# Test smart-lint with sample JSON
echo '{"hook_event_name":"PostToolUse","tool_name":"Edit","tool_input":{"file_path":"test.py"}}' | claude-python-guardrails smart-lint

# Test smart-test
echo '{"hook_event_name":"PostToolUse","tool_name":"Write","tool_input":{"file_path":"test.py"}}' | claude-python-guardrails smart-test
```

## 🆚 Migration from Generic Hooks

### Before (Generic Bash Hooks)
```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "~/.claude/hooks/smart-lint.sh"
          },
          {
            "type": "command",
            "command": "~/.claude/hooks/smart-test.sh"
          }
        ]
      }
    ]
  }
}
```

### After (Python-Specific Binary)
```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write|NotebookEdit",
        "hooks": [
          {
            "type": "command",
            "command": "claude-python-guardrails smart-lint"
          },
          {
            "type": "command",
            "command": "claude-python-guardrails smart-test"
          }
        ]
      }
    ]
  }
}
```

### Benefits of Migration
- **🎯 Python-focused**: No unnecessary multi-language detection overhead
- **⚡ Faster execution**: Compiled Rust vs interpreted bash
- **🧠 Smarter exclusions**: Context-aware YAML patterns
- **🔒 Better concurrency**: PID-based locking with cooldowns
- **📊 Structured errors**: Clear, actionable error messages
- **🛡️ Type safety**: Rust guarantees vs shell script brittleness

## 📖 Related Documentation

- **Main README**: [README.md](./README.md) - General tool usage
- **Configuration**: [guardrails.yaml](./guardrails.yaml) - Exclusion patterns  
- **Claude Code Hooks**: [Official Documentation](https://docs.anthropic.com/en/docs/claude-code/hooks-reference)

## 🤝 Contributing

If you encounter issues with Claude Code integration:

1. **Check existing issues**: Search for similar problems
2. **Test locally**: Verify the binary works outside Claude Code
3. **Provide details**: Include hook configuration and error messages
4. **Submit PR**: Improvements to hook integration welcome

---

**🐍 Python-specific automation for Claude Code - faster, smarter, and more reliable than generic hooks!**