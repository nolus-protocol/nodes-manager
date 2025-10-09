// File: agent/src/services/job_manager.rs
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::types::{JobInfo, JobStatus};

#[derive(Clone)]
pub struct JobManager {
    jobs: Arc<RwLock<HashMap<String, JobInfo>>>,
}

impl Default for JobManager {
    fn default() -> Self {
        Self::new()
    }
}

impl JobManager {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_job(&self, operation_type: &str, target_name: &str) -> String {
        let job_id = format!(
            "{}_{}_{}",
            operation_type,
            target_name,
            Utc::now().timestamp()
        );

        let job_info = JobInfo {
            job_id: job_id.clone(),
            operation_type: operation_type.to_string(),
            target_name: target_name.to_string(),
            status: JobStatus::Running,
            started_at: Utc::now(),
            completed_at: None,
            result: None,
            error_message: None,
        };

        let mut jobs = self.jobs.write().await;
        jobs.insert(job_id.clone(), job_info);

        info!(
            "Created job {}: {} for {}",
            job_id, operation_type, target_name
        );
        job_id
    }

    pub async fn complete_job(&self, job_id: &str, result: serde_json::Value) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = JobStatus::Completed;
            job.completed_at = Some(Utc::now());
            job.result = Some(result);
            info!("Job {} completed successfully", job_id);
        }
    }

    pub async fn fail_job(&self, job_id: &str, error_message: String) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = JobStatus::Failed;
            job.completed_at = Some(Utc::now());
            job.error_message = Some(error_message.clone());
            warn!("Job {} failed: {}", job_id, error_message);
        }
    }

    pub async fn get_job_status(&self, job_id: &str) -> Option<JobInfo> {
        let jobs = self.jobs.read().await;
        jobs.get(job_id).cloned()
    }

    pub async fn cleanup_old_jobs(&self, max_hours: i64) -> u32 {
        let mut jobs = self.jobs.write().await;
        let cutoff = Utc::now() - chrono::Duration::hours(max_hours);
        let initial_count = jobs.len();

        jobs.retain(|job_id, job| {
            let should_keep = job.started_at > cutoff;
            if !should_keep {
                info!("Cleaned up old job: {} ({})", job_id, job.operation_type);
            }
            should_keep
        });

        let cleaned = initial_count - jobs.len();
        if cleaned > 0 {
            info!("Cleaned up {} old jobs older than {}h", cleaned, max_hours);
        }
        cleaned as u32
    }

    pub async fn get_running_jobs(&self) -> Vec<JobInfo> {
        let jobs = self.jobs.read().await;
        jobs.values()
            .filter(|job| matches!(job.status, JobStatus::Running))
            .cloned()
            .collect()
    }
}
