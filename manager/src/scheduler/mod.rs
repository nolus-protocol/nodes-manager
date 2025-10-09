//! Cron-based scheduling system for automated maintenance operations
//!
//! This module provides automated scheduling for:
//! - Node pruning operations (reduce disk usage)
//! - Hermes relayer restarts (improve performance)
//! - Snapshot creation (backup and disaster recovery)
//!
//! # Features
//!
//! - **Cron-based scheduling**: Uses 6-field cron expressions (sec min hour day month dow)
//! - **Timezone-aware**: All schedules run in the timezone where Manager is deployed
//! - **Smart conflict prevention**: Skips operations if node is already in maintenance
//! - **Automatic retry**: Failed operations can be rescheduled
//! - **Database logging**: All operations logged for audit trail
//!
//! # Configuration
//!
//! Schedules are defined per-node in `config/{server}.toml`:
//!
//! ```toml
//! [[nodes]]
//! name = "osmosis-1"
//! pruning_enabled = true
//! pruning_schedule = "0 0 2 * * *"  # Daily at 2 AM
//! snapshots_enabled = true
//! snapshot_schedule = "0 0 3 * * 0"  # Weekly on Sunday at 3 AM
//! ```

pub mod operations;
pub use operations::MaintenanceScheduler;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// Operation types and results - may be used for future scheduler enhancements
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OperationType {
    NodePruning,
    HermesRestart,
    SystemMaintenance,
    SnapshotCreation,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub success: bool,
    pub message: String,
    pub duration_seconds: u64,
    pub executed_at: DateTime<Utc>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationStatus {
    Scheduled,
    Running,
    Completed,
    Failed,
    Disabled,
    Overdue,
}
