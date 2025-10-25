// File: manager/src/services/mod.rs

//! Business logic services layer
//!
//! This module contains high-level service implementations that orchestrate
//! lower-level components to provide business functionality.
//!
//! # Services
//!
//! - **AlertService**: Centralized webhook-based alerting with progressive rate limiting
//! - **OperationExecutor**: Generic background operation executor with tracking and alerting
//! - **MaintenanceService**: Orchestrates maintenance operations (pruning, snapshots, etc.)
//! - **HermesService**: Manages Hermes relayer instances
//! - **HealthService**: Health monitoring and status queries
//! - **SnapshotService**: Snapshot creation, restoration, and management
//!
//! # Design Principles
//!
//! - Services wrap Arc-shared state for concurrent access
//! - Each service focuses on a single domain (alerts, health, snapshots, etc.)
//! - Services coordinate between HTTP agents, database, and tracking systems

pub mod alert_service;
pub mod hermes_service;
pub mod maintenance_service;
pub mod operation_executor;
pub mod snapshot_service;
pub mod state_sync_service;

pub use alert_service::AlertService;
pub use hermes_service::HermesService;
pub use maintenance_service::MaintenanceService;
pub use operation_executor::OperationExecutor;
pub use snapshot_service::SnapshotService;
pub use state_sync_service::StateSyncService;
