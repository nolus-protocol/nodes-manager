# Blockchain Nodes Manager

A comprehensive Rust-based system for managing 20+ blockchain nodes with health monitoring, automated pruning using cosmos-pruner, snapshot management with auto-restore, log monitoring, and Hermes relayer management through a web interface.

## Features

### Core Functionality
- **Health Monitoring**: Real-time RPC status checks with configurable intervals
- **Automated Pruning**: Integration with `cosmos-pruner` tool for efficient blockchain data management
- **Snapshot System**: Create, restore, and manage LZ4-compressed blockchain snapshots with auto-restore capability
- **Log Monitoring**: Pattern-based log monitoring with configurable alerts and context extraction
- **Hermes Management**: Smart relayer restarts with RPC-based dependency validation
- **Web Interface**: RESTful API with comprehensive endpoints for all operations
- **SSH Management**: Fresh connection per operation with automatic cleanup
- **Configuration**: Hot-reload capability with multi-server support

### Advanced Capabilities
- **Parallel Operations**: Execute maintenance across multiple servers simultaneously
- **Dependency Validation**: Hermes restarts only when dependent nodes are healthy and synced
- **Scheduled Maintenance**: Cron-based automation with timezone awareness
- **Real-time Monitoring**: Continuous health checks with database persistence
- **Batch Operations**: Execute pruning/restarts across multiple nodes efficiently
- **Maintenance Tracking**: Track operation status with duration estimates and stuck operation detection
- **Auto-Restore**: Automatically restore from snapshots when corruption patterns detected
- **Scheduled Snapshots**: Automatic snapshot creation with configurable retention policies
- **Emergency Cleanup**: Force cleanup of stuck operations and maintenance windows

### Monitoring Features
- **Process Monitoring**: Detect stuck pruning processes and silent failures
- **Log Pattern Detection**: Monitor logs for specific error patterns with context extraction
- **Rate-Limited Alerting**: Prevent alarm spam with configurable rate limiting
- **Maintenance Windows**: Visual indication when nodes are undergoing maintenance
- **Health Recovery Notifications**: Automatic notifications when nodes recover

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
         │  │Config Mgmt  │  │  Database   │  │SSH Mgmt  │  │ Log  ││
         │  │(Hot Reload) │  │  (SQLite)   │  │(Fresh    │  │Monitor││
         │  │             │  │             │  │Conn)     │  │      ││
         │  └─────────────┘  └─────────────┘  └──────────┘  └──────┘│
         │  ┌─────────────┐  ┌─────────────┐  ┌──────────┐         │
         │  │Maintenance  │  │  Snapshot   │  │ Auto     │         │
         │  │Tracker      │  │  Manager    │  │ Restore  │         │
         │  │             │  │  (LZ4)      │  │          │         │
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
- SSH access to all blockchain servers
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

# Ensure SSH keys have correct permissions
chmod 600 /path/to/your/ssh/keys
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
ssh_key_path = "/path/to/ssh/key"
ssh_username = "root"
max_concurrent_ssh = 5
ssh_timeout_seconds = 30

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

# Snapshot configuration (optional)
snapshots_enabled = true
snapshot_backup_path = "/backup/snapshots/osmosis"
auto_restore_enabled = true

# Scheduled snapshots (optional)
snapshot_schedule = "0 0 2 * * 0"  # Sundays at 2AM UTC
snapshot_retention_count = 7  # Keep 7 most recent snapshots

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

#### Snapshot Management
```bash
# Create manual snapshot (LZ4 compressed)
POST /api/snapshots/{node_name}/create

# List all snapshots for a node
GET /api/snapshots/{node_name}/list

# Restore from latest snapshot
POST /api/snapshots/{node_name}/restore

# Delete specific snapshot
DELETE /api/snapshots/{node_name}/{filename}

# Get snapshot statistics
GET /api/snapshots/{node_name}/stats

# Check auto-restore triggers
POST /api/snapshots/{node_name}/check-restore

# Cleanup old snapshots (keep N most recent)
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

# Execute immediate snapshot creation
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

# Schedule snapshot creation
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

# SSH connections status
GET /api/system/ssh-connections

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
2. Stop blockchain service
3. Optional: Truncate logs if enabled
4. Execute cosmos-pruner with configured parameters
5. Start blockchain service
6. Verify service health
7. Send completion notification

### Snapshot System with LZ4 Compression
**Features:**
- **LZ4 Compression**: Fast compression with good ratios
- **Automatic Backups**: Scheduled snapshot creation
- **Retention Management**: Configurable cleanup of old snapshots
- **Validator State Preservation**: Backs up and restores validator state separately
- **Long Operation Support**: 24-hour timeout for large snapshots

**Snapshot Process:**
1. Start maintenance tracking (24-hour timeout)
2. Stop blockchain service
3. Backup current validator state
4. Create LZ4-compressed archive: `tar -cf - data wasm | lz4 -z -c > snapshot.lz4`
5. Restart blockchain service
6. Apply retention policy if configured

**Auto-Restore System:**
- Monitors `/var/log/{log_path}/out1.log` for trigger words
- Automatically restores from latest snapshot when corruption detected
- Prevents infinite loops with 2-hour cooldown between attempts
- Sends critical alerts if auto-restore fails

### Log Monitoring System
**Features:**
- **Pattern-Based Detection**: Monitor logs for specific error patterns
- **Context Extraction**: Include configurable lines before/after matches
- **Rate-Limited Alerts**: Same rate limiting as health alerts (0, 6, 12, 24, 48 hours)
- **Healthy Nodes Only**: Only monitors logs when nodes are healthy
- **Fresh SSH Connections**: Each check uses independent connection

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

### SSH Management
**Fresh Connection Model:**
- Each operation uses a dedicated SSH connection
- Automatic connection cleanup after operation
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

## Monitoring & Debugging

### System Status
```bash
# Overall system status with snapshot info
curl http://localhost:8095/api/system/status

# Maintenance tracking status
curl http://localhost:8095/api/maintenance/active

# Stuck operation detection
curl http://localhost:8095/api/maintenance/stuck

# Snapshot statistics
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
- SSH connection failures automatically trigger fresh connections
- Database cleanup runs hourly for old records
- Maintenance windows automatically expire after 25 hours
- Snapshot operations support up to 24-hour timeouts
- Auto-restore attempts have 2-hour cooldown periods

## Security Considerations

- SSH key permissions: `chmod 600 /path/to/keys`
- Config files may contain sensitive information
- Use firewalls to restrict API access
- Monitor SSH connection limits per server
- Regular security updates for all dependencies
- Snapshot backup paths should be secured
- Log monitoring may capture sensitive information in context

## Production Deployment

### Backup Strategy
- **Database**: Regular backups of `data/nodes.db`
- **Configuration**: Backup `config/*.toml` files separately
- **Snapshots**: Configure separate backup storage for snapshots
- **Log Rotation**: Set up log rotation for maintenance logs

### Storage Requirements
- **Database**: ~10-50MB for typical deployments
- **Logs**: Variable based on retention policies
- **Snapshots**: Can be very large (GBs to TBs depending on blockchain data)

### Performance Considerations
- **LZ4 Compression**: Faster than gzip, good balance of speed/compression
- **Snapshot Retention**: Configure appropriate retention counts to manage disk usage
- **Log Monitoring**: Monitor disk I/O impact of frequent log reads
- **Maintenance Windows**: Plan maintenance schedules to avoid conflicts

## Performance

- **Health checks**: 20+ nodes in <5 seconds (parallel execution)
- **SSH connections**: Fresh connection per operation (no pooling overhead)
- **Database**: SQLite with indexed queries for fast access
- **Memory usage**: ~50-100MB typical operation (includes snapshot management)
- **Pruning operations**: 10-300 minutes depending on node size (5-hour timeout)
- **Snapshot creation**: 30-1440 minutes depending on data size (24-hour timeout)
- **Log monitoring**: ~1-5 seconds per node every 5 minutes
- **LZ4 compression**: 50-200 MB/s typical compression speed

## Configuration Examples

### Complete Node Configuration
```toml
[nodes.osmosis-mainnet]
# Basic configuration
rpc_url = "http://192.168.1.100:26657"
network = "osmosis-1"
server_host = "mainnet-server"
enabled = true

# Pruning configuration
pruning_enabled = true
pruning_schedule = "0 0 6 * * 2"  # Tuesdays at 6AM
pruning_keep_blocks = 8000
pruning_keep_versions = 8000
pruning_deploy_path = "/opt/deploy/osmosis"
pruning_service_name = "osmosis"

# Log configuration
log_path = "/var/log/osmosis"
truncate_logs_enabled = false

# Snapshot configuration
snapshots_enabled = true
snapshot_backup_path = "/backup/snapshots/osmosis"
auto_restore_enabled = true

# Scheduled snapshots
snapshot_schedule = "0 0 2 * * 0"  # Weekly on Sunday at 2AM
snapshot_retention_count = 4  # Keep 4 most recent snapshots
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
