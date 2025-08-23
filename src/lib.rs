use anyhow::{Context, Result};
use globset::{Glob, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::path::Path;

// New modules for automation functionality
pub mod automation;
pub mod discovery;
pub mod locking;
pub mod protocol;

// Re-export commonly used types for convenience
pub use automation::{AutomationConfig, AutomationResult, AutomationRunner};
pub use discovery::{ProjectType, PythonLinter, PythonProject, PythonTester};
pub use locking::{LockGuard, ProcessLock};
pub use protocol::{HookInput, HookResponse};

/// Main configuration structure for guardrails
#[derive(Debug, Serialize, Deserialize)]
pub struct GuardrailsConfig {
    pub exclude: ExclusionConfig,
    #[serde(default)]
    pub rules: RulesConfig,
    #[serde(default)]
    pub automation: AutomationYamlConfig,
}

/// Exclusion configuration
#[derive(Debug, Serialize, Deserialize)]
pub struct ExclusionConfig {
    /// Global patterns to exclude everywhere
    pub patterns: Vec<String>,
    /// Python-specific exclusions
    #[serde(default)]
    pub python: PythonExclusions,
}

/// Python-specific exclusion rules
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PythonExclusions {
    /// Files to skip during linting
    #[serde(default)]
    pub lint_skip: Vec<String>,
    /// Files to skip during testing
    #[serde(default)]
    pub test_skip: Vec<String>,
}

/// Additional rules configuration
#[derive(Debug, Serialize, Deserialize)]
pub struct RulesConfig {
    /// Maximum file size to process
    #[serde(default = "default_max_file_size")]
    pub max_file_size: String,
    /// Skip binary files
    #[serde(default = "default_true")]
    pub skip_binary_files: bool,
    /// Skip generated files
    #[serde(default = "default_true")]
    pub skip_generated_files: bool,
}

impl Default for RulesConfig {
    fn default() -> Self {
        Self {
            max_file_size: default_max_file_size(),
            skip_binary_files: default_true(),
            skip_generated_files: default_true(),
        }
    }
}

/// Automation configuration for YAML files
#[derive(Debug, Serialize, Deserialize)]
pub struct AutomationYamlConfig {
    /// Linting automation settings
    #[serde(default)]
    pub lint: AutomationCommandConfig,
    /// Testing automation settings
    #[serde(default)]
    pub test: AutomationCommandConfig,
}

/// Configuration for a specific automation command
#[derive(Debug, Serialize, Deserialize)]
pub struct AutomationCommandConfig {
    /// Whether this command is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Cooldown period in seconds
    #[serde(default = "default_cooldown_seconds")]
    pub cooldown_seconds: u64,
    /// Timeout in seconds
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    /// Preferred tool to use (optional)
    pub preferred_tool: Option<String>,
}

impl Default for AutomationYamlConfig {
    fn default() -> Self {
        Self {
            lint: AutomationCommandConfig::default(),
            test: AutomationCommandConfig::default(),
        }
    }
}

impl Default for AutomationCommandConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            cooldown_seconds: default_cooldown_seconds(),
            timeout_seconds: default_timeout_seconds(),
            preferred_tool: None,
        }
    }
}

impl From<&AutomationYamlConfig> for AutomationConfig {
    fn from(yaml_config: &AutomationYamlConfig) -> Self {
        Self {
            lint_enabled: yaml_config.lint.enabled,
            test_enabled: yaml_config.test.enabled,
            lint_cooldown_seconds: yaml_config.lint.cooldown_seconds,
            test_cooldown_seconds: yaml_config.test.cooldown_seconds,
            lint_timeout_seconds: yaml_config.lint.timeout_seconds,
            test_timeout_seconds: yaml_config.test.timeout_seconds,
        }
    }
}

fn default_max_file_size() -> String {
    "10MB".to_string()
}

fn default_true() -> bool {
    true
}

fn default_cooldown_seconds() -> u64 {
    2
}

fn default_timeout_seconds() -> u64 {
    20
}

/// The main guardrails checker
pub struct GuardrailsChecker {
    config: GuardrailsConfig,
    global_globset: globset::GlobSet,
    lint_globset: globset::GlobSet,
    test_globset: globset::GlobSet,
    max_file_size_bytes: u64,
}

impl GuardrailsChecker {
    /// Create a new checker from a config file path
    pub fn from_file<P: AsRef<Path>>(config_path: P) -> Result<Self> {
        let content = std::fs::read_to_string(config_path)
            .context("Failed to read guardrails config file")?;
        Self::from_yaml(&content)
    }

    /// Create a new checker from YAML content
    pub fn from_yaml(yaml_content: &str) -> Result<Self> {
        let config: GuardrailsConfig =
            serde_yaml::from_str(yaml_content).context("Failed to parse guardrails YAML config")?;
        Self::from_config(config)
    }

    /// Create a new checker from a config struct
    pub fn from_config(config: GuardrailsConfig) -> Result<Self> {
        // Build global pattern matcher
        let mut global_builder = GlobSetBuilder::new();
        for pattern in &config.exclude.patterns {
            let glob =
                Glob::new(pattern).with_context(|| format!("Invalid glob pattern: {pattern}"))?;
            global_builder.add(glob);
        }
        let global_globset = global_builder
            .build()
            .context("Failed to build global glob set")?;

        // Build lint-specific pattern matcher
        let mut lint_builder = GlobSetBuilder::new();
        for pattern in &config.exclude.python.lint_skip {
            let glob = Glob::new(pattern)
                .with_context(|| format!("Invalid lint skip pattern: {pattern}"))?;
            lint_builder.add(glob);
        }
        let lint_globset = lint_builder
            .build()
            .context("Failed to build lint glob set")?;

        // Build test-specific pattern matcher
        let mut test_builder = GlobSetBuilder::new();
        for pattern in &config.exclude.python.test_skip {
            let glob = Glob::new(pattern)
                .with_context(|| format!("Invalid test skip pattern: {pattern}"))?;
            test_builder.add(glob);
        }
        let test_globset = test_builder
            .build()
            .context("Failed to build test glob set")?;

        // Parse max file size
        let max_file_size_bytes = parse_file_size(&config.rules.max_file_size)?;

        Ok(Self {
            config,
            global_globset,
            lint_globset,
            test_globset,
            max_file_size_bytes,
        })
    }

    /// Check if a file should be excluded for any operation
    pub fn should_exclude(&self, file_path: &Path) -> Result<bool> {
        self.should_exclude_context(file_path, &ExclusionContext::Any)
    }

    /// Check if a file should be excluded for linting
    pub fn should_exclude_lint(&self, file_path: &Path) -> Result<bool> {
        self.should_exclude_context(file_path, &ExclusionContext::Lint)
    }

    /// Check if a file should be excluded for testing
    pub fn should_exclude_test(&self, file_path: &Path) -> Result<bool> {
        self.should_exclude_context(file_path, &ExclusionContext::Test)
    }

    /// Check exclusion with specific context
    fn should_exclude_context(&self, file_path: &Path, context: &ExclusionContext) -> Result<bool> {
        // Always check global patterns first
        if self.global_globset.is_match(file_path) {
            return Ok(true);
        }

        // Check context-specific patterns
        match context {
            ExclusionContext::Any => {
                // For general exclusion, check both lint and test patterns
                if self.lint_globset.is_match(file_path) || self.test_globset.is_match(file_path) {
                    return Ok(true);
                }
            }
            ExclusionContext::Lint => {
                if self.lint_globset.is_match(file_path) {
                    return Ok(true);
                }
            }
            ExclusionContext::Test => {
                if self.test_globset.is_match(file_path) {
                    return Ok(true);
                }
            }
        }

        // Check file-based rules
        if file_path.exists() {
            // Check file size
            if let Ok(metadata) = std::fs::metadata(file_path) {
                if metadata.len() > self.max_file_size_bytes {
                    return Ok(true);
                }
            }

            // Check if binary file
            if self.config.rules.skip_binary_files && is_binary_file(file_path)? {
                return Ok(true);
            }

            // Check if generated file
            if self.config.rules.skip_generated_files && is_generated_file(file_path) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Get the config for inspection
    pub fn config(&self) -> &GuardrailsConfig {
        &self.config
    }
}

/// Context for exclusion checking
#[derive(Debug, Clone)]
enum ExclusionContext {
    Any,
    Lint,
    Test,
}

/// Parse file size string like "10MB" to bytes
fn parse_file_size(size_str: &str) -> Result<u64> {
    let size_str = size_str.trim().to_uppercase();

    if let Some(num_str) = size_str.strip_suffix("KB") {
        let num: f64 = num_str.parse().context("Invalid file size number")?;
        Ok((num * 1024.0) as u64)
    } else if let Some(num_str) = size_str.strip_suffix("MB") {
        let num: f64 = num_str.parse().context("Invalid file size number")?;
        Ok((num * 1024.0 * 1024.0) as u64)
    } else if let Some(num_str) = size_str.strip_suffix("GB") {
        let num: f64 = num_str.parse().context("Invalid file size number")?;
        Ok((num * 1024.0 * 1024.0 * 1024.0) as u64)
    } else {
        // Assume bytes if no suffix
        size_str.parse().context("Invalid file size")
    }
}

/// Check if a file is binary by reading the first few bytes
fn is_binary_file(file_path: &Path) -> Result<bool> {
    use std::io::Read;

    let mut file =
        std::fs::File::open(file_path).context("Failed to open file for binary check")?;

    let mut buffer = [0; 1024];
    let bytes_read = file
        .read(&mut buffer)
        .context("Failed to read file for binary check")?;

    // Simple binary detection: look for null bytes
    Ok(buffer[..bytes_read].contains(&0))
}

/// Check if a file is likely generated based on common patterns
fn is_generated_file(file_path: &Path) -> bool {
    let path_str = file_path.to_string_lossy().to_lowercase();
    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Common generated file patterns
    let generated_patterns = [
        "_pb2.py",      // Protocol buffers
        "_pb2_grpc.py", // gRPC
        ".generated.",  // Generic generated
        "_generated.",  // Generic generated
        ".pb.go",       // Go protocol buffers
        ".g.dart",      // Dart generated
        "generated",    // Directory name
        ".gen.",        // Generic generated
    ];

    generated_patterns
        .iter()
        .any(|pattern| path_str.contains(pattern) || filename.contains(pattern))
}

/// Default guardrails configuration
pub fn default_config() -> GuardrailsConfig {
    GuardrailsConfig {
        exclude: ExclusionConfig {
            patterns: vec![
                "*.pyc".to_string(),
                "__pycache__/".to_string(),
                ".venv/**".to_string(),
                "venv/**".to_string(),
                ".git/".to_string(),
                "*.egg-info/".to_string(),
                ".pytest_cache/".to_string(),
                ".mypy_cache/".to_string(),
                "target/**".to_string(),       // Rust builds
                "node_modules/**".to_string(), // Node.js
                "dist/**".to_string(),
                "build/**".to_string(),
            ],
            python: PythonExclusions {
                lint_skip: vec![
                    "migrations/**".to_string(),
                    "*/migrations/**".to_string(),
                    "*_pb2.py".to_string(),
                    "*_pb2_grpc.py".to_string(),
                    "*.generated.py".to_string(),
                    "*_generated.py".to_string(),
                ],
                test_skip: vec![
                    "conftest.py".to_string(),
                    "**/conftest.py".to_string(),
                    "test_*.py".to_string(),
                    "*_test.py".to_string(),
                    "tests/fixtures/**".to_string(),
                    "tests/data/**".to_string(),
                ],
            },
        },
        rules: RulesConfig::default(),
        automation: AutomationYamlConfig::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_basic_exclusion() -> Result<()> {
        let config = default_config();
        let checker = GuardrailsChecker::from_config(config)?;

        assert!(checker.should_exclude(Path::new("__pycache__/test.pyc"))?);
        assert!(checker.should_exclude(Path::new(".venv/test.py"))?);
        assert!(!checker.should_exclude(Path::new("src/main.py"))?);

        Ok(())
    }

    #[test]
    fn test_lint_specific_exclusion() -> Result<()> {
        let config = default_config();
        let checker = GuardrailsChecker::from_config(config)?;

        assert!(checker.should_exclude_lint(Path::new("migrations/0001_initial.py"))?);
        assert!(checker.should_exclude_lint(Path::new("proto_pb2.py"))?);
        assert!(!checker.should_exclude_lint(Path::new("src/main.py"))?);

        Ok(())
    }

    #[test]
    fn test_test_specific_exclusion() -> Result<()> {
        let config = default_config();
        let checker = GuardrailsChecker::from_config(config)?;

        assert!(checker.should_exclude_test(Path::new("conftest.py"))?);
        assert!(checker.should_exclude_test(Path::new("tests/conftest.py"))?);
        assert!(checker.should_exclude_test(Path::new("test_models.py"))?);
        assert!(checker.should_exclude_test(Path::new("models_test.py"))?);
        assert!(!checker.should_exclude_test(Path::new("src/models.py"))?);

        Ok(())
    }

    #[test]
    fn test_file_size_parsing() -> Result<()> {
        assert_eq!(parse_file_size("1024")?, 1024);
        assert_eq!(parse_file_size("10KB")?, 10 * 1024);
        assert_eq!(parse_file_size("5MB")?, 5 * 1024 * 1024);
        assert_eq!(parse_file_size("2GB")?, 2 * 1024 * 1024 * 1024);

        // Test case insensitive
        assert_eq!(parse_file_size("10kb")?, 10 * 1024);
        assert_eq!(parse_file_size("5mb")?, 5 * 1024 * 1024);

        // Test with spaces
        assert_eq!(parse_file_size(" 10MB ")?, 10 * 1024 * 1024);

        Ok(())
    }

    #[test]
    fn test_file_size_parsing_errors() {
        assert!(parse_file_size("invalid").is_err());
        assert!(parse_file_size("10XB").is_err());
        assert!(parse_file_size("").is_err());
        assert!(parse_file_size("MB").is_err());
    }

    #[test]
    fn test_yaml_config_parsing() -> Result<()> {
        let yaml = r#"
exclude:
  patterns:
    - "*.tmp"
    - "temp/"
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

        let checker = GuardrailsChecker::from_yaml(yaml)?;
        assert!(checker.should_exclude(Path::new("test.tmp"))?);
        assert!(checker.should_exclude_lint(Path::new("generated/models.py"))?);
        assert!(checker.should_exclude_test(Path::new("fixtures/data.py"))?);

        Ok(())
    }

    #[test]
    fn test_yaml_config_parsing_errors() {
        let invalid_yaml = r#"
exclude:
  patterns:
    - invalid: yaml: structure
"#;
        assert!(GuardrailsChecker::from_yaml(invalid_yaml).is_err());

        let invalid_glob = r#"
exclude:
  patterns:
    - "[invalid-glob"
"#;
        assert!(GuardrailsChecker::from_yaml(invalid_glob).is_err());
    }

    #[test]
    fn test_generated_file_detection() {
        assert!(is_generated_file(Path::new("models_pb2.py")));
        assert!(is_generated_file(Path::new("service_pb2_grpc.py")));
        assert!(is_generated_file(Path::new("schema.generated.py")));
        assert!(is_generated_file(Path::new("types_generated.py")));
        assert!(is_generated_file(Path::new("proto.pb.go")));
        assert!(is_generated_file(Path::new("widgets.g.dart")));
        assert!(is_generated_file(Path::new("generated/models.py")));
        assert!(is_generated_file(Path::new("src/generated/types.py")));
        assert!(is_generated_file(Path::new("output.gen.js")));

        assert!(!is_generated_file(Path::new("models.py")));
        assert!(!is_generated_file(Path::new("service.py")));
        assert!(!is_generated_file(Path::new("regular_file.py")));
    }

    #[test]
    fn test_binary_file_detection() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Create a text file
        let text_file = temp_dir.path().join("text.txt");
        fs::write(&text_file, "This is a text file\nwith multiple lines")?;
        assert!(!is_binary_file(&text_file)?);

        // Create a binary file (with null bytes)
        let binary_file = temp_dir.path().join("binary.bin");
        fs::write(&binary_file, b"Binary\x00content\x00here")?;
        assert!(is_binary_file(&binary_file)?);

        // Create empty file
        let empty_file = temp_dir.path().join("empty.txt");
        fs::write(&empty_file, "")?;
        assert!(!is_binary_file(&empty_file)?);

        Ok(())
    }

    #[test]
    fn test_file_size_rules() -> Result<()> {
        let temp_dir = TempDir::new()?;

        let config = GuardrailsConfig {
            exclude: ExclusionConfig {
                patterns: vec![],
                python: PythonExclusions::default(),
            },
            rules: RulesConfig {
                max_file_size: "10".to_string(), // 10 bytes
                skip_binary_files: false,
                skip_generated_files: false,
            },
            automation: AutomationYamlConfig::default(),
        };
        let checker = GuardrailsChecker::from_config(config)?;

        // Small file should be included
        let small_file = temp_dir.path().join("small.txt");
        fs::write(&small_file, "small")?; // 5 bytes
        assert!(!checker.should_exclude(&small_file)?);

        // Large file should be excluded
        let large_file = temp_dir.path().join("large.txt");
        fs::write(&large_file, "this is a large file content")?; // > 10 bytes
        assert!(checker.should_exclude(&large_file)?);

        Ok(())
    }

    #[test]
    fn test_exclusion_context_combinations() -> Result<()> {
        let config = GuardrailsConfig {
            exclude: ExclusionConfig {
                patterns: vec!["*.global".to_string()],
                python: PythonExclusions {
                    lint_skip: vec!["*.lint".to_string()],
                    test_skip: vec!["*.test".to_string()],
                },
            },
            rules: RulesConfig::default(),
            automation: AutomationYamlConfig::default(),
        };
        let checker = GuardrailsChecker::from_config(config)?;

        // Global exclusions affect everything
        assert!(checker.should_exclude(Path::new("file.global"))?);
        assert!(checker.should_exclude_lint(Path::new("file.global"))?);
        assert!(checker.should_exclude_test(Path::new("file.global"))?);

        // Lint-specific exclusions
        assert!(!checker.should_exclude_test(Path::new("file.lint"))?);
        assert!(checker.should_exclude_lint(Path::new("file.lint"))?);
        assert!(checker.should_exclude(Path::new("file.lint"))?); // Any check includes both

        // Test-specific exclusions
        assert!(!checker.should_exclude_lint(Path::new("file.test"))?);
        assert!(checker.should_exclude_test(Path::new("file.test"))?);
        assert!(checker.should_exclude(Path::new("file.test"))?); // Any check includes both

        // Regular files
        assert!(!checker.should_exclude(Path::new("file.py"))?);
        assert!(!checker.should_exclude_lint(Path::new("file.py"))?);
        assert!(!checker.should_exclude_test(Path::new("file.py"))?);

        Ok(())
    }

    #[test]
    fn test_default_config_structure() {
        let config = default_config();

        // Should have reasonable defaults
        assert!(!config.exclude.patterns.is_empty());
        assert!(!config.exclude.python.lint_skip.is_empty());
        assert!(!config.exclude.python.test_skip.is_empty());

        // Should exclude common Python artifacts
        assert!(config.exclude.patterns.contains(&"*.pyc".to_string()));
        assert!(config
            .exclude
            .patterns
            .contains(&"__pycache__/".to_string()));

        // Should exclude migrations from linting
        assert!(config
            .exclude
            .python
            .lint_skip
            .iter()
            .any(|p| p.contains("migrations")));

        // Should exclude test files from testing
        assert!(config
            .exclude
            .python
            .test_skip
            .contains(&"test_*.py".to_string()));
    }

    #[test]
    fn test_config_with_missing_sections() -> Result<()> {
        // Config with minimal sections should still work
        let yaml = r#"
exclude:
  patterns:
    - "*.tmp"
"#;

        let checker = GuardrailsChecker::from_yaml(yaml)?;
        assert!(checker.should_exclude(Path::new("test.tmp"))?);
        assert!(!checker.should_exclude(Path::new("test.py"))?);

        // Empty python section should not cause issues
        assert!(!checker.should_exclude_lint(Path::new("anything.py"))?);
        assert!(!checker.should_exclude_test(Path::new("anything.py"))?);

        Ok(())
    }

    #[test]
    fn test_nonexistent_file_handling() -> Result<()> {
        let config = default_config();
        let checker = GuardrailsChecker::from_config(config)?;

        // Non-existent files should still be processed for glob matching
        assert!(checker.should_exclude(Path::new("/nonexistent/__pycache__/test.pyc"))?);
        assert!(!checker.should_exclude(Path::new("/nonexistent/src/main.py"))?);

        // But file-based rules (size, binary, generated) won't apply
        // since the file doesn't exist to check

        Ok(())
    }
}
