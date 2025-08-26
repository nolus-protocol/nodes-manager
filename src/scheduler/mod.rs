// File: src/scheduler/mod.rs

pub mod operations;

pub use operations::MaintenanceScheduler;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledOperation {
    pub id: String,
    pub operation_type: OperationType,
    pub target_name: String,
    pub schedule: String,
    pub enabled: bool,
    pub next_run: Option<DateTime<Utc>>,
    pub last_run: Option<DateTime<Utc>>,
    pub last_result: Option<OperationResult>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OperationType {
    NodePruning,
    HermesRestart,
    SystemMaintenance,
    SnapshotCreation,  // NEW: Scheduled snapshot creation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub success: bool,
    pub message: String,
    pub duration_seconds: u64,
    pub executed_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct SchedulerConfig {
    pub max_concurrent_operations: usize,
    pub operation_timeout_minutes: u64,
    pub retry_failed_operations: bool,
    pub cleanup_completed_after_days: u32,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_operations: 5,
            operation_timeout_minutes: 60,
            retry_failed_operations: true,
            cleanup_completed_after_days: 30,
        }
    }
}

impl ScheduledOperation {
    pub fn new_pruning(target_name: String, schedule: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            operation_type: OperationType::NodePruning,
            target_name,
            schedule,
            enabled: true,
            next_run: None,
            last_run: None,
            last_result: None,
            created_at: Utc::now(),
        }
    }

    pub fn new_hermes_restart(target_name: String, schedule: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            operation_type: OperationType::HermesRestart,
            target_name,
            schedule,
            enabled: true,
            next_run: None,
            last_run: None,
            last_result: None,
            created_at: Utc::now(),
        }
    }

    // NEW: Scheduled snapshot creation
    pub fn new_snapshot_creation(target_name: String, schedule: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            operation_type: OperationType::SnapshotCreation,
            target_name,
            schedule,
            enabled: true,
            next_run: None,
            last_run: None,
            last_result: None,
            created_at: Utc::now(),
        }
    }

    pub fn update_result(&mut self, result: OperationResult) {
        self.last_run = Some(result.executed_at);
        self.last_result = Some(result);
    }

    pub fn is_overdue(&self) -> bool {
        if let Some(next_run) = self.next_run {
            return Utc::now() > next_run;
        }
        false
    }

    pub fn get_status(&self) -> OperationStatus {
        if !self.enabled {
            return OperationStatus::Disabled;
        }

        if let Some(last_result) = &self.last_result {
            if !last_result.success {
                return OperationStatus::Failed;
            }
        }

        if self.is_overdue() {
            OperationStatus::Overdue
        } else {
            OperationStatus::Scheduled
        }
    }
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

pub fn create_operation_summary(operations: &[ScheduledOperation]) -> OperationsSummary {
    let total = operations.len();
    let enabled = operations.iter().filter(|op| op.enabled).count();
    let failed = operations.iter().filter(|op| {
        matches!(op.get_status(), OperationStatus::Failed)
    }).count();
    let overdue = operations.iter().filter(|op| {
        matches!(op.get_status(), OperationStatus::Overdue)
    }).count();

    let next_run = operations
        .iter()
        .filter_map(|op| op.next_run)
        .min();

    // NEW: Count operation types
    let pruning_ops = operations.iter()
        .filter(|op| matches!(op.operation_type, OperationType::NodePruning))
        .count();
    let hermes_ops = operations.iter()
        .filter(|op| matches!(op.operation_type, OperationType::HermesRestart))
        .count();
    let snapshot_ops = operations.iter()
        .filter(|op| matches!(op.operation_type, OperationType::SnapshotCreation))
        .count();

    OperationsSummary {
        total_operations: total,
        enabled_operations: enabled,
        failed_operations: failed,
        overdue_operations: overdue,
        next_scheduled_run: next_run,
        pruning_operations: pruning_ops,
        hermes_operations: hermes_ops,
        snapshot_operations: snapshot_ops,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsSummary {
    pub total_operations: usize,
    pub enabled_operations: usize,
    pub failed_operations: usize,
    pub overdue_operations: usize,
    pub next_scheduled_run: Option<DateTime<Utc>>,
    pub pruning_operations: usize,
    pub hermes_operations: usize,
    pub snapshot_operations: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduled_operation_creation() {
        let op = ScheduledOperation::new_pruning(
            "test-node".to_string(),
            "0 0 12 * * 2".to_string()
        );

        assert_eq!(op.target_name, "test-node");
        assert_eq!(op.schedule, "0 0 12 * * 2");
        assert!(matches!(op.operation_type, OperationType::NodePruning));
        assert!(op.enabled);
    }

    #[test]
    fn test_snapshot_operation_creation() {
        let op = ScheduledOperation::new_snapshot_creation(
            "test-node".to_string(),
            "0 0 6 * * 0".to_string()
        );

        assert_eq!(op.target_name, "test-node");
        assert_eq!(op.schedule, "0 0 6 * * 0");
        assert!(matches!(op.operation_type, OperationType::SnapshotCreation));
        assert!(op.enabled);
    }
}
