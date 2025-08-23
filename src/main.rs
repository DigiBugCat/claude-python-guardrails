use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use claude_python_guardrails::{
    default_config, AutomationConfig, AutomationRunner, CerebrasConfig, ExclusionAnalysis,
    GuardrailsChecker, HookInput, SmartExclusionAnalyzer,
};
use std::path::Path;

/// Claude Code Python automation hooks - AI-powered linting and testing automation
#[derive(Parser)]
#[command(name = "claude-python-guardrails")]
#[command(
    about = "Claude Code Python automation hooks - AI-powered linting and testing automation"
)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// AI-powered file analysis (reads Claude Code hook JSON from stdin)
    Analyze {
        /// Output format (json or text)
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Linting automation (reads Claude Code hook JSON from stdin)
    Lint,
    /// Testing automation (reads Claude Code hook JSON from stdin)
    Test,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging (safe to call multiple times)
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    let _ = env_logger::try_init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Analyze { ref format } => handle_analyze_command(&cli, format).await,

        Commands::Lint => {
            let result = handle_smart_automation(&cli, "lint").await?;
            if let Some(message) = result.message() {
                eprintln!("{message}");
            }
            std::process::exit(result.exit_code());
        }

        Commands::Test => {
            let result = handle_smart_automation(&cli, "test").await?;
            if let Some(message) = result.message() {
                eprintln!("{message}");
            }
            std::process::exit(result.exit_code());
        }
    }
}

fn get_default_checker() -> GuardrailsChecker {
    // Always use hardcoded default configuration for hooks
    GuardrailsChecker::from_config(default_config())
        .expect("Default configuration should always be valid")
}

async fn handle_smart_automation(
    _cli: &Cli,
    operation: &str,
) -> Result<claude_python_guardrails::AutomationResult> {
    use claude_python_guardrails::AutomationResult;

    let checker = get_default_checker();
    let automation_config = AutomationConfig::from(&checker.config().automation);
    let runner = AutomationRunner::new(automation_config, checker);

    match operation {
        "lint" => runner.handle_smart_lint().await,
        "test" => runner.handle_smart_test().await,
        _ => Ok(AutomationResult::NoAction),
    }
}

async fn handle_analyze_command(cli: &Cli, format: &str) -> Result<()> {
    // Read JSON input from stdin (Claude Code hook format)
    let hook_input = match HookInput::from_stdin() {
        Ok(input) => input,
        Err(_) => {
            if cli.verbose {
                eprintln!("ℹ️  No JSON input available on stdin.");
            }
            std::process::exit(0);
        }
    };

    // Only process PostToolUse events for edit tools
    if !hook_input.should_process() {
        if cli.verbose {
            eprintln!("ℹ️  Ignoring event type: {}", hook_input.hook_event_name);
        }
        std::process::exit(0);
    }

    // Extract file path from hook input
    let file_path = match hook_input.file_path() {
        Some(path) => path,
        None => {
            if cli.verbose {
                eprintln!("❌ No file path found in hook input");
            }
            std::process::exit(0);
        }
    };

    // Check if file exists
    if !file_path.exists() {
        if cli.verbose {
            eprintln!("❌ File does not exist: {}", file_path.display());
        }
        std::process::exit(0);
    }

    // Initialize Cerebras configuration
    let cerebras_config = CerebrasConfig::default();

    if !cerebras_config.enabled && cli.verbose {
        eprintln!("⚠️  Cerebras integration disabled. Set CEREBRAS_API_KEY environment variable to enable AI analysis.");
        eprintln!("Falling back to basic heuristic analysis...\n");
    }

    let analyzer = SmartExclusionAnalyzer::new(cerebras_config);

    if cli.verbose {
        eprintln!("🔍 Analyzing file: {}", file_path.display());
        eprintln!("Output format: {}", format);
        eprintln!();
    }

    match analyzer.analyze_file(&file_path).await {
        Ok(analysis) => {
            display_analysis(&file_path, &analysis, format, cli.verbose)?;

            // Analysis completed successfully - exit 0 regardless of exclusion decision
            // The exclusion recommendation is communicated through the output content
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("❌ Analysis failed: {}", e);
            std::process::exit(2);
        }
    }
}

fn display_analysis(
    file: &Path,
    analysis: &ExclusionAnalysis,
    format: &str,
    verbose: bool,
) -> Result<()> {
    match format.to_lowercase().as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(analysis)
                .context("Failed to serialize analysis to JSON")?;
            println!("{}", json);
        }
        "text" => {
            display_text_format(file, analysis, verbose);
        }
        _ => {
            // Default to text format for unknown format types
            display_text_format(file, analysis, verbose);
        }
    }

    Ok(())
}

fn display_text_format(file: &Path, analysis: &ExclusionAnalysis, verbose: bool) {
    println!("📁 File Analysis: {}", file.display());
    println!("{}", "═".repeat(60));

    println!("📋 File Type: {}", analysis.file_type);
    println!("🎯 Purpose: {}", analysis.purpose);
    println!();

    println!("🚫 Exclusion Recommendations:");
    println!(
        "  • General Processing: {}",
        if analysis.should_exclude_general {
            "❌ EXCLUDE"
        } else {
            "✅ INCLUDE"
        }
    );
    println!(
        "  • Linting: {}",
        if analysis.should_exclude_lint {
            "❌ EXCLUDE"
        } else {
            "✅ INCLUDE"
        }
    );
    println!(
        "  • Testing: {}",
        if analysis.should_exclude_test {
            "❌ EXCLUDE"
        } else {
            "✅ INCLUDE"
        }
    );
    println!();

    println!("🤔 Reasoning:");
    println!("{}", analysis.reasoning);
    println!();

    println!("💡 Configuration Recommendation:");
    println!("{}", analysis.exclusion_recommendation);

    if verbose {
        println!();
        println!("🔧 Debug Information:");
        println!("  • Analysis completed successfully");
        println!("  • File exists and is readable");
    }
}

