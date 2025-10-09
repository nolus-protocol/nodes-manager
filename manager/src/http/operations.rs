// File: manager/src/http/operations.rs
use serde::{Deserialize, Serialize};

// Operation result structures - may be used for batch operations in future
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperationResult {
    pub success_count: usize,
    pub failure_count: usize,
    pub results: Vec<OperationResult>,
    pub summary: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub target_name: String,
    pub operation_type: String,
    pub success: bool,
    pub message: String,
    pub duration_seconds: Option<f64>,
    pub details: Option<serde_json::Value>,
}
