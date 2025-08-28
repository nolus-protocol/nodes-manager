// File: agent/src/types.rs
use serde::{Deserialize, Serialize};

// === REQUEST STRUCTURES ===

#[derive(Debug, Deserialize)]
pub struct CommandRequest {
    pub command: String,
}

#[derive(Debug, Deserialize)]
pub struct ServiceRequest {
    pub service_name: String,
}

#[derive(Debug, Deserialize)]
pub struct LogTruncateRequest {
    pub log_path: String,
    pub service_name: String,
}

#[derive(Debug, Deserialize)]
pub struct PruningRequest {
    pub deploy_path: String,
    pub keep_blocks: u64,
    pub keep_versions: u64,
    pub service_name: String,
    pub log_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SnapshotRequest {
    pub node_name: String,
    pub deploy_path: String,
    pub backup_path: String,
    pub service_name: String,
    pub log_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RestoreRequest {
    pub node_name: String,
    pub deploy_path: String,
    pub snapshot_file: String,
    pub validator_backup_file: Option<String>,
    pub service_name: String,
    pub log_path: Option<String>,
}

// === RESPONSE STRUCTURES ===

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            output: None,
            error: Some(message),
            status: None,
            uptime_seconds: None,
            filename: None,
            size_bytes: None,
            path: None,
            compression: None,
        }
    }
}

impl ApiResponse<()> {
    pub fn success() -> Self {
        Self {
            success: true,
            data: None,
            output: None,
            error: None,
            status: None,
            uptime_seconds: None,
            filename: None,
            size_bytes: None,
            path: None,
            compression: None,
        }
    }

    pub fn success_with_output(output: String) -> Self {
        Self {
            success: true,
            data: None,
            output: Some(output),
            error: None,
            status: None,
            uptime_seconds: None,
            filename: None,
            size_bytes: None,
            path: None,
            compression: None,
        }
    }

    pub fn success_with_status(status: String) -> Self {
        Self {
            success: true,
            data: None,
            output: None,
            error: None,
            status: Some(status),
            uptime_seconds: None,
            filename: None,
            size_bytes: None,
            path: None,
            compression: None,
        }
    }

    pub fn success_with_uptime(uptime_seconds: u64) -> Self {
        Self {
            success: true,
            data: None,
            output: None,
            error: None,
            status: None,
            uptime_seconds: Some(uptime_seconds),
            filename: None,
            size_bytes: None,
            path: None,
            compression: None,
        }
    }

    pub fn success_with_snapshot(filename: String, size_bytes: u64, path: String) -> Self {
        Self {
            success: true,
            data: None,
            output: None,
            error: None,
            status: None,
            uptime_seconds: None,
            filename: Some(filename),
            size_bytes: Some(size_bytes),
            path: Some(path),
            compression: Some("lz4".to_string()),
        }
    }
}

// === INTERNAL STRUCTURES ===

#[derive(Debug)]
pub struct SnapshotInfo {
    pub filename: String,
    pub size_bytes: u64,
    pub path: String,
}
