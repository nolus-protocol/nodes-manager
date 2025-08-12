# Blockchain Nodes Manager

A comprehensive Rust-based system for managing 20+ blockchain nodes with health monitoring, automated pruning using cosmos-pruner, and Hermes relayer management through a web interface.

## 🚀 Features

### Core Functionality
- **Health Monitoring**: Real-time RPC status checks with configurable intervals
- **Automated Pruning**: Integration with `cosmos-pruner` tool for efficient blockchain data management
- **Hermes Management**: Smart relayer restarts with RPC-based dependency validation
- **Web Interface**: RESTful API with comprehensive endpoints for all operations
- **SSH Management**: Async connection pooling with per-server concurrency limits
- **Configuration**: Hot-reload capability with multi-server support

### Advanced Capabilities
- **Parallel Operations**: Execute maintenance across multiple servers simultaneously
- **Dependency Validation**: Hermes restarts only when dependent nodes are healthy and synced
- **Scheduled Maintenance**: Cron-based automation with timezone awareness
- **Real-time Monitoring**: Continuous health checks with database persistence
- **Batch Operations**: Execute pruning/restarts across multiple nodes efficiently

## 🏗️ Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Web Interface │    │  Health Monitor │    │ Maintenance     │
│   (Axum + API)  │    │  (RPC Polling)  │    │ Scheduler       │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
         ┌─────────────────────────────────────────────────┐
         │              Core Engine                        │
         │  ┌─────────────┐  ┌─────────────┐  ┌──────────┐│
         │  │Config Mgmt  │  │  Database   │  │SSH Pool  ││
         │  │(Hot Reload) │  │  (SQLite)   │  │Manager   ││
         │  └─────────────┘  └─────────────┘  └──────────┘│
         └─────────────────────────────────────────────────┘
                                 │
         ┌─────────────────────────────────────────────────┐
         │            Blockchain Infrastructure            │
         │  ┌─────────────┐  ┌─────────────┐  ┌──────────┐│
         │  │   Cosmos    │  │   Hermes    │  │ Remote   ││
         │  │   Nodes     │  │  Relayers   │  │ Servers  ││
         │  └─────────────┘  └─────────────┘  └──────────┘│
         └─────────────────────────────────────────────────┘
```

## 🛠️ Installation

### Prerequisites
- Rust 1.70+
- `cosmos-pruner` tool installed on target servers
- SSH access to all blockchain servers
- SQLite3

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

## ⚙️ Configuration

### Main Configuration
Create `config/main.toml` with:
```toml
host = "0.0.0.0"
port = 8095
check_interval_seconds = 90
rpc_timeout_seconds = 10
alarm_webhook_url = "http://your-n8n-instance/webhook/node-alarm"
hermes_min_uptime_minutes = 5
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
pruning_enabled = true
pruning_schedule = "0 0 6 * * 2"  # Tuesdays at 6AM UTC
pruning_keep_blocks = 8000
pruning_keep_versions = 8000
pruning_deploy_path = "/opt/deploy/osmosis/data"
pruning_service_name = "osmosis"

[hermes.relay-discovery]
server_host = "discovery"
service_name = "hermes"
log_path = "/var/log/hermes"
restart_schedule = "0 0 16 * * 2"  # Tuesdays at 4PM UTC
dependent_nodes = ["discovery-osmosis-1", "discovery-neutron-1"]
```

## 🚀 Usage

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

# Force health check
POST /api/nodes/{name}/check
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

# Batch pruning multiple nodes
POST /api/maintenance/prune-multiple
{
  "node_names": ["discovery-osmosis-1", "enterprise-neutron-1"]
}

# Get maintenance logs
GET /api/maintenance/logs
```

#### Hermes Management
```bash
# Restart Hermes instance
POST /api/hermes/{name}/restart

# Restart all Hermes instances
POST /api/hermes/restart-all

# Get Hermes status
GET /api/hermes/{name}/status
```

#### Configuration Management
```bash
# Reload configuration
POST /api/config/reload

# Validate configuration
POST /api/config/validate

# Update node configuration
PUT /api/config/nodes/{name}
```

## 🔧 Key Features in Detail

### Pruning with cosmos-pruner
The system uses the `cosmos-pruner` tool instead of custom scripts:
```bash
cosmos-pruner prune /opt/deploy/osmosis/data --blocks=8000 --versions=8000
```

**Process:**
1. Stop blockchain service
2. Execute cosmos-pruner with configured parameters
3. Restart blockchain service
4. Verify service health

### Intelligent Hermes Restart
Hermes relayers restart only when ALL dependent nodes are:
- ✅ **Healthy** (RPC status check passes)
- ✅ **Synced** (not catching up)
- ✅ **Recent** (health data less than 5 minutes old)

### Async SSH Operations
- **Connection pooling** per server
- **Configurable concurrency limits** (3-5 connections per server)
- **Parallel execution** across different servers
- **Sequential execution** on same server (prevents conflicts)

### Timezone Handling
⚠️ **Important**: All cron schedules run in the timezone where the Node Manager is deployed.

**Time Conversion Example:**
- Local time: 10:00 EEST (UTC+3)
- Config schedule: `"0 0 7 * * 2"` (7:00 AM UTC)
- Result: Runs at 10:00 AM local time

## 🔍 Monitoring & Debugging

### System Status
```bash
# Overall system status
curl http://localhost:8095/api/system/status

# SSH connections status
curl http://localhost:8095/api/system/ssh-connections

# Running operations
curl http://localhost:8095/api/system/operations
```

### Health Check Endpoint
```bash
curl http://localhost:8095/health
```

### Logs and Troubleshooting
- Health checks run every 90 seconds (configurable)
- Failed operations are logged with detailed error messages
- SSH connection failures automatically trigger reconnection
- Database cleanup runs hourly for old records

## 🔐 Security Considerations

- SSH key permissions: `chmod 600 /path/to/keys`
- Config files may contain sensitive information
- Use firewalls to restrict API access
- Monitor SSH connection limits per server
- Regular security updates for all dependencies

## 🛡️ Production Deployment

### Backup Strategy
- Regular database backups: `data/nodes.db`
- Configuration backup: `config/*.toml` (keep separate secure copies)
- Log rotation for maintenance logs

## 📊 Performance

- **Health checks**: 20+ nodes in <5 seconds (parallel execution)
- **SSH connections**: Pooled and reused (configurable limits)
- **Database**: SQLite with indexed queries for fast access
- **Memory usage**: ~20-50MB typical operation
- **Pruning operations**: Depends on node size (typically 10-30 minutes)

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## 📄 License

MIT License - see [LICENSE](LICENSE) file for details.

## 🆘 Support

For issues, questions, or contributions:
- GitHub Issues: [Create an issue](https://github.com/nolus-protocol/nodes-manager/issues)
- API Documentation: `GET /api/docs` when service is running

## 🔗 Related Projects

- [cosmos-pruner](https://github.com/osmosis-labs/cosmos-pruner) - Blockchain state pruning tool
- [Hermes](https://github.com/informalsystems/hermes) - IBC relayer
- [Cosmos SDK](https://github.com/cosmos/cosmos-sdk) - Blockchain application framework
