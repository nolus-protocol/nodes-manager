//! Common test data and constants

use chrono::Utc;
use uuid::Uuid;

/// Generate a random job ID for testing
pub fn random_job_id() -> String {
    Uuid::new_v4().to_string()
}

/// Get current timestamp for testing
pub fn now() -> chrono::DateTime<Utc> {
    Utc::now()
}

/// Common test network names
pub mod networks {
    pub const OSMOSIS: &str = "osmosis-1";
    pub const COSMOS: &str = "cosmoshub-4";
    pub const JUNO: &str = "juno-1";
    pub const PIRIN: &str = "pirin-1";
}

/// Common test node names
pub mod nodes {
    pub const NODE_1: &str = "test-node-1";
    pub const NODE_2: &str = "test-node-2";
    pub const NODE_3: &str = "test-node-3";
}

/// Common test server names
pub mod servers {
    pub const SERVER_1: &str = "test-server-1";
    pub const SERVER_2: &str = "test-server-2";
}

/// Common operation types
pub mod operations {
    pub const PRUNING: &str = "pruning";
    pub const SNAPSHOT_CREATE: &str = "snapshot_create";
    pub const SNAPSHOT_RESTORE: &str = "snapshot_restore";
    pub const STATE_SYNC: &str = "state_sync";
    pub const RESTART: &str = "restart";
}
