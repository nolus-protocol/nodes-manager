// File: manager/src/http/mod.rs
//! HTTP communication module for agent management
//!
//! This module handles all HTTP communication with remote agents deployed on blockchain servers.
//! Each server runs an agent on port 8745 that executes operations locally.
//!
//! # Architecture
//!
//! ```text
//! Manager → HTTP Request → Agent (port 8745)
//!    ↓           ↓
//!  Config    Operation
//!    ↓           ↓
//! Tracking ← Job Status ← Agent Response
//! ```
//!
//! # Communication Pattern
//!
//! 1. Manager sends operation request to agent
//! 2. Agent returns job ID for long-running operations
//! 3. Manager polls for completion status
//! 4. Operation completes or fails with detailed error
//!
//! # Safety Features
//!
//! - Direct HTTP per operation (no persistent connections)
//! - Automatic maintenance window tracking
//! - Operation timeout handling
//! - Busy node detection to prevent concurrent operations

pub mod agent_manager;
pub mod operations;

pub use agent_manager::HttpAgentManager;
