use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::PathBuf;

/// Input structure for Claude Code hook events
#[derive(Debug, Deserialize)]
pub struct HookInput {
    pub hook_event_name: String,
    pub tool_name: String,
    pub tool_input: ToolInput,
}

/// Tool input containing file paths
#[derive(Debug, Deserialize)]
pub struct ToolInput {
    pub file_path: Option<String>,
    pub notebook_path: Option<String>,
}

/// Response structure for hook communication (not currently used, but ready for future)
#[derive(Debug, Serialize)]
pub struct HookResponse {
    pub action: String,
    pub message: Option<String>,
}

impl HookInput {
    /// Read and parse JSON input from stdin
    pub fn from_stdin() -> Result<Self> {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .context("Failed to read from stdin")?;

        if buffer.trim().is_empty() {
            return Err(anyhow::anyhow!("No input available on stdin"));
        }

        serde_json::from_str(&buffer).context("Failed to parse JSON input")
    }

    /// Check if this is a PostToolUse event we should handle
    pub fn should_process(&self) -> bool {
        self.hook_event_name == "PostToolUse" && self.is_edit_tool()
    }

    /// Check if this is an edit-related tool
    pub fn is_edit_tool(&self) -> bool {
        matches!(
            self.tool_name.as_str(),
            "Edit" | "MultiEdit" | "Write" | "NotebookEdit"
        )
    }

    /// Extract the file path from the tool input
    pub fn file_path(&self) -> Option<PathBuf> {
        match self.tool_name.as_str() {
            "NotebookEdit" => self
                .tool_input
                .notebook_path
                .as_ref()
                .map(|p| PathBuf::from(p)),
            _ => self.tool_input.file_path.as_ref().map(|p| PathBuf::from(p)),
        }
    }
}

impl HookResponse {
    /// Create a continue response (no message to user)
    pub fn continue_silent() -> Self {
        Self {
            action: "continue".to_string(),
            message: None,
        }
    }

    /// Create a block response with error message
    pub fn block_with_error(message: &str) -> Self {
        Self {
            action: "block".to_string(),
            message: Some(message.to_string()),
        }
    }

    /// Create a continue response with success message
    pub fn continue_with_success(message: &str) -> Self {
        Self {
            action: "continue".to_string(),
            message: Some(message.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_input_parsing() {
        let json = r#"{
            "hook_event_name": "PostToolUse",
            "tool_name": "Edit",
            "tool_input": {
                "file_path": "/path/to/file.py"
            }
        }"#;

        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.hook_event_name, "PostToolUse");
        assert_eq!(input.tool_name, "Edit");
        assert_eq!(input.file_path(), Some(PathBuf::from("/path/to/file.py")));
        assert!(input.should_process());
        assert!(input.is_edit_tool());
    }

    #[test]
    fn test_notebook_edit_parsing() {
        let json = r#"{
            "hook_event_name": "PostToolUse",
            "tool_name": "NotebookEdit",
            "tool_input": {
                "notebook_path": "/path/to/notebook.ipynb"
            }
        }"#;

        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(
            input.file_path(),
            Some(PathBuf::from("/path/to/notebook.ipynb"))
        );
        assert!(input.should_process());
    }

    #[test]
    fn test_non_edit_tool() {
        let json = r#"{
            "hook_event_name": "PostToolUse",
            "tool_name": "Bash",
            "tool_input": {
                "command": "ls -la"
            }
        }"#;

        let input: HookInput = serde_json::from_str(json).unwrap();
        assert!(!input.should_process());
        assert!(!input.is_edit_tool());
    }

    #[test]
    fn test_hook_response_creation() {
        let continue_resp = HookResponse::continue_silent();
        assert_eq!(continue_resp.action, "continue");
        assert!(continue_resp.message.is_none());

        let block_resp = HookResponse::block_with_error("Test error");
        assert_eq!(block_resp.action, "block");
        assert_eq!(block_resp.message, Some("Test error".to_string()));

        let success_resp = HookResponse::continue_with_success("Test success");
        assert_eq!(success_resp.action, "continue");
        assert_eq!(success_resp.message, Some("Test success".to_string()));
    }
}
