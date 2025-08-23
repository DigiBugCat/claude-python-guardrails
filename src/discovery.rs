use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use which::which;

/// Represents different Python tools available for linting
#[derive(Debug, Clone, PartialEq)]
pub enum PythonLinter {
    Ruff,
    Flake8,
    Pylint,
}

/// Represents different Python tools available for testing
#[derive(Debug, Clone, PartialEq)]
pub enum PythonTester {
    Pytest,
    PytestModule,
    Unittest,
}

/// Information about a discovered Python project
#[derive(Debug)]
pub struct PythonProject {
    pub root: PathBuf,
    pub project_type: ProjectType,
    pub available_linters: Vec<PythonLinter>,
    pub available_testers: Vec<PythonTester>,
}

/// Type of Python project detected
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectType {
    Modern,    // Has pyproject.toml
    Classical, // Has setup.py
    Simple,    // Has requirements.txt or just .py files
    Git,       // Git repository with Python files
}

impl PythonProject {
    /// Discover Python project information starting from the given directory
    pub fn discover<P: AsRef<Path>>(start_dir: P) -> Result<Self> {
        let start_path = start_dir.as_ref();
        let project_root =
            Self::find_project_root(start_path).context("Failed to find Python project root")?;

        let project_type = Self::detect_project_type(&project_root);
        let available_linters = Self::detect_available_linters();
        let available_testers = Self::detect_available_testers();

        Ok(Self {
            root: project_root,
            project_type,
            available_linters,
            available_testers,
        })
    }

    /// Walk up the directory tree to find the Python project root
    fn find_project_root(start_dir: &Path) -> Option<PathBuf> {
        // Convert to absolute path if needed
        let absolute_start = if start_dir.is_absolute() {
            start_dir.to_path_buf()
        } else {
            std::env::current_dir().ok()?.join(start_dir)
        };

        let mut current_dir = absolute_start.as_path();

        loop {
            // Check for Python project markers
            if Self::is_python_project_root(current_dir) {
                return Some(current_dir.to_path_buf());
            }

            // Move up one directory
            match current_dir.parent() {
                Some(parent) => current_dir = parent,
                None => break,
            }
        }

        // No project root found, return the starting directory
        Some(absolute_start)
    }

    /// Check if a directory contains Python project markers
    fn is_python_project_root(dir: &Path) -> bool {
        // Primary markers
        if dir.join("pyproject.toml").exists()
            || dir.join("setup.py").exists()
            || dir.join("setup.cfg").exists()
        {
            return true;
        }

        // Secondary markers
        if dir.join("requirements.txt").exists()
            || dir.join("requirements").is_dir()
            || dir.join("Pipfile").exists()
            || dir.join("poetry.lock").exists()
        {
            return true;
        }

        // Git repository with Python files
        if dir.join(".git").exists() {
            // Check for Python files in reasonable depth
            if Self::has_python_files(dir, 3) {
                return true;
            }
        }

        false
    }

    /// Check if directory has Python files within given depth
    fn has_python_files(dir: &Path, max_depth: usize) -> bool {
        if max_depth == 0 {
            return false;
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_file() && path.extension().is_some_and(|ext| ext == "py") {
                    return true;
                }

                if path.is_dir()
                    && !path
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
                    && Self::has_python_files(&path, max_depth - 1)
                {
                    return true;
                }
            }
        }

        false
    }

    /// Detect the type of Python project
    fn detect_project_type(root: &Path) -> ProjectType {
        if root.join("pyproject.toml").exists() {
            ProjectType::Modern
        } else if root.join("setup.py").exists() {
            ProjectType::Classical
        } else if root.join("requirements.txt").exists()
            || root.join("requirements").is_dir()
            || root.join("Pipfile").exists()
            || root.join("poetry.lock").exists()
        {
            ProjectType::Simple
        } else if root.join(".git").exists() {
            ProjectType::Git
        } else {
            ProjectType::Simple
        }
    }

    /// Detect available Python linting tools
    fn detect_available_linters() -> Vec<PythonLinter> {
        let mut linters = Vec::new();

        if which("ruff").is_ok() {
            linters.push(PythonLinter::Ruff);
        }
        if which("flake8").is_ok() {
            linters.push(PythonLinter::Flake8);
        }
        if which("pylint").is_ok() {
            linters.push(PythonLinter::Pylint);
        }

        linters
    }

    /// Detect available Python testing tools
    fn detect_available_testers() -> Vec<PythonTester> {
        let mut testers = Vec::new();

        if which("pytest").is_ok() {
            testers.push(PythonTester::Pytest);
        }

        if which("python").is_ok() || which("python3").is_ok() {
            testers.push(PythonTester::PytestModule);
            testers.push(PythonTester::Unittest);
        }

        testers
    }

    /// Get the preferred linter (first available in priority order)
    pub fn preferred_linter(&self) -> Option<&PythonLinter> {
        self.available_linters.first()
    }

    /// Get the preferred tester (first available in priority order)
    pub fn preferred_tester(&self) -> Option<&PythonTester> {
        self.available_testers.first()
    }

    /// Check if the project has any linting tools available
    pub fn has_linter(&self) -> bool {
        !self.available_linters.is_empty()
    }

    /// Check if the project has any testing tools available
    pub fn has_tester(&self) -> bool {
        !self.available_testers.is_empty()
    }
}

impl PythonLinter {
    /// Get the command to run this linter
    pub fn command(&self) -> &'static str {
        match self {
            PythonLinter::Ruff => "ruff",
            PythonLinter::Flake8 => "flake8",
            PythonLinter::Pylint => "pylint",
        }
    }

    /// Get the arguments to run this linter on the current directory
    pub fn args(&self) -> Vec<&'static str> {
        match self {
            PythonLinter::Ruff => vec!["check", "."],
            PythonLinter::Flake8 => vec!["."],
            PythonLinter::Pylint => vec!["."],
        }
    }

    /// Get the human-readable name for error messages
    pub fn display_name(&self) -> &'static str {
        match self {
            PythonLinter::Ruff => "ruff check .",
            PythonLinter::Flake8 => "flake8 .",
            PythonLinter::Pylint => "pylint .",
        }
    }
}

impl PythonTester {
    /// Get the command to run this tester
    pub fn command(&self) -> &'static str {
        match self {
            PythonTester::Pytest => "pytest",
            PythonTester::PytestModule => "python",
            PythonTester::Unittest => "python",
        }
    }

    /// Get the arguments to run this tester
    pub fn args(&self) -> Vec<&'static str> {
        match self {
            PythonTester::Pytest => vec![],
            PythonTester::PytestModule => vec!["-m", "pytest"],
            PythonTester::Unittest => vec!["-m", "unittest", "discover"],
        }
    }

    /// Get the human-readable name for error messages
    pub fn display_name(&self) -> &'static str {
        match self {
            PythonTester::Pytest => "pytest",
            PythonTester::PytestModule => "python -m pytest",
            PythonTester::Unittest => "python -m unittest discover",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_project_type_detection() {
        let temp_dir = TempDir::new().unwrap();

        // Test modern project (pyproject.toml)
        fs::write(temp_dir.path().join("pyproject.toml"), "[tool.poetry]").unwrap();
        assert_eq!(
            PythonProject::detect_project_type(temp_dir.path()),
            ProjectType::Modern
        );

        // Clean up
        fs::remove_file(temp_dir.path().join("pyproject.toml")).unwrap();

        // Test classical project (setup.py)
        fs::write(
            temp_dir.path().join("setup.py"),
            "from setuptools import setup",
        )
        .unwrap();
        assert_eq!(
            PythonProject::detect_project_type(temp_dir.path()),
            ProjectType::Classical
        );

        // Clean up
        fs::remove_file(temp_dir.path().join("setup.py")).unwrap();

        // Test simple project (requirements.txt)
        fs::write(temp_dir.path().join("requirements.txt"), "requests").unwrap();
        assert_eq!(
            PythonProject::detect_project_type(temp_dir.path()),
            ProjectType::Simple
        );
    }

    #[test]
    fn test_python_files_detection() {
        let temp_dir = TempDir::new().unwrap();

        // Create a Python file
        fs::write(temp_dir.path().join("main.py"), "print('hello')").unwrap();

        assert!(PythonProject::has_python_files(temp_dir.path(), 1));

        // Test nested Python files
        let subdir = temp_dir.path().join("src");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("app.py"), "print('app')").unwrap();

        assert!(PythonProject::has_python_files(temp_dir.path(), 2));
    }

    #[test]
    fn test_is_python_project_root() {
        let temp_dir = TempDir::new().unwrap();

        // Empty directory should not be considered a project root
        assert!(!PythonProject::is_python_project_root(temp_dir.path()));

        // Adding pyproject.toml should make it a project root
        fs::write(temp_dir.path().join("pyproject.toml"), "[tool.poetry]").unwrap();
        assert!(PythonProject::is_python_project_root(temp_dir.path()));

        // Clean up
        fs::remove_file(temp_dir.path().join("pyproject.toml")).unwrap();

        // Adding setup.py should make it a project root
        fs::write(
            temp_dir.path().join("setup.py"),
            "from setuptools import setup",
        )
        .unwrap();
        assert!(PythonProject::is_python_project_root(temp_dir.path()));
    }

    #[test]
    fn test_linter_commands() {
        assert_eq!(PythonLinter::Ruff.command(), "ruff");
        assert_eq!(PythonLinter::Ruff.args(), vec!["check", "."]);
        assert_eq!(PythonLinter::Ruff.display_name(), "ruff check .");

        assert_eq!(PythonLinter::Flake8.command(), "flake8");
        assert_eq!(PythonLinter::Flake8.args(), vec!["."]);

        assert_eq!(PythonLinter::Pylint.command(), "pylint");
        assert_eq!(PythonLinter::Pylint.args(), vec!["."]);
    }

    #[test]
    fn test_tester_commands() {
        assert_eq!(PythonTester::Pytest.command(), "pytest");
        assert_eq!(PythonTester::Pytest.args(), Vec::<&str>::new());

        assert_eq!(PythonTester::PytestModule.command(), "python");
        assert_eq!(PythonTester::PytestModule.args(), vec!["-m", "pytest"]);

        assert_eq!(PythonTester::Unittest.command(), "python");
        assert_eq!(
            PythonTester::Unittest.args(),
            vec!["-m", "unittest", "discover"]
        );
    }

    #[test]
    fn test_project_discovery() {
        let temp_dir = TempDir::new().unwrap();

        // Create a basic Python project
        fs::write(temp_dir.path().join("pyproject.toml"), "[tool.poetry]").unwrap();
        fs::write(temp_dir.path().join("main.py"), "print('hello')").unwrap();

        // Create subdirectory to test discovery
        let subdir = temp_dir.path().join("src");
        fs::create_dir(&subdir).unwrap();

        let project = PythonProject::discover(&subdir).unwrap();
        assert_eq!(project.root, temp_dir.path());
        assert_eq!(project.project_type, ProjectType::Modern);
    }
}
