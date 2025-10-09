//! Application-wide constants for timeouts, limits, and configuration values

//! Central repository for all configuration constants and magic numbers
//!
//! This module organizes constants by category to improve maintainability
//! and provide a single source of truth for timeouts, intervals, and limits.

#![allow(dead_code)] // Some constants are defined for future use

use std::time::Duration;

/// HTTP client timeout constants
pub mod http {
    use super::Duration;
    
    /// Default timeout for HTTP requests to agents
    pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
    
    /// Timeout for establishing HTTP connections
    pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
    
    /// Interval between polling for job status
    pub const JOB_POLL_INTERVAL: Duration = Duration::from_secs(10);
    
    /// Maximum time to wait for job completion (for very long operations)
    pub const MAX_JOB_WAIT: Duration = Duration::from_secs(86400); // 24 hours
}

/// Operation timeout constants (in hours)
pub mod operation_timeouts {
    /// Timeout for pruning operations
    pub const PRUNING_HOURS: u64 = 5;
    
    /// Timeout for snapshot creation
    pub const SNAPSHOT_CREATION_HOURS: u64 = 24;
    
    /// Timeout for snapshot restoration
    pub const SNAPSHOT_RESTORE_HOURS: u64 = 24;
    
    /// Timeout for state sync operations
    pub const STATE_SYNC_HOURS: u64 = 24;
    
    /// Timeout for node restart operations
    pub const NODE_RESTART_MINUTES: u64 = 30;
    
    /// Timeout for Hermes restart operations
    pub const HERMES_RESTART_MINUTES: u64 = 15;
    
    /// Sleep duration after stopping a node service before starting it (seconds)
    pub const NODE_RESTART_SLEEP_SECONDS: u64 = 5;
    
    /// Sleep duration after stopping Hermes service before starting it (seconds)
    pub const HERMES_RESTART_SLEEP_SECONDS: u64 = 3;
}

/// Cleanup and maintenance constants
pub mod cleanup {
    /// Hours after which stuck operations are cleaned up
    pub const OPERATION_CLEANUP_HOURS: i64 = 24;
    
    /// Hours after which stuck maintenance windows are cleaned up
    pub const MAINTENANCE_CLEANUP_HOURS: i64 = 48;
    
    /// Maximum hours for stuck maintenance detection (emergency cleanup)
    pub const MAINTENANCE_MAX_HOURS: i64 = 48;
    
    /// Hours after which old jobs are cleaned up from agent
    pub const JOB_CLEANUP_HOURS: i64 = 48;
    
    /// Cleanup interval in seconds
    pub const CLEANUP_INTERVAL_SECONDS: u64 = 3600; // 1 hour
}

/// Alert system constants
pub mod alerts {
    /// Number of consecutive unhealthy checks before first alert
    pub const FIRST_ALERT_AFTER_CHECKS: u32 = 3;
    
    /// Hours between first and second alert
    pub const SECOND_ALERT_INTERVAL_HOURS: i64 = 6;
    
    /// Hours between second and third alert
    pub const THIRD_ALERT_INTERVAL_HOURS: i64 = 6;
    
    /// Hours between third and fourth alert
    pub const FOURTH_ALERT_INTERVAL_HOURS: i64 = 12;
    
    /// Hours between subsequent alerts after fourth
    pub const SUBSEQUENT_ALERT_INTERVAL_HOURS: i64 = 24;
    
    /// Webhook request timeout
    pub const WEBHOOK_TIMEOUT_SECONDS: u64 = 10;
    
    /// Minimum hours between auto-restore attempts (cooldown)
    pub const AUTO_RESTORE_COOLDOWN_HOURS: i64 = 2;
}

/// Default configuration values
pub mod defaults {
    /// Default health check interval in seconds
    pub const HEALTH_CHECK_INTERVAL_SECONDS: u64 = 90;
    
    /// Default RPC timeout in seconds
    pub const RPC_TIMEOUT_SECONDS: u64 = 10;
    
    /// Default request timeout in seconds for server config
    pub const SERVER_REQUEST_TIMEOUT_SECONDS: u64 = 300;
    
    /// Default state sync trust height offset
    pub const STATE_SYNC_TRUST_HEIGHT_OFFSET: u32 = 2000;
    
    /// Default state sync timeout in seconds
    pub const STATE_SYNC_MAX_TIMEOUT_SECONDS: u64 = 600;
    
    /// Default hermes minimum uptime in minutes before restart
    pub const HERMES_MIN_UPTIME_MINUTES: u32 = 5;
}

/// Limits and constraints
pub mod limits {
    /// Maximum number of maintenance operations to query
    pub const MAX_MAINTENANCE_OPERATIONS: i32 = 100;
    
    /// Maximum concurrent operations per server
    pub const MAX_CONCURRENT_OPERATIONS: usize = 5;
    
    /// Maximum retry attempts for operations
    pub const MAX_RETRY_ATTEMPTS: u32 = 3;
}

/// Agent server constants
pub mod agent {
    /// Default port for agent HTTP server
    pub const DEFAULT_PORT: u16 = 8745;
    
    /// Default bind address for agent
    pub const DEFAULT_BIND_ADDRESS: &str = "0.0.0.0:8745";
}
