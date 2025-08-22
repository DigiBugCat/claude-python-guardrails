use anyhow::Result;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Helper to run the CLI binary and return output + exit code
fn run_cli(args: &[&str]) -> Result<(String, String, i32)> {
    let output = Command::new("cargo")
        .arg("run")
        .arg("--")
        .args(args)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    Ok((stdout, stderr, exit_code))
}

/// Helper to run CLI in a specific directory
fn run_cli_in_dir(args: &[&str], dir: &std::path::Path) -> Result<(String, String, i32)> {
    let output = Command::new("cargo")
        .arg("run")
        .arg("--")
        .args(args)
        .current_dir(dir)
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
    assert!(stdout.contains("Simple exclusion checker for Python projects"));
    assert!(stdout.contains("check"));
    assert!(stdout.contains("lint"));
    assert!(stdout.contains("test"));
    assert!(stdout.contains("init"));
    assert!(stdout.contains("validate"));

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
fn test_check_command_included_file() -> Result<()> {
    let (_stdout, _stderr, exit_code) = run_cli(&["check", "src/main.py"])?;

    // Exit code 0 means included (not excluded)
    assert_eq!(exit_code, 0);

    Ok(())
}

#[test]
fn test_check_command_excluded_file() -> Result<()> {
    let (_stdout, _stderr, exit_code) = run_cli(&["check", "__pycache__/test.pyc"])?;

    // Exit code 1 means excluded
    assert_eq!(exit_code, 1);

    Ok(())
}

#[test]
fn test_check_command_verbose() -> Result<()> {
    let (stdout, _stderr, exit_code) = run_cli(&["-v", "check", "src/main.py"])?;

    assert_eq!(exit_code, 0);
    assert!(stdout.contains("INCLUDED"));
    assert!(stdout.contains("src/main.py"));

    let (stdout, _stderr, exit_code) = run_cli(&["-v", "check", "__pycache__/test.pyc"])?;

    assert_eq!(exit_code, 1);
    assert!(stdout.contains("EXCLUDED"));
    assert!(stdout.contains("__pycache__/test.pyc"));

    Ok(())
}

#[test]
fn test_lint_command() -> Result<()> {
    // Regular Python file should be included for linting
    let (_stdout, _stderr, exit_code) = run_cli(&["lint", "src/models.py"])?;
    assert_eq!(exit_code, 0);

    // Migration files should be excluded from linting
    let (_stdout, _stderr, exit_code) = run_cli(&["lint", "migrations/0001_initial.py"])?;
    assert_eq!(exit_code, 1);

    // Generated files should be excluded from linting
    let (_stdout, _stderr, exit_code) = run_cli(&["lint", "proto_pb2.py"])?;
    assert_eq!(exit_code, 1);

    Ok(())
}

#[test]
fn test_test_command() -> Result<()> {
    // Regular Python file should be included for testing
    let (_stdout, _stderr, exit_code) = run_cli(&["test", "src/models.py"])?;
    assert_eq!(exit_code, 0);

    // Test files themselves should be excluded from testing
    let (_stdout, _stderr, exit_code) = run_cli(&["test", "test_models.py"])?;
    assert_eq!(exit_code, 1);

    // conftest.py should be excluded from testing
    let (_stdout, _stderr, exit_code) = run_cli(&["test", "conftest.py"])?;
    assert_eq!(exit_code, 1);

    Ok(())
}

#[test]
fn test_init_command() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("guardrails.yaml");

    // Init should create a config file
    let (stdout, _stderr, exit_code) = run_cli_in_dir(&["init"], temp_dir.path())?;

    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Created default configuration"));
    assert!(config_path.exists());

    // Verify the created config is valid YAML
    let content = fs::read_to_string(&config_path)?;
    let _config: serde_yaml::Value = serde_yaml::from_str(&content)?;

    Ok(())
}

#[test]
fn test_init_command_custom_output() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("custom-config.yaml");

    // Init with custom output file
    let (stdout, _stderr, exit_code) =
        run_cli_in_dir(&["init", "-o", "custom-config.yaml"], temp_dir.path())?;

    assert_eq!(exit_code, 0);
    assert!(stdout.contains("custom-config.yaml"));
    assert!(config_path.exists());

    Ok(())
}

#[test]
fn test_validate_command_valid_config() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("guardrails.yaml");

    // First create a valid config
    let valid_config = r#"
exclude:
  patterns:
    - "*.tmp"
    - "__pycache__/"
  python:
    lint_skip:
      - "migrations/**"
    test_skip:
      - "test_*.py"
rules:
  max_file_size: "10MB"
  skip_binary_files: true
  skip_generated_files: true
"#;

    fs::write(&config_path, valid_config)?;

    let (stdout, _stderr, exit_code) =
        run_cli_in_dir(&["validate", "guardrails.yaml"], temp_dir.path())?;

    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Valid configuration"));

    Ok(())
}

#[test]
fn test_validate_command_verbose() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("test-config.yaml");

    let valid_config = r#"
exclude:
  patterns:
    - "*.tmp"
  python:
    lint_skip:
      - "generated/**"
    test_skip:
      - "fixtures/**"
rules:
  max_file_size: "5MB"
  skip_binary_files: true
  skip_generated_files: false
"#;

    fs::write(&config_path, valid_config)?;

    let (stdout, _stderr, exit_code) =
        run_cli_in_dir(&["-v", "validate", "test-config.yaml"], temp_dir.path())?;

    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Configuration is valid"));
    assert!(stdout.contains("Global patterns:"));
    assert!(stdout.contains("Lint skip patterns:"));
    assert!(stdout.contains("Test skip patterns:"));

    Ok(())
}

#[test]
fn test_validate_command_invalid_config() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("invalid.yaml");

    // Create invalid YAML
    let invalid_config = r#"
exclude:
  patterns:
    - "[invalid-glob-pattern"
"#;

    fs::write(&config_path, invalid_config)?;

    let (stdout, _stderr, exit_code) =
        run_cli_in_dir(&["validate", "invalid.yaml"], temp_dir.path())?;

    assert_eq!(exit_code, 1);
    assert!(stdout.contains("Invalid configuration"));

    Ok(())
}

#[test]
fn test_validate_command_missing_config() -> Result<()> {
    let temp_dir = TempDir::new()?;

    let (stdout, _stderr, exit_code) =
        run_cli_in_dir(&["validate", "nonexistent.yaml"], temp_dir.path())?;

    assert_eq!(exit_code, 1);
    assert!(stdout.contains("Invalid configuration"));

    Ok(())
}

#[test]
fn test_custom_config_file() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("custom.yaml");

    // Create custom config that excludes .custom files
    let custom_config = r#"
exclude:
  patterns:
    - "*.custom"
  python:
    lint_skip: []
    test_skip: []
rules:
  max_file_size: "10MB"
  skip_binary_files: false
  skip_generated_files: false
"#;

    fs::write(&config_path, custom_config)?;

    // Test with custom config
    let (_stdout, _stderr, exit_code) = run_cli_in_dir(
        &["-c", "custom.yaml", "check", "test.custom"],
        temp_dir.path(),
    )?;

    assert_eq!(exit_code, 1); // Should be excluded

    // Test without custom config (should be included)
    let (_stdout, _stderr, exit_code) = run_cli_in_dir(&["check", "test.custom"], temp_dir.path())?;

    assert_eq!(exit_code, 0); // Should be included with default config

    Ok(())
}

#[test]
fn test_config_file_auto_discovery() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("guardrails.yaml");

    // Create config that excludes .auto files
    let auto_config = r#"
exclude:
  patterns:
    - "*.auto"
  python:
    lint_skip: []
    test_skip: []
rules:
  max_file_size: "10MB"
  skip_binary_files: false
  skip_generated_files: false
"#;

    fs::write(&config_path, auto_config)?;

    // Should auto-discover guardrails.yaml in current directory
    let (_stdout, _stderr, exit_code) = run_cli_in_dir(&["check", "test.auto"], temp_dir.path())?;

    assert_eq!(exit_code, 1); // Should be excluded per custom config

    Ok(())
}

#[test]
fn test_different_file_types() -> Result<()> {
    // Test Python files
    let (_stdout, _stderr, exit_code) = run_cli(&["check", "example.py"])?;
    assert_eq!(exit_code, 0);

    // Test non-Python files
    let (_stdout, _stderr, exit_code) = run_cli(&["check", "example.rs"])?;
    assert_eq!(exit_code, 0);

    // Test common exclusions
    let (_stdout, _stderr, exit_code) = run_cli(&["check", ".venv/lib/python.py"])?;
    assert_eq!(exit_code, 1);

    let (_stdout, _stderr, exit_code) = run_cli(&["check", "target/debug/main"])?;
    assert_eq!(exit_code, 1);

    let (_stdout, _stderr, exit_code) = run_cli(&["check", "node_modules/package/index.js"])?;
    assert_eq!(exit_code, 1);

    Ok(())
}

#[test]
fn test_error_handling_invalid_args() -> Result<()> {
    // Test invalid subcommand
    let (_stdout, _stderr, exit_code) = run_cli(&["invalid-command"])?;
    assert_ne!(exit_code, 0);

    // Test missing file argument
    let (_stdout, _stderr, exit_code) = run_cli(&["check"])?;
    assert_ne!(exit_code, 0);

    let (_stdout, _stderr, exit_code) = run_cli(&["lint"])?;
    assert_ne!(exit_code, 0);

    let (_stdout, _stderr, exit_code) = run_cli(&["test"])?;
    assert_ne!(exit_code, 0);

    Ok(())
}

#[test]
fn test_concurrent_operations() -> Result<()> {
    // Test that multiple operations can run concurrently without issues
    let handles: Vec<_> = (0..5)
        .map(|i| {
            std::thread::spawn(move || {
                let file_path = format!("concurrent_test_{i}.py");
                run_cli(&["check", &file_path])
            })
        })
        .collect();

    for handle in handles {
        let (_, _, exit_code) = handle.join().unwrap()?;
        assert_eq!(exit_code, 0); // All should be included
    }

    Ok(())
}
