# Nodes Manager

[![Tests](https://github.com/nolus-protocol/nodes-manager/actions/workflows/test.yml/badge.svg)](https://github.com/nolus-protocol/nodes-manager/actions/workflows/test.yml)
[![Release](https://github.com/nolus-protocol/nodes-manager/actions/workflows/release.yml/badge.svg)](https://github.com/nolus-protocol/nodes-manager/actions/workflows/release.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A comprehensive Rust-based infrastructure management system for 20+ blockchain nodes with advanced health monitoring, automated maintenance, network-wide snapshot management, state sync orchestration, and ETL service monitoring through a centralized web interface.

## Features

### Core Functionality

- **Multi-Service Health Monitoring**: Real-time monitoring of blockchain nodes, Hermes relayers, and ETL services with RPC-based status checks
- **Automated Pruning**: Seamless integration with `cosmos-pruner` tool for efficient blockchain data management
- **Network-Based Snapshot System**: Create, restore, and manage network-wide LZ4-compressed blockchain snapshots with cross-node recovery and validator state preservation
- **State Sync Orchestration**: Automated state sync execution for rapid node synchronization from trusted snapshots
- **ETL Service Monitoring**: HTTP health checks for custom ETL services with configurable endpoints and timeout handling
- **Log Monitoring**: Pattern-based log monitoring with configurable alerts and context extraction
- **Hermes Management**: Smart relayer restarts with RPC-based dependency validation
- **Web Interface**: RESTful API with comprehensive endpoints for all operations
- **Centralized Alert System**: Progressive rate-limited webhook notifications with recovery detection
- **Configuration Hot-Reload**: Multi-server support with automatic configuration reloading
- **Smart Path Derivation**: Automatic path configuration based on server-level defaults

### Advanced Capabilities

- **Parallel Operations**: Execute maintenance across multiple servers simultaneously
- **Dependency Validation**: Hermes restarts only when dependent nodes are healthy and synced
- **Scheduled Maintenance**: Cron-based automation with timezone awareness for pruning, snapshots, and service restarts
- **Real-time Monitoring**: Continuous health checks with SQLite persistence and historical tracking
- **Batch Operations**: Execute operations across multiple nodes efficiently with built-in safety checks
- **Maintenance Tracking**: Track operation status with duration estimates, stuck operation detection, and automatic cleanup
- **Cross-Node Recovery**: Network-based snapshots allow any node on the same network to restore from shared snapshots
- **Validator State Preservation**: Auto-restore preserves current validator signing state to prevent double-signing
- **Scheduled Snapshots**: Automatic network snapshot creation with configurable retention policies
- **Emergency Cleanup**: Force cleanup of stuck operations and maintenance windows
- **Progressive Alerting**: Rate-limited alerts (0, 6, 12, 24, 48 hours) prevent notification spam
- **Auto-Restore Detection**: Automatic restoration from snapshots when corruption patterns detected in logs

### State Sync Features

- **Automated State Sync**: Orchestrated state sync execution with RPC parameter fetching
- **Fail-Fast Design**: Immediate failure on any error with detailed logging
- **WASM Cache Management**: Smart cleanup of WASM cache during state sync
- **Config Management**: Automatic state sync enablement/disablement in config.toml
- **Timeout Handling**: Configurable sync timeout with automatic failure detection
- **Multi-Chain Support**: Automatic daemon binary detection for different Cosmos chains

### Snapshot System Features

- **Network-Based Naming**: Snapshots are named by network with block height (e.g., `pirin-1_20250121_17154420` for network `pirin-1`, date `20250121`, block height `17154420`) enabling cross-node recovery
- **Validator Safety**: Current validator state is preserved during restore to prevent consensus violations
- **LZ4 Compression**: Fast background compression with good ratios
- **Automatic Backups**: Scheduled network snapshot creation
- **Retention Management**: Configurable cleanup of old network snapshots
- **Cross-Node Recovery**: Any node on the same network can restore from the same snapshot
- **Long Operation Support**: 24-hour timeout for large snapshots

### Monitoring Features

- **Process Monitoring**: Detect stuck pruning processes and silent failures
- **Log Pattern Detection**: Monitor logs for specific error patterns with context extraction
- **Rate-Limited Alerting**: Prevent alarm spam with progressive alert scheduling (3 checks, then 6h, 6h, 12h, 24h intervals)
- **Maintenance Windows**: Visual indication when nodes are undergoing maintenance
- **Health Recovery Notifications**: Automatic notifications when nodes recover from failures
- **Auto-Restore Triggers**: Automatic restoration from snapshots when corruption patterns detected
- **ETL Health Checks**: HTTP-based health monitoring for custom ETL services
- **Catching Up Detection**: Clear distinction between "Synced" and "Catching Up" states

### Alert System Features

- **Centralized AlertService**: Single webhook configuration for all alerts across the system
- **Progressive Rate Limiting**: Smart alert scheduling to prevent notification fatigue
- **Recovery Detection**: Automatic recovery notifications when services return to healthy state
- **Alert Types**: Node health, auto-restore, snapshot operations, Hermes restarts, log patterns, maintenance
- **Severity Levels**: Critical, Warning, Info, Recovery
- **Webhook Testing**: Startup webhook connectivity validation

## Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Web Interface │    │  Health Monitor │    │ Maintenance     │
│   (Axum + API)  │    │  (RPC Polling)  │    │ Scheduler       │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
         ┌───────────────────────────────────────────────────────────┐
         │              Core Engine                                  │
         │  ┌─────────────┐  ┌─────────────┐  ┌──────────┐  ┌──────┐│
         │  │Config Mgmt  │  │  Database   │  │HTTP Agent│  │Alert ││
         │  │(Hot Reload) │  │  (SQLite)   │  │Manager   │  │System││
         │  │             │  │             │  │          │  │      ││
         │  └─────────────┘  └─────────────┘  └──────────┘  └──────┘│
         │  ┌─────────────┐  ┌─────────────┐  ┌──────────┐         │
         │  │Maintenance  │  │ Network     │  │ State    │         │
         │  │Tracker      │  │ Snapshot    │  │ Sync     │         │
         │  │             │  │ Manager     │  │ Manager  │         │
         │  └─────────────┘  └─────────────┘  └──────────┘         │
         └───────────────────────────────────────────────────────────┘
                                 │
         ┌───────────────────────────────────────────────────────────┐
         │            Blockchain Infrastructure                      │
         │  ┌─────────────┐  ┌─────────────┐  ┌──────────┐         │
         │  │   Cosmos    │  │   Hermes    │  │   ETL    │         │
         │  │   Nodes     │  │  Relayers   │  │ Services │         │
         │  └─────────────┘  └─────────────┘  └──────────┘         │
         └───────────────────────────────────────────────────────────┘
                                 │
         ┌───────────────────────────────────────────────────────────┐
         │         HTTP Agents (per server - port 8745)              │
         │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐ │
         │  │Pruning   │  │Snapshots │  │Restore   │  │State Sync│ │
         │  │          │  │          │  │          │  │          │ │
         │  └──────────┘  └──────────┘  └──────────┘  └──────────┘ │
         │  ┌──────────┐  ┌──────────┐  ┌──────────┐               │
         │  │Systemctl │  │Commands  │  │Logs      │               │
         │  │          │  │          │  │          │               │
         │  └──────────┘  └──────────┘  └──────────┘               │
         └───────────────────────────────────────────────────────────┘
```

## Installation

### Prerequisites

- Rust 1.70+
- `cosmos-pruner` tool installed on target servers
- HTTP agent access to all blockchain servers
- SQLite3
- `lz4` compression tool installed on target servers (for snapshots)

### Build & Setup

```bash
# Clone repository
git clone https://github.com/nolus-protocol/nodes-manager.git
cd nodes-manager

# Build release version
cargo build --release

# Create required directories
mkdir -p data static config

# Set up configuration files
# Create your configuration files based on the examples below

# Ensure proper API key setup for HTTP agents
export AGENT_API_KEY="your-secure-api-key-here"
```

### Agent Installation

On each blockchain server, install and run the agent:

```bash
# Copy agent binary to server
scp target/release/agent user@server:/usr/local/bin/

# Set up systemd service (optional)
# Create /etc/systemd/system/blockchain-agent.service

# Set API key and start
export AGENT_API_KEY="your-secure-api-key-here"
/usr/local/bin/agent
```

## Configuration

### Main Configuration

Create `config/main.toml` with:

```toml
host = "0.0.0.0"
port = 8095
check_interval_seconds = 90
rpc_timeout_seconds = 10

# Alert webhook (required for alerts)
alarm_webhook_url = "https://your-webhook-endpoint.com/webhook/node-alarm"

# Hermes configuration
hermes_min_uptime_minutes = 5

# Auto-restore trigger words (optional)
auto_restore_trigger_words = [
    "AppHash",
    "wrong Block.Header.AppHash",
    "database corruption",
    "state sync failed",
    "panic:"
]

# Log monitoring context (lines before/after match)
log_monitoring_context_lines = 2
```

### Server Configuration with Smart Defaults

Create files like `config/enterprise.toml` with server-level defaults:

```toml
[server]
host = "192.168.11.206"
agent_port = 8745
api_key = "your-secure-api-key-here"
request_timeout_seconds = 300
max_concurrent_requests = 5

# Smart defaults - automatically derive paths for all nodes on this server
[defaults]
base_deploy_path = "/opt/deploy"           # Auto-derives deploy_path
base_log_path = "/var/log"                 # Auto-derives log_path
base_backup_path = "/backup/snapshots"     # Auto-derives snapshot_backup_path

# Minimal node configuration - paths are auto-derived!
[nodes.enterprise-osmosis]
rpc_url = "http://192.168.11.206:26657"
network = "osmosis-1"
server_host = "enterprise"
enabled = true

# Pruning configuration
pruning_enabled = true
service_name = "osmosis"  # MANDATORY - systemd service name, used for path auto-derivation
pruning_schedule = "0 0 6 * * 2"  # Tuesdays at 6AM UTC
pruning_keep_blocks = 8000
pruning_keep_versions = 8000
# deploy_path auto-derived: /opt/deploy/osmosis (home directory)
# Note: pruning operations automatically use /opt/deploy/osmosis/data

# Snapshot configuration
snapshots_enabled = true
# snapshot_backup_path auto-derived: /backup/snapshots
# Snapshots use deploy_path directly: /opt/deploy/osmosis
auto_restore_enabled = true

# Scheduled snapshots (optional)
snapshot_schedule = "0 0 2 * * 0"  # Sundays at 2AM UTC
snapshot_retention_count = 7

# Log configuration
# log_path auto-derived: /var/log/osmosis
truncate_logs_enabled = false

# Per-node log monitoring (optional)
log_monitoring_enabled = true
log_monitoring_patterns = [
    "Possibly no price is available!",
    "failed to lock fees to pay for"
]

# State sync configuration (optional)
[nodes.enterprise-neutron]
rpc_url = "http://192.168.11.206:26957"
network = "neutron-1"
server_host = "enterprise"
enabled = true
pruning_enabled = true
snapshots_enabled = true

# State sync settings
state_sync_enabled = true
state_sync_schedule = "0 0 3 * * 6"  # Saturdays at 3AM UTC
state_sync_rpc_sources = [
    "https://rpc1.neutron.quokkastake.io:443",
    "https://rpc2.neutron.quokkastake.io:443"
]
state_sync_trust_height_offset = 2000
state_sync_max_sync_timeout_seconds = 600

# Hermes relayer configuration
[hermes.relay-enterprise]
server_host = "enterprise"
service_name = "hermes"
log_path = "/var/log/hermes"
restart_schedule = "0 0 16 * * 2"  # Tuesdays at 4PM UTC
dependent_nodes = ["enterprise-osmosis", "enterprise-neutron"]
truncate_logs_enabled = false

# ETL service monitoring
[etl.osmosis-etl]
server_host = "enterprise"
host = "192.168.11.206"
port = 8080
endpoint = "/health"
enabled = true
timeout_seconds = 10
description = "Osmosis ETL indexer service"
```

## Usage

### Start the Manager Service

```bash
# Start the manager
./target/release/manager

# Or with custom config location
CONFIG_DIR=./custom-config ./target/release/manager
```

The manager will:
- ✅ Load configuration from `config/*.toml` files
- ✅ Initialize SQLite database
- ✅ Test alert webhook connectivity
- ✅ Clean up stuck operations from previous runs
- ✅ Start health monitoring for nodes, Hermes, and ETL services
- ✅ Start scheduled maintenance tasks
- ✅ Launch web API on configured port (default 8095)

### API Endpoints

#### Health Monitoring

```bash
# Get all blockchain nodes health
GET /api/health/nodes
GET /api/health/nodes?include_disabled=true

# Get specific node health
GET /api/health/nodes/{node_name}

# Get all Hermes instances health
GET /api/health/hermes

# Get specific Hermes health
GET /api/health/hermes/{hermes_name}

# Get all ETL services health
GET /api/health/etl
GET /api/health/etl?include_disabled=true

# Get specific ETL service health
GET /api/health/etl/{service_name}

# Force refresh ETL health
POST /api/health/etl/refresh
```

#### Configuration Management

```bash
# Get all node configurations
GET /api/config/nodes

# Get all Hermes configurations
GET /api/config/hermes

# Get all ETL configurations
GET /api/config/etl
```

#### Manual Operations (Non-Blocking)

```bash
# Restart node (returns immediately)
POST /api/maintenance/nodes/{node_name}/restart

# Prune node (returns immediately)
POST /api/maintenance/nodes/{node_name}/prune

# Restart Hermes (returns immediately)
POST /api/maintenance/hermes/{hermes_name}/restart

# Execute state sync (returns immediately) - New in v1.3.0
POST /api/state-sync/{node_name}/execute
```

#### Snapshot Management

```bash
# Create network snapshot (returns immediately)
POST /api/snapshots/{node_name}/create

# List all network snapshots
GET /api/snapshots/{node_name}/list

# Get snapshot statistics
GET /api/snapshots/{node_name}/stats

# Delete specific snapshot (returns immediately)
DELETE /api/snapshots/{node_name}/{filename}

# Cleanup old snapshots (returns immediately)
POST /api/snapshots/{node_name}/cleanup?retention_count=5

# Restore from latest snapshot (returns immediately)
POST /api/snapshots/{node_name}/restore

# Check auto-restore triggers
GET /api/snapshots/{node_name}/check-triggers

# Get auto-restore status
GET /api/snapshots/{node_name}/auto-restore-status
```

#### Operation Management

```bash
# Get all active operations
GET /api/operations/active

# Check specific target status
GET /api/operations/{target_name}/status

# Cancel operation
POST /api/operations/{target_name}/cancel

# Emergency cleanup (remove stuck operations)
POST /api/operations/emergency-cleanup?max_hours=12
```

#### Maintenance Schedule

```bash
# Get scheduled maintenance operations
GET /api/maintenance/schedule
```

## Key Features in Detail

### Pruning with cosmos-pruner

The system uses the `cosmos-pruner` tool with extended timeouts for large datasets:

```bash
cosmos-pruner prune /opt/deploy/osmosis/data --blocks=8000 --versions=8000
```

**Process:**
1. Start maintenance tracking (5-hour timeout)
2. Stop blockchain service via HTTP agent
3. Optional: Truncate logs if enabled
4. Execute cosmos-pruner with configured parameters
5. Start blockchain service via HTTP agent
6. Verify service health
7. Send completion notification via AlertService

### Network-Based Snapshot System with Validator State Preservation

**Features:**
- **Network-Based Naming**: Snapshots named by network with block height (e.g., `pirin-1_20250121_17154420`) for cross-node compatibility. The block height provides a precise reference point for the snapshot state.
- **Cross-Node Recovery**: Any node on the same network can restore from shared network snapshots
- **Validator State Preservation**: Current validator signing state is backed up and restored to prevent double-signing
- **LZ4 Compression**: Fast background compression with good ratios
- **Automatic Backups**: Scheduled network snapshot creation
- **Retention Management**: Configurable cleanup of old network snapshots
- **Long Operation Support**: 24-hour timeout for large snapshots

**Network Snapshot Process:**
1. Query RPC for current block height
2. Build snapshot name: `{network}_{date}_{block_height}`
3. Start maintenance tracking (24-hour timeout)
4. Stop blockchain service via HTTP agent
5. Create network-named directory with block height
6. Copy data and wasm directories (INCLUDING validator state for compatibility)
7. Start blockchain service via HTTP agent
8. Apply network retention policy if configured
9. Verify snapshot integrity

**Cross-Node Restore Process:**
1. Start maintenance tracking (24-hour timeout)
2. Stop blockchain service via HTTP agent
3. **Backup current validator state** (critical for validator safety)
4. Delete existing data and wasm directories
5. Copy data and wasm from network snapshot
6. **Restore backed up validator state** (prevents double-signing)
7. Set proper permissions
8. Start blockchain service via HTTP agent
9. Verify service health
10. Send completion notification

**Snapshot Selection:**
- Restore automatically finds the latest snapshot for the network
- Sorting uses **numeric comparison on block height** (not alphabetical)
- Ensures correct selection: block 17154420 is chosen over 02000000
- Example: `pirin-1_20250121_17154420` vs `pirin-1_20241115_02000000`

**Snapshot Retention Policy:**
- Retention is **network-based**, not per-node (shared snapshots)
- Configured via `snapshot_retention_count` (e.g., keep 3 most recent)
- Cleanup uses **filesystem creation timestamps** for sorting
- Works with both old timestamp format and new block height format
- Also cleans up orphaned `.tar.lz4` files without directories

**Auto-Restore System:**
- Monitors node logs for trigger words (configurable patterns)
- Automatically restores from latest network snapshot when corruption detected
- **Preserves current validator state** during auto-restore
- Prevents infinite loops with 2-hour cooldown between attempts
- Sends critical alerts if auto-restore fails

### State Sync Orchestration

**Features:**
- **Automated RPC Fetching**: Automatically fetches trust height and hash from configured RPC sources
- **Fail-Fast Design**: Immediate failure on any error with comprehensive logging
- **Multi-Chain Support**: Automatic daemon binary detection (nolusd, osmosisd, neutrond, etc.)
- **WASM Cache Management**: Smart cleanup of WASM cache during state sync
- **Config Management**: Automatic state sync parameter injection and cleanup
- **Timeout Handling**: Configurable sync timeout with status monitoring

**State Sync Process:**
1. Fetch state sync parameters from RPC sources
2. Stop blockchain service
3. Truncate logs (if configured)
4. Update config.toml with state sync parameters
5. Execute `unsafe-reset-all` to wipe chain state
6. Clean WASM cache (preserve blobs, delete cache only)
7. Start blockchain service
8. Wait for state sync completion with timeout
9. Disable state sync in config
10. Restart service with clean config

### ETL Service Monitoring

**Features:**
- **HTTP Health Checks**: Configurable endpoints for ETL service health validation
- **Response Time Tracking**: Monitor response times and status codes
- **Integration with AlertService**: Same progressive alerting as blockchain nodes
- **Flexible Configuration**: Per-service timeout, endpoint, and description

**Configuration:**
```toml
[etl.my-etl-service]
server_host = "enterprise"
host = "192.168.11.206"
port = 8080
endpoint = "/health"
enabled = true
timeout_seconds = 10
description = "Custom ETL service"
```

### Centralized Alert System

**Progressive Rate Limiting:**
- **First Alert**: After 3 consecutive unhealthy checks
- **Second Alert**: 6 hours after first alert
- **Third Alert**: 6 hours after second alert (12 hours total)
- **Fourth Alert**: 12 hours after third alert (24 hours total)
- **Fifth+ Alerts**: Every 24 hours thereafter

**Alert Types:**
- `NodeHealth`: Health state changes for blockchain nodes and ETL services
- `AutoRestore`: Automatic restoration attempts and results
- `Snapshot`: Snapshot creation and restoration operations
- `Hermes`: Hermes relayer restart operations
- `LogPattern`: Log pattern detection alerts
- `Maintenance`: Maintenance operation notifications

**Severity Levels:**
- `Critical`: Node failures, auto-restore failures
- `Warning`: Degraded performance, approaching limits
- `Info`: Maintenance operations, scheduled tasks
- `Recovery`: Service recovery from failure state

**Webhook Payload:**
```json
{
  "timestamp": "2025-01-15T10:30:00Z",
  "alert_type": "NodeHealth",
  "severity": "Critical",
  "node_name": "enterprise-osmosis",
  "message": "Node health check failed: RPC timeout",
  "server_host": "enterprise",
  "details": {
    "block_height": 12345678,
    "error": "connection timeout"
  }
}
```

### Intelligent Hermes Restart

Hermes relayers restart only when ALL dependent nodes are:
- **Healthy** (RPC status check passes)
- **Synced** (not catching up)
- **Recent** (health data less than 5 minutes old)
- **Minimum Uptime** (configurable minimum uptime before restart)

### Maintenance Tracking System

**Real-time Status:**
- Track all operations with start time and duration estimates
- Visual indication in web interface when nodes are in maintenance
- Automatic cleanup of expired maintenance windows (48-hour maximum)
- Detection of stuck operations with process monitoring
- Startup cleanup of stuck operations from previous runs

**Emergency Features:**
- Force kill stuck pruning processes
- Emergency clear all maintenance windows
- Manual maintenance window cleanup per node
- Overdue operation detection (3x estimated duration)

**Operation Timeouts:**
- Pruning: 5 hours
- Snapshot creation: 24 hours
- Snapshot restore: 24 hours
- State sync: 24 hours
- Node restart: 30 minutes
- Hermes restart: 15 minutes

### HTTP Agent Management

**Direct Communication Model:**
- Each operation uses HTTP POST to dedicated agents
- Automatic operation cleanup after completion
- No persistent connection pooling (prevents conflicts)
- Configurable timeouts per server
- Parallel execution across different servers
- Sequential execution on same server for safety
- Job-based async execution with status polling

### Smart Path Derivation

**Server-Level Defaults:**
Instead of repeating paths for every node, define them once at the server level:

```toml
[defaults]
base_deploy_path = "/opt/deploy"
base_log_path = "/var/log"
base_backup_path = "/backup/snapshots"
```

**Automatic Derivation:**

If you specify `service_name` (MANDATORY) and base paths in `[defaults]`, paths will be auto-derived:
- `deploy_path` → `{base_deploy_path}/{service_name}` (home directory for the node)
- Pruning operations automatically append `/data` to `deploy_path`
- Snapshot operations use `deploy_path` directly
- `log_path` → `{base_log_path}/{service_name}`
- `snapshot_backup_path` → `{base_backup_path}`

**Example:**
```toml
[defaults]
base_deploy_path = "/opt/deploy"
base_log_path = "/var/log"
base_backup_path = "/backup/snapshots"

[nodes.osmosis-1]
service_name = "osmosis"  # MANDATORY
# ... rest of config
```

**Derived paths:**
- Deploy path: `/opt/deploy/osmosis` (home directory)
- Log path: `/var/log/osmosis`
- Backup path: `/backup/snapshots`
- Note: Pruning operations automatically use `/opt/deploy/osmosis/data`

### Timezone Handling

**Important**: All cron schedules run in the timezone where the Manager is deployed.

**Time Conversion Example:**
- Manager timezone: UTC+3 (EEST)
- Local time: 10:00 AM EEST
- Config schedule: `"0 0 7 * * 2"` (7:00 AM UTC)
- Result: Runs at 10:00 AM local time

## Cross-Node Recovery Examples

### Example 1: Network Snapshot Creation

```bash
# Create snapshot on node1 - creates network-based snapshot
curl -X POST http://localhost:8095/api/snapshots/pirin-node-1/create

# Result: Creates snapshot named "pirin-1_20250121_17154420"
# (network: pirin-1, date: 20250121, block height: 17154420)
# This snapshot can be used by ANY node on pirin-1 network
```

### Example 2: Cross-Node Restore

```bash
# Node7 can restore from snapshot created by Node1
curl -X POST http://localhost:8095/api/snapshots/pirin-node-7/restore

# Result: Restores from latest pirin-1 network snapshot
# Preserves pirin-node-7's current validator state
```

### Example 3: Validator Safety

```bash
# During restore, the system:
# 1. Backs up current validator state from pirin-node-7
# 2. Restores blockchain data from network snapshot
# 3. Restores pirin-node-7's validator state (not from snapshot)
# 4. Node resumes with correct signing state - no double-signing risk
```

## Monitoring & Debugging

### System Status

```bash
# Overall system status with network snapshot info
curl http://localhost:8095/api/health/nodes

# Active operations
curl http://localhost:8095/api/operations/active

# Check specific node status
curl http://localhost:8095/api/operations/pirin-node-1/status

# Network snapshot statistics
curl http://localhost:8095/api/snapshots/pirin-node-1/stats
```

### Database Inspection

The SQLite database (`data/nodes.db`) contains:

**Tables:**
- `health_records`: Node health history with block height, sync status
- `maintenance_operations`: All maintenance operations with status and timing

**Startup Cleanup:**
- Automatically cleans up stuck operations (> 1 hour in running/started state)
- Marks them as 'failed' with cleanup message
- Logs cleaned operations for audit trail

### Logs and Troubleshooting

- Health checks run every 90 seconds (configurable)
- ETL health checks run every 90 seconds (same as nodes)
- Failed operations are logged with detailed error messages
- HTTP agent connection failures automatically trigger retries
- Maintenance windows automatically expire after 48 hours
- Network snapshot operations support up to 24-hour timeouts
- Auto-restore attempts have 2-hour cooldown periods
- Cross-node recovery capability eliminates single points of failure

## Security Considerations

- API key authentication for all HTTP agent communications
- Config files contain sensitive information (use appropriate permissions)
- Use firewalls to restrict API access
- Monitor HTTP agent connection limits per server
- Regular security updates for all dependencies
- Network snapshot backup paths should be secured
- Validator state files are never included in shared snapshots
- Alert webhook URLs may contain sensitive endpoints

## Production Deployment

### Backup Strategy

- **Database**: Regular backups of `data/nodes.db`
- **Configuration**: Backup `config/*.toml` files separately
- **Network Snapshots**: Configure separate backup storage for network snapshots
- **Validator States**: Individual validator states are preserved per node
- **Log Rotation**: Set up log rotation for maintenance logs

### Storage Requirements

- **Database**: ~10-50MB for typical deployments
- **Logs**: Variable based on retention policies
- **Network Snapshots**: Can be very large (GBs to TBs depending on blockchain data)
- **Per-network Storage**: One snapshot location serves all nodes on same network

### Performance Considerations

- **LZ4 Compression**: Faster than gzip, good balance of speed/compression
- **Network Snapshot Retention**: Configure appropriate retention counts to manage disk usage
- **Alert Rate Limiting**: Progressive scheduling prevents webhook overload
- **Maintenance Windows**: Plan maintenance schedules to avoid conflicts
- **Cross-Node Recovery**: Reduces individual node storage requirements
- **ETL Monitoring**: Minimal overhead with configurable timeout

## Performance

- **Health checks**: 20+ nodes in <5 seconds (parallel execution)
- **ETL checks**: Multiple services in <2 seconds (parallel HTTP requests)
- **HTTP agent operations**: Direct communication per operation
- **Database**: SQLite with indexed queries for fast access
- **Memory usage**: ~50-150MB typical operation (includes network snapshot management)
- **Pruning operations**: 10-300 minutes depending on node size (5-hour timeout)
- **Network snapshot creation**: 30-1440 minutes depending on data size (24-hour timeout)
- **Cross-node restore**: 30-60 minutes depending on snapshot size with validator state preservation
- **State sync**: 5-20 minutes depending on network and RPC performance
- **Alert processing**: <100ms per alert with webhook delivery
- **LZ4 compression**: 50-200 MB/s typical compression speed

## Configuration Examples

### Complete Node Configuration

```toml
[server]
host = "192.168.1.100"
agent_port = 8745
api_key = "secure-key-here"

[defaults]
base_deploy_path = "/opt/deploy"
base_log_path = "/var/log"
base_backup_path = "/backup/snapshots"

[nodes.mainnet-pirin]
rpc_url = "http://192.168.1.100:26657"
network = "pirin-1"
server_host = "mainnet-server"
enabled = true

# Pruning
pruning_enabled = true
pruning_schedule = "0 0 6 * * 2"
pruning_keep_blocks = 8000
pruning_keep_versions = 8000

# Snapshots
snapshots_enabled = true
auto_restore_enabled = true
snapshot_schedule = "0 0 2 * * 0"
snapshot_retention_count = 4

# State sync
state_sync_enabled = true
state_sync_schedule = "0 0 3 * * 6"
state_sync_rpc_sources = [
    "https://rpc1.example.com",
    "https://rpc2.example.com"
]

# Logs
log_monitoring_enabled = true
log_monitoring_patterns = [
    "panic:",
    "database corruption"
]
```

### Multiple Nodes Same Network

```toml
# All nodes on pirin-1 network share the same snapshot location

[nodes.pirin-node-1]
network = "pirin-1"
snapshots_enabled = true  # Can create snapshots

[nodes.pirin-node-2]
network = "pirin-1"
snapshots_enabled = false  # Only restores, doesn't create

[nodes.pirin-node-7]
network = "pirin-1"
auto_restore_enabled = true  # Can auto-restore from network snapshots
```

### ETL Service Configuration

```toml
[etl.osmosis-indexer]
server_host = "mainnet-server"
host = "192.168.1.100"
port = 8080
endpoint = "/health"
enabled = true
timeout_seconds = 10
description = "Osmosis blockchain indexer"

[etl.neutron-api]
server_host = "mainnet-server"
host = "192.168.1.100"
port = 3000
endpoint = "/api/health"
enabled = true
timeout_seconds = 5
description = "Neutron API service"
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Support

For issues, questions, or contributions:
- GitHub Issues: [Create an issue](https://github.com/nolus-protocol/nodes-manager/issues)

## Related Projects

- [cosmos-pruner](https://github.com/osmosis-labs/cosmos-pruner) - Blockchain state pruning tool
- [Hermes](https://github.com/informalsystems/hermes) - IBC relayer
- [Cosmos SDK](https://github.com/cosmos/cosmos-sdk) - Blockchain application framework
- [LZ4](https://lz4.github.io/lz4/) - Fast compression algorithm
