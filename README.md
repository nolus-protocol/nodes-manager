# Blockchain Nodes Manager

A comprehensive Rust-based system for managing 20+ blockchain nodes with health monitoring, automated pruning using cosmos-pruner, network-based snapshot management with auto-restore, log monitoring, and Hermes relayer management through a web interface.

## Features

### Core Functionality
- **Health Monitoring**: Real-time RPC status checks with configurable intervals
- **Automated Pruning**: Integration with `cosmos-pruner` tool for efficient blockchain data management
- **Network-Based Snapshot System**: Create, restore, and manage network-wide LZ4-compressed blockchain snapshots with cross-node recovery and validator state preservation
- **Log Monitoring**: Pattern-based log monitoring with configurable alerts and context extraction
- **Hermes Management**: Smart relayer restarts with RPC-based dependency validation
- **Web Interface**: RESTful API with comprehensive endpoints for all operations
- **HTTP Agent Management**: Direct HTTP communication with agents for all operations
- **Configuration**: Hot-reload capability with multi-server support

### Advanced Capabilities
- **Parallel Operations**: Execute maintenance across multiple servers simultaneously
- **Dependency Validation**: Hermes restarts only when dependent nodes are healthy and synced
- **Scheduled Maintenance**: Cron-based automation with timezone awareness
- **Real-time Monitoring**: Continuous health checks with database persistence
- **Batch Operations**: Execute pruning/restarts across multiple nodes efficiently
- **Maintenance Tracking**: Track operation status with duration estimates and stuck operation detection
- **Cross-Node Recovery**: Network-based snapshots allow any node on the same network to restore from shared snapshots
- **Validator State Preservation**: Auto-restore preserves current validator signing state to prevent double-signing
- **Scheduled Snapshots**: Automatic network snapshot creation with configurable retention policies
- **Emergency Cleanup**: Force cleanup of stuck operations and maintenance windows

### Snapshot System Features
- **Network-Based Naming**: Snapshots are named by network (e.g., `pirin-1_20250101_120000`) enabling cross-node recovery
- **Validator Safety**: Current validator state is preserved during restore to prevent consensus violations
- **LZ4 Compression**: Fast background compression with good ratios
- **Automatic Backups**: Scheduled network snapshot creation
- **Retention Management**: Configurable cleanup of old network snapshots
- **Cross-Node Recovery**: Any node on the same network can restore from the same snapshot
- **Long Operation Support**: 24-hour timeout for large snapshots

### Monitoring Features
- **Process Monitoring**: Detect stuck pruning processes and silent failures
- **Log Pattern Detection**: Monitor logs for specific error patterns with context extraction
- **Rate-Limited Alerting**: Prevent alarm spam with configurable rate limiting
- **Maintenance Windows**: Visual indication when nodes are undergoing maintenance
- **Health Recovery Notifications**: Automatic notifications when nodes recover
- **Auto-Restore Triggers**: Automatic restoration from snapshots when corruption patterns detected

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
         │  │Config Mgmt  │  │  Database   │  │HTTP Agent│  │ Log  ││
         │  │(Hot Reload) │  │  (SQLite)   │  │Manager   │  │Monitor││
         │  │             │  │             │  │          │  │      ││
         │  └─────────────┘  └─────────────┘  └──────────┘  └──────┘│
         │  ┌─────────────┐  ┌─────────────┐  ┌──────────┐         │
         │  │Maintenance  │  │ Network     │  │ Auto     │         │
         │  │Tracker      │  │ Snapshot    │  │ Restore  │         │
         │  │             │  │ Manager     │  │          │         │
         │  └─────────────┘  └─────────────┘  └──────────┘         │
         └───────────────────────────────────────────────────────────┘
                                 │
         ┌───────────────────────────────────────────────────────────┐
         │            Blockchain Infrastructure                      │
         │  ┌─────────────┐  ┌─────────────┐  ┌──────────┐         │
         │  │   Cosmos    │  │   Hermes    │  │ Remote   │         │
         │  │   Nodes     │  │  Relayers   │  │ Servers  │         │
         │  └─────────────┘  └─────────────┘  └──────────┘         │
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
mkdir -p config
# Create your configuration files based on the examples below

# Ensure proper API key setup for HTTP agents
export AGENT_API_KEY="your-secure-api-key-here"
```

## Configuration

### Main Configuration
Create `config/main.toml` with:
```toml
host = "0.0.0.0"
port = 8095
check_interval_seconds = 90
rpc_timeout_seconds = 10
alarm_webhook_url = "http://your-n8n-instance/webhook/node-alarm"
hermes_min_uptime_minutes = 5

# Auto-restore trigger words (optional)
auto_restore_trigger_words = [
    "AppHash",
    "wrong Block.Header.AppHash",
    "database corruption",
    "state sync failed"
]

# Log monitoring configuration (optional)
log_monitoring_enabled = true
log_monitoring_patterns = [
    "Possibly no price is available!",
    "failed to lock fees to pay for",
    "consensus failure",
    "panic:"
]
log_monitoring_interval_minutes = 5
log_monitoring_context_lines = 2
```

### Server Configuration Example
Create files like `config/discovery.toml` with this structure:
```toml
[server]
host = "192.168.11.206"
agent_port = 8745
api_key = "your-secure-api-key-here"
request_timeout_seconds = 300
max_concurrent_requests = 5

[nodes.osmosis-1]
rpc_url = "http://192.168.11.206:26657"
network = "osmosis-1"
server_host = "discovery"
enabled = true

# Pruning configuration
pruning_enabled = true
pruning_schedule = "0 0 6 * * 2"  # Tuesdays at 6AM UTC
pruning_keep_blocks = 8000
pruning_keep_versions = 8000
pruning_deploy_path = "/opt/deploy/osmosis"
pruning_service_name = "osmosis"

# Log configuration (for log monitoring)
log_path = "/var/log/osmosis"
truncate_logs_enabled = false

# Network-based snapshot configuration (optional)
snapshots_enabled = true
snapshot_backup_path = "/backup/snapshots/osmosis"
auto_restore_enabled = true

# Scheduled network snapshots (optional)
snapshot_schedule = "0 0 2 * * 0"  # Sundays at 2AM UTC
snapshot_retention_count = 7  # Keep 7 most recent network snapshots

[hermes.relay-discovery]
server_host = "discovery"
service_name = "hermes"
log_path = "/var/log/hermes"
restart_schedule = "0 0 16 * * 2"  # Tuesdays at 4PM UTC
dependent_nodes = ["discovery-osmosis-1", "discovery-neutron-1"]
```

## Usage

### Start the Service
```bash
./target/release/nodes-manager
```

### API Endpoints

#### Health Monitoring
```bash
# Get all nodes health
GET /api/nodes/health

# Get specific node health
GET /api/nodes/{name}/health

# Get node health history
GET /api/nodes/{name}/history?limit=50

# Force health check
POST /api/nodes/{name}/check
```

#### Network-Based Snapshot Management
```bash
# Create network snapshot (named by network, e.g., pirin-1_20250101_120000)
POST /api/snapshots/{node_name}/create

# List all network snapshots (any node on network can see same snapshots)
GET /api/snapshots/{node_name}/list

# Restore from latest network snapshot (preserves current validator state)
POST /api/snapshots/{node_name}/restore

# Delete specific network snapshot
DELETE /api/snapshots/{node_name}/{filename}

# Get network snapshot statistics
GET /api/snapshots/{node_name}/stats

# Check auto-restore triggers
POST /api/snapshots/{node_name}/check-restore

# Cleanup old network snapshots (keep N most recent)
POST /api/snapshots/{node_name}/cleanup?retention_count=5
```

#### Maintenance Operations
```bash
# Execute immediate pruning
POST /api/maintenance/run-now
{
  "operation_type": "pruning",
  "target_name": "discovery-osmosis-1",
  "schedule": "immediate"
}

# Execute immediate network snapshot creation
POST /api/maintenance/run-now
{
  "operation_type": "snapshot_creation",
  "target_name": "discovery-osmosis-1",
  "schedule": "immediate"
}

# Batch pruning multiple nodes
POST /api/maintenance/prune-multiple
{
  "node_names": ["discovery-osmosis-1", "enterprise-neutron-1"]
}

# Get maintenance logs
GET /api/maintenance/logs?limit=100

# Get scheduled operations
GET /api/maintenance/schedule

# Schedule network snapshot creation
POST /api/maintenance/schedule-snapshot
{
  "operation_type": "snapshot_creation",
  "target_name": "discovery-osmosis-1",
  "schedule": "0 0 2 * * 0"
}
```

#### Maintenance Tracking
```bash
# Get active maintenance operations
GET /api/maintenance/active

# Get maintenance statistics
GET /api/maintenance/stats

# Get detailed maintenance report
GET /api/maintenance/report

# Check for stuck operations
GET /api/maintenance/stuck

# Emergency kill stuck processes
POST /api/maintenance/kill-stuck

# Emergency clear all maintenance windows
POST /api/maintenance/emergency-clear

# Clear specific node maintenance
POST /api/maintenance/clear/{node_name}
```

#### Hermes Management
```bash
# Get all Hermes instances
GET /api/hermes/instances

# Restart Hermes instance
POST /api/hermes/{name}/restart

# Get Hermes status with uptime
GET /api/hermes/{name}/status

# Restart all Hermes instances
POST /api/hermes/restart-all
```

#### Configuration Management
```bash
# Get all node configurations
GET /api/config/nodes

# Update node configuration
PUT /api/config/nodes/{name}
{
  "snapshots_enabled": true,
  "snapshot_schedule": "0 0 2 * * 0",
  "snapshot_retention_count": 5
}

# Get all Hermes configurations
GET /api/config/hermes

# Get all server configurations
GET /api/config/servers

# Reload configuration
POST /api/config/reload

# Validate configuration
POST /api/config/validate
```

#### System Status
```bash
# Overall system status
GET /api/system/status

# HTTP agent connections status
GET /api/system/agent-connections

# Running operations
GET /api/system/operations

# Health check endpoint
GET /api/system/health

# Test server connectivity
GET /api/system/connectivity
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
7. Send completion notification

### Network-Based Snapshot System with Validator State Preservation
**Features:**
- **Network-Based Naming**: Snapshots named by network (e.g., `pirin-1_20250101_120000`) for cross-node compatibility
- **Cross-Node Recovery**: Any node on the same network can restore from shared network snapshots
- **Validator State Preservation**: Current validator signing state is backed up and restored to prevent double-signing
- **LZ4 Compression**: Fast background compression with good ratios
- **Automatic Backups**: Scheduled network snapshot creation
- **Retention Management**: Configurable cleanup of old network snapshots
- **Long Operation Support**: 24-hour timeout for large snapshots

**Network Snapshot Process:**
1. Start maintenance tracking (24-hour timeout)
2. Stop blockchain service via HTTP agent
3. Create network-named directory: `{network}_{timestamp}`
4. Copy data and wasm directories (excluding validator state)
5. Remove any validator state files from snapshot
6. Start blockchain service via HTTP agent
7. Apply network retention policy if configured
8. Background LZ4 compression (optional)

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

**Auto-Restore System:**
- Monitors `/var/log/{log_path}/out1.log` for trigger words
- Automatically restores from latest network snapshot when corruption detected
- **Preserves current validator state** during auto-restore
- Prevents infinite loops with 2-hour cooldown between attempts
- Sends critical alerts if auto-restore fails

### Log Monitoring System
**Features:**
- **Pattern-Based Detection**: Monitor logs for specific error patterns
- **Context Extraction**: Include configurable lines before/after matches
- **Rate-Limited Alerts**: Same rate limiting as health alerts (0, 6, 12, 24, 48 hours)
- **Healthy Nodes Only**: Only monitors logs when nodes are healthy
- **HTTP Agent Communication**: Each check uses HTTP agent for log access

**Configuration:**
```toml
log_monitoring_enabled = true
log_monitoring_patterns = [
    "Possibly no price is available!",
    "failed to lock fees to pay for"
]
log_monitoring_interval_minutes = 5
log_monitoring_context_lines = 2
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
- Automatic cleanup of expired maintenance windows (25-hour maximum)
- Detection of stuck operations with process monitoring

**Emergency Features:**
- Force kill stuck pruning processes
- Emergency clear all maintenance windows
- Manual maintenance window cleanup per node
- Overdue operation detection (3x estimated duration)

### HTTP Agent Management
**Direct Communication Model:**
- Each operation uses HTTP POST to dedicated agents
- Automatic operation cleanup after completion
- No persistent connection pooling (prevents conflicts)
- Configurable timeouts per server
- Parallel execution across different servers
- Sequential execution on same server for safety

### Timezone Handling
**Important**: All cron schedules run in the timezone where the Node Manager is deployed.

**Time Conversion Example:**
- Local time: 10:00 EEST (UTC+3)
- Config schedule: `"0 0 7 * * 2"` (7:00 AM UTC)
- Result: Runs at 10:00 AM local time

## Cross-Node Recovery Examples

### Example 1: Network Snapshot Creation
```bash
# Create snapshot on node1 - creates network-based snapshot
curl -X POST http://localhost:8095/api/snapshots/pirin-node-1/create

# Result: Creates snapshot named "pirin-1_20250101_120000"
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
curl http://localhost:8095/api/system/status

# Maintenance tracking status
curl http://localhost:8095/api/maintenance/active

# Stuck operation detection
curl http://localhost:8095/api/maintenance/stuck

# Network snapshot statistics
curl http://localhost:8095/api/snapshots/{node_name}/stats
```

### Health Check Endpoint
```bash
curl http://localhost:8095/health
```

### Logs and Troubleshooting
- Health checks run every 90 seconds (configurable)
- Log monitoring runs every 5 minutes (configurable)
- Failed operations are logged with detailed error messages
- HTTP agent connection failures automatically trigger retries
- Database cleanup runs hourly for old records
- Maintenance windows automatically expire after 25 hours
- Network snapshot operations support up to 24-hour timeouts
- Auto-restore attempts have 2-hour cooldown periods
- Cross-node recovery capability eliminates single points of failure

## Security Considerations

- API key authentication for all HTTP agent communications
- Config files may contain sensitive information
- Use firewalls to restrict API access
- Monitor HTTP agent connection limits per server
- Regular security updates for all dependencies
- Network snapshot backup paths should be secured
- Log monitoring may capture sensitive information in context
- Validator state files are never included in shared snapshots

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
- **Log Monitoring**: Monitor disk I/O impact of frequent log reads via HTTP agents
- **Maintenance Windows**: Plan maintenance schedules to avoid conflicts
- **Cross-Node Recovery**: Reduces individual node storage requirements

## Performance

- **Health checks**: 20+ nodes in <5 seconds (parallel execution)
- **HTTP agent operations**: Direct communication per operation
- **Database**: SQLite with indexed queries for fast access
- **Memory usage**: ~50-100MB typical operation (includes network snapshot management)
- **Pruning operations**: 10-300 minutes depending on node size (5-hour timeout)
- **Network snapshot creation**: 30-1440 minutes depending on data size (24-hour timeout)
- **Cross-node restore**: 30-60 minutes depending on snapshot size with validator state preservation
- **Log monitoring**: ~1-5 seconds per node every 5 minutes via HTTP agents
- **LZ4 compression**: 50-200 MB/s typical compression speed

## Configuration Examples

### Complete Node Configuration with Network Snapshots
```toml
[nodes.pirin-mainnet]
# Basic configuration
rpc_url = "http://192.168.1.100:26657"
network = "pirin-1"  # Network name used for snapshot naming
server_host = "mainnet-server"
enabled = true

# Pruning configuration
pruning_enabled = true
pruning_schedule = "0 0 6 * * 2"  # Tuesdays at 6AM
pruning_keep_blocks = 8000
pruning_keep_versions = 8000
pruning_deploy_path = "/opt/deploy/pirin"
pruning_service_name = "pirin"

# Log configuration
log_path = "/var/log/pirin"
truncate_logs_enabled = false

# Network-based snapshot configuration
snapshots_enabled = true
snapshot_backup_path = "/backup/snapshots/pirin-network"  # Shared location for network
auto_restore_enabled = true

# Scheduled network snapshots
snapshot_schedule = "0 0 2 * * 0"  # Weekly on Sunday at 2AM
snapshot_retention_count = 4  # Keep 4 most recent network snapshots
```

### Multiple Nodes Same Network Configuration
```toml
# All nodes on pirin-1 network share the same snapshot location
[nodes.pirin-node-1]
network = "pirin-1"
snapshot_backup_path = "/backup/snapshots/pirin-network"
snapshots_enabled = true  # Can create snapshots

[nodes.pirin-node-2]
network = "pirin-1"
snapshot_backup_path = "/backup/snapshots/pirin-network"
snapshots_enabled = false  # Only restores, doesn't create

[nodes.pirin-node-7]
network = "pirin-1"
snapshot_backup_path = "/backup/snapshots/pirin-network"
auto_restore_enabled = true  # Can auto-restore from network snapshots
```

### Log Monitoring Patterns
```toml
# Common error patterns to monitor
log_monitoring_patterns = [
    # Price feed issues
    "Possibly no price is available!",
    "failed to lock fees to pay for",

    # Consensus issues
    "consensus failure",
    "failed to verify block",
    "invalid block",

    # System issues
    "panic:",
    "out of memory",
    "disk full",
    "database corruption",

    # Network issues
    "connection refused",
    "timeout",
    "network unreachable"
]
```

## API Documentation

When the service is running, comprehensive API documentation is available at:
```
GET /api/docs
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
- API Documentation: `GET /api/docs` when service is running

## Related Projects

- [cosmos-pruner](https://github.com/osmosis-labs/cosmos-pruner) - Blockchain state pruning tool
- [Hermes](https://github.com/informalsystems/hermes) - IBC relayer
- [Cosmos SDK](https://github.com/cosmos/cosmos-sdk) - Blockchain application framework
- [LZ4](https://lz4.github.io/lz4/) - Fast compression algorithm
