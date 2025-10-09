// File: manager/src/services/mod.rs

//! Business logic services layer
//!
//! This module contains high-level service implementations that orchestrate
//! lower-level components to provide business functionality.
//!
//! # Services
//!
//! - **AlertService**: Centralized webhook-based alerting with progressive rate limiting
//!
//! # Design Principles
//!
//! - Services wrap Arc-shared state for concurrent access
//! - Each service focuses on a single domain (alerts, health, snapshots, etc.)
//! - Services coordinate between HTTP agents, database, and tracking systems

pub mod alert_service;

pub use alert_service::AlertService;
