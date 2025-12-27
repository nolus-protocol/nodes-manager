// File: agent/src/types.rs
use chrono::{DateTime, Utc};
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

// NEW: Request to delete all files in a directory
#[derive(Debug, Deserialize)]
pub struct LogDeleteAllRequest {
    pub log_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PruningRequest {
    pub deploy_path: String,
    pub keep_blocks: u64,
    pub keep_versions: u64,
    pub service_name: String,
    pub log_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SnapshotRequest {
    pub node_name: String,
    pub snapshot_name: String, // Pre-built by manager: network_date_blockheight (e.g., "pirin-1_20250121_17154420")
    pub deploy_path: String,
    pub backup_path: String,
    pub service_name: String,
    pub log_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RestoreRequest {
    pub node_name: String,
    pub deploy_path: String,
    pub snapshot_dir: String, // FIXED: Changed from snapshot_file to snapshot_dir
    pub service_name: String,
    pub log_path: Option<String>,
}

// === JOB TRACKING STRUCTURES ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInfo {
    pub job_id: String,
    pub operation_type: String,
    pub target_name: String,
    pub status: JobStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result: Option<serde_json::Value>,
    pub error_message: Option<String>,
}

// === RESPONSE STRUCTURES ===

#[derive(Debug, Serialize, Default)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_status: Option<String>,
}

impl<T: Default> ApiResponse<T> {
    pub fn error(message: String) -> Self {
        Self {
            error: Some(message),
            ..Default::default()
        }
    }
}

impl ApiResponse<()> {
    pub fn success() -> Self {
        Self {
            success: true,
            ..Default::default()
        }
    }

    pub fn success_with_output(output: String) -> Self {
        Self {
            success: true,
            output: Some(output),
            ..Default::default()
        }
    }

    pub fn success_with_status(status: String) -> Self {
        Self {
            success: true,
            status: Some(status),
            ..Default::default()
        }
    }

    pub fn success_with_uptime(uptime_seconds: u64) -> Self {
        Self {
            success: true,
            uptime_seconds: Some(uptime_seconds),
            ..Default::default()
        }
    }

    pub fn success_with_job(job_id: String, job_status: String) -> Self {
        Self {
            success: true,
            job_id: Some(job_id),
            job_status: Some(job_status),
            ..Default::default()
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StateSyncRequest {
    pub service_name: String,
    pub home_dir: String,
    pub config_path: String,
    pub daemon_binary: String,
    pub rpc_servers: Vec<String>,
    pub trust_height: i64,
    pub trust_hash: String,
    pub timeout_seconds: u64,
    pub log_path: Option<String>,
}
