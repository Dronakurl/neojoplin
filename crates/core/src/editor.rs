// External editor integration for NeoJoplin

use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Editor configuration
#[derive(Debug, Clone)]
pub struct EditorConfig {
    /// Editor command (e.g., "helix", "vim", "code")
    pub command: String,
    /// Editor arguments
    pub args: Vec<String>,
    /// Use embedded terminal (for TUI mode)
    pub embedded: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        // Try to get editor from environment
        let command = std::env::var("EDITOR").unwrap_or_else(|_| {
            // Try common editors in order of preference
            if Self::command_exists("helix") {
                "helix".to_string()
            } else if Self::command_exists("vim") {
                "vim".to_string()
            } else if Self::command_exists("vi") {
                "vi".to_string()
            } else if Self::command_exists("nano") {
                "nano".to_string()
            } else {
                "ed".to_string() // Last resort
            }
        });

        Self {
            command,
            args: Vec::new(),
            embedded: false,
        }
    }
}

impl EditorConfig {
    /// Check if a command exists in PATH
    fn command_exists(cmd: &str) -> bool {
        Command::new("which")
            .arg(cmd)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Create editor from environment
    pub fn from_env() -> Result<Self> {
        Ok(Self::default())
    }

    /// Create editor with specific command
    pub fn new(command: String) -> Self {
        Self {
            command,
            args: Vec::new(),
            embedded: false,
        }
    }

    /// Add arguments to editor command
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }
}

/// Edit content using external editor
pub struct Editor {
    config: EditorConfig,
    temp_dir: PathBuf,
}

impl Editor {
    /// Create new editor with default config
    pub fn new() -> Result<Self> {
        let config = EditorConfig::from_env()?;
        Self::with_config(config)
    }

    /// Create editor with specific config
    pub fn with_config(config: EditorConfig) -> Result<Self> {
        // Store temporary editor files under the user's home directory so note
        // contents aren't exposed via a shared system temp directory.
        let home_dir = dirs::home_dir().context("Failed to locate home directory")?;
        let temp_dir = home_dir
            .join(".local")
            .join("state")
            .join("neojoplin")
            .join("editor");
        std::fs::create_dir_all(&temp_dir).context("Failed to create temp directory")?;
        secure_directory_permissions(&temp_dir)?;

        Ok(Self { config, temp_dir })
    }

    /// Edit content and return modified content
    pub async fn edit(&self, content: &str, file_hint: &str) -> Result<String> {
        // Create temp file
        let temp_file = self.create_temp_file(content, file_hint)?;

        // Get original modification time
        let original_mtime = self.get_mtime(&temp_file)?;

        // Launch editor
        let editor_result = self.launch_editor(&temp_file).await;

        // Read modified content
        let modified_content = self.read_temp_file(&temp_file)?;

        // Check if file was modified
        let new_mtime = self.get_mtime(&temp_file)?;
        let was_modified = new_mtime > original_mtime;

        // Cleanup temp file
        let _ = std::fs::remove_file(&temp_file);

        if editor_result.is_err() && !was_modified {
            return editor_result.map(|_| content.to_string());
        }

        if was_modified {
            Ok(modified_content)
        } else {
            Ok(content.to_string()) // Return original if not modified
        }
    }

    /// Create temp file with content
    fn create_temp_file(&self, content: &str, hint: &str) -> Result<PathBuf> {
        let filename = format!(
            "{}-{}.md",
            sanitize_file_hint(hint),
            uuid::Uuid::new_v4().simple()
        );
        let temp_file = self.temp_dir.join(filename);

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_file)
            .context("Failed to create temp file")?;

        secure_file_permissions(&temp_file)?;

        file.write_all(content.as_bytes())
            .context("Failed to write to temp file")?;

        file.flush().context("Failed to flush temp file")?;

        Ok(temp_file)
    }

    /// Launch external editor
    async fn launch_editor(&self, temp_file: &Path) -> Result<()> {
        // Build command
        let mut cmd = Command::new(&self.config.command);

        // Add arguments (temp file should be last)
        for arg in &self.config.args {
            cmd.arg(arg);
        }
        cmd.arg(temp_file);

        // Inherit stdin/stdout/stderr for interactive editors
        cmd.stdin(std::process::Stdio::inherit());
        cmd.stdout(std::process::Stdio::inherit());
        cmd.stderr(std::process::Stdio::inherit());

        // Spawn editor process
        let mut child = cmd.spawn().context("Failed to launch editor")?;

        // Wait for editor to complete
        let status = child.wait().context("Failed to wait for editor")?;

        if !status.success() {
            anyhow::bail!("Editor exited with non-zero status: {:?}", status);
        }

        Ok(())
    }

    /// Read temp file content
    fn read_temp_file(&self, temp_file: &Path) -> Result<String> {
        std::fs::read_to_string(temp_file).context("Failed to read temp file")
    }

    /// Get file modification time
    fn get_mtime(&self, temp_file: &Path) -> Result<std::time::SystemTime> {
        let metadata = std::fs::metadata(temp_file).context("Failed to get file metadata")?;

        metadata
            .modified()
            .context("Failed to get modification time")
    }

    /// Cleanup temp files
    pub fn cleanup(&self) -> Result<()> {
        if self.temp_dir.exists() {
            std::fs::remove_dir_all(&self.temp_dir).context("Failed to cleanup temp directory")?;
        }
        Ok(())
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        // Best effort cleanup
        let _ = self.cleanup();
    }
}

fn sanitize_file_hint(hint: &str) -> String {
    let mut sanitized: String = hint
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();

    sanitized.truncate(32);

    if sanitized.trim_matches('_').is_empty() {
        "note".to_string()
    } else {
        sanitized
    }
}

#[cfg(unix)]
fn secure_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .context("Failed to secure temp directory permissions")?;
    Ok(())
}

#[cfg(not(unix))]
fn secure_directory_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn secure_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .context("Failed to secure temp file permissions")?;
    Ok(())
}

#[cfg(not(unix))]
fn secure_file_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_config_default() {
        let config = EditorConfig::default();
        assert!(!config.command.is_empty());
        assert!(config.args.is_empty());
    }

    #[test]
    fn test_editor_config_new() {
        let config = EditorConfig::new("vim".to_string());
        assert_eq!(config.command, "vim");
    }

    #[test]
    fn test_editor_config_with_args() {
        let config = EditorConfig::new("code".to_string()).with_args(vec!["--wait".to_string()]);
        assert_eq!(config.command, "code");
        assert_eq!(config.args, vec!["--wait".to_string()]);
    }

    #[test]
    fn test_editor_temp_dir_is_under_home() {
        let editor = Editor::with_config(EditorConfig::new("true".to_string())).unwrap();
        let home = dirs::home_dir().unwrap();

        assert!(editor.temp_dir.starts_with(home));
    }
}
