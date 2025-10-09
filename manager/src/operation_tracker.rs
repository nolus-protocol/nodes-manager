//! Operation tracking for long-running tasks
//!
//! This module tracks active operations to prevent conflicts and provide visibility.
//! Unlike maintenance tracking (which focuses on maintenance windows), this tracks
//! the actual execution state of operations.
//!
//! # Key Features
//!
//! - **Concurrent operation prevention**: Only one operation per target (node/service)
//! - **Operation state tracking**: Track what's running, when it started, who initiated it
//! - **Automatic cleanup**: Stuck operations cleaned after 24 hours
//! - **Status API**: Query active operations and their duration
//!
//! # Usage
//!
//! ```ignore
//! // Try to start operation (fails if target is busy)
//! tracker.try_start_operation("osmosis-1", "pruning", Some("user@example.com")).await?;
//!
//! // Perform operation...
//!
//! // Mark as finished
//! tracker.finish_operation("osmosis-1").await;
//! ```

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, instrument, warn};

#[derive(Debug, Clone, Serialize)]
pub struct ActiveOperation {
    pub operation_type: String,
    pub target_name: String,
    pub started_at: DateTime<Utc>,
    pub user_info: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationStatus {
    pub busy_nodes: HashMap<String, ActiveOperation>,
    pub total_active: usize,
}

pub struct SimpleOperationTracker {
    active_operations: Arc<RwLock<HashMap<String, ActiveOperation>>>, // target_name -> operation
}

impl SimpleOperationTracker {
    pub fn new() -> Self {
        Self {
            active_operations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Try to start an operation on a target (node/hermes)
    /// Returns error if target is already busy
    #[instrument(skip(self), fields(target = %target_name, operation = %operation_type))]
    pub async fn try_start_operation(
        &self,
        target_name: &str,
        operation_type: &str,
        user_info: Option<String>,
    ) -> Result<()> {
        let mut active = self.active_operations.write().await;

        if let Some(current_op) = active.get(target_name) {
            let duration = Utc::now().signed_duration_since(current_op.started_at);
            let duration_str = if duration.num_hours() > 0 {
                format!("{}h {}m", duration.num_hours(), duration.num_minutes() % 60)
            } else {
                format!("{}m", duration.num_minutes())
            };

            return Err(anyhow::anyhow!(
                "Target {} is currently busy with '{}' (started {} ago). Check the UI or wait for completion.",
                target_name, current_op.operation_type, duration_str
            ));
        }

        let operation = ActiveOperation {
            operation_type: operation_type.to_string(),
            target_name: target_name.to_string(),
            started_at: Utc::now(),
            user_info,
        };

        active.insert(target_name.to_string(), operation);
        info!("Started operation '{}' on {}", operation_type, target_name);
        Ok(())
    }

    /// Mark an operation as finished
    #[instrument(skip(self), fields(target = %target_name))]
    pub async fn finish_operation(&self, target_name: &str) {
        let mut active = self.active_operations.write().await;
        if let Some(op) = active.remove(target_name) {
            let duration = Utc::now().signed_duration_since(op.started_at);
            info!(
                "Finished operation '{}' on {} (took {}m)",
                op.operation_type,
                target_name,
                duration.num_minutes()
            );
        }
    }

    /// Force cancel an operation (for manual cleanup)
    pub async fn cancel_operation(&self, target_name: &str) -> Result<()> {
        let mut active = self.active_operations.write().await;
        if let Some(op) = active.remove(target_name) {
            warn!(
                "Cancelled operation '{}' on {} (was running for {}m)",
                op.operation_type,
                target_name,
                Utc::now()
                    .signed_duration_since(op.started_at)
                    .num_minutes()
            );
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "No active operation found on {}",
                target_name
            ))
        }
    }

    /// Get current operation status
    pub async fn get_operation_status(&self) -> OperationStatus {
        let active = self.active_operations.read().await;
        OperationStatus {
            busy_nodes: active.clone(),
            total_active: active.len(),
        }
    }

    /// Check if a specific target is busy
    pub async fn is_busy(&self, target_name: &str) -> bool {
        let active = self.active_operations.read().await;
        active.contains_key(target_name)
    }

    /// Get active operation for a specific target
    pub async fn get_active_operation(&self, target_name: &str) -> Option<ActiveOperation> {
        let active = self.active_operations.read().await;
        active.get(target_name).cloned()
    }

    /// Cleanup operations older than specified hours (for stuck operations)
    pub async fn cleanup_old_operations(&self, max_hours: i64) -> u32 {
        let mut active = self.active_operations.write().await;
        let cutoff = Utc::now() - chrono::Duration::hours(max_hours);
        let initial_count = active.len();

        active.retain(|target_name, operation| {
            let should_keep = operation.started_at > cutoff;
            if !should_keep {
                warn!(
                    "Cleaned up stuck operation '{}' on {} (was running for {}h)",
                    operation.operation_type,
                    target_name,
                    Utc::now()
                        .signed_duration_since(operation.started_at)
                        .num_hours()
                );
            }
            should_keep
        });

        let cleaned_count = initial_count - active.len();
        if cleaned_count > 0 {
            warn!(
                "Emergency cleanup: removed {} stuck operations older than {}h",
                cleaned_count, max_hours
            );
        }

        cleaned_count as u32
    }
}

impl Default for SimpleOperationTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SimpleOperationTracker {
    fn clone(&self) -> Self {
        Self {
            active_operations: self.active_operations.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_operation_tracking() {
        let tracker = SimpleOperationTracker::new();

        // Should be able to start operation
        assert!(tracker
            .try_start_operation("node-1", "restart", None)
            .await
            .is_ok());

        // Should be busy now
        assert!(tracker.is_busy("node-1").await);

        // Should not be able to start another operation
        assert!(tracker
            .try_start_operation("node-1", "snapshot", None)
            .await
            .is_err());

        // Should be able to finish operation
        tracker.finish_operation("node-1").await;

        // Should not be busy anymore
        assert!(!tracker.is_busy("node-1").await);

        // Should be able to start operation again
        assert!(tracker
            .try_start_operation("node-1", "snapshot", None)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_multiple_targets() {
        let tracker = SimpleOperationTracker::new();

        // Should be able to start operations on different targets
        assert!(tracker
            .try_start_operation("node-1", "restart", None)
            .await
            .is_ok());
        assert!(tracker
            .try_start_operation("node-2", "snapshot", None)
            .await
            .is_ok());
        assert!(tracker
            .try_start_operation("hermes-1", "restart", None)
            .await
            .is_ok());

        let status = tracker.get_operation_status().await;
        assert_eq!(status.total_active, 3);
        assert!(status.busy_nodes.contains_key("node-1"));
        assert!(status.busy_nodes.contains_key("node-2"));
        assert!(status.busy_nodes.contains_key("hermes-1"));
    }

    #[tokio::test]
    async fn test_cancel_operation() {
        let tracker = SimpleOperationTracker::new();

        tracker
            .try_start_operation("node-1", "snapshot", None)
            .await
            .unwrap();
        assert!(tracker.is_busy("node-1").await);

        tracker.cancel_operation("node-1").await.unwrap();
        assert!(!tracker.is_busy("node-1").await);

        // Should fail to cancel non-existent operation
        assert!(tracker.cancel_operation("node-1").await.is_err());
    }
}
