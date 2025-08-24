use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, ExitStatus};
use std::time::Duration;

use crate::cerebras::{CerebrasConfig, SmartExclusionAnalyzer};
use crate::discovery::PythonProject;
use crate::locking::LockGuard;
use crate::protocol::HookInput;
use crate::GuardrailsChecker;

/// Output from running a command including exit status and captured output
#[derive(Debug)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

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
    analyzer: SmartExclusionAnalyzer,
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
        let cerebras_config = CerebrasConfig::default();
        let analyzer = SmartExclusionAnalyzer::new(cerebras_config);

        Self {
            config,
            checker,
            analyzer,
        }
    }

    /// Handle smart-lint command from Claude Code hook
    pub async fn handle_smart_lint(&self) -> Result<AutomationResult> {
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
        let _guard =
            match LockGuard::try_acquire(&project.root, "lint", self.config.lint_cooldown_seconds)?
            {
                Some(guard) => guard,
                None => return Ok(AutomationResult::Skipped),
            };

        // Find and run linter for the specific file
        self.run_lint_command(&project, &file_path).await
    }

    /// Handle smart-test command from Claude Code hook
    pub async fn handle_smart_test(&self) -> Result<AutomationResult> {
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
        let _guard =
            match LockGuard::try_acquire(&project.root, "test", self.config.test_cooldown_seconds)?
            {
                Some(guard) => guard,
                None => return Ok(AutomationResult::Skipped),
            };

        // Find and run test command for the specific file
        self.run_test_command(&project, &file_path).await
    }

    /// Run linting command for a specific file in the project
    async fn run_lint_command(
        &self,
        project: &PythonProject,
        source_file: &Path,
    ) -> Result<AutomationResult> {
        let linter = match project.preferred_linter() {
            Some(linter) => linter,
            None => {
                log::debug!("No Python linter found in project");
                return Ok(AutomationResult::NoAction);
            }
        };

        // Only lint Python files (.py extension)
        if source_file.extension().and_then(|ext| ext.to_str()) != Some("py") {
            log::debug!(
                "Skipping linting for non-Python file: {}",
                source_file.display()
            );
            return Ok(AutomationResult::NoAction);
        }

        log::debug!(
            "Running {} on file: {}",
            linter.display_name(),
            source_file.display()
        );

        let file_path_str = source_file.to_string_lossy();

        // Step 1: Try formatting first (if formatter available)
        if let Some(formatter) = project.preferred_formatter() {
            log::debug!("Formatting file with {}", formatter.display_name());
            let format_args = formatter.format_args(&file_path_str);
            let format_args_str: Vec<&str> = format_args.iter().map(|s| s.as_str()).collect();

            let _format_output = self.run_command_with_timeout(
                formatter.command(),
                &format_args_str,
                &project.root,
                self.config.lint_timeout_seconds,
            )?;
            // Don't fail on format errors - just log and continue
            log::debug!("Formatting completed, now checking for lint issues");
        }

        // Step 2: Try auto-fix linting issues (if supported)
        if linter.supports_autofix() {
            log::debug!("Attempting auto-fix with {}", linter.command());
            let fix_args = linter.fix_args(&file_path_str);
            let fix_args_str: Vec<&str> = fix_args.iter().map(|s| s.as_str()).collect();

            let _fix_output = self.run_command_with_timeout(
                linter.command(),
                &fix_args_str,
                &project.root,
                self.config.lint_timeout_seconds,
            )?;
            // Don't fail on fix errors - just log and continue to check
            log::debug!("Auto-fix completed, now checking for remaining issues");
        }

        // Step 3: Run linter on the specific file to check remaining issues
        let file_args = linter.file_args(&file_path_str);
        let file_args_str: Vec<&str> = file_args.iter().map(|s| s.as_str()).collect();

        let output = self.run_command_with_timeout(
            linter.command(),
            &file_args_str,
            &project.root,
            self.config.lint_timeout_seconds,
        )?;

        if output.success {
            let has_formatter = project.preferred_formatter().is_some();
            let has_autofix = linter.supports_autofix();

            let message = match (has_formatter, has_autofix) {
                (true, true) => {
                    "✨ Formatted, auto-fixed, and verified. Continue with your task.".to_string()
                }
                (true, false) => {
                    "✨ Formatted and lints verified. Continue with your task.".to_string()
                }
                (false, true) => {
                    "✨ Auto-fixed lint issues and verified. Continue with your task.".to_string()
                }
                (false, false) => "👉 Lints pass. Continue with your task.".to_string(),
            };
            Ok(AutomationResult::Success(message))
        } else {
            // Use AI analysis for comprehensive lint failure analysis
            let combined_output = if !output.stderr.is_empty() {
                format!("{}\n{}", output.stdout, output.stderr)
            } else {
                output.stdout
            };

            // Run AI analysis if available
            let message = if !combined_output.trim().is_empty() {
                match self
                    .analyzer
                    .analyze_lint_output(&combined_output, Some(&project.root))
                    .await
                {
                    Ok(analysis) => {
                        let mut detailed_message = String::new();
                        detailed_message.push_str("⛔ LINT ISSUES FOUND:\n\n");

                        if analysis.has_real_issues {
                            // Show filtered output with only real issues
                            if !analysis.filtered_output.trim().is_empty() {
                                detailed_message.push_str(&analysis.filtered_output);
                                detailed_message.push_str("\n\n");
                            }

                            // Add AI reasoning about whether linter is being overzealous
                            if !analysis.reasoning.trim().is_empty() {
                                detailed_message.push_str("💡 **Analysis:**\n");
                                detailed_message.push_str(&analysis.reasoning);

                                // Check if linter might be overzealous
                                if analysis.reasoning.contains("style")
                                    || analysis.reasoning.contains("convention")
                                    || analysis.reasoning.contains("optional")
                                {
                                    detailed_message.push_str("\n\n🤔 **Note:** Some of these might be style preferences rather than real issues.");
                                }
                            }
                        } else {
                            detailed_message.push_str("✅ **AI Analysis Result:**\n");
                            detailed_message.push_str(&analysis.reasoning);
                            detailed_message.push_str(
                                "\n\n👉 Linter appears overzealous. You can continue with your task.",
                            );

                            // Return success if no real issues found
                            return Ok(AutomationResult::Success(detailed_message));
                        }

                        detailed_message
                    }
                    Err(e) => {
                        log::warn!("AI analysis failed: {}", e);
                        // Fallback to showing raw output
                        format!(
                            "⛔ LINT FAILURES:\n\n{}\n\n⚠️ Could not determine if linter is being overzealous (AI unavailable)",
                            combined_output.trim()
                        )
                    }
                }
            } else {
                "⛔ Lint check failed".to_string()
            };

            Ok(AutomationResult::Failure(message))
        }
    }

    /// Run test command for a specific file in the project
    async fn run_test_command(
        &self,
        project: &PythonProject,
        source_file: &Path,
    ) -> Result<AutomationResult> {
        let tester = match project.preferred_tester() {
            Some(tester) => tester,
            None => {
                log::debug!("No Python tester found in project");
                return Ok(AutomationResult::NoAction);
            }
        };

        // Only test Python files (.py extension)
        if source_file.extension().and_then(|ext| ext.to_str()) != Some("py") {
            log::debug!(
                "Skipping tests for non-Python file: {}",
                source_file.display()
            );
            return Ok(AutomationResult::NoAction);
        }

        // Find the corresponding test file for the edited source file
        let test_file = match self.find_test_file_for_source(source_file, &project.root) {
            Some(test_file) => test_file,
            None => {
                log::debug!("No test file found for: {}", source_file.display());
                return Ok(AutomationResult::Success(format!(
                    "📝 No tests found for {}.\n\n💡 Consider creating tests at:\n  • tests/test_{}.py\n  • tests/unit/test_{}.py\n\n👉 Continue with your task.",
                    source_file.file_name().unwrap_or_default().to_string_lossy(),
                    source_file.file_stem().unwrap_or_default().to_string_lossy(),
                    source_file.file_stem().unwrap_or_default().to_string_lossy()
                )));
            }
        };

        log::debug!(
            "Running {} on test file: {}",
            tester.display_name(),
            test_file.display()
        );

        // Create command arguments that include the specific test file
        let base_args = tester.args();
        let test_file_str = test_file.to_string_lossy();

        // Build combined args by collecting base args and adding the test file
        let mut combined_args: Vec<&str> = base_args.to_vec();
        combined_args.push(&test_file_str);

        let output = self.run_command_with_timeout(
            tester.command(),
            &combined_args,
            &project.root,
            self.config.test_timeout_seconds,
        )?;

        // Always combine stdout/stderr output for analysis
        let combined_output = if !output.stderr.is_empty() {
            format!("{}\n{}", output.stdout, output.stderr)
        } else {
            output.stdout
        };

        // Now that tests have been run, analyze the output with AI
        // We already have the source file as a parameter, no need to search for it

        match self
            .analyzer
            .analyze_test_output(&combined_output, &project.root, Some(source_file))
            .await
        {
            Ok(analysis) => {
                if output.success {
                    // Tests passed - check for edge case coverage
                    let mut message = String::new();
                    message.push_str("✅ Tests pass!\n\n");

                    // Check if edge cases are missing
                    let missing_edge_cases = analysis.coverage_analysis.contains("edge case")
                        || analysis.coverage_analysis.contains("error handling")
                        || analysis.coverage_analysis.contains("boundary")
                        || analysis.coverage_analysis.contains("exception")
                        || analysis.quality_assessment.contains("edge case")
                        || analysis.quality_assessment.contains("error handling")
                        || analysis.quality_assessment.contains("failure");

                    if !analysis.coverage_analysis.is_empty() {
                        message.push_str(&format!(
                            "📋 **Coverage**: {}\n",
                            analysis.coverage_analysis
                        ));
                    }

                    if !analysis.quality_assessment.is_empty() {
                        message.push_str(&format!(
                            "🎯 **Quality**: {}\n\n",
                            analysis.quality_assessment
                        ));
                    }

                    if missing_edge_cases {
                        message.push_str("⚠️ **STRONGLY CONSIDER**: Implement the missing edge cases and error handling tests mentioned above. Robust code requires comprehensive test coverage including failure scenarios.\n\n");
                    }

                    message.push_str("👉 Continue with your task.");

                    Ok(AutomationResult::Success(message))
                } else {
                    // Tests failed - provide comprehensive failure analysis
                    let mut detailed_message = String::new();
                    detailed_message.push_str("⛔ TESTS FAILED:\n\n");

                    // Add AI analysis
                    detailed_message
                        .push_str(&format!("📊 **Analysis**: {}\n\n", analysis.summary));

                    if !analysis.failed_tests.is_empty() {
                        detailed_message.push_str("🔍 **Failed Tests**:\n");
                        for test in &analysis.failed_tests {
                            detailed_message.push_str(&format!(
                                "  • {}: {} - {}\n    💡 Fix: {}\n",
                                test.test_name,
                                test.error_type,
                                test.error_message,
                                test.suggested_fix
                            ));
                        }
                        detailed_message.push('\n');
                    }

                    if !analysis.coverage_analysis.is_empty() {
                        detailed_message.push_str(&format!(
                            "📋 **Coverage**: {}\n\n",
                            analysis.coverage_analysis
                        ));
                    }

                    detailed_message.push_str("📄 **Full Output**:\n");
                    detailed_message.push_str(combined_output.trim());

                    // Add the blocking message
                    detailed_message
                        .push_str("\n\n⛔ Must fix all test failures before continuing");

                    Ok(AutomationResult::Failure(detailed_message))
                }
            }
            Err(e) => {
                log::warn!("AI analysis failed: {}", e);
                // Fallback to basic behavior when AI analysis fails
                if output.success {
                    Ok(AutomationResult::Success(
                        "👉 Tests pass. Continue with your task.".to_string(),
                    ))
                } else if !combined_output.trim().is_empty() {
                    Ok(AutomationResult::Failure(format!(
                        "⛔ TESTS FAILED:\n\n{}\n\n⛔ Must fix all test failures before continuing",
                        combined_output.trim()
                    )))
                } else {
                    Ok(AutomationResult::Failure(
                        "⛔ Test failures detected. Must fix before continuing".to_string(),
                    ))
                }
            }
        }
    }

    /// Run a command with timeout, capturing output
    fn run_command_with_timeout(
        &self,
        command: &str,
        args: &[&str],
        working_dir: &Path,
        timeout_seconds: u64,
    ) -> Result<CommandOutput> {
        // Create command
        let mut cmd = Command::new(command);
        cmd.args(args)
            .current_dir(working_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // Spawn process
        let mut child = cmd.spawn().context("Failed to spawn command")?;

        // Wait with timeout
        let result = self.wait_with_timeout(&mut child, Duration::from_secs(timeout_seconds))?;

        match result {
            Some(status) => {
                // Get output
                let output = child
                    .wait_with_output()
                    .context("Failed to get command output")?;
                Ok(CommandOutput {
                    success: status.success(),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                })
            }
            None => {
                // Timeout - kill the process
                let _ = child.kill();
                let _ = child.wait();
                Ok(CommandOutput {
                    success: false,
                    stdout: String::new(),
                    stderr: "Command timed out".to_string(),
                })
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

    /// Find the corresponding test file for a given source file
    fn find_test_file_for_source(
        &self,
        source_file: &Path,
        project_root: &Path,
    ) -> Option<std::path::PathBuf> {
        let source_name = source_file.file_stem()?.to_str()?;

        // Check if the edited file is already a test file
        if let Some(file_name) = source_file.file_name()?.to_str() {
            if file_name.starts_with("test_")
                || file_name.contains("_test.py")
                || file_name.contains("test.py")
            {
                // If it's already a test file, return it as the test to run
                return Some(source_file.to_path_buf());
            }
        }

        // List of possible test file patterns
        let test_patterns = vec![
            format!("test_{}.py", source_name),
            format!("{}_test.py", source_name),
            format!("test{}.py", source_name),
        ];

        // Base test directories to search recursively
        let base_test_directories = vec![
            project_root.join("tests"),
            project_root.join("test"),
            project_root.to_path_buf(), // Same directory as source
            source_file.parent()?.to_path_buf(), // Source file's directory
        ];

        // Search recursively in test directories
        for base_dir in &base_test_directories {
            if let Some(test_file) = Self::find_test_file_recursive(base_dir, &test_patterns) {
                log::debug!("Found test file: {}", test_file.display());
                return Some(test_file);
            }
        }

        log::debug!(
            "No test file found for source file: {}",
            source_file.display()
        );
        None
    }

    /// Recursively search for test files matching the given patterns in a directory tree
    fn find_test_file_recursive(dir: &Path, patterns: &[String]) -> Option<std::path::PathBuf> {
        if !dir.exists() || !dir.is_dir() {
            return None;
        }

        // First, check current directory for matching test files
        for pattern in patterns {
            let test_file_path = dir.join(pattern);
            if test_file_path.exists() && test_file_path.is_file() {
                return Some(test_file_path);
            }
        }

        // Then recursively search subdirectories
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip hidden directories and common non-test directories
                    if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                        if dir_name.starts_with('.')
                            || dir_name == "__pycache__"
                            || dir_name == "node_modules"
                            || dir_name == ".git"
                        {
                            continue;
                        }
                    }

                    // Recursively search the subdirectory
                    if let Some(test_file) = Self::find_test_file_recursive(&path, patterns) {
                        return Some(test_file);
                    }
                }
            }
        }

        None
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
        assert_eq!(AutomationResult::Success("test".to_string()).exit_code(), 2);
        assert_eq!(AutomationResult::Failure("test".to_string()).exit_code(), 2);
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
        let output = runner.run_command_with_timeout("echo", &["hello"], temp_dir.path(), 5)?;
        assert!(output.success);

        // Test command that should timeout (sleep for longer than timeout)
        let output = runner.run_command_with_timeout("sleep", &["10"], temp_dir.path(), 1)?;
        assert!(!output.success);

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
