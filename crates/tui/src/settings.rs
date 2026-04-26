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
    AutoSync,
    Status,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSyncStatus {
    last_sync_time: Option<i64>,
    last_sync_success: bool,
    last_sync_error: Option<String>,
    last_sync_target_name: Option<String>,
    last_sync_encryption_enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MasterKeySummary {
    pub id: String,
    pub created_time: i64,
    pub updated_time: i64,
    pub source_application: String,
    pub encryption_method: Option<i32>,
    pub enabled: Option<bool>,
    pub has_been_used: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct StoredMasterKeyFile {
    id: String,
    created_time: i64,
    updated_time: i64,
    #[serde(default)]
    source_application: String,
    #[serde(default)]
    encryption_method: Option<i32>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default, rename = "hasBeenUsed")]
    has_been_used: Option<bool>,
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

pub const AUTO_SYNC_INTERVAL_OPTIONS: &[u64] = &[0, 300, 600, 1800, 3600, 43200, 86400];

#[derive(Debug, Clone)]
pub struct AutoSyncSettings {
    pub interval_seconds: u64,
    pub selected_option_index: usize,
}

impl Default for AutoSyncSettings {
    fn default() -> Self {
        Self {
            interval_seconds: 300,
            selected_option_index: 1,
        }
    }
}

impl AutoSyncSettings {
    pub fn option_index(&self) -> usize {
        AUTO_SYNC_INTERVAL_OPTIONS
            .iter()
            .position(|value| *value == self.interval_seconds)
            .unwrap_or(0)
    }

    pub fn move_selection(&mut self, forward: bool) {
        let current_index = self
            .selected_option_index
            .min(AUTO_SYNC_INTERVAL_OPTIONS.len() - 1);
        self.selected_option_index = if forward {
            (current_index + 1) % AUTO_SYNC_INTERVAL_OPTIONS.len()
        } else if current_index == 0 {
            AUTO_SYNC_INTERVAL_OPTIONS.len() - 1
        } else {
            current_index - 1
        };
    }

    pub fn sync_selection_to_value(&mut self) {
        self.selected_option_index = self.option_index();
    }

    pub fn selected_interval_seconds(&self) -> u64 {
        AUTO_SYNC_INTERVAL_OPTIONS[self
            .selected_option_index
            .min(AUTO_SYNC_INTERVAL_OPTIONS.len() - 1)]
    }

    pub fn apply_selected(&mut self) -> bool {
        let selected = self.selected_interval_seconds();
        let changed = self.interval_seconds != selected;
        self.interval_seconds = selected;
        changed
    }
}

#[derive(Debug, Clone)]
pub struct SyncStatusSettings {
    pub last_sync_time: Option<i64>,
    pub last_sync_success: bool,
    pub last_sync_error: Option<String>,
    pub last_sync_target_name: Option<String>,
    pub last_sync_encryption_enabled: bool,
    pub current_conflict_count: usize,
    pub current_encryption_enabled: bool,
    pub current_auto_sync_interval_seconds: u64,
    pub next_auto_sync_in_seconds: Option<u64>,
}

impl Default for SyncStatusSettings {
    fn default() -> Self {
        Self {
            last_sync_time: None,
            last_sync_success: false,
            last_sync_error: None,
            last_sync_target_name: None,
            last_sync_encryption_enabled: false,
            current_conflict_count: 0,
            current_encryption_enabled: false,
            current_auto_sync_interval_seconds: 0,
            next_auto_sync_in_seconds: None,
        }
    }
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
    pub master_keys: Vec<MasterKeySummary>,
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
            master_keys: Vec::new(),
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
    pub auto_sync: AutoSyncSettings,
    pub status: SyncStatusSettings,
    pub encryption: EncryptionSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            current_tab: SettingsTab::Sync,
            sync: SyncSettings::default(),
            auto_sync: AutoSyncSettings::default(),
            status: SyncStatusSettings::default(),
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
            SettingsTab::Sync => SettingsTab::AutoSync,
            SettingsTab::AutoSync => SettingsTab::Status,
            SettingsTab::Status => SettingsTab::Encryption,
            SettingsTab::Encryption => SettingsTab::Sync,
        };
    }

    /// Cycle to previous settings tab
    pub fn cycle_tab_backward(&mut self) {
        self.current_tab = match self.current_tab {
            SettingsTab::Sync => SettingsTab::Encryption,
            SettingsTab::AutoSync => SettingsTab::Sync,
            SettingsTab::Status => SettingsTab::AutoSync,
            SettingsTab::Encryption => SettingsTab::Status,
        };
    }

    /// Load all settings from disk
    pub async fn load_all_settings(&mut self, data_dir: &Path) -> Result<()> {
        self.load_encryption_settings(data_dir).await?;
        self.load_sync_settings(data_dir).await?;
        self.load_sync_status(data_dir).await?;
        Ok(())
    }

    /// Load sync settings from Joplin-compatible format
    pub async fn load_sync_settings(&mut self, data_dir: &Path) -> Result<()> {
        self.sync.targets.clear();
        self.sync.current_target_index = None;
        self.auto_sync = AutoSyncSettings::default();

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
        }

        let config_path = data_dir.join("settings.json");

        if !config_path.exists() && self.sync.targets.is_empty() {
            // Try to load from old sync-config.json format
            return self.migrate_old_sync_config(data_dir).await;
        }

        let config = if config_path.exists() {
            let content = tokio::fs::read_to_string(&config_path).await?;
            serde_json::from_str::<serde_json::Value>(&content)?
        } else {
            serde_json::json!({})
        };

        self.auto_sync.interval_seconds = config
            .get("sync.interval")
            .and_then(|value| value.as_u64())
            .unwrap_or(self.auto_sync.interval_seconds);
        self.auto_sync.sync_selection_to_value();

        // Parse active target ID
        let active_id = config
            .get("sync.target")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        // Load WebDAV target (ID 6)
        if active_id == 6 && self.sync.targets.is_empty() {
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

        let mut config = if config_path.exists() {
            let content = tokio::fs::read_to_string(&config_path).await?;
            serde_json::from_str::<serde_json::Value>(&content)?
        } else {
            serde_json::json!({
                "$schema": "https://joplinapp.org/schema/settings.json",
            })
        };
        config["sync.target"] = serde_json::json!(target_id);
        config["sync.interval"] = serde_json::json!(self.auto_sync.interval_seconds);

        // Save current WebDAV target
        if let Some(idx) = self.sync.current_target_index {
            if let Some(target) = self.sync.targets.get(idx) {
                if target.target_type == SyncTargetType::WebDAV {
                    config["sync.6.path"] = serde_json::json!(target.url);
                    config["sync.6.username"] = serde_json::json!(target.username);
                    config["sync.6.password"] = serde_json::json!(target.password);
                }
            }
        } else {
            config["sync.6.path"] = serde_json::json!("");
            config["sync.6.username"] = serde_json::json!("");
            config["sync.6.password"] = serde_json::json!("");
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

        self.encryption.master_keys.clear();

        if keys_dir.exists() {
            let mut entries = tokio::fs::read_dir(&keys_dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                if entry.path().extension().is_some_and(|e| e == "json") {
                    let content = tokio::fs::read_to_string(entry.path()).await?;
                    let key: StoredMasterKeyFile = serde_json::from_str(&content)?;
                    self.encryption.master_keys.push(MasterKeySummary {
                        id: key.id,
                        created_time: key.created_time,
                        updated_time: key.updated_time,
                        source_application: key.source_application,
                        encryption_method: key.encryption_method,
                        enabled: key.enabled,
                        has_been_used: key.has_been_used,
                    });
                }
            }
            self.encryption
                .master_keys
                .sort_by(|a, b| a.created_time.cmp(&b.created_time));
        }
        self.encryption.master_key_count = self.encryption.master_keys.len();

        self.update_encryption_status();
        self.status.current_encryption_enabled = self.encryption.enabled;
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

        let reusable_key = find_reusable_master_key(data_dir, password).await?;

        // Create E2EE service and reuse or generate a master key
        let mut e2ee = E2eeService::new();
        e2ee.set_master_password(password.to_string());

        let (key_id, master_key, reused_existing_key) =
            if let Some((key_id, master_key)) = reusable_key {
                (key_id, master_key, true)
            } else {
                let (key_id, master_key) = e2ee.generate_master_key(password)?;
                (key_id, master_key, false)
            };

        // Save master key to file
        let keys_dir = data_dir.join("keys");
        tokio::fs::create_dir_all(&keys_dir).await?;
        let key_path = keys_dir.join(format!("{}.json", key_id));
        if !reused_existing_key || !key_path.exists() {
            let master_key_json = serde_json::to_string_pretty(&master_key)?;
            tokio::fs::write(&key_path, master_key_json).await?;
        }

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
        if !reused_existing_key || self.encryption.master_key_count == 0 {
            self.encryption.master_key_count += 1;
        }
        self.encryption.show_password_prompt = false;
        self.encryption.show_new_key_prompt = false;
        self.encryption.password_input.clear();
        self.encryption.confirm_password_input.clear();
        self.encryption.password_error = None;
        self.encryption.password_success = true;

        self.update_encryption_status();
        self.status.current_encryption_enabled = self.encryption.enabled;
        Ok(())
    }

    pub async fn load_sync_status(&mut self, data_dir: &Path) -> Result<()> {
        let status_path = sync_status_path(data_dir);
        if !status_path.exists() {
            return Ok(());
        }

        let content = tokio::fs::read_to_string(&status_path).await?;
        let stored: StoredSyncStatus = serde_json::from_str(&content)?;
        self.status.last_sync_time = stored.last_sync_time;
        self.status.last_sync_success = stored.last_sync_success;
        self.status.last_sync_error = stored.last_sync_error;
        self.status.last_sync_target_name = stored.last_sync_target_name;
        self.status.last_sync_encryption_enabled = stored.last_sync_encryption_enabled;
        Ok(())
    }

    pub async fn save_sync_status(&self, data_dir: &Path) -> Result<()> {
        let stored = StoredSyncStatus {
            last_sync_time: self.status.last_sync_time,
            last_sync_success: self.status.last_sync_success,
            last_sync_error: self.status.last_sync_error.clone(),
            last_sync_target_name: self.status.last_sync_target_name.clone(),
            last_sync_encryption_enabled: self.status.last_sync_encryption_enabled,
        };
        tokio::fs::write(
            sync_status_path(data_dir),
            serde_json::to_string_pretty(&stored)?,
        )
        .await?;
        Ok(())
    }

    pub fn record_sync_result(
        &mut self,
        target_name: String,
        success: bool,
        error: Option<String>,
        encryption_enabled: bool,
    ) {
        self.status.last_sync_time = Some(joplin_domain::now_ms());
        self.status.last_sync_success = success;
        self.status.last_sync_error = error;
        self.status.last_sync_target_name = Some(target_name);
        self.status.last_sync_encryption_enabled = encryption_enabled;
        self.status.current_encryption_enabled = self.encryption.enabled;
    }

    pub fn update_runtime_status(&mut self, conflict_count: usize) {
        self.status.current_conflict_count = conflict_count;
        self.status.current_encryption_enabled = self.encryption.enabled;
        self.status.current_auto_sync_interval_seconds = self.auto_sync.interval_seconds;
    }

    pub fn set_next_auto_sync_in_seconds(&mut self, seconds: Option<u64>) {
        self.status.next_auto_sync_in_seconds = seconds;
    }

    /// Disable encryption
    pub async fn disable_encryption(&mut self, data_dir: &Path) -> Result<()> {
        let config_path = data_dir.join("encryption.json");

        let existing_config = if config_path.exists() {
            let content = tokio::fs::read_to_string(&config_path).await?;
            serde_json::from_str::<serde_json::Value>(&content)?
        } else {
            serde_json::json!({})
        };
        let mut config = existing_config;
        config["enabled"] = serde_json::json!(false);
        if config.get("active_master_key_id").is_none() {
            if let Some(key_id) = &self.encryption.active_master_key_id {
                config["active_master_key_id"] = serde_json::json!(key_id);
            }
        }
        tokio::fs::write(&config_path, serde_json::to_string_pretty(&config)?).await?;

        self.encryption.enabled = false;
        self.encryption.password_success = true;

        self.update_encryption_status();
        self.status.current_encryption_enabled = self.encryption.enabled;
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

async fn find_reusable_master_key(
    data_dir: &Path,
    password: &str,
) -> Result<Option<(String, joplin_sync::MasterKey)>> {
    let config_path = data_dir.join("encryption.json");
    let preferred_key_id = if config_path.exists() {
        let content = tokio::fs::read_to_string(&config_path).await?;
        serde_json::from_str::<serde_json::Value>(&content)
            .ok()
            .and_then(|config| {
                config
                    .get("active_master_key_id")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
            })
    } else {
        None
    };

    let keys_dir = data_dir.join("keys");
    if !keys_dir.exists() {
        return Ok(None);
    }

    let mut candidate_paths = Vec::new();
    if let Some(key_id) = preferred_key_id {
        let preferred_path = keys_dir.join(format!("{}.json", key_id));
        if preferred_path.exists() {
            candidate_paths.push(preferred_path);
        }
    }

    let mut entries = tokio::fs::read_dir(&keys_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") && !candidate_paths.contains(&path) {
            candidate_paths.push(path);
        }
    }

    for path in candidate_paths {
        let content = tokio::fs::read_to_string(&path).await?;
        let master_key: joplin_sync::MasterKey = serde_json::from_str(&content)?;
        let mut e2ee = E2eeService::new();
        e2ee.set_master_password(password.to_string());
        if e2ee.load_master_key(&master_key).is_ok() {
            return Ok(Some((master_key.id.clone(), master_key)));
        }
    }

    Ok(None)
}

fn sync_targets_path(data_dir: &Path) -> PathBuf {
    data_dir.join("sync-targets.json")
}

fn sync_status_path(data_dir: &Path) -> PathBuf {
    data_dir.join("sync-status.json")
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
        assert_eq!(settings.current_tab, SettingsTab::AutoSync);
        settings.cycle_tab_forward();
        assert_eq!(settings.current_tab, SettingsTab::Status);
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
