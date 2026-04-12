// Settings management for TUI

use neojoplin_e2ee::MasterKey;
use anyhow::Result;
use std::path::PathBuf;

/// Settings menu tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Encryption,
    About,
}

impl Default for SettingsTab {
    fn default() -> Self {
        Self::General
    }
}

/// Encryption settings state
#[derive(Debug, Clone)]
pub struct EncryptionSettings {
    pub enabled: bool,
    pub active_master_key_id: Option<String>,
    pub master_key_count: usize,
    pub status_message: String,
    pub show_password_prompt: bool,
    pub show_new_key_prompt: bool,
    pub password_input: String,
    pub confirm_password_input: String,
    pub password_error: Option<String>,
    pub password_success: bool,
}

impl Default for EncryptionSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            active_master_key_id: None,
            master_key_count: 0,
            status_message: "Encryption not configured".to_string(),
            show_password_prompt: false,
            show_new_key_prompt: false,
            password_input: String::new(),
            confirm_password_input: String::new(),
            password_error: None,
            password_success: false,
        }
    }
}

/// Application settings
#[derive(Debug, Clone)]
pub struct Settings {
    pub current_tab: SettingsTab,
    pub encryption: EncryptionSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            current_tab: SettingsTab::General,
            encryption: EncryptionSettings::default(),
        }
    }
}

impl Settings {
    /// Create new settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Load encryption settings from disk
    pub async fn load_encryption_settings(&mut self, data_dir: &PathBuf) -> Result<()> {
        let config_path = data_dir.join("encryption.json");
        let keys_dir = data_dir.join("keys");

        if config_path.exists() {
            let config_content = tokio::fs::read_to_string(&config_path).await?;
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&config_content) {
                self.encryption.enabled = config["enabled"].as_bool().unwrap_or(false);
                self.encryption.active_master_key_id = config["active_master_key_id"]
                    .as_str()
                    .map(|s| s.to_string());
            }
        }

        // Count master keys
        if keys_dir.exists() {
            let mut entries = tokio::fs::read_dir(&keys_dir).await?;
            let mut count = 0;
            while let Some(entry) = entries.next_entry().await? {
                if entry.path().extension().map_or(false, |e| e == "json") {
                    count += 1;
                }
            }
            self.encryption.master_key_count = count;
        }

        self.update_encryption_status();
        Ok(())
    }

    /// Update encryption status message
    fn update_encryption_status(&mut self) {
        if self.encryption.enabled {
            if let Some(ref key_id) = self.encryption.active_master_key_id {
                self.encryption.status_message = format!("Enabled (Key: {})", &key_id[..8]);
            } else {
                self.encryption.status_message = "Enabled (No active key)".to_string();
            }
        } else {
            self.encryption.status_message = format!("Disabled ({} keys available)", self.encryption.master_key_count);
        }
    }

    /// Enable encryption with new master key
    pub async fn enable_encryption(&mut self, password: &str, data_dir: &PathBuf) -> Result<()> {
        if password.len() < 8 {
            self.encryption.password_error = Some("Password must be at least 8 characters".to_string());
            return Ok(());
        }

        if password != self.encryption.confirm_password_input {
            self.encryption.password_error = Some("Passwords do not match".to_string());
            return Ok(());
        }

        // Create master key
        let master_key = MasterKey::new();
        let key_id = master_key.id.clone();

        // Encrypt master key with password
        let encrypted_master_key = master_key.encrypt_with_password(password)?;

        // Save to file
        let keys_dir = data_dir.join("keys");
        tokio::fs::create_dir_all(&keys_dir).await?;
        let key_path = keys_dir.join(format!("{}.json", key_id));
        tokio::fs::write(&key_path, encrypted_master_key).await?;

        // Save active key ID to config
        let config_path = data_dir.join("encryption.json");
        let config = serde_json::json!({
            "enabled": true,
            "active_master_key_id": key_id
        });
        tokio::fs::write(&config_path, config.to_string()).await?;

        // Update state
        self.encryption.enabled = true;
        self.encryption.active_master_key_id = Some(key_id.clone());
        self.encryption.master_key_count += 1;
        self.encryption.show_password_prompt = false;
        self.encryption.show_new_key_prompt = false;
        self.encryption.password_input.clear();
        self.encryption.confirm_password_input.clear();
        self.encryption.password_error = None;
        self.encryption.password_success = true;

        self.update_encryption_status();
        Ok(())
    }

    /// Disable encryption
    pub async fn disable_encryption(&mut self, data_dir: &PathBuf) -> Result<()> {
        let config_path = data_dir.join("encryption.json");

        if config_path.exists() {
            tokio::fs::remove_file(&config_path).await.ok();
        }

        self.encryption.enabled = false;
        self.encryption.active_master_key_id = None;
        self.encryption.password_success = true;

        self.update_encryption_status();
        Ok(())
    }

    /// Add character to password input
    pub fn add_password_char(&mut self, c: char) {
        self.encryption.password_input.push(c);
        self.encryption.password_error = None;
        self.encryption.password_success = false;
    }

    /// Add character to confirm password input
    pub fn add_confirm_password_char(&mut self, c: char) {
        self.encryption.confirm_password_input.push(c);
        self.encryption.password_error = None;
        self.encryption.password_success = false;
    }

    /// Remove last character from password input
    pub fn remove_password_char(&mut self) {
        self.encryption.password_input.pop();
    }

    /// Remove last character from confirm password input
    pub fn remove_confirm_password_char(&mut self) {
        self.encryption.confirm_password_input.pop();
    }

    /// Clear password inputs
    pub fn clear_passwords(&mut self) {
        self.encryption.password_input.clear();
        self.encryption.confirm_password_input.clear();
        self.encryption.password_error = None;
        self.encryption.password_success = false;
    }

    /// Show new key password prompt
    pub fn show_new_key_prompt(&mut self) {
        self.encryption.show_new_key_prompt = true;
        self.encryption.show_password_prompt = false;
        self.clear_passwords();
    }

    /// Hide password prompts
    pub fn hide_password_prompts(&mut self) {
        self.encryption.show_password_prompt = false;
        self.encryption.show_new_key_prompt = false;
        self.clear_passwords();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_default() {
        let settings = Settings::new();
        assert_eq!(settings.current_tab, SettingsTab::General);
        assert_eq!(settings.encryption.enabled, false);
    }

    #[test]
    fn test_settings_tabs() {
        let settings = Settings::new();
        assert_eq!(settings.current_tab, SettingsTab::General);
    }

    #[test]
    fn test_encryption_password_input() {
        let mut settings = Settings::new();
        settings.add_password_char('a');
        settings.add_password_char('b');
        settings.add_password_char('c');
        assert_eq!(settings.encryption.password_input, "abc");

        settings.remove_password_char();
        assert_eq!(settings.encryption.password_input, "ab");

        settings.clear_passwords();
        assert_eq!(settings.encryption.password_input, "");
    }
}
