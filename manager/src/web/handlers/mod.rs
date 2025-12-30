//! HTTP request handlers for the Manager API.
//!
//! This module is organized by domain:
//! - `admin` - CRUD operations for servers, nodes, hermes, and settings
//! - `common` - Shared types, query structs, and utilities
//! - `config` - Read-only configuration endpoints
//! - `health` - Health monitoring endpoints
//! - `maintenance` - Manual operation execution endpoints
//! - `operations` - Operation tracking and management
//! - `snapshots` - Snapshot and state sync operations

pub mod admin;
pub mod common;
pub mod config;
pub mod health;
pub mod maintenance;
pub mod operations;
pub mod snapshots;

// Re-export all public handler functions for convenience
// Note: common module is internal, used only by sibling modules
pub use admin::*;
pub use config::*;
pub use health::*;
pub use maintenance::*;
pub use operations::*;
pub use snapshots::*;
