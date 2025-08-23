use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::SystemTime;

/// Manages PID-based locking to prevent concurrent operations
pub struct ProcessLock {
    lock_file: PathBuf,
    operation: String,
    cooldown_seconds: u64,
}

impl ProcessLock {
    /// Create a new process lock for the given workspace and operation
    pub fn new(workspace_dir: &Path, operation: &str, cooldown_seconds: u64) -> Result<Self> {
        let workspace_hash = Self::hash_workspace(workspace_dir)?;
        let lock_file_name = format!("claude-python-guardrails-{operation}-{workspace_hash}.lock");
        let lock_file = PathBuf::from("/tmp").join(lock_file_name);

        Ok(Self {
            lock_file,
            operation: operation.to_string(),
            cooldown_seconds,
        })
    }

    /// Check if we should skip execution due to another running process or recent completion
    pub fn should_skip(&self) -> Result<bool> {
        if !self.lock_file.exists() {
            return Ok(false);
        }

        let lock_content = fs::read_to_string(&self.lock_file)
            .context("Failed to read lock file")?;

        let lines: Vec<&str> = lock_content.lines().collect();

        // Check if another process is running (PID in first line)
        if let Some(pid_line) = lines.first() {
            if let Ok(pid) = pid_line.trim().parse::<u32>() {
                if Self::is_process_running(pid) {
                    log::debug!(
                        "{} is already running (PID: {}), skipping",
                        self.operation,
                        pid
                    );
                    return Ok(true);
                }
            }
        }

        // Check completion timestamp (second line)
        if let Some(timestamp_line) = lines.get(1) {
            if let Ok(timestamp) = timestamp_line.trim().parse::<i64>() {
                let completion_time = DateTime::from_timestamp(timestamp, 0)
                    .ok_or_else(|| anyhow::anyhow!("Invalid timestamp in lock file"))?;

                let now = Utc::now();
                let duration_since_completion = now.signed_duration_since(completion_time);

                if duration_since_completion.num_seconds() < self.cooldown_seconds as i64 {
                    log::debug!(
                        "{} completed {}s ago (cooldown: {}s), skipping",
                        self.operation,
                        duration_since_completion.num_seconds(),
                        self.cooldown_seconds
                    );
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Acquire the lock by writing our PID to the lock file
    pub fn acquire(&self) -> Result<()> {
        let pid = process::id();
        fs::write(&self.lock_file, pid.to_string())
            .context("Failed to write PID to lock file")?;

        log::debug!("Acquired lock for {} (PID: {})", self.operation, pid);
        Ok(())
    }

    /// Release the lock by clearing PID and writing completion timestamp
    pub fn release(&self) -> Result<()> {
        let now = Utc::now();
        let timestamp = now.timestamp();

        let content = format!("\n{}", timestamp);
        fs::write(&self.lock_file, content)
            .context("Failed to write completion timestamp to lock file")?;

        log::debug!("Released lock for {} at {}", self.operation, now);
        Ok(())
    }

    /// Generate a hash of the workspace directory for unique lock files
    fn hash_workspace(workspace_dir: &Path) -> Result<String> {
        let absolute_path = workspace_dir
            .canonicalize()
            .context("Failed to canonicalize workspace path")?;

        let mut hasher = Sha256::new();
        hasher.update(absolute_path.to_string_lossy().as_bytes());
        let result = hasher.finalize();

        Ok(format!("{:x}", result)[..16].to_string())
    }

    /// Check if a process with the given PID is still running
    fn is_process_running(pid: u32) -> bool {
        #[cfg(unix)]
        {
            use std::process::Command;
            Command::new("kill")
                .args(&["-0", &pid.to_string()])
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
        }

        #[cfg(windows)]
        {
            use std::process::Command;
            Command::new("tasklist")
                .args(&["/FI", &format!("PID eq {}", pid)])
                .output()
                .map(|output| {
                    String::from_utf8_lossy(&output.stdout)
                        .contains(&pid.to_string())
                })
                .unwrap_or(false)
        }
    }
}

/// RAII guard that automatically releases the lock when dropped
pub struct LockGuard {
    lock: ProcessLock,
}

impl LockGuard {
    /// Try to acquire a lock, returning None if should skip
    pub fn try_acquire(
        workspace_dir: &Path,
        operation: &str,
        cooldown_seconds: u64,
    ) -> Result<Option<Self>> {
        let lock = ProcessLock::new(workspace_dir, operation, cooldown_seconds)?;

        if lock.should_skip()? {
            return Ok(None);
        }

        lock.acquire()?;
        Ok(Some(Self { lock }))
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if let Err(e) = self.lock.release() {
            eprintln!("Warning: Failed to release lock: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_lock_creation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let lock = ProcessLock::new(temp_dir.path(), "test", 5)?;

        assert!(lock.lock_file.to_string_lossy().contains("claude-python-guardrails-test-"));
        assert!(lock.lock_file.to_string_lossy().contains(".lock"));
        Ok(())
    }

    #[test]
    fn test_workspace_hashing() -> Result<()> {
        let temp_dir1 = TempDir::new()?;
        let temp_dir2 = TempDir::new()?;

        let hash1 = ProcessLock::hash_workspace(temp_dir1.path())?;
        let hash2 = ProcessLock::hash_workspace(temp_dir2.path())?;

        assert_ne!(hash1, hash2);
        assert_eq!(hash1.len(), 16); // First 16 chars of SHA256 hex
        Ok(())
    }

    #[test]
    fn test_lock_acquire_and_release() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let lock = ProcessLock::new(temp_dir.path(), "test", 1)?;

        // Should not skip initially
        assert!(!lock.should_skip()?);

        // Acquire lock
        lock.acquire()?;

        // Lock file should exist and contain our PID
        assert!(lock.lock_file.exists());
        let content = fs::read_to_string(&lock.lock_file)?;
        let pid: u32 = content.trim().parse().expect("Invalid PID in lock file");
        assert_eq!(pid, process::id());

        // Release lock
        lock.release()?;

        // Lock file should still exist but with completion timestamp
        let content = fs::read_to_string(&lock.lock_file)?;
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].is_empty()); // Empty PID line
        assert!(lines[1].parse::<i64>().is_ok()); // Valid timestamp

        Ok(())
    }

    #[test]
    fn test_cooldown_behavior() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let lock = ProcessLock::new(temp_dir.path(), "test", 2)?;

        // Acquire and release
        lock.acquire()?;
        lock.release()?;

        // Should skip due to cooldown
        assert!(lock.should_skip()?);

        // Wait for cooldown to expire
        thread::sleep(Duration::from_secs(3));

        // Should not skip anymore
        assert!(!lock.should_skip()?);

        Ok(())
    }

    #[test]
    fn test_lock_guard() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // First guard should acquire successfully
        let guard1 = LockGuard::try_acquire(temp_dir.path(), "test", 1)?;
        assert!(guard1.is_some());

        // Second guard should return None (already locked)
        let guard2 = LockGuard::try_acquire(temp_dir.path(), "test", 1)?;
        assert!(guard2.is_none());

        // Drop first guard
        drop(guard1);

        // Third guard should skip due to cooldown
        let guard3 = LockGuard::try_acquire(temp_dir.path(), "test", 10)?;
        assert!(guard3.is_none());

        Ok(())
    }
}