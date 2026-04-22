// Sync-related types and utilities

use crate::error::*;
use serde::{Deserialize, Serialize};

/// Sync state for tracking sync progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub phase: SyncPhase,
    pub current_item: usize,
    pub total_items: usize,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub start_time: i64,
}

impl SyncState {
    pub fn new() -> Self {
        Self {
            phase: SyncPhase::Upload,
            current_item: 0,
            total_items: 0,
            errors: Vec::new(),
            warnings: Vec::new(),
            start_time: crate::domain::now_ms(),
        }
    }

    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    pub fn progress_percent(&self) -> f64 {
        if self.total_items == 0 {
            0.0
        } else {
            (self.current_item as f64 / self.total_items as f64) * 100.0
        }
    }
}

impl Default for SyncState {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a sync phase
#[derive(Debug, Clone)]
pub struct PhaseResult {
    pub successes: usize,
    pub failures: Vec<ItemError>,
    pub warnings: Vec<String>,
}

impl PhaseResult {
    pub fn new() -> Self {
        Self {
            successes: 0,
            failures: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn add_success(&mut self) {
        self.successes += 1;
    }

    pub fn add_failure(&mut self, item_error: ItemError) {
        self.failures.push(item_error);
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    pub fn total_processed(&self) -> usize {
        self.successes + self.failures.len()
    }
}

impl Default for PhaseResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Error for a specific item during sync
#[derive(Debug, Clone)]
pub struct ItemError {
    pub item_type: String,
    pub item_id: String,
    pub error: String,
}

impl ItemError {
    pub fn new(item_type: String, item_id: String, error: String) -> Self {
        Self {
            item_type,
            item_id,
            error,
        }
    }
}

/// Conflict resolution strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Keep local version (discard remote)
    KeepLocal,

    /// Keep remote version (discard local)
    KeepRemote,

    /// Create conflict copy (keep both)
    CreateConflictCopy,

    /// Manual resolution (prompt user)
    Manual,
}

/// Conflict information
#[derive(Debug, Clone)]
pub struct ConflictInfo {
    pub item_type: String,
    pub item_id: String,
    pub local_updated_time: i64,
    pub remote_updated_time: i64,
    pub local_title: String,
    pub remote_title: String,
}

impl ConflictInfo {
    pub fn new(
        item_type: String,
        item_id: String,
        local_updated_time: i64,
        remote_updated_time: i64,
        local_title: String,
        remote_title: String,
    ) -> Self {
        Self {
            item_type,
            item_id,
            local_updated_time,
            remote_updated_time,
            local_title,
            remote_title,
        }
    }

    /// Determine if this is actually a conflict (both sides modified since last sync)
    pub fn is_conflict(&self, last_sync_time: i64) -> bool {
        self.local_updated_time > last_sync_time && self.remote_updated_time > last_sync_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_state_progress() {
        let mut state = SyncState::new();
        state.total_items = 100;
        state.current_item = 50;

        assert_eq!(state.progress_percent(), 50.0);
    }

    #[test]
    fn test_sync_state_empty() {
        let state = SyncState::new();
        assert_eq!(state.progress_percent(), 0.0);
    }

    #[test]
    fn test_phase_result() {
        let mut result = PhaseResult::new();
        result.add_success();
        result.add_success();
        result.add_warning("Warning".to_string());

        assert_eq!(result.successes, 2);
        assert_eq!(result.total_processed(), 2);
        assert!(result.has_warnings());
        assert!(!result.has_failures());
    }

    #[test]
    fn test_conflict_detection() {
        let conflict = ConflictInfo::new(
            "note".to_string(),
            "note-123".to_string(),
            1000,
            2000,
            "Local Title".to_string(),
            "Remote Title".to_string(),
        );

        // Both modified since sync at 500 -> conflict
        assert!(conflict.is_conflict(500));

        // Only local modified since sync at 1500 -> no conflict
        assert!(!conflict.is_conflict(1500));
    }
}
