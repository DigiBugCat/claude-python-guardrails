use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, ExitStatus};
use std::time::Duration;

use crate::discovery::PythonProject;
use crate::locking::LockGuard;
use crate::protocol::HookInput;
use crate::cerebras::{CerebrasConfig, SmartExclusionAnalyzer};
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

        // Find and run linter
        self.run_lint_command(&project).await
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

    /// Run linting command for the project
    async fn run_lint_command(&self, project: &PythonProject) -> Result<AutomationResult> {
        let linter = match project.preferred_linter() {
            Some(linter) => linter,
            None => {
                log::debug!("No Python linter found in project");
                return Ok(AutomationResult::NoAction);
            }
        };

        log::debug!(
            "Running {} in {}",
            linter.display_name(),
            project.root.display()
        );

        let output = self.run_command_with_timeout(
            linter.command(),
            &linter.args(),
            &project.root,
            self.config.lint_timeout_seconds,
        )?;

        if output.success {
            Ok(AutomationResult::Success(
                "👉 Lints pass. Continue with your task.".to_string(),
            ))
        } else {
            // Use AI analysis for comprehensive lint failure analysis
            let combined_output = if !output.stderr.is_empty() {
                format!("{}\n{}", output.stdout, output.stderr)
            } else {
                output.stdout
            };

            // Run AI analysis if available
            let message = if !combined_output.trim().is_empty() {
                match self.analyzer.analyze_lint_output(&combined_output, Some(&project.root)).await {
                    Ok(analysis) => {
                        let mut detailed_message = String::new();
                        detailed_message.push_str("⛔ LINT ISSUES FOUND:\n\n");
                        
                        if analysis.has_real_issues {
                            // Show filtered output with only real issues
                            if !analysis.filtered_output.trim().is_empty() {
                                detailed_message.push_str(&analysis.filtered_output);
                                detailed_message.push_str("\n\n");
                            }
                            
                            // Add AI reasoning
                            if !analysis.reasoning.trim().is_empty() {
                                detailed_message.push_str("💡 **Analysis:**\n");
                                detailed_message.push_str(&analysis.reasoning);
                                detailed_message.push_str("\n\n");
                            }
                            
                            detailed_message.push_str(&format!(
                                "📝 **Fix Command:** Run 'cd {} && {}' to address these issues",
                                project.root.display(),
                                linter.display_name()
                            ));
                        } else {
                            detailed_message.push_str("✅ **AI Analysis Result:**\n");
                            detailed_message.push_str(&analysis.reasoning);
                            detailed_message.push_str("\n\n👉 No real issues found. You can continue with your task.");
                            
                            // Return success if no real issues found
                            return Ok(AutomationResult::Success(detailed_message));
                        }
                        
                        detailed_message
                    }
                    Err(e) => {
                        log::warn!("AI analysis failed: {}", e);
                        // Fallback to showing raw output
                        format!(
                            "⛔ LINT FAILURES:\n\n{}\n\nRun 'cd {} && {}' to fix these issues",
                            combined_output.trim(),
                            project.root.display(),
                            linter.display_name()
                        )
                    }
                }
            } else {
                format!(
                    "⛔ BLOCKING: Run 'cd {} && {}' to fix lint failures",
                    project.root.display(),
                    linter.display_name()
                )
            };

            Ok(AutomationResult::Failure(message))
        }
    }

    /// Run test command for a specific file in the project
    async fn run_test_command(&self, project: &PythonProject, source_file: &Path) -> Result<AutomationResult> {
        let tester = match project.preferred_tester() {
            Some(tester) => tester,
            None => {
                log::debug!("No Python tester found in project");
                return Ok(AutomationResult::NoAction);
            }
        };

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
        let mut combined_args: Vec<&str> = base_args.iter().copied().collect();
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

        // Always run AI analysis regardless of test success/failure
        // We already have the source file as a parameter, no need to search for it
        
        match self.analyzer.analyze_test_output(&combined_output, &project.root, Some(source_file)).await {
            Ok(analysis) => {
                if output.success {
                    // Tests passed - check for coverage gaps and improvements
                    let mut message = String::new();
                    
                    // Check if there are important missing tests or coverage gaps
                    let has_suggestions = !analysis.missing_tests.is_empty() 
                        || analysis.coverage_analysis.contains("missing") 
                        || analysis.coverage_analysis.contains("gap")
                        || analysis.quality_assessment.contains("improve")
                        || analysis.recommendations.contains("add")
                        || analysis.recommendations.contains("consider");
                    
                    if has_suggestions {
                        message.push_str("✅ Tests pass, but coverage gaps detected:\n\n");
                        
                        if !analysis.coverage_analysis.is_empty() {
                            message.push_str(&format!("📋 **Coverage Analysis**: {}\n\n", analysis.coverage_analysis));
                        }
                        
                        if !analysis.missing_tests.is_empty() {
                            message.push_str("➕ **Recommended Additional Tests**:\n");
                            for missing_test in &analysis.missing_tests {
                                message.push_str(&format!("  • {}\n", missing_test));
                            }
                            message.push_str("\n");
                        }
                        
                        if !analysis.recommendations.is_empty() {
                            message.push_str(&format!("💡 **Suggestions**: {}\n\n", analysis.recommendations));
                        }
                        
                        message.push_str("👉 Continue with your task, but consider adding these tests.");
                    } else {
                        message.push_str("✅ Tests pass with excellent coverage!\n\n");
                        
                        if !analysis.coverage_analysis.is_empty() {
                            message.push_str(&format!("📋 **Coverage**: {}\n", analysis.coverage_analysis));
                        }
                        
                        if !analysis.quality_assessment.is_empty() {
                            message.push_str(&format!("🎯 **Quality**: {}\n\n", analysis.quality_assessment));
                        }
                        
                        message.push_str("👉 Continue with your task.");
                    }
                    
                    Ok(AutomationResult::Success(message))
                } else {
                    // Tests failed - provide comprehensive failure analysis
                    let mut detailed_message = String::new();
                    detailed_message.push_str("⛔ TESTS FAILED:\n\n");
                    
                    // Add AI analysis
                    detailed_message.push_str(&format!("📊 **Analysis**: {}\n\n", analysis.summary));
                    
                    if !analysis.failed_tests.is_empty() {
                        detailed_message.push_str("🔍 **Failed Tests**:\n");
                        for test in &analysis.failed_tests {
                            detailed_message.push_str(&format!(
                                "  • {}: {} - {}\n    💡 Fix: {}\n", 
                                test.test_name, test.error_type, test.error_message, test.suggested_fix
                            ));
                        }
                        detailed_message.push_str("\n");
                    }
                    
                    if !analysis.coverage_analysis.is_empty() {
                        detailed_message.push_str(&format!("📋 **Coverage**: {}\n\n", analysis.coverage_analysis));
                    }
                    
                    if !analysis.missing_tests.is_empty() {
                        detailed_message.push_str("➕ **Consider Adding**:\n");
                        for missing_test in &analysis.missing_tests {
                            detailed_message.push_str(&format!("  • {}\n", missing_test));
                        }
                        detailed_message.push_str("\n");
                    }
                    
                    detailed_message.push_str(&format!("🛠️  **Next Steps**: {}\n\n", analysis.recommendations));
                    detailed_message.push_str("📄 **Full Output**:\n");
                    detailed_message.push_str(&combined_output.trim());
                    detailed_message.push_str(&format!("\n\nRun 'cd {} && {}' to retry", project.root.display(), tester.display_name()));
                    
                    Ok(AutomationResult::Failure(detailed_message))
                }
            },
            Err(e) => {
                log::warn!("AI analysis failed: {}", e);
                // Fallback to basic behavior when AI analysis fails
                if output.success {
                    Ok(AutomationResult::Success(
                        "👉 Tests pass. Continue with your task.".to_string(),
                    ))
                } else if !combined_output.trim().is_empty() {
                    Ok(AutomationResult::Failure(format!(
                        "⛔ TESTS FAILED:\n\n{}\n\nRun 'cd {} && {}' to fix these test failures",
                        combined_output.trim(),
                        project.root.display(),
                        tester.display_name()
                    )))
                } else {
                    Ok(AutomationResult::Failure(format!(
                        "⛔ BLOCKING: Run 'cd {} && {}' to fix test failures",
                        project.root.display(),
                        tester.display_name()
                    )))
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
                let output = child.wait_with_output().context("Failed to get command output")?;
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
    fn find_test_file_for_source(&self, source_file: &Path, project_root: &Path) -> Option<std::path::PathBuf> {
        let source_name = source_file.file_stem()?.to_str()?;
        
        // Check if the edited file is already a test file
        if let Some(file_name) = source_file.file_name()?.to_str() {
            if file_name.starts_with("test_") || file_name.contains("_test.py") || file_name.contains("test.py") {
                // If it's already a test file, return it as the test to run
                return Some(source_file.to_path_buf());
            }
        }
        
        // List of possible test file patterns and locations
        let test_patterns = vec![
            format!("test_{}.py", source_name),
            format!("{}_test.py", source_name),
            format!("test{}.py", source_name),
        ];
        
        let test_directories = vec![
            project_root.join("tests"),
            project_root.join("test"),
            project_root.join("tests").join("unit"),
            project_root.join("tests").join("integration"),
            project_root.join("test").join("unit"),
            project_root.to_path_buf(), // Same directory as source
            source_file.parent()?.to_path_buf(), // Source file's directory
        ];
        
        // Search for test file in various locations
        for test_dir in &test_directories {
            for pattern in &test_patterns {
                let test_file_path = test_dir.join(pattern);
                if test_file_path.exists() && test_file_path.is_file() {
                    log::debug!("Found test file: {}", test_file_path.display());
                    return Some(test_file_path);
                }
            }
        }
        
        log::debug!("No test file found for source file: {}", source_file.display());
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
