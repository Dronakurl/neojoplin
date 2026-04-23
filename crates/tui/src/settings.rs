// Settings management for TUI

use anyhow::Result;
use joplin_sync::E2eeService;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Settings menu tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Sync,
    Encryption,
}

/// Active field in the encryption password prompt
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EncryptionField {
    #[default]
    Password,
    Confirm,
}

/// Sync target types (matching Joplin's target IDs)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncTargetType {
    None = 0,         // No sync
    Memory = 1,       // Memory sync (testing)
    FileSystem = 2,   // Local filesystem
    OneDrive = 3,     // Microsoft OneDrive
    Nextcloud = 5,    // Nextcloud
    WebDAV = 6,       // WebDAV
    Dropbox = 7,      // Dropbox
    AmazonS3 = 8,     // Amazon S3
    JoplinServer = 9, // Joplin Server
}

/// WebDAV sync target configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncTarget {
    pub id: String,
    pub name: String,
    pub target_type: SyncTargetType,
    pub url: String,
    pub username: String,
    pub password: String,
    pub remote_path: String,
    pub ignore_tls_errors: bool,
}

/// Form field for sync target input (URL is the full WebDAV URL including path)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormField {
    Name,
    Url,
    Username,
    Password,
}

/// Connection test result
#[derive(Debug, Clone)]
pub enum ConnectionResult {
    Success(String),
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSyncTargets {
    current_target_id: Option<String>,
    targets: Vec<SyncTarget>,
}

/// Sync settings state
#[derive(Debug, Clone, Default)]
pub struct SyncSettings {
    pub targets: Vec<SyncTarget>,
    pub current_target_index: Option<usize>,
    pub show_add_form: bool,
    pub show_edit_form: bool,
    pub editing_target_index: Option<usize>,

    // Form input state
    pub active_field: Option<FormField>,
    pub name_input: String,
    pub url_input: String,
    pub username_input: String,
    pub password_input: String,

    // Form validation and feedback
    pub form_error: Option<String>,
    pub testing_connection: bool,
    pub connection_result: Option<ConnectionResult>,

    // Delete confirmation
    pub confirm_delete: bool,
}

impl SyncSettings {
    /// Clear all form inputs
    pub fn clear_form(&mut self) {
        self.name_input.clear();
        self.url_input.clear();
        self.username_input.clear();
        self.password_input.clear();
        self.form_error = None;
        self.connection_result = None;
        self.active_field = None;
        self.confirm_delete = false;
    }

    /// Add character to name input
    pub fn add_name_char(&mut self, c: char) {
        self.name_input.push(c);
        self.form_error = None;
    }

    /// Add character to URL input
    pub fn add_url_char(&mut self, c: char) {
        self.url_input.push(c);
        self.form_error = None;
    }

    /// Add character to username input
    pub fn add_username_char(&mut self, c: char) {
        self.username_input.push(c);
        self.form_error = None;
    }

    /// Add character to password input
    pub fn add_password_char(&mut self, c: char) {
        self.password_input.push(c);
        self.form_error = None;
    }

    /// Remove last character from name input
    pub fn remove_name_char(&mut self) {
        self.name_input.pop();
    }

    /// Remove last character from URL input
    pub fn remove_url_char(&mut self) {
        self.url_input.pop();
    }

    /// Remove last character from username input
    pub fn remove_username_char(&mut self) {
        self.username_input.pop();
    }

    /// Remove last character from password input
    pub fn remove_password_char(&mut self) {
        self.password_input.pop();
    }

    /// Cycle to next form field
    pub fn cycle_field_forward(&mut self) {
        self.active_field = match self.active_field {
            Some(FormField::Name) => Some(FormField::Url),
            Some(FormField::Url) => Some(FormField::Username),
            Some(FormField::Username) => Some(FormField::Password),
            Some(FormField::Password) => Some(FormField::Name),
            None => Some(FormField::Name),
        };
    }

    /// Cycle to previous form field
    pub fn cycle_field_backward(&mut self) {
        self.active_field = match self.active_field {
            Some(FormField::Name) => Some(FormField::Password),
            Some(FormField::Url) => Some(FormField::Name),
            Some(FormField::Username) => Some(FormField::Url),
            Some(FormField::Password) => Some(FormField::Username),
            None => Some(FormField::Name),
        };
    }

    /// Load target data into form for editing
    pub fn load_target_to_form(&mut self, index: usize) {
        if let Some(target) = self.targets.get(index) {
            self.name_input = target.name.clone();
            self.url_input = target.url.clone();
            self.username_input = target.username.clone();
            self.password_input = target.password.clone();
            self.active_field = Some(FormField::Name);
        }
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
    /// Which field is active in the password prompt form
    pub active_field: EncryptionField,
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
            active_field: EncryptionField::Password,
        }
    }
}

/// Application settings
#[derive(Debug, Clone)]
pub struct Settings {
    pub current_tab: SettingsTab,
    pub sync: SyncSettings,
    pub encryption: EncryptionSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            current_tab: SettingsTab::Sync,
            sync: SyncSettings::default(),
            encryption: EncryptionSettings::default(),
        }
    }
}

impl Settings {
    /// Create new settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Cycle to next settings tab
    pub fn cycle_tab_forward(&mut self) {
        self.current_tab = match self.current_tab {
            SettingsTab::Sync => SettingsTab::Encryption,
            SettingsTab::Encryption => SettingsTab::Sync,
        };
    }

    /// Cycle to previous settings tab
    pub fn cycle_tab_backward(&mut self) {
        self.current_tab = match self.current_tab {
            SettingsTab::Sync => SettingsTab::Encryption,
            SettingsTab::Encryption => SettingsTab::Sync,
        };
    }

    /// Load all settings from disk
    pub async fn load_all_settings(&mut self, data_dir: &Path) -> Result<()> {
        self.load_encryption_settings(data_dir).await?;
        self.load_sync_settings(data_dir).await?;
        Ok(())
    }

    /// Load sync settings from Joplin-compatible format
    pub async fn load_sync_settings(&mut self, data_dir: &Path) -> Result<()> {
        self.sync.targets.clear();
        self.sync.current_target_index = None;

        let targets_path = sync_targets_path(data_dir);
        if targets_path.exists() {
            let content = tokio::fs::read_to_string(&targets_path).await?;
            let stored: StoredSyncTargets = serde_json::from_str(&content)?;
            self.sync.targets = stored.targets;
            self.sync.current_target_index = stored.current_target_id.and_then(|target_id| {
                self.sync
                    .targets
                    .iter()
                    .position(|target| target.id == target_id)
            });

            if self.sync.current_target_index.is_none() && !self.sync.targets.is_empty() {
                self.sync.current_target_index = Some(0);
            }

            return Ok(());
        }

        let config_path = data_dir.join("settings.json");

        if !config_path.exists() {
            // Try to load from old sync-config.json format
            return self.migrate_old_sync_config(data_dir).await;
        }

        let content = tokio::fs::read_to_string(&config_path).await?;
        let config: serde_json::Value = serde_json::from_str(&content)?;

        // Parse active target ID
        let active_id = config
            .get("sync.target")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        // Load WebDAV target (ID 6)
        if active_id == 6 {
            let url = config
                .get("sync.6.path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let username = config
                .get("sync.6.username")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let password = config
                .get("sync.6.password")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let (_, remote_path) = split_sync_url(&url);
            let target = SyncTarget {
                id: "webdav-default".to_string(),
                name: "WebDAV".to_string(),
                target_type: SyncTargetType::WebDAV,
                url,
                username,
                password,
                remote_path,
                ignore_tls_errors: false,
            };

            self.sync.targets.push(target);
            self.sync.current_target_index = Some(0);
        } else if self.sync.targets.is_empty() {
            // No targets configured, set to None
            self.sync.current_target_index = None;
        }

        Ok(())
    }

    /// Save sync settings to Joplin-compatible format
    pub async fn save_sync_settings(&self, data_dir: &Path) -> Result<()> {
        let stored = StoredSyncTargets {
            current_target_id: self
                .sync
                .current_target_index
                .and_then(|idx| self.sync.targets.get(idx))
                .map(|target| target.id.clone()),
            targets: self.sync.targets.clone(),
        };
        tokio::fs::write(
            sync_targets_path(data_dir),
            serde_json::to_string_pretty(&stored)?,
        )
        .await?;

        let config_path = data_dir.join("settings.json");

        let target_id: u32 = if self.sync.current_target_index.is_some() {
            6
        } else {
            0
        };

        let mut config = serde_json::json!({
            "$schema": "https://joplinapp.org/schema/settings.json",
            "sync.target": target_id,
        });

        // Save current WebDAV target
        if let Some(idx) = self.sync.current_target_index {
            if let Some(target) = self.sync.targets.get(idx) {
                if target.target_type == SyncTargetType::WebDAV {
                    config["sync.6.path"] = serde_json::json!(target.url);
                    config["sync.6.username"] = serde_json::json!(target.username);
                    config["sync.6.password"] = serde_json::json!(target.password);
                }
            }
        }

        let content = serde_json::to_string_pretty(&config)?;
        tokio::fs::write(&config_path, content).await?;

        Ok(())
    }

    /// Migrate old sync-config.json to new format
    async fn migrate_old_sync_config(&mut self, data_dir: &Path) -> Result<()> {
        let old_path = data_dir.join("sync-config.json");

        if old_path.exists() {
            let content = tokio::fs::read_to_string(&old_path).await?;
            if let Ok(old_config) = serde_json::from_str::<serde_json::Value>(&content) {
                let url = old_config.get("url").and_then(|v| v.as_str()).unwrap_or("");

                let (_, remote_path) = split_sync_url(url);
                let target = SyncTarget {
                    id: "migrated".to_string(),
                    name: "Migrated WebDAV".to_string(),
                    target_type: SyncTargetType::WebDAV,
                    url: url.to_string(),
                    username: String::new(),
                    password: String::new(),
                    remote_path,
                    ignore_tls_errors: false,
                };

                self.sync.targets.push(target);
                self.sync.current_target_index = Some(0);

                // Save in new format
                self.save_sync_settings(data_dir).await?;
            }
        }

        Ok(())
    }

    /// Load encryption settings from disk
    pub async fn load_encryption_settings(&mut self, data_dir: &Path) -> Result<()> {
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
                if entry.path().extension().is_some_and(|e| e == "json") {
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
            self.encryption.status_message = format!(
                "Disabled ({} keys available)",
                self.encryption.master_key_count
            );
        }
    }

    /// Enable encryption with new master key
    pub async fn enable_encryption(&mut self, password: &str, data_dir: &Path) -> Result<()> {
        if password.is_empty() {
            self.encryption.password_error = Some("Password cannot be empty".to_string());
            return Ok(());
        }

        if password != self.encryption.confirm_password_input {
            self.encryption.password_error = Some("Passwords do not match".to_string());
            return Ok(());
        }

        // Create E2EE service and generate master key
        let mut e2ee = E2eeService::new();
        e2ee.set_master_password(password.to_string());

        let (key_id, master_key) = e2ee.generate_master_key(password)?;

        // Save master key to file
        let keys_dir = data_dir.join("keys");
        tokio::fs::create_dir_all(&keys_dir).await?;
        let key_path = keys_dir.join(format!("{}.json", key_id));
        let master_key_json = serde_json::to_string_pretty(&master_key)?;
        tokio::fs::write(&key_path, master_key_json).await?;

        // Save active key ID and master password to config
        let config_path = data_dir.join("encryption.json");
        let config = serde_json::json!({
            "enabled": true,
            "active_master_key_id": key_id,
            "master_password": password
        });
        tokio::fs::write(&config_path, serde_json::to_string_pretty(&config)?).await?;

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
    pub async fn disable_encryption(&mut self, data_dir: &Path) -> Result<()> {
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
        self.encryption.active_field = EncryptionField::Password;
        self.clear_passwords();
    }

    /// Hide password prompts
    pub fn hide_password_prompts(&mut self) {
        self.encryption.show_password_prompt = false;
        self.encryption.show_new_key_prompt = false;
        self.encryption.active_field = EncryptionField::Password;
        self.clear_passwords();
    }
}

fn sync_targets_path(data_dir: &Path) -> PathBuf {
    data_dir.join("sync-targets.json")
}

fn split_sync_url(full_url: &str) -> (String, String) {
    let trimmed = full_url.trim_end_matches('/');
    let scheme_end = trimmed.find("://").map(|i| i + 3).unwrap_or(0);
    if let Some(slash_pos) = trimmed[scheme_end..].rfind('/') {
        let abs_pos = scheme_end + slash_pos;
        let base = &trimmed[..abs_pos];
        let path = &trimmed[abs_pos..];
        if !path.is_empty() && path != "/" {
            return (base.to_string(), path.to_string());
        }
    }

    (trimmed.to_string(), "/neojoplin".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_default() {
        let settings = Settings::new();
        assert_eq!(settings.current_tab, SettingsTab::Sync);
        assert!(!settings.encryption.enabled);
    }

    #[test]
    fn test_settings_tabs() {
        let mut settings = Settings::new();
        assert_eq!(settings.current_tab, SettingsTab::Sync);
        settings.cycle_tab_forward();
        assert_eq!(settings.current_tab, SettingsTab::Encryption);
        settings.cycle_tab_forward();
        assert_eq!(settings.current_tab, SettingsTab::Sync);
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
