use anyhow::Result;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// Helper to run the CLI binary with JSON input via stdin and return output + exit code
fn run_cli_with_stdin(args: &[&str], stdin_input: &str) -> Result<(String, String, i32)> {
    let mut child = Command::new("cargo")
        .arg("run")
        .arg("--manifest-path")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .arg("--")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(stdin_input.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    Ok((stdout, stderr, exit_code))
}

/// Helper to create Claude Code hook JSON for PostToolUse Edit event
fn create_hook_json(file_path: &str) -> String {
    format!(
        r#"{{
            "hook_event_name": "PostToolUse",
            "tool_name": "Edit",
            "tool_input": {{
                "file_path": "{}"
            }}
        }}"#,
        file_path
    )
}

/// Helper to create Claude Code hook JSON for PostToolUse Write event
fn create_write_hook_json(file_path: &str) -> String {
    format!(
        r#"{{
            "hook_event_name": "PostToolUse",
            "tool_name": "Write",
            "tool_input": {{
                "file_path": "{}"
            }}
        }}"#,
        file_path
    )
}

/// Helper to run the CLI binary and return output + exit code (for commands that don't need stdin)
fn run_cli(args: &[&str]) -> Result<(String, String, i32)> {
    let output = Command::new("cargo")
        .arg("run")
        .arg("--manifest-path")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .arg("--")
        .args(args)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    Ok((stdout, stderr, exit_code))
}

#[test]
fn test_help_command() -> Result<()> {
    let (stdout, _stderr, exit_code) = run_cli(&["--help"])?;

    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Claude Code Python automation hooks"));
    assert!(stdout.contains("lint"));
    assert!(stdout.contains("test"));
    assert!(stdout.contains("analyze"));

    // Should NOT contain removed commands
    assert!(!stdout.contains("check"));
    assert!(!stdout.contains("init"));
    assert!(!stdout.contains("validate"));

    Ok(())
}

#[test]
fn test_version_command() -> Result<()> {
    let (stdout, _stderr, exit_code) = run_cli(&["--version"])?;

    assert_eq!(exit_code, 0);
    assert!(stdout.contains("claude-python-guardrails"));

    Ok(())
}

#[test]
fn test_lint_with_hook_input() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let py_file = temp_dir.path().join("test.py");
    fs::write(&py_file, "print('hello world')")?;

    let hook_json = create_hook_json(py_file.to_str().unwrap());
    let (_stdout, stderr, exit_code) = run_cli_with_stdin(&["lint"], &hook_json)?;

    // lint should complete (exit code 0 or 2, never 1)
    assert!(exit_code == 0 || exit_code == 2);

    // Should handle the hook input without errors about missing stdin
    assert!(!stderr.contains("No JSON input available"));

    Ok(())
}

#[test]
fn test_test_with_hook_input() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let py_file = temp_dir.path().join("test.py");
    fs::write(&py_file, "def test_something(): pass")?;

    let hook_json = create_hook_json(py_file.to_str().unwrap());
    let (_stdout, stderr, exit_code) = run_cli_with_stdin(&["test"], &hook_json)?;

    // test command should handle the hook input properly
    // Exit codes can vary based on whether test tools are available:
    // 0 = no action needed or tools not found (silent)
    // 1 = system error (e.g., tool discovery failed)
    // 2 = test result message (pass or fail)
    assert!(exit_code == 0 || exit_code == 1 || exit_code == 2);

    // Should handle the hook input without errors about missing stdin
    assert!(!stderr.contains("No JSON input available"));

    Ok(())
}

#[test]
fn test_analyze_with_hook_input() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let py_file = temp_dir.path().join("models.py");
    fs::write(&py_file, "class UserModel: pass")?;

    let hook_json = create_hook_json(py_file.to_str().unwrap());
    let (stdout, _stderr, exit_code) = run_cli_with_stdin(&["analyze"], &hook_json)?;

    assert_eq!(exit_code, 0);
    assert!(stdout.contains("📁 File Analysis:"));
    assert!(stdout.contains("🚫 Exclusion Recommendations:"));

    Ok(())
}

#[test]
fn test_analyze_with_hook_input_json_format() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let py_file = temp_dir.path().join("script.py");
    fs::write(&py_file, "import os\nprint('test')")?;

    let hook_json = create_hook_json(py_file.to_str().unwrap());
    let (stdout, _stderr, exit_code) =
        run_cli_with_stdin(&["analyze", "--format", "json"], &hook_json)?;

    assert_eq!(exit_code, 0);

    // Should be valid JSON
    let analysis: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Should have required fields
    assert!(analysis.get("should_exclude_general").is_some());
    assert!(analysis.get("should_exclude_lint").is_some());
    assert!(analysis.get("should_exclude_test").is_some());

    Ok(())
}

#[test]
fn test_hooks_ignore_non_edit_events() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let py_file = temp_dir.path().join("test.py");
    fs::write(&py_file, "print('hello')")?;

    // Test with non-PostToolUse event
    let non_edit_hook = format!(
        r#"{{
            "hook_event_name": "PreToolUse",
            "tool_name": "Edit",
            "tool_input": {{
                "file_path": "{}"
            }}
        }}"#,
        py_file.to_str().unwrap()
    );

    let (_stdout, _stderr, exit_code) = run_cli_with_stdin(&["lint"], &non_edit_hook)?;
    assert_eq!(exit_code, 0); // Should exit quietly

    let (_stdout, _stderr, exit_code) = run_cli_with_stdin(&["test"], &non_edit_hook)?;
    assert_eq!(exit_code, 0); // Should exit quietly

    let (_stdout, _stderr, exit_code) = run_cli_with_stdin(&["analyze"], &non_edit_hook)?;
    assert_eq!(exit_code, 0); // Should exit quietly

    Ok(())
}

#[test]
fn test_hooks_ignore_non_edit_tools() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let py_file = temp_dir.path().join("test.py");
    fs::write(&py_file, "print('hello')")?;

    // Test with non-edit tool
    let non_edit_tool = format!(
        r#"{{
            "hook_event_name": "PostToolUse",
            "tool_name": "Read",
            "tool_input": {{
                "file_path": "{}"
            }}
        }}"#,
        py_file.to_str().unwrap()
    );

    let (_stdout, _stderr, exit_code) = run_cli_with_stdin(&["lint"], &non_edit_tool)?;
    assert_eq!(exit_code, 0); // Should exit quietly

    Ok(())
}

#[test]
fn test_hooks_handle_missing_file() -> Result<()> {
    let hook_json = create_hook_json("/nonexistent/file.py");

    let (_stdout, _stderr, exit_code) = run_cli_with_stdin(&["lint"], &hook_json)?;
    assert_eq!(exit_code, 0); // Should exit quietly for nonexistent files

    let (_stdout, _stderr, exit_code) = run_cli_with_stdin(&["test"], &hook_json)?;
    assert_eq!(exit_code, 0); // Should exit quietly for nonexistent files

    let (_stdout, _stderr, exit_code) = run_cli_with_stdin(&["analyze"], &hook_json)?;
    assert_eq!(exit_code, 0); // Should exit quietly for nonexistent files

    Ok(())
}

#[test]
fn test_hooks_without_stdin() -> Result<()> {
    // Test what happens when hooks are called without stdin input
    // Should exit with code 0 (silent success) when no input is available
    let (_stdout, _stderr, exit_code) = run_cli(&["lint"])?;
    assert_eq!(exit_code, 0);

    let (_stdout, _stderr, exit_code) = run_cli(&["test"])?;
    assert_eq!(exit_code, 0);

    let (_stdout, _stderr, exit_code) = run_cli(&["analyze"])?;
    assert_eq!(exit_code, 0);

    Ok(())
}

#[test]
fn test_verbose_mode_with_hooks() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let py_file = temp_dir.path().join("example.py");
    fs::write(&py_file, "print('test')")?;

    let hook_json = create_hook_json(py_file.to_str().unwrap());

    // Test verbose analyze
    let (_stdout, stderr, exit_code) = run_cli_with_stdin(&["-v", "analyze"], &hook_json)?;
    assert_eq!(exit_code, 0);
    assert!(stderr.contains("🔍 Analyzing file:") || stderr.contains("Analyzing file:"));

    Ok(())
}

#[test]
fn test_different_tool_types() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let py_file = temp_dir.path().join("test.py");
    fs::write(&py_file, "print('hello')")?;

    // Test Write tool (should be processed)
    let write_hook = create_write_hook_json(py_file.to_str().unwrap());
    let (_stdout, _stderr, exit_code) = run_cli_with_stdin(&["lint"], &write_hook)?;
    assert!(exit_code == 0 || exit_code == 2); // Should process

    // Test MultiEdit tool (should be processed)
    let multiedit_hook = format!(
        r#"{{
            "hook_event_name": "PostToolUse",
            "tool_name": "MultiEdit", 
            "tool_input": {{
                "file_path": "{}"
            }}
        }}"#,
        py_file.to_str().unwrap()
    );
    let (_stdout, _stderr, exit_code) = run_cli_with_stdin(&["lint"], &multiedit_hook)?;
    assert!(exit_code == 0 || exit_code == 2); // Should process

    Ok(())
}
