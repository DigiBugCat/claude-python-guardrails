use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use claude_python_guardrails::{default_config, AutomationConfig, AutomationRunner, GuardrailsChecker};
use std::path::PathBuf;

/// Simple exclusion checker for Python projects using Claude Code
#[derive(Parser)]
#[command(name = "claude-python-guardrails")]
#[command(about = "Simple exclusion checker for Python projects using Claude Code")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to configuration file
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Check if a file should be excluded (general)
    Check {
        /// File path to check
        file: PathBuf,
    },
    /// Check if a file should be excluded from linting
    Lint {
        /// File path to check
        file: PathBuf,
    },
    /// Check if a file should be excluded from testing
    Test {
        /// File path to check
        file: PathBuf,
    },
    /// Generate default configuration file
    Init {
        /// Output file path (default: guardrails.yaml)
        #[arg(short, long, default_value = "guardrails.yaml")]
        output: PathBuf,
    },
    /// Validate configuration file
    Validate {
        /// Configuration file to validate (default: guardrails.yaml)
        #[arg(default_value = "guardrails.yaml")]
        config: PathBuf,
    },
    /// Smart linting automation (Claude Code hook compatible)
    SmartLint,
    /// Smart testing automation (Claude Code hook compatible)
    SmartTest,
}

fn main() -> Result<()> {
    // Initialize logging (safe to call multiple times)
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    let _ = env_logger::try_init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Check { ref file } => {
            let checker = load_checker(&cli)?;
            let excluded = checker.should_exclude(file)?;

            if cli.verbose {
                if excluded {
                    println!("❌ {}: EXCLUDED", file.display());
                } else {
                    println!("✅ {}: INCLUDED", file.display());
                }
            }

            std::process::exit(if excluded { 1 } else { 0 });
        }

        Commands::Lint { ref file } => {
            let checker = load_checker(&cli)?;
            let excluded = checker.should_exclude_lint(file)?;

            if cli.verbose {
                if excluded {
                    println!("❌ {}: EXCLUDED from linting", file.display());
                } else {
                    println!("✅ {}: INCLUDED for linting", file.display());
                }
            }

            std::process::exit(if excluded { 1 } else { 0 });
        }

        Commands::Test { ref file } => {
            let checker = load_checker(&cli)?;
            let excluded = checker.should_exclude_test(file)?;

            if cli.verbose {
                if excluded {
                    println!("❌ {}: EXCLUDED from testing", file.display());
                } else {
                    println!("✅ {}: INCLUDED for testing", file.display());
                }
            }

            std::process::exit(if excluded { 1 } else { 0 });
        }

        Commands::Init { output } => {
            if output.exists() {
                return Err(anyhow::anyhow!(
                    "Configuration file {} already exists. Use --force to overwrite.",
                    output.display()
                ));
            }

            let config = default_config();
            let yaml = serde_yaml::to_string(&config)
                .context("Failed to serialize default configuration")?;

            std::fs::write(&output, yaml).with_context(|| {
                format!("Failed to write configuration to {}", output.display())
            })?;

            println!("✅ Created default configuration: {}", output.display());
            Ok(())
        }

        Commands::Validate { config } => match GuardrailsChecker::from_file(&config) {
            Ok(checker) => {
                if cli.verbose {
                    println!("✅ Configuration is valid: {}", config.display());
                    println!(
                        "Global patterns: {}",
                        checker.config().exclude.patterns.len()
                    );
                    println!(
                        "Lint skip patterns: {}",
                        checker.config().exclude.python.lint_skip.len()
                    );
                    println!(
                        "Test skip patterns: {}",
                        checker.config().exclude.python.test_skip.len()
                    );
                } else {
                    println!("✅ Valid configuration");
                }
                Ok(())
            }
            Err(e) => {
                println!("❌ Invalid configuration: {e}");
                std::process::exit(1);
            }
        },

        Commands::SmartLint => {
            let result = handle_smart_automation(&cli, "lint")?;
            if let Some(message) = result.message() {
                eprintln!("{}", message);
            }
            std::process::exit(result.exit_code());
        }

        Commands::SmartTest => {
            let result = handle_smart_automation(&cli, "test")?;
            if let Some(message) = result.message() {
                eprintln!("{}", message);
            }
            std::process::exit(result.exit_code());
        }
    }
}

fn load_checker(cli: &Cli) -> Result<GuardrailsChecker> {
    match &cli.config {
        Some(config_path) => {
            if cli.verbose {
                println!("Loading configuration from: {}", config_path.display());
            }
            GuardrailsChecker::from_file(config_path)
                .with_context(|| format!("Failed to load config from {}", config_path.display()))
        }
        None => {
            // Try to find guardrails.yaml in current directory
            let default_config_path = PathBuf::from("guardrails.yaml");
            if default_config_path.exists() {
                if cli.verbose {
                    println!("Loading configuration from: guardrails.yaml");
                }
                GuardrailsChecker::from_file(&default_config_path)
            } else {
                if cli.verbose {
                    println!("Using default configuration");
                }
                GuardrailsChecker::from_config(default_config())
            }
        }
    }
}

fn handle_smart_automation(cli: &Cli, operation: &str) -> Result<claude_python_guardrails::AutomationResult> {
    use claude_python_guardrails::AutomationResult;

    let checker = load_checker(cli)?;
    let automation_config = AutomationConfig::from(&checker.config().automation);
    let runner = AutomationRunner::new(automation_config, checker);

    match operation {
        "lint" => runner.handle_smart_lint(),
        "test" => runner.handle_smart_test(),
        _ => Ok(AutomationResult::NoAction),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_init_command() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("test_guardrails.yaml");

        // Test that init creates a valid config file
        let config = default_config();
        let yaml = serde_yaml::to_string(&config)?;
        fs::write(&config_path, yaml)?;

        // Verify we can load the generated config
        let checker = GuardrailsChecker::from_file(&config_path)?;
        assert!(!checker.config().exclude.patterns.is_empty());

        Ok(())
    }

    #[test]
    fn test_checker_loading() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("guardrails.yaml");

        let config_yaml = r#"
exclude:
  patterns:
    - "*.tmp"
  python:
    lint_skip:
      - "test_*"
    test_skip: []
rules:
  max_file_size: "5MB"
  skip_binary_files: true
  skip_generated_files: true
"#;

        fs::write(&config_path, config_yaml)?;

        let cli = Cli {
            command: Commands::Check {
                file: PathBuf::from("test.py"),
            },
            config: Some(config_path),
            verbose: false,
        };

        let checker = load_checker(&cli)?;

        // Test the loaded configuration
        assert!(checker.should_exclude(&PathBuf::from("test.tmp"))?);
        assert!(checker.should_exclude_lint(&PathBuf::from("test_example.py"))?);
        assert!(!checker.should_exclude(&PathBuf::from("src/main.py"))?);

        Ok(())
    }
}
