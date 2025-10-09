// File: src/snapshot/mod.rs

//! Network-based snapshot management with cross-node recovery
//!
//! This module provides network-wide blockchain snapshot functionality with validator
//! state preservation and cross-node recovery capabilities.
//!
//! # Key Features
//!
//! - **Network-Based Snapshots**: Named by network (e.g., `pirin-1_20250115_120000`)
//! - **Cross-Node Recovery**: Any node on same network can restore from shared snapshots
//! - **Validator Safety**: Current validator state preserved during restore (prevents double-signing)
//! - **LZ4 Compression**: Fast background compression with good ratios
//! - **Retention Management**: Configurable cleanup of old snapshots
//!
//! # Snapshot Process
//!
//! 1. Stop blockchain service
//! 2. Copy data/wasm directories (excluding validator state)
//! 3. Start blockchain service
//! 4. Background LZ4 compression
//!
//! # Restore Process
//!
//! 1. Stop blockchain service
//! 2. **Backup current validator state**
//! 3. Restore blockchain data from snapshot
//! 4. **Restore backed-up validator state**
//! 5. Start blockchain service

pub mod manager;

pub use manager::{SnapshotManager, SnapshotInfo, SnapshotStats};
