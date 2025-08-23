use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, ExitStatus};
use std::time::Duration;

use crate::discovery::{PythonLinter, PythonProject, PythonTester};
use crate::locking::LockGuard;
use crate::protocol::HookInput;
use crate::GuardrailsChecker;

/// Configuration for automation behavior
#[derive(Debug, Clone)]
pub struct AutomationConfig {
    pub lint_enabled: bool,
    pub test_enabled: bool,
    pub lint_cooldown_seconds: u64,
    pub test_cooldown_seconds: u64,
    pub lint_timeout_seconds: u64,
    pub test_timeout_seconds: u64,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            lint_enabled: true,
            test_enabled: true,
            lint_cooldown_seconds: 2,
            test_cooldown_seconds: 2,
            lint_timeout_seconds: 20,
            test_timeout_seconds: 20,
        }
    }
}

/// Main automation orchestrator
pub struct AutomationRunner {
    config: AutomationConfig,
    checker: GuardrailsChecker,
}

/// Result of running an automation command
#[derive(Debug)]
pub enum AutomationResult {
    /// No command found or file excluded - exit silently
    NoAction,
    /// Command succeeded - show success message and exit 2
    Success(String),
    /// Command failed - show error message and exit 2
    Failure(String),
    /// Should skip due to concurrency control
    Skipped,
}

impl AutomationRunner {
    /// Create a new automation runner
    pub fn new(config: AutomationConfig, checker: GuardrailsChecker) -> Self {
        Self { config, checker }
    }

    /// Handle smart-lint command from Claude Code hook
    pub fn handle_smart_lint(&self) -> Result<AutomationResult> {
        if !self.config.lint_enabled {
            log::debug!("Smart lint is disabled");
            return Ok(AutomationResult::NoAction);
        }

        let hook_input = match HookInput::from_stdin() {
            Ok(input) => input,
            Err(_) => {
                log::debug!("No input available on stdin");
                return Ok(AutomationResult::NoAction);
            }
        };

        if !hook_input.should_process() {
            log::debug!("Ignoring event type: {}", hook_input.hook_event_name);
            return Ok(AutomationResult::NoAction);
        }

        let file_path = match hook_input.file_path() {
            Some(path) => path,
            None => {
                log::debug!("No file path found in JSON input");
                return Ok(AutomationResult::NoAction);
            }
        };

        if !file_path.exists() {
            log::debug!("File does not exist: {}", file_path.display());
            return Ok(AutomationResult::NoAction);
        }

        // Check if file should be excluded from linting
        if self.checker.should_exclude_lint(&file_path)? {
            log::debug!("File should be skipped: {}", file_path.display());
            return Ok(AutomationResult::NoAction);
        }

        // Change to file's directory
        let file_dir = file_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        // Discover Python project
        let project = PythonProject::discover(&file_dir)?;

        // Try to acquire lock
        let _guard = match LockGuard::try_acquire(
            &project.root,
            "lint",
            self.config.lint_cooldown_seconds,
        )? {
            Some(guard) => guard,
            None => return Ok(AutomationResult::Skipped),
        };

        // Find and run linter
        self.run_lint_command(&project)
    }

    /// Handle smart-test command from Claude Code hook
    pub fn handle_smart_test(&self) -> Result<AutomationResult> {
        if !self.config.test_enabled {
            log::debug!("Smart test is disabled");
            return Ok(AutomationResult::NoAction);
        }

        let hook_input = match HookInput::from_stdin() {
            Ok(input) => input,
            Err(_) => {
                log::debug!("No input available on stdin");
                return Ok(AutomationResult::NoAction);
            }
        };

        if !hook_input.should_process() {
            log::debug!("Ignoring event type: {}", hook_input.hook_event_name);
            return Ok(AutomationResult::NoAction);
        }

        let file_path = match hook_input.file_path() {
            Some(path) => path,
            None => {
                log::debug!("No file path found in JSON input");
                return Ok(AutomationResult::NoAction);
            }
        };

        if !file_path.exists() {
            log::debug!("File does not exist: {}", file_path.display());
            return Ok(AutomationResult::NoAction);
        }

        // Check if file should be excluded from testing
        if self.checker.should_exclude_test(&file_path)? {
            log::debug!("File should be skipped: {}", file_path.display());
            return Ok(AutomationResult::NoAction);
        }

        // Change to file's directory
        let file_dir = file_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        // Discover Python project
        let project = PythonProject::discover(&file_dir)?;

        // Try to acquire lock
        let _guard = match LockGuard::try_acquire(
            &project.root,
            "test",
            self.config.test_cooldown_seconds,
        )? {
            Some(guard) => guard,
            None => return Ok(AutomationResult::Skipped),
        };

        // Find and run test command
        self.run_test_command(&project)
    }

    /// Run linting command for the project
    fn run_lint_command(&self, project: &PythonProject) -> Result<AutomationResult> {
        let linter = match project.preferred_linter() {
            Some(linter) => linter,
            None => {
                log::debug!("No Python linter found in project");
                return Ok(AutomationResult::NoAction);
            }
        };

        log::debug!("Running {} in {}", linter.display_name(), project.root.display());

        let success = self.run_command_with_timeout(
            linter.command(),
            &linter.args(),
            &project.root,
            self.config.lint_timeout_seconds,
        )?;

        if success {
            Ok(AutomationResult::Success(
                "👉 Lints pass. Continue with your task.".to_string(),
            ))
        } else {
            Ok(AutomationResult::Failure(format!(
                "⛔ BLOCKING: Run 'cd {} && {}' to fix lint failures",
                project.root.display(),
                linter.display_name()
            )))
        }
    }

    /// Run test command for the project
    fn run_test_command(&self, project: &PythonProject) -> Result<AutomationResult> {
        let tester = match project.preferred_tester() {
            Some(tester) => tester,
            None => {
                log::debug!("No Python tester found in project");
                return Ok(AutomationResult::NoAction);
            }
        };

        log::debug!("Running {} in {}", tester.display_name(), project.root.display());

        let success = self.run_command_with_timeout(
            tester.command(),
            &tester.args(),
            &project.root,
            self.config.test_timeout_seconds,
        )?;

        if success {
            Ok(AutomationResult::Success(
                "👉 Tests pass. Continue with your task.".to_string(),
            ))
        } else {
            Ok(AutomationResult::Failure(format!(
                "⛔ BLOCKING: Run 'cd {} && {}' to fix test failures",
                project.root.display(),
                tester.display_name()
            )))
        }
    }

    /// Run a command with timeout, returning true if successful
    fn run_command_with_timeout(
        &self,
        command: &str,
        args: &[&str],
        working_dir: &Path,
        timeout_seconds: u64,
    ) -> Result<bool> {
        // Create command
        let mut cmd = Command::new(command);
        cmd.args(args)
            .current_dir(working_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        // Spawn process
        let mut child = cmd.spawn().context("Failed to spawn command")?;

        // Wait with timeout
        let result = self.wait_with_timeout(&mut child, Duration::from_secs(timeout_seconds))?;

        match result {
            Some(status) => Ok(status.success()),
            None => {
                // Timeout - kill the process
                let _ = child.kill();
                let _ = child.wait();
                Ok(false)
            }
        }
    }

    /// Wait for process with timeout
    fn wait_with_timeout(
        &self,
        child: &mut std::process::Child,
        timeout: Duration,
    ) -> Result<Option<ExitStatus>> {
        use std::thread;
        use std::time::Instant;

        let start = Instant::now();

        loop {
            match child.try_wait()? {
                Some(status) => return Ok(Some(status)),
                None => {
                    if start.elapsed() >= timeout {
                        return Ok(None);
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
}

impl AutomationResult {
    /// Convert to appropriate exit code for Claude Code hooks
    pub fn exit_code(&self) -> i32 {
        match self {
            AutomationResult::NoAction | AutomationResult::Skipped => 0,
            AutomationResult::Success(_) | AutomationResult::Failure(_) => 2,
        }
    }

    /// Get message to display to user (if any)
    pub fn message(&self) -> Option<&str> {
        match self {
            AutomationResult::Success(msg) | AutomationResult::Failure(msg) => Some(msg),
            AutomationResult::NoAction | AutomationResult::Skipped => None,
        }
    }

    /// Check if this represents a failure
    pub fn is_failure(&self) -> bool {
        matches!(self, AutomationResult::Failure(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::default_config;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_runner() -> AutomationRunner {
        let config = AutomationConfig::default();
        let checker = GuardrailsChecker::from_config(default_config()).unwrap();
        AutomationRunner::new(config, checker)
    }

    #[test]
    fn test_automation_config_defaults() {
        let config = AutomationConfig::default();
        assert!(config.lint_enabled);
        assert!(config.test_enabled);
        assert_eq!(config.lint_cooldown_seconds, 2);
        assert_eq!(config.test_cooldown_seconds, 2);
        assert_eq!(config.lint_timeout_seconds, 20);
        assert_eq!(config.test_timeout_seconds, 20);
    }

    #[test]
    fn test_automation_result_exit_codes() {
        assert_eq!(AutomationResult::NoAction.exit_code(), 0);
        assert_eq!(AutomationResult::Skipped.exit_code(), 0);
        assert_eq!(
            AutomationResult::Success("test".to_string()).exit_code(),
            2
        );
        assert_eq!(
            AutomationResult::Failure("test".to_string()).exit_code(),
            2
        );
    }

    #[test]
    fn test_automation_result_messages() {
        assert_eq!(AutomationResult::NoAction.message(), None);
        assert_eq!(AutomationResult::Skipped.message(), None);
        assert_eq!(
            AutomationResult::Success("success".to_string()).message(),
            Some("success")
        );
        assert_eq!(
            AutomationResult::Failure("failure".to_string()).message(),
            Some("failure")
        );
    }

    #[test]
    fn test_command_timeout() -> Result<()> {
        let runner = create_test_runner();
        let temp_dir = TempDir::new()?;

        // Test successful quick command
        let success = runner.run_command_with_timeout("echo", &["hello"], temp_dir.path(), 5)?;
        assert!(success);

        // Test command that should timeout (sleep for longer than timeout)
        let success = runner.run_command_with_timeout("sleep", &["10"], temp_dir.path(), 1)?;
        assert!(!success);

        Ok(())
    }

    #[test]
    fn test_runner_creation() {
        let config = AutomationConfig {
            lint_enabled: false,
            test_enabled: true,
            lint_cooldown_seconds: 5,
            test_cooldown_seconds: 3,
            lint_timeout_seconds: 30,
            test_timeout_seconds: 25,
        };

        let checker = GuardrailsChecker::from_config(default_config()).unwrap();
        let runner = AutomationRunner::new(config.clone(), checker);

        assert!(!runner.config.lint_enabled);
        assert!(runner.config.test_enabled);
        assert_eq!(runner.config.lint_cooldown_seconds, 5);
        assert_eq!(runner.config.test_cooldown_seconds, 3);
    }
}