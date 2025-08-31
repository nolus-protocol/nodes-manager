// File: manager/src/scheduler/mod.rs

pub mod operations;

pub use operations::MaintenanceScheduler;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OperationType {
    NodePruning,
    HermesRestart,
    SystemMaintenance,
    SnapshotCreation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub success: bool,
    pub message: String,
    pub duration_seconds: u64,
    pub executed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationStatus {
    Scheduled,
    Running,
    Completed,
    Failed,
    Disabled,
    Overdue,
}
