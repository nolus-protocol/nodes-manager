# Blockchain Nodes Manager

A comprehensive Rust-based system for managing 20+ blockchain nodes with health monitoring, automated pruning using cosmos-pruner, snapshot management with auto-restore, log monitoring, and Hermes relayer management through a web interface.

## Features

### Core Functionality
- **Health Monitoring**: Real-time RPC status checks with progressive alerting
- **Automated Pruning**: Integration with `cosmos-pruner` tool for efficient blockchain data management
- **Snapshot System**: Create, restore, and manage directory-based blockchain snapshots with auto-restore capability
- **Log Monitoring**: Pattern-based log monitoring with configurable alerts and context extraction
- **Hermes Management**: Smart relayer restarts with dependency validation
- **Web Interface**: Modern dashboard with real-time status and manual operation controls
- **HTTP Agent Management**: Lightweight agents deployed on each server for operation execution
- **Configuration**: Hot-reload capability with multi-server support

### Advanced Capabilities
- **Parallel Operations**: Execute maintenance across multiple servers simultaneously
- **Dependency Validation**: Hermes restarts only when dependent nodes are healthy and synced
- **Scheduled Maintenance**: Cron-based automation with 6-field format support
- **Real-time Monitoring**: Continuous health checks with database persistence
- **Maintenance Tracking**: Track operation status with duration estimates and stuck operation detection
- **Auto-Restore**: Automatically restore from snapshots when corruption patterns detected
- **Scheduled Snapshots**: Automatic snapshot creation with configurable retention policies
- **Emergency Cleanup**: Force cleanup of stuck operations and maintenance windows

### Monitoring Features
- **Block Progression Tracking**: Detect stuck nodes by monitoring block height advancement
- **Per-Node Log Pattern Detection**: Configure individual log monitoring patterns per node
- **Rate-Limited Alerting**: Progressive alerting (immediate, 3h, 6h, 12h, 24h intervals)
- **Maintenance Windows**: Visual indication when nodes are undergoing maintenance
- **Health Recovery Notifications**: Automatic notifications when nodes recover
- **Auto-Restore Cooldown**: Prevent infinite restore loops with 2-hour cooldown periods

## Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Web Dashboard │    │  Health Monitor │    │ Maintenance     │
│   (Axum + API)  │    │  (RPC Polling)  │    │ Scheduler       │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
         ┌───────────────────────────────────────────────────────────┐
         │              Manager Service (Central Hub)                │
         │  ┌─────────────┐  ┌─────────────┐  ┌──────────┐  ┌──────┐│
         │  │Config Mgmt  │  │  Database   │  │HTTP Mgmt │  │ Log  ││
         │  │(Hot Reload) │  │  (SQLite)   │  │(Agent    │  │Monitor││
         │  │             │  │             │  │Comm)     │  │      ││
         │  └─────────────┘  └─────────────┘  └──────────┘  └──────┘│
         │  ┌─────────────┐  ┌─────────────┐  ┌──────────┐         │
         │  │Maintenance  │  │  Snapshot   │  │ Auto     │         │
         │  │Tracker      │  │  Manager    │  │ Restore  │         │
         │  │             │  │  (Directory)│  │          │         │
         │  └─────────────┘  └─────────────┘  └──────────┘         │
         └───────────────────────────────────────────────────────────┘
                                 │
         ┌───────────────────────────────────────────────────────────┐
         │            Blockchain Infrastructure                      │
         │                                                           │
         │  Server 1         Server 2         Server 3              │
         │  ┌─────────┐      ┌─────────┐      ┌─────────┐            │
         │  │HTTP Agent│      │HTTP Agent│      │HTTP Agent│           │
         │  │:8745     │      │:8745     │      │:8745     │           │
         │  └─────────┘      └─────────┘      └─────────┘            │
         │  ┌─────────┐      ┌─────────┐      ┌─────────┐            │
         │  │Cosmos   │      │Cosmos   │      │Hermes   │            │
         │  │Nodes    │      │Nodes    │      │Relayers │            │
         │  └─────────┘      └─────────┘      └─────────┘            │
         └───────────────────────────────────────────────────────────┘
```

## Installation

### Prerequisites
- Rust 1.70+
- `cosmos-pruner` tool installed on target servers
- HTTP agent deployed on all blockchain servers
- SQLite3
- `lz4` tool installed on target servers (for background compression)

### Build & Setup
```bash
# Clone repository
git clone https://github.com/nolus-protocol/nodes-manager.git
cd nodes-manager

# Build manager service
cargo build --release --bin manager

# Build agent service
cargo build --release --bin agent

# Create required directories
mkdir -p data static config

# Set up configuration files
mkdir -p config
```

## Configuration

### Main Configuration
Create `config/main.toml`:
```toml
host = "0.0.0.0"
port = 8095
check_interval_seconds = 90
rpc_timeout_seconds = 10
alarm_webhook_url = "http://your-webhook-endpoint/alert"
hermes_min_uptime_minutes = 5

# Auto-restore trigger words (optional)
auto_restore_trigger_words = [
    "AppHash",
    "wrong Block.Header.AppHash",
    "database corruption",
    "state sync failed"
]

# Auto-restore trigger words (global setting)
auto_restore_trigger_words = [
    "AppHash",
    "wrong Block.Header.AppHash",
    "database corruption",
    "state sync failed"
]
```

### Server Configuration Example
Create files like `config/server1.toml`:
```toml
[server]
host = "192.168.1.100"
agent_port = 8745
api_key = "your-secure-api-key-here"
request_timeout_seconds = 300

[nodes.osmosis-1]
rpc_url = "http://192.168.1.100:26657"
network = "osmosis-1"
server_host = "server1"
enabled = true

# Pruning configuration
pruning_enabled = true
pruning_schedule = "0 0 6 * * 2"  # Tuesdays at 6AM (6-field cron)
pruning_keep_blocks = 8000
pruning_keep_versions = 8000
pruning_deploy_path = "/opt/deploy/osmosis"
pruning_service_name = "osmosis"

# Log configuration with per-node monitoring
log_path = "/var/log/osmosis"
truncate_logs_enabled = false
log_monitoring_enabled = true
log_monitoring_patterns = [
    "Possibly no price is available!",
    "failed to lock fees to pay for",
    "consensus failure",
    "panic:"
]
log_monitoring_context_lines = 3

# Per-node log monitoring (optional)
log_monitoring_enabled = true
log_monitoring_patterns = [
    "Possibly no price is available!",
    "failed to lock fees to pay for",
    "consensus failure"
]
log_monitoring_context_lines = 2

# Snapshot configuration
snapshots_enabled = true
snapshot_backup_path = "/backup/snapshots/osmosis"
auto_restore_enabled = true

# Scheduled snapshots
snapshot_schedule = "0 0 2 * * 0"  # Sundays at 2AM (6-field cron)
snapshot_retention_count = 7  # Keep 7 most recent snapshots

[hermes.relay-server1]
server_host = "server1"
service_name = "hermes"
log_path = "/var/log/hermes"
restart_schedule = "0 0 16 * * 2"  # Tuesdays at 4PM (6-field cron)
dependent_nodes = ["server1-osmosis-1", "server1-neutron-1"]
```

## Deployment

### 1. Deploy HTTP Agents
On each blockchain server:
```bash
# Copy agent binary
scp target/release/agent user@server:/usr/local/bin/

# Set API key environment variable
export AGENT_API_KEY="your-secure-api-key-here"

# Start agent service
/usr/local/bin/agent
```

### 2. Start Manager Service
On the management server:
```bash
# Start manager service
./target/release/manager
```

## Usage

### Web Interface
Access the dashboard at `http://localhost:8095` for:
- Real-time node health status
- Manual operation triggers (pruning, snapshots, restarts)
- Maintenance status visualization
- Operation tracking and cancellation

### API Endpoints

#### Health Monitoring
```bash
# Get all nodes health with status details
GET /api/health/nodes

# Get specific node health
GET /api/health/nodes/{node_name}

# Get all Hermes instances
GET /api/health/hermes

# Get specific Hermes instance
GET /api/health/hermes/{hermes_name}
```

#### Manual Operations
```bash
# Execute manual node pruning
POST /api/maintenance/nodes/{node_name}/prune

# Create manual snapshot
POST /api/snapshots/{node_name}/create

# Restore from latest snapshot
POST /api/snapshots/{node_name}/restore

# Restart Hermes instance
POST /api/maintenance/hermes/{hermes_name}/restart
```

#### Snapshot Management
```bash
# List all snapshots for a node
GET /api/snapshots/{node_name}/list

# Get snapshot statistics
GET /api/snapshots/{node_name}/stats

# Delete specific snapshot
DELETE /api/snapshots/{node_name}/{filename}

# Cleanup old snapshots (keep N most recent)
POST /api/snapshots/{node_name}/cleanup?retention_count=5

# Check auto-restore triggers
GET /api/snapshots/{node_name}/check-triggers

# Get auto-restore status
GET /api/snapshots/{node_name}/auto-restore-status
```

#### Operation Management
```bash
# Get active operations
GET /api/operations/active

# Cancel specific operation
POST /api/operations/{target_name}/cancel

# Check target status
GET /api/operations/{target_name}/status

# Emergency cleanup old operations
POST /api/operations/emergency-cleanup?max_hours=12
```

#### Configuration
```bash
# Get all node configurations
GET /api/config/nodes

# Get all Hermes configurations
GET /api/config/hermes
```

## Key Features in Detail

### Directory-Based Snapshot System
**Process:**
1. Stop blockchain service
2. Create timestamped directory: `{node_name}_{YYYYMMDD_HHMMSS}`
3. Copy `data` and `wasm` directories to snapshot directory
4. Backup validator state (`priv_validator_state.json`)
5. Restart blockchain service
6. Spawn background LZ4 compression task
7. Apply retention policy if configured

**Auto-Restore System:**
- Monitors logs of **unhealthy nodes only** for corruption triggers
- Restores from latest snapshot directory when triggers detected
- 2-hour cooldown between restore attempts
- Only checks each node once per unhealthy period
- Skips nodes in maintenance mode entirely

### Health Monitoring with Progressive Alerting
**Node States:**
- **Synced**: Healthy and up-to-date
- **Catching Up**: Syncing (normal operation)
- **Unhealthy**: RPC failure or stuck blocks
- **Maintenance**: Operations in progress

**Alert Progression:**
- Immediate: Node becomes unhealthy
- 3 hours: Still unhealthy
- 6 hours: Extended outage
- 12 hours: Critical state
- 24+ hours: Repeat every 24 hours

### Maintenance Coordination
**Features:**
- Prevents health checks during maintenance operations
- Visual indicators in web interface
- Automatic cleanup of stuck operations (24-48 hour timeouts)
- Emergency cleanup capabilities
- Operation conflict prevention (one operation per target)

### Scheduled Operations (6-Field Cron)
**Format:** `second minute hour day month dayofweek`
**Examples:**
- `0 0 6 * * 2` - Tuesdays at 6:00 AM
- `0 30 14 * * 1,3,5` - Monday, Wednesday, Friday at 2:30 PM
- `0 0 2 * * 0` - Sundays at 2:00 AM

### HTTP Agent Communication
**Features:**
- Lightweight agents (port 8745) on each server
- API key authentication
- No timeout limits for long operations
- Concurrent operations across different servers
- Sequential operations per server for safety

## Monitoring & Debugging

### System Status
```bash
# Overall system health
curl http://localhost:8095/api/health/nodes

# Active operations
curl http://localhost:8095/api/operations/active

# Snapshot statistics
curl http://localhost:8095/api/snapshots/{node_name}/stats
```

### Log Monitoring
- Monitors **healthy nodes only** every 5 minutes
- Extracts configurable context lines around pattern matches
- Rate-limited alerts prevent spam
- Fresh HTTP connections for each check

### Troubleshooting
- Health checks every 90 seconds (configurable)
- Block progression tracking detects stuck nodes
- Maintenance windows prevent health check conflicts
- Auto-restore has 2-hour cooldown periods
- Emergency cleanup for stuck operations

## Security Considerations

- Secure API keys for agent communication
- Firewall restrictions for agent ports (8745)
- Config files contain sensitive information
- Webhook URL security for notifications
- Regular security updates for dependencies

## Performance Characteristics

- **Health Checks**: 20+ nodes in ~5 seconds (parallel execution)
- **Agent Communication**: Direct HTTP, no connection pooling overhead
- **Database**: SQLite with indexed queries, ~10-50MB typical size
- **Memory Usage**: ~50-100MB typical operation
- **Snapshot Creation**: 30-1440 minutes (24-hour timeout)
- **Pruning Operations**: 10-300 minutes (5-hour timeout)
- **Background Compression**: LZ4 at 50-200 MB/s

## Configuration Examples

### Complete Node Setup
```toml
[nodes.osmosis-mainnet]
rpc_url = "http://192.168.1.100:26657"
network = "osmosis-1"
server_host = "mainnet-server"
enabled = true

# Pruning (6-field cron format)
pruning_enabled = true
pruning_schedule = "0 0 6 * * 2"  # Tuesdays at 6AM
pruning_keep_blocks = 8000
pruning_keep_versions = 8000
pruning_deploy_path = "/opt/deploy/osmosis"
pruning_service_name = "osmosis"

# Snapshots (6-field cron format)
snapshots_enabled = true
snapshot_backup_path = "/backup/snapshots/osmosis"
auto_restore_enabled = true
snapshot_schedule = "0 0 2 * * 0"  # Sundays at 2AM
snapshot_retention_count = 4

# Logging
log_path = "/var/log/osmosis"
truncate_logs_enabled = false
```

### Agent Deployment
```bash
# On each blockchain server
export AGENT_API_KEY="your-secure-key"
/usr/local/bin/agent

# Agent will listen on :8745
# Manager connects via HTTP with API key authentication
```

## Architecture Benefits

- **Reliability**: Operation conflict prevention, maintenance coordination
- **Scalability**: Parallel operations, efficient HTTP communication
- **Safety**: Auto-restore with cooldowns, progressive alerting
- **Visibility**: Real-time dashboard, comprehensive API
- **Automation**: Scheduled operations, retention management
- **Recovery**: Directory-based snapshots, corruption detection

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Support

- GitHub Issues: [Create an issue](https://github.com/nolus-protocol/nodes-manager/issues)
- Web Dashboard: Real-time system status and operations

## Related Projects

- [cosmos-pruner](https://github.com/osmosis-labs/cosmos-pruner) - Blockchain state pruning tool
- [Hermes](https://github.com/informalsystems/hermes) - IBC relayer implementation
- [LZ4](https://lz4.github.io/lz4/) - Fast compression algorithm
