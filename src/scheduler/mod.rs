// File: src/scheduler/mod.rs

pub mod operations;

pub use operations::MaintenanceScheduler;

use anyhow::Result;
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

pub fn validate_cron_expression(expression: &str) -> Result<()> {
    // Basic validation for cron expression format
    let parts: Vec<&str> = expression.split_whitespace().collect();

    if parts.len() != 6 {
        return Err(anyhow::anyhow!(
            "Cron expression must have exactly 6 parts (second minute hour day month weekday), got: {}",
            expression
        ));
    }

    // Basic range validation for each field
    for (i, part) in parts.iter().enumerate() {
        if part == &"*" {
            continue; // Wildcard is always valid
        }

        // Simple numeric validation - in a real implementation you'd be more thorough
        if !part.chars().all(|c| c.is_ascii_digit() || c == '-' || c == ',' || c == '/') {
            return Err(anyhow::anyhow!(
                "Invalid character in cron field {}: {}",
                i + 1,
                part
            ));
        }
    }

    Ok(())
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

    OperationsSummary {
        total_operations: total,
        enabled_operations: enabled,
        failed_operations: failed,
        overdue_operations: overdue,
        next_scheduled_run: next_run,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsSummary {
    pub total_operations: usize,
    pub enabled_operations: usize,
    pub failed_operations: usize,
    pub overdue_operations: usize,
    pub next_scheduled_run: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_cron_expression() {
        // Valid expressions
        assert!(validate_cron_expression("0 0 12 * * 2").is_ok());
        assert!(validate_cron_expression("0 30 14 * * 1-5").is_ok());

        // Invalid expressions
        assert!(validate_cron_expression("invalid").is_err());
        assert!(validate_cron_expression("0 0 12 *").is_err()); // Too few parts
    }

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
}
