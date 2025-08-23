use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Configuration for the Cerebras AI integration
#[derive(Debug, Clone)]
pub struct CerebrasConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub enabled: bool,
}

impl Default for CerebrasConfig {
    fn default() -> Self {
        Self {
            api_key: std::env::var("CEREBRAS_API_KEY").unwrap_or_default(),
            base_url: "https://api.cerebras.ai/v1".to_string(),
            model: "qwen-3-coder-480b".to_string(),
            enabled: std::env::var("CEREBRAS_API_KEY").is_ok(),
        }
    }
}

/// Request structure for Cerebras Chat API
#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    top_p: f32,
    response_format: ResponseFormat,
}

/// Chat message structure
#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

/// Response format specification for structured output
#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
    json_schema: JsonSchema,
}

/// JSON Schema definition
#[derive(Debug, Serialize)]
struct JsonSchema {
    name: String,
    description: String,
    schema: serde_json::Value,
}

/// Response from Cerebras API
#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

/// Individual choice from the response
#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

/// Response message
#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
}

/// Analysis result for file exclusion recommendations
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ExclusionAnalysis {
    pub should_exclude_general: bool,
    pub should_exclude_lint: bool,
    pub should_exclude_test: bool,
    pub reasoning: String,
    pub file_type: String,
    pub purpose: String,
    pub exclusion_recommendation: String,
}

/// Analysis result for comprehensive test analysis
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TestFailureAnalysis {
    pub has_failures: bool,
    pub summary: String,
    pub failed_tests: Vec<FailedTest>,
    pub analysis: String,
    pub recommendations: String,
    pub coverage_analysis: String,
    pub missing_tests: Vec<String>,
    pub quality_assessment: String,
}

/// Details of a specific failed test
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FailedTest {
    pub test_name: String,
    pub error_type: String,
    pub error_message: String,
    pub suggested_fix: String,
}

/// Analysis result for lint output
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LintAnalysis {
    pub has_real_issues: bool,
    pub filtered_output: String,
    pub reasoning: String,
    pub issue_count: u32,
    pub recommendations: String,
}

/// Smart exclusion analyzer using Cerebras AI
#[derive(Debug)]
pub struct SmartExclusionAnalyzer {
    client: Client,
    config: CerebrasConfig,
}

impl SmartExclusionAnalyzer {
    /// Create a new analyzer with the given configuration
    pub fn new(config: CerebrasConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    /// Analyze a file to determine appropriate exclusion patterns
    pub async fn analyze_file(&self, file_path: &Path) -> Result<ExclusionAnalysis> {
        if !self.config.enabled {
            return Ok(self.heuristic_analysis(file_path));
        }

        let file_content = self.read_file_content(file_path)?;

        // Handle API errors gracefully with conservative defaults
        match self.call_cerebras_api(file_path, &file_content).await {
            Ok(analysis) => Ok(analysis),
            Err(e) => {
                eprintln!("Warning: Cerebras API call failed: {}", e);
                Ok(self.conservative_analysis(file_path, "API error occurred"))
            }
        }
    }

    /// Read file content with error handling for binary/large files
    fn read_file_content(&self, file_path: &Path) -> Result<String> {
        let metadata = std::fs::metadata(file_path)
            .with_context(|| format!("Failed to read metadata for {}", file_path.display()))?;

        // Skip very large files (>1MB)
        if metadata.len() > 1024 * 1024 {
            return Ok("[File too large to analyze]".to_string());
        }

        match std::fs::read_to_string(file_path) {
            Ok(content) => Ok(content),
            Err(_) => {
                // Likely a binary file
                Ok(format!(
                    "[Binary file: {}]",
                    file_path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or("unknown")
                ))
            }
        }
    }

    /// Make API call to Cerebras for file analysis
    async fn call_cerebras_api(
        &self,
        file_path: &Path,
        file_content: &str,
    ) -> Result<ExclusionAnalysis> {
        let prompt = self.create_analysis_prompt(file_path, file_content);

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt,
            }],
            temperature: 0.7,
            top_p: 0.8,
            response_format: ResponseFormat {
                format_type: "json_schema".to_string(),
                json_schema: JsonSchema {
                    name: "exclusion_analysis".to_string(),
                    description: "Analysis of file exclusion requirements".to_string(),
                    schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "should_exclude_general": {
                                "type": "boolean",
                                "description": "Whether file should be excluded from general processing"
                            },
                            "should_exclude_lint": {
                                "type": "boolean",
                                "description": "Whether file should be excluded from linting"
                            },
                            "should_exclude_test": {
                                "type": "boolean",
                                "description": "Whether file should be excluded from testing"
                            },
                            "reasoning": {
                                "type": "string",
                                "description": "Detailed reasoning for exclusion recommendations"
                            },
                            "file_type": {
                                "type": "string",
                                "description": "Detected file type/category"
                            },
                            "purpose": {
                                "type": "string",
                                "description": "Primary purpose of the file"
                            },
                            "exclusion_recommendation": {
                                "type": "string",
                                "description": "Specific recommendation for guardrails configuration"
                            }
                        },
                        "required": [
                            "should_exclude_general",
                            "should_exclude_lint",
                            "should_exclude_test",
                            "reasoning",
                            "file_type",
                            "purpose",
                            "exclusion_recommendation"
                        ]
                    }),
                },
            },
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .with_context(|| "Failed to send request to Cerebras API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Cerebras API request failed with status {}: {}",
                status,
                error_text
            ));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .with_context(|| "Failed to parse Cerebras API response")?;

        let content = chat_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_ref())
            .ok_or_else(|| anyhow::anyhow!("No content in Cerebras API response"))?;

        let analysis: ExclusionAnalysis = serde_json::from_str(content)
            .with_context(|| "Failed to parse exclusion analysis from Cerebras response")?;

        Ok(analysis)
    }

    /// Create the analysis prompt for the given file
    fn create_analysis_prompt(&self, file_path: &Path, file_content: &str) -> String {
        // For now, we'll use a comprehensive prompt that covers all aspects
        // This will be split into separate prompts for each context in the future
        self.create_comprehensive_analysis_prompt(file_path, file_content)
    }

    /// Create test exclusion analysis prompt (based on test-filter.py)
    #[allow(dead_code)]
    fn create_test_analysis_prompt(&self, file_path: &Path, file_content: &str) -> String {
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        // Get project context - look for existing tests
        let project_root = file_path.parent().unwrap_or(Path::new("."));
        let mut test_context = String::new();

        // Look for test directory and existing tests
        let test_dirs = ["tests", "test"];
        for test_dir_name in test_dirs.iter() {
            let test_dir = project_root.join(test_dir_name);
            if test_dir.exists() {
                let module_name = file_path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("module");

                let possible_test_files = [
                    test_dir.join(format!("test_{}.py", module_name)),
                    test_dir.join(format!("{}_test.py", module_name)),
                    test_dir
                        .join("unit")
                        .join(format!("test_{}.py", module_name)),
                    test_dir
                        .join("integration")
                        .join(format!("test_{}.py", module_name)),
                ];

                for test_file in &possible_test_files {
                    if test_file.exists() {
                        test_context = format!(
                            "\n\nExisting test file found at: {}",
                            test_file
                                .strip_prefix(project_root)
                                .unwrap_or(test_file)
                                .display()
                        );
                        break;
                    }
                }

                if test_context.is_empty() {
                    test_context = format!(
                        "\n\nNo test file found. Suggested location: {}/unit/test_{}.py",
                        test_dir_name, module_name
                    );
                }
                break;
            }
        }

        format!(
            r#"You are an expert software developer analyzing whether a file needs tests.

File: {}
File name: {}
File type: {}{}

File content:
```{}
{}
```

Analyze this file and determine:
1. Does this file need tests? Consider:
   - Files with business logic, algorithms, or complex operations NEED tests
   - Pure type definitions, interfaces, and data models usually DON'T need tests
   - Configuration files, constants, and simple data structures usually DON'T need tests
   - Utility functions and helpers usually NEED tests
   - Files that only import/export other modules DON'T need tests
   - Test files themselves DON'T need tests
   - Example/demo files usually DON'T need tests
   - Generated files DON'T need tests

2. If tests exist, are they sufficient or do they need improvement?

3. If it needs tests, what SPECIFIC tests should be written? Include:
   - Test function names
   - What each test should verify
   - Any edge cases to cover

4. What specific action should be taken? Be VERY CLEAR and DIRECTIVE:
   - If tests are missing and needed: "⚠️ STOP! CREATE TESTS FIRST: Write test file at tests/unit/test_<module>.py with tests for X, Y, Z before continuing with any other code changes"
   - If tests exist but incomplete: "⚠️ ADD MISSING TESTS: The existing test file needs tests for functions X, Y, Z"
   - If no tests needed: "No action needed - file is configuration/type definitions only"
   - If tests are sufficient: "Tests are adequate - continue with development"

IMPORTANT: If tests are needed but missing, use strong, clear language that makes it obvious that tests MUST be written before proceeding. Use warning emojis and capital letters for emphasis."#,
            file_path.display(),
            file_name,
            extension,
            test_context,
            extension,
            file_content
        )
    }

    /// Create lint exclusion analysis prompt (based on general code analysis)
    #[allow(dead_code)]
    fn create_lint_analysis_prompt(&self, file_path: &Path, _file_content: &str) -> String {
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        format!(
            r#"You are an expert Python developer analyzing whether a file should be excluded from linting.

File: {}
File name: {}
File type: {}

File content should be analyzed to determine exclusion.

Determine if this file should be excluded from linting based on:

**EXCLUDE FROM LINTING:**
- Test files that may have acceptable style violations for readability
- Example/demo code that intentionally breaks conventions for illustration
- Legacy code being gradually phased out
- Generated code files that can't be reformatted (migrations, protobuf, etc.)
- Files with complex generated code patterns
- Vendor/third-party code that shouldn't be modified

**INCLUDE IN LINTING:**
- All production application code
- Configuration files (to ensure consistency)
- Utility and helper modules
- Documentation that contains code examples
- User-authored Python files

**IMPORTANT CONSIDERATIONS:**
- Generated files like *_pb2.py, *_pb2_grpc.py should be EXCLUDED
- Migration files (Django/Alembic) should be EXCLUDED
- Test files MAY be excluded if they need relaxed style rules
- Configuration files should generally be INCLUDED for consistency

Analyze this file and provide a clear recommendation."#,
            file_path.display(),
            file_name,
            extension
        )
    }

    /// Create general exclusion analysis prompt
    #[allow(dead_code)]
    fn create_general_analysis_prompt(&self, file_path: &Path, file_content: &str) -> String {
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        format!(
            r#"You are an expert software developer analyzing whether a file should be excluded from ALL code quality processing.

File: {}
File name: {}
File type: {}

File content:
```{extension}
{file_content}
```

Determine if this file should be COMPLETELY EXCLUDED from processing based on:

**EXCLUDE FROM ALL PROCESSING:**
- Binary files, images, databases
- Compiled files (.pyc, .pyo, .pyd)
- Cache directories (__pycache__)
- Generated/compiled artifacts
- Third-party vendored code that shouldn't be modified
- Build outputs and temporary files

**INCLUDE IN PROCESSING:**
- All user-authored source code
- Configuration files
- Documentation files
- Test files
- Scripts and utilities
- Any file that developers actively maintain

**KEY PRINCIPLE:**
Only exclude files that are:
1. Not authored by developers (generated/compiled)
2. Binary/non-text files
3. Temporary or cache files
4. Third-party code that shouldn't be modified

User-authored code should almost always be included in quality processing."#,
            file_path.display(),
            file_name,
            extension
        )
    }

    /// Create a comprehensive analysis prompt (covers all exclusion contexts)
    fn create_comprehensive_analysis_prompt(&self, file_path: &Path, file_content: &str) -> String {
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        // Get project context - look for existing configuration
        let project_root = file_path.parent().unwrap_or(Path::new("."));
        let mut context_info = String::new();

        // Check for common project files to understand context
        if project_root.join("pyproject.toml").exists() {
            context_info.push_str("Project uses pyproject.toml configuration.\n");
        }
        if project_root.join("setup.py").exists() {
            context_info.push_str("Project uses setup.py configuration.\n");
        }
        if project_root.join("requirements.txt").exists() {
            context_info.push_str("Project uses requirements.txt for dependencies.\n");
        }

        format!(
            r#"You are an expert software developer analyzing Python files for intelligent exclusion patterns in a code quality toolchain.

File: {}
File name: {}
File type: {}{}

File content:
```{}
{}
```

**YOUR TASK:** Analyze this file and make SPECIFIC, CLEAR decisions for each exclusion context:

**CONTEXT 1: GENERAL EXCLUSION** (exclude from ALL processing)
EXCLUDE if file is:
- Generated/compiled: *_pb2.py, *_pb2_grpc.py, migrations, .pyc, .pyo, .pyd
- Binary/non-text: images, databases, compiled artifacts  
- Cache/temporary: __pycache__, .pytest_cache, build artifacts
- Third-party vendor code that shouldn't be modified

**CONTEXT 2: LINT EXCLUSION** (exclude from linting/formatting)
EXCLUDE if file is:
- Generated code that can't be reformatted (migrations, protobuf)
- Test files that intentionally break style rules for readability
- Example/demo code that breaks conventions for illustration  
- Legacy code being gradually phased out
- Vendor/third-party code

**CONTEXT 3: TEST EXCLUSION** (exclude from test requirements)  
EXCLUDE if file is:
- Pure configuration with ONLY constants/settings (no logic)
- Simple data models without business logic
- Test files themselves (test_*.py, *_test.py)
- Files with only imports/exports
- Example/demo files
- Generated files

**CRITICAL ANALYSIS POINTS:**
- Files with business logic, algorithms, complex operations → NEED TESTS
- Configuration files → usually DON'T need tests but DO need linting
- Utility functions/helpers → NEED TESTS and linting  
- Generated files → usually exclude from EVERYTHING
- User-authored Python code → usually include in linting, may need tests

**PROJECT CONTEXT:**
{}

**REQUIRED OUTPUT:** For each exclusion type, provide:
1. Clear YES/NO decision with STRONG reasoning
2. Specific actionable recommendation
3. Use warning emojis (⚠️) and capital letters for emphasis when files NEED tests

Be DIRECTIVE and use CLEAR language. If unsure, err on the side of INCLUDING files in quality checks."#,
            file_path.display(),
            file_name,
            extension,
            if !context_info.is_empty() {
                format!("\n\nProject context:\n{}", context_info)
            } else {
                String::new()
            },
            extension,
            file_content,
            context_info
        )
    }

    /// Provide heuristic analysis when Cerebras API is not configured
    fn heuristic_analysis(&self, file_path: &Path) -> ExclusionAnalysis {
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        // Basic heuristics for common file types
        let (should_exclude_general, should_exclude_lint, should_exclude_test, reasoning) =
            match (file_name, extension) {
                (name, _) if name.starts_with("test_") || name.ends_with("_test.py") => {
                    (false, true, true, "Test files should be excluded from testing requirements and may have relaxed linting")
                },
                (_, "pyc") | (_, "pyo") | (_, "pyd") => {
                    (true, true, true, "Compiled Python files should be excluded from all processing")
                },
                (name, _) if name.contains("__pycache__") => {
                    (true, true, true, "Python cache files should be excluded from all processing")
                },
                (name, _) if name.starts_with(".") => {
                    (true, true, true, "Hidden files typically don't require processing")
                },
                (_, "py") => {
                    (false, false, false, "Regular Python files should be processed normally")
                },
                _ => {
                    (true, true, true, "Non-Python files excluded from Python-specific processing")
                }
            };

        ExclusionAnalysis {
            should_exclude_general,
            should_exclude_lint,
            should_exclude_test,
            reasoning: reasoning.to_string(),
            file_type: format!("{} file", extension),
            purpose: "Unknown (analyzed without AI)".to_string(),
            exclusion_recommendation: format!(
                "Based on file pattern analysis: general={}, lint={}, test={}",
                should_exclude_general, should_exclude_lint, should_exclude_test
            ),
        }
    }

    /// Conservative analysis when API fails - assumes files need full processing
    fn conservative_analysis(&self, _file_path: &Path, reason: &str) -> ExclusionAnalysis {
        ExclusionAnalysis {
            should_exclude_general: false,  // Don't exclude - process normally
            should_exclude_lint: false,     // Don't exclude - show all lint issues  
            should_exclude_test: false,     // Don't exclude - assume tests needed
            reasoning: format!("{}, using conservative defaults - assuming file needs full processing", reason),
            file_type: "Unknown (API unavailable)".to_string(),
            purpose: "Unknown - assuming requires full validation".to_string(),
            exclusion_recommendation: "⚠️ Could not analyze file due to API error. File will be processed normally. Ensure tests exist for this file if it contains business logic.".to_string(),
        }
    }

    /// Analyze test output comprehensively using Cerebras AI
    pub async fn analyze_test_output(
        &self,
        output: &str,
        project_path: &Path,
        source_file: Option<&Path>,
    ) -> Result<TestFailureAnalysis> {
        if !self.config.enabled {
            return Ok(self.basic_test_failure_analysis(output));
        }

        // Handle API errors gracefully with basic analysis
        match self
            .call_cerebras_comprehensive_test_analysis(output, project_path, source_file)
            .await
        {
            Ok(analysis) => Ok(analysis),
            Err(e) => {
                eprintln!("Warning: Cerebras test analysis failed: {}", e);
                Ok(self.basic_test_failure_analysis(output))
            }
        }
    }

    /// Analyze lint output using Cerebras AI
    pub async fn analyze_lint_output(
        &self,
        output: &str,
        file_path: Option<&Path>,
    ) -> Result<LintAnalysis> {
        if !self.config.enabled {
            return Ok(self.basic_lint_analysis(output));
        }

        // Handle API errors gracefully with basic analysis
        match self.call_cerebras_lint_analysis(output, file_path).await {
            Ok(analysis) => Ok(analysis),
            Err(e) => {
                eprintln!("Warning: Cerebras lint analysis failed: {}", e);
                Ok(self.basic_lint_analysis(output))
            }
        }
    }

    /// Make comprehensive API call to Cerebras for test analysis
    async fn call_cerebras_comprehensive_test_analysis(
        &self,
        output: &str,
        project_path: &Path,
        source_file: Option<&Path>,
    ) -> Result<TestFailureAnalysis> {
        let prompt = self.create_comprehensive_test_prompt(output, project_path, source_file);

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt,
            }],
            temperature: 0.3,
            top_p: 0.9,
            response_format: ResponseFormat {
                format_type: "json_schema".to_string(),
                json_schema: JsonSchema {
                    name: "test_failure_analysis".to_string(),
                    description: "Analysis of test failure output".to_string(),
                    schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "has_failures": {
                                "type": "boolean",
                                "description": "Whether there are actual test failures"
                            },
                            "summary": {
                                "type": "string",
                                "description": "Brief summary of test execution results"
                            },
                            "failed_tests": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "test_name": {"type": "string"},
                                        "error_type": {"type": "string"},
                                        "error_message": {"type": "string"},
                                        "suggested_fix": {"type": "string"}
                                    },
                                    "required": ["test_name", "error_type", "error_message", "suggested_fix"]
                                }
                            },
                            "analysis": {
                                "type": "string",
                                "description": "Detailed analysis of test execution and failures"
                            },
                            "recommendations": {
                                "type": "string",
                                "description": "Specific actionable recommendations for immediate fixes"
                            },
                            "coverage_analysis": {
                                "type": "string",
                                "description": "Analysis of test coverage gaps and missing scenarios"
                            },
                            "missing_tests": {
                                "type": "array",
                                "items": {"type": "string"},
                                "description": "List of specific test functions or scenarios that should be added"
                            },
                            "quality_assessment": {
                                "type": "string",
                                "description": "Assessment of overall test quality and completeness"
                            }
                        },
                        "required": ["has_failures", "summary", "failed_tests", "analysis", "recommendations", "coverage_analysis", "missing_tests", "quality_assessment"]
                    }),
                },
            },
        };

        let response = self
            .client
            .post(&format!("{}/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Cerebras API")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("API request failed: {}", response.status()));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .context("Failed to parse response JSON")?;

        let content = chat_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_ref())
            .context("No content in API response")?;

        let analysis: TestFailureAnalysis =
            serde_json::from_str(content).context("Failed to parse analysis JSON")?;

        Ok(analysis)
    }

    /// Make API call to Cerebras for lint output analysis
    async fn call_cerebras_lint_analysis(
        &self,
        output: &str,
        file_path: Option<&Path>,
    ) -> Result<LintAnalysis> {
        let prompt = self.create_lint_output_prompt(output, file_path);

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt,
            }],
            temperature: 0.3,
            top_p: 0.9,
            response_format: ResponseFormat {
                format_type: "json_schema".to_string(),
                json_schema: JsonSchema {
                    name: "lint_analysis".to_string(),
                    description: "Analysis of linter output".to_string(),
                    schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "has_real_issues": {
                                "type": "boolean",
                                "description": "Whether there are real issues that need fixing"
                            },
                            "filtered_output": {
                                "type": "string",
                                "description": "Linter output with only real issues (empty if no real issues)"
                            },
                            "reasoning": {
                                "type": "string",
                                "description": "Brief explanation of what was filtered and why"
                            },
                            "issue_count": {
                                "type": "integer",
                                "description": "Number of real issues found"
                            },
                            "recommendations": {
                                "type": "string",
                                "description": "Specific recommendations for fixing the issues"
                            }
                        },
                        "required": ["has_real_issues", "filtered_output", "reasoning", "issue_count", "recommendations"]
                    }),
                },
            },
        };

        let response = self
            .client
            .post(&format!("{}/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Cerebras API")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("API request failed: {}", response.status()));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .context("Failed to parse response JSON")?;

        let content = chat_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_ref())
            .context("No content in API response")?;

        let analysis: LintAnalysis =
            serde_json::from_str(content).context("Failed to parse analysis JSON")?;

        Ok(analysis)
    }

    /// Create comprehensive prompt for test analysis including source code and coverage analysis
    fn create_comprehensive_test_prompt(
        &self,
        output: &str,
        project_path: &Path,
        source_file: Option<&Path>,
    ) -> String {
        let mut source_content = String::new();
        let mut test_content = String::new();
        let mut file_context = String::new();

        // Read source file if provided
        if let Some(source_path) = source_file {
            file_context = format!("Source file: {}", source_path.display());

            if let Ok(content) = self.read_file_content(source_path) {
                source_content =
                    format!("\n\nSource code being tested:\n```python\n{}\n```", content);
            }

            // Try to find corresponding test file
            let source_name = source_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");
            let possible_test_paths = vec![
                format!("test_{}.py", source_name),
                format!("tests/test_{}.py", source_name),
                format!("tests/unit/test_{}.py", source_name),
                format!("test/test_{}.py", source_name),
            ];

            for test_path_str in &possible_test_paths {
                let test_path = project_path.join(test_path_str);
                if test_path.exists() {
                    if let Ok(content) = self.read_file_content(&test_path) {
                        test_content = format!(
                            "\n\nExisting test file ({}): \n```python\n{}\n```",
                            test_path.display(),
                            content
                        );
                        break;
                    }
                }
            }

            if test_content.is_empty() {
                test_content = "\n\n⚠️ No test file found for this source file.".to_string();
            }
        }

        format!(
            r#"You are an expert Python developer conducting a comprehensive test analysis.

Project: {}
{}{}{}

Test execution output:
```
{}
```

Provide a comprehensive analysis covering:

## 1. Test Execution Analysis
- **Summary**: What happened? (passed/failed/errors)
- **Failed Tests**: For each failure, identify:
  - Test name and location
  - Error type (AssertionError, ImportError, fixture issues, etc.)
  - Root cause analysis
  - Specific fix needed

## 2. Test Coverage & Completeness Analysis
Analyze the source code and existing tests to determine:
- **Coverage gaps**: What functionality lacks tests?
- **Missing test scenarios**: Edge cases, error conditions, boundary values
- **Test quality**: Are tests comprehensive enough?

## 3. Test Improvement Recommendations
- **New tests needed**: Specific test functions to add with their purpose
- **Existing test improvements**: How to make current tests better
- **Test structure**: Better organization, fixtures, parameterization
- **Performance considerations**: Slow tests, redundant tests

## 4. Actionable Next Steps
Provide SPECIFIC, IMMEDIATE actions:
- Commands to run for debugging
- Code changes needed (with examples)
- New test functions to write (with names and purposes)
- Dependencies or configuration fixes

## 5. Quality Assessment
Rate the current test suite (if tests exist):
- Completeness: Does it test all functionality? 
- Quality: Are tests well-written and maintainable?
- Missing scenarios: What important cases are untested?

Focus on being COMPREHENSIVE, SPECIFIC, and ACTIONABLE. Even if tests pass, suggest improvements and additional test coverage."#,
            project_path.display(),
            file_context,
            source_content,
            test_content,
            output
        )
    }

    /// Create prompt for lint output analysis  
    fn create_lint_output_prompt(&self, output: &str, file_path: Option<&Path>) -> String {
        let file_context = if let Some(path) = file_path {
            format!("\nFile being linted: {}", path.display())
        } else {
            String::new()
        };

        format!(
            r#"You are an expert Python developer analyzing linter output.{}

Linter output:
```
{}
```

Analyze this linter output and determine which issues are real problems vs false positives.

Common false positives to filter out:
- TC003/TC004 errors about imports when Pydantic models need types at runtime for validation
- Import errors for types that are actually needed for runtime validation (like Callable, Awaitable in Pydantic models)
- Issues with TYPE_CHECKING blocks when the types are needed by Pydantic Field definitions
- "Moving imports into TYPE_CHECKING block" suggestions when those imports are used in Pydantic model fields

Remember that Pydantic needs certain types at runtime for validation, not just for type checking.

Provide:
1. **Real Issues**: Filter out false positives and return only issues that need fixing
2. **Issue Count**: Number of real issues found
3. **Reasoning**: Brief explanation of what was filtered and why
4. **Recommendations**: Specific suggestions for fixing the real issues

If all issues are false positives, return empty filtered_output and explain why in the reasoning."#,
            file_context, output
        )
    }

    /// Basic test failure analysis when AI is not available
    fn basic_test_failure_analysis(&self, output: &str) -> TestFailureAnalysis {
        let has_failures =
            output.contains("FAILED") || output.contains("ERROR") || output.contains("FAIL");
        let line_count = output.lines().count();

        TestFailureAnalysis {
            has_failures,
            summary: if has_failures {
                format!("Test failures detected in {} lines of output", line_count)
            } else {
                "No clear test failures detected".to_string()
            },
            failed_tests: vec![], // Can't parse specific tests without AI
            analysis: "Basic analysis without AI - full output shown".to_string(),
            recommendations: if has_failures {
                "Review the test output above for specific failure details. Run tests individually with -v flag for more details.".to_string()
            } else {
                "Tests appear to have passed. Consider reviewing test coverage.".to_string()
            },
            coverage_analysis:
                "AI analysis not available. Consider manually reviewing test coverage.".to_string(),
            missing_tests: vec![], // Can't determine without AI analysis
            quality_assessment: "Unable to assess test quality without AI analysis.".to_string(),
        }
    }

    /// Basic lint analysis when AI is not available
    fn basic_lint_analysis(&self, output: &str) -> LintAnalysis {
        let has_issues = !output.trim().is_empty();
        let line_count = output.lines().count();

        LintAnalysis {
            has_real_issues: has_issues,
            filtered_output: output.to_string(),
            reasoning: "Basic analysis without AI - showing all linter output".to_string(),
            issue_count: line_count as u32,
            recommendations: if has_issues {
                "Review the linter output above and fix the reported issues.".to_string()
            } else {
                "No linting issues detected.".to_string()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let config = CerebrasConfig::default();
        assert_eq!(config.base_url, "https://api.cerebras.ai/v1");
        assert_eq!(config.model, "qwen-3-coder-480b");
        // enabled depends on CEREBRAS_API_KEY env var
    }

    #[test]
    fn test_analyzer_creation() {
        let config = CerebrasConfig::default();
        let analyzer = SmartExclusionAnalyzer::new(config);
        // Just verify it can be created without panicking
        assert!(!analyzer.config.base_url.is_empty());
    }

    #[test]
    fn test_default_analysis_patterns() {
        let config = CerebrasConfig {
            enabled: false, // Force fallback analysis
            ..CerebrasConfig::default()
        };
        let analyzer = SmartExclusionAnalyzer::new(config);

        // Test Python cache file
        let analysis = analyzer.heuristic_analysis(Path::new("__pycache__/module.pyc"));
        assert!(analysis.should_exclude_general);
        assert!(analysis.should_exclude_lint);
        assert!(analysis.should_exclude_test);

        // Test regular Python file
        let analysis = analyzer.heuristic_analysis(Path::new("src/main.py"));
        assert!(!analysis.should_exclude_general);
        assert!(!analysis.should_exclude_lint);
        assert!(!analysis.should_exclude_test);

        // Test test file
        let analysis = analyzer.heuristic_analysis(Path::new("test_module.py"));
        assert!(!analysis.should_exclude_general);
        assert!(analysis.should_exclude_lint);
        assert!(analysis.should_exclude_test);
    }

    #[tokio::test]
    async fn test_read_file_content() {
        let config = CerebrasConfig::default();
        let analyzer = SmartExclusionAnalyzer::new(config);

        // Create a temporary file with some content
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "print('Hello, World!')").unwrap();

        let content = analyzer.read_file_content(temp_file.path()).unwrap();
        assert!(content.contains("Hello, World!"));
    }

    #[tokio::test]
    async fn test_analyze_file_without_api_key() {
        let config = CerebrasConfig {
            enabled: false,
            ..CerebrasConfig::default()
        };
        let analyzer = SmartExclusionAnalyzer::new(config);

        // Create a temporary Python file with proper extension
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_file_path = temp_dir.path().join("test.py");
        std::fs::write(&temp_file_path, "def hello(): return 'world'").unwrap();

        let analysis = analyzer.analyze_file(&temp_file_path).await.unwrap();
        assert!(!analysis.should_exclude_general);
        assert!(analysis.purpose.contains("analyzed without AI"));
    }

    #[test]
    fn test_conservative_analysis() {
        let analyzer = SmartExclusionAnalyzer::new(CerebrasConfig::default());
        let test_file = Path::new("src/business_logic.py");

        let analysis = analyzer.conservative_analysis(test_file, "API error occurred");

        // Conservative analysis should never exclude files
        assert!(
            !analysis.should_exclude_general,
            "Conservative analysis should not exclude files from general processing"
        );
        assert!(
            !analysis.should_exclude_lint,
            "Conservative analysis should not exclude files from linting"
        );
        assert!(
            !analysis.should_exclude_test,
            "Conservative analysis should not exclude files from testing"
        );

        // Should mention conservative defaults
        assert!(
            analysis.reasoning.contains("conservative defaults"),
            "Reasoning should mention conservative defaults, got: {}",
            analysis.reasoning
        );
        assert!(
            analysis.reasoning.contains("API error occurred"),
            "Reasoning should include the error reason, got: {}",
            analysis.reasoning
        );

        // Should indicate API unavailability
        assert_eq!(analysis.file_type, "Unknown (API unavailable)");
        assert!(analysis.purpose.contains("Unknown - assuming requires"));

        // Should include warning in recommendation
        assert!(analysis.exclusion_recommendation.contains("⚠️"));
        assert!(analysis.exclusion_recommendation.contains("API error"));
    }

    #[test]
    fn test_heuristic_analysis_python_files() {
        let analyzer = SmartExclusionAnalyzer::new(CerebrasConfig::default());

        // Test regular Python file
        let regular_py = Path::new("src/models.py");
        let analysis = analyzer.heuristic_analysis(regular_py);
        assert!(!analysis.should_exclude_general);
        assert!(!analysis.should_exclude_lint);
        assert!(!analysis.should_exclude_test);
        assert!(analysis.reasoning.contains("Regular Python files"));

        // Test test file
        let test_file = Path::new("test_example.py");
        let analysis = analyzer.heuristic_analysis(test_file);
        assert!(!analysis.should_exclude_general);
        assert!(analysis.should_exclude_lint);
        assert!(analysis.should_exclude_test);
        assert!(analysis.reasoning.contains("Test files"));

        // Test cache file (.pyc extension matches first)
        let cache_file = Path::new("__pycache__/module.pyc");
        let analysis = analyzer.heuristic_analysis(cache_file);
        assert!(analysis.should_exclude_general);
        assert!(analysis.should_exclude_lint);
        assert!(analysis.should_exclude_test);
        assert!(analysis.reasoning.contains("Compiled Python files"));

        // Test cache directory file (filename contains __pycache__)
        let cache_dir_file = Path::new("module__pycache__temp.py");
        let analysis = analyzer.heuristic_analysis(cache_dir_file);
        assert!(analysis.should_exclude_general);
        assert!(analysis.should_exclude_lint);
        assert!(analysis.should_exclude_test);
        assert!(analysis.reasoning.contains("Python cache files"));
    }

    #[test]
    fn test_heuristic_analysis_non_python_files() {
        let analyzer = SmartExclusionAnalyzer::new(CerebrasConfig::default());

        // Test Rust file
        let rust_file = Path::new("src/main.rs");
        let analysis = analyzer.heuristic_analysis(rust_file);
        assert!(analysis.should_exclude_general);
        assert!(analysis.should_exclude_lint);
        assert!(analysis.should_exclude_test);
        assert!(analysis.reasoning.contains("Non-Python files"));

        // Test config file
        let config_file = Path::new("config.yaml");
        let analysis = analyzer.heuristic_analysis(config_file);
        assert!(analysis.should_exclude_general);
        assert!(analysis.should_exclude_lint);
        assert!(analysis.should_exclude_test);
    }

    #[test]
    fn test_analyze_file_with_enabled_config_but_no_api() {
        // Test what happens when API is enabled but we can't actually make a call
        // This simulates the API error handling by testing with an enabled config
        // but in a controlled environment where we know it will fail
        let config = CerebrasConfig {
            enabled: true,
            api_key: "fake-key-for-test".to_string(),
            ..CerebrasConfig::default()
        };
        let analyzer = SmartExclusionAnalyzer::new(config);

        // We can't easily mock the HTTP client, but we can verify the conservative_analysis
        // method directly works as expected for the fallback case
        let conservative_result =
            analyzer.conservative_analysis(Path::new("test.py"), "simulated API error");

        // Conservative analysis should never exclude
        assert!(!conservative_result.should_exclude_general);
        assert!(!conservative_result.should_exclude_lint);
        assert!(!conservative_result.should_exclude_test);
        assert!(conservative_result
            .reasoning
            .contains("conservative defaults"));
        assert!(conservative_result
            .reasoning
            .contains("simulated API error"));
    }
}
