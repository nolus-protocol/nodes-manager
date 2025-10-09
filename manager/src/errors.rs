//! Custom error types for the nodes manager
//!
//! Provides structured error handling with context for different failure scenarios.

use std::fmt;

/// Main error type for the nodes manager
#[derive(Debug)]
pub enum ManagerError {
    /// Configuration-related errors
    Config(ConfigError),

    /// HTTP communication errors with agents
    Http(HttpError),

    /// Database operation errors
    Database(DatabaseError),

    /// Node operation errors (pruning, restart, etc.)
    NodeOperation(NodeOperationError),

    /// Maintenance tracking errors
    Maintenance(MaintenanceError),

    /// Other errors with context
    Other(String),
}

/// Configuration error variants
#[derive(Debug)]
pub enum ConfigError {
    /// Failed to load configuration file
    LoadFailed { path: String, reason: String },

    /// Invalid configuration value
    InvalidValue { field: String, reason: String },

    /// Missing required configuration
    MissingRequired { field: String },

    /// Configuration parsing error
    ParseError { reason: String },
}

/// HTTP communication error variants
#[derive(Debug)]
pub enum HttpError {
    /// Connection to agent failed
    ConnectionFailed { host: String, reason: String },

    /// Request timeout
    Timeout { host: String, operation: String },

    /// Invalid response from agent
    InvalidResponse { host: String, reason: String },

    /// Authentication failed
    AuthenticationFailed { host: String },

    /// Agent returned error
    AgentError { host: String, message: String },
}

/// Database error variants
#[derive(Debug)]
pub enum DatabaseError {
    /// Connection failed
    ConnectionFailed { reason: String },

    /// Query execution failed
    QueryFailed { query: String, reason: String },

    /// Data serialization/deserialization error
    SerializationError { reason: String },
}

/// Node operation error variants
#[derive(Debug)]
pub enum NodeOperationError {
    /// Node not found in configuration
    NodeNotFound { node_name: String },

    /// Node is busy with another operation
    NodeBusy {
        node_name: String,
        current_operation: String,
    },

    /// Operation failed
    OperationFailed {
        node_name: String,
        operation: String,
        reason: String,
    },

    /// Operation timeout
    OperationTimeout {
        node_name: String,
        operation: String,
    },

    /// Invalid operation state
    InvalidState { node_name: String, reason: String },
}

/// Maintenance tracking error variants
#[derive(Debug)]
pub enum MaintenanceError {
    /// Node is already in maintenance
    AlreadyInMaintenance {
        node_name: String,
        operation: String,
    },

    /// No active maintenance found
    NoActiveMaintenance { node_name: String },

    /// Failed to start maintenance window
    StartFailed { node_name: String, reason: String },

    /// Failed to end maintenance window
    EndFailed { node_name: String, reason: String },
}

// Implement Display for all error types
impl fmt::Display for ManagerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManagerError::Config(e) => write!(f, "Configuration error: {}", e),
            ManagerError::Http(e) => write!(f, "HTTP error: {}", e),
            ManagerError::Database(e) => write!(f, "Database error: {}", e),
            ManagerError::NodeOperation(e) => write!(f, "Node operation error: {}", e),
            ManagerError::Maintenance(e) => write!(f, "Maintenance error: {}", e),
            ManagerError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::LoadFailed { path, reason } => {
                write!(f, "Failed to load config from '{}': {}", path, reason)
            }
            ConfigError::InvalidValue { field, reason } => {
                write!(f, "Invalid value for '{}': {}", field, reason)
            }
            ConfigError::MissingRequired { field } => {
                write!(f, "Missing required field: {}", field)
            }
            ConfigError::ParseError { reason } => {
                write!(f, "Failed to parse config: {}", reason)
            }
        }
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpError::ConnectionFailed { host, reason } => {
                write!(f, "Connection to {} failed: {}", host, reason)
            }
            HttpError::Timeout { host, operation } => {
                write!(f, "Timeout while {} on {}", operation, host)
            }
            HttpError::InvalidResponse { host, reason } => {
                write!(f, "Invalid response from {}: {}", host, reason)
            }
            HttpError::AuthenticationFailed { host } => {
                write!(f, "Authentication failed for {}", host)
            }
            HttpError::AgentError { host, message } => {
                write!(f, "Agent error from {}: {}", host, message)
            }
        }
    }
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatabaseError::ConnectionFailed { reason } => {
                write!(f, "Database connection failed: {}", reason)
            }
            DatabaseError::QueryFailed { query, reason } => {
                write!(f, "Query '{}' failed: {}", query, reason)
            }
            DatabaseError::SerializationError { reason } => {
                write!(f, "Serialization error: {}", reason)
            }
        }
    }
}

impl fmt::Display for NodeOperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeOperationError::NodeNotFound { node_name } => {
                write!(f, "Node '{}' not found", node_name)
            }
            NodeOperationError::NodeBusy {
                node_name,
                current_operation,
            } => {
                write!(
                    f,
                    "Node '{}' is busy with: {}",
                    node_name, current_operation
                )
            }
            NodeOperationError::OperationFailed {
                node_name,
                operation,
                reason,
            } => {
                write!(
                    f,
                    "Operation '{}' failed on '{}': {}",
                    operation, node_name, reason
                )
            }
            NodeOperationError::OperationTimeout {
                node_name,
                operation,
            } => {
                write!(f, "Operation '{}' timed out on '{}'", operation, node_name)
            }
            NodeOperationError::InvalidState { node_name, reason } => {
                write!(f, "Invalid state for '{}': {}", node_name, reason)
            }
        }
    }
}

impl fmt::Display for MaintenanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MaintenanceError::AlreadyInMaintenance {
                node_name,
                operation,
            } => {
                write!(
                    f,
                    "Node '{}' is already in maintenance: {}",
                    node_name, operation
                )
            }
            MaintenanceError::NoActiveMaintenance { node_name } => {
                write!(f, "No active maintenance for node '{}'", node_name)
            }
            MaintenanceError::StartFailed { node_name, reason } => {
                write!(
                    f,
                    "Failed to start maintenance for '{}': {}",
                    node_name, reason
                )
            }
            MaintenanceError::EndFailed { node_name, reason } => {
                write!(
                    f,
                    "Failed to end maintenance for '{}': {}",
                    node_name, reason
                )
            }
        }
    }
}

// Implement std::error::Error
impl std::error::Error for ManagerError {}
impl std::error::Error for ConfigError {}
impl std::error::Error for HttpError {}
impl std::error::Error for DatabaseError {}
impl std::error::Error for NodeOperationError {}
impl std::error::Error for MaintenanceError {}

// Conversions from anyhow::Error for gradual migration
impl From<anyhow::Error> for ManagerError {
    fn from(err: anyhow::Error) -> Self {
        ManagerError::Other(err.to_string())
    }
}

// Conversion helpers for sub-errors
impl From<ConfigError> for ManagerError {
    fn from(err: ConfigError) -> Self {
        ManagerError::Config(err)
    }
}

impl From<HttpError> for ManagerError {
    fn from(err: HttpError) -> Self {
        ManagerError::Http(err)
    }
}

impl From<DatabaseError> for ManagerError {
    fn from(err: DatabaseError) -> Self {
        ManagerError::Database(err)
    }
}

impl From<NodeOperationError> for ManagerError {
    fn from(err: NodeOperationError) -> Self {
        ManagerError::NodeOperation(err)
    }
}

impl From<MaintenanceError> for ManagerError {
    fn from(err: MaintenanceError) -> Self {
        ManagerError::Maintenance(err)
    }
}
