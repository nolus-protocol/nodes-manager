// File: manager/src/services/maintenance_service.rs
use crate::config::Config;
use crate::database::Database;
use crate::http::HttpAgentManager;
use crate::services::operation_executor::OperationExecutor;
use anyhow::Result;
use std::sync::Arc;

pub struct MaintenanceService {
    http_manager: Arc<HttpAgentManager>,
    operation_executor: Arc<OperationExecutor>,
}

impl MaintenanceService {
    pub fn new(
        config: Arc<Config>,
        database: Arc<Database>,
        http_manager: Arc<HttpAgentManager>,
        alert_service: Arc<crate::services::AlertService>,
    ) -> Self {
        // Create OperationExecutor for delegating all operations
        let operation_executor = Arc::new(OperationExecutor::new(config, database, alert_service));

        Self {
            http_manager,
            operation_executor,
        }
    }

    /// Execute an operation immediately (used by scheduler)
    /// Delegates to OperationExecutor for consistent tracking and alerting
    pub async fn execute_immediate_operation(
        &self,
        operation_type: &str,
        target_name: &str,
    ) -> Result<String> {
        let http_manager = self.http_manager.clone();
        let target_name_clone = target_name.to_string();
        let operation_type_owned = operation_type.to_string();

        // Delegate to OperationExecutor with appropriate operation
        self.operation_executor
            .execute_async(operation_type, target_name, move || {
                let http_manager = http_manager.clone();
                let target_name = target_name_clone.clone();
                let op_type = operation_type_owned.clone();
                async move {
                    match op_type.as_str() {
                        "pruning" => http_manager.execute_node_pruning(&target_name).await,
                        "snapshot_creation" => http_manager
                            .create_node_snapshot(&target_name)
                            .await
                            .map(|_| ()),
                        "node_restart" => http_manager.restart_node(&target_name).await,
                        _ => Err(anyhow::anyhow!("Unknown operation type: {}", op_type)),
                    }
                }
            })
            .await
    }
}
