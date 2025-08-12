# Blockchain Nodes Manager

A robust, production-ready Rust application for managing 20+ blockchain nodes with automated health monitoring, scheduled maintenance, and Hermes relayer management through a comprehensive web interface and REST API.

## 🌟 Features

- **🔍 Real-time Health Monitoring** - Continuous monitoring of blockchain nodes via RPC calls
- **⚙️ Automated Maintenance** - Scheduled pruning operations with configurable parameters
- **🔗 Hermes Relayer Management** - Automated restart and dependency management for IBC relayers
- **🖥️ Multi-Server Support** - Manage nodes across multiple servers with SSH connection pooling
- **📊 Web Dashboard** - Clean web interface with real-time status updates
- **🛡️ Concurrent Safety** - Server-specific connection limits and parallel operations
- **📈 Health Metrics** - Comprehensive node health tracking with historical data
- **🔧 Hot Configuration Reload** - Update configurations without service restart
- **📋 Comprehensive Logging** - Structured logging with configurable levels
- **🎯 Batch Operations** - Execute operations across multiple nodes simultaneously

## 🏗️ Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Web Interface │    │  Health Monitor │    │ Maintenance     │
│   (Axum + HTML)│    │  (1-2 min cycle)│    │ Scheduler       │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
         ┌─────────────────────────────────────────────────┐
         │              Core Engine                        │
         │  ┌─────────────┐  ┌─────────────┐  ┌──────────┐│
         │  │Config Mgmt  │  │  Database   │  │SSH Client││
         │  └─────────────┘  └─────────────┘  └──────────┘│
         └─────────────────────────────────────────────────┘
                                 │
         ┌─────────────────────────────────────────────────┐
         │              External Systems                   │
         │  ┌─────────────┐  ┌─────────────┐  ┌──────────┐│
         │  │Blockchain   │  │   n8n       │  │ Remote   ││
         │  │RPC Nodes    │  │(Webhooks)   │  │ Servers  ││
         │  └─────────────┘  └─────────────┘  └──────────┘│
         └─────────────────────────────────────────────────┘
```

## 🚀 Quick Start

### Prerequisites

- Rust 1.70+
- SSH access to blockchain node servers
- SQLite (automatically managed)

### Installation

1. **Clone the repository**
   ```bash
   git clone https://github.com/your-username/blockchain-nodes-manager.git
   cd blockchain-nodes-manager
   ```

2. **Build the application**
   ```bash
   cargo build --release
   ```

3. **Set up configuration**
   ```bash
   mkdir -p config
   cp config/main.toml.example config/main.toml
   cp config/server.toml.example config/discovery.toml
   # Edit configuration files as needed
   ```

4. **Run the application**
   ```bash
   ./target/release/nodes-manager
   ```

5. **Access the web interface**
   ```
   http://localhost:8095
   ```

## ⚙️ Configuration

### Main Configuration (`config/main.toml`)

```toml
# Server settings
host = "0.0.0.0"
port = 8095

# Monitoring settings
check_interval_seconds = 90
rpc_timeout_seconds = 10
alarm_webhook_url = "http://your-n8n-instance/webhook/node-alarm"

# Hermes settings
hermes_min_uptime_minutes = 5
```

### Server Configuration (`config/server_name.toml`)

```toml
[server]
host = "192.168.1.10"
ssh_key_path = "/path/to/ssh-key"
ssh_username = "root"
max_concurrent_ssh = 5
ssh_timeout_seconds = 30

[nodes.node-name]
rpc_url = "http://192.168.1.10:26657"
network = "osmosis-1"
server_host = "server_name"
enabled = true
pruning_enabled = true
pruning_schedule = "0 0 12 * * 2"  # Tuesdays at 12:00
pruning_keep_blocks = 8000
pruning_keep_versions = 8000
pruning_deploy_path = "/opt/deploy/osmosis"
pruning_service_name = "osmosis"

[hermes.relay-name]
server_host = "server_name"
service_name = "hermes"
log_path = "/var/log/hermes"
restart_schedule = "0 0 16 * * 2"  # Tuesdays at 16:00
dependent_nodes = ["node-name"]
```

## 📡 API Reference

### Health Monitoring

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/nodes/health` | Get health status of all nodes |
| `GET` | `/api/nodes/{name}/health` | Get specific node health |
| `GET` | `/api/nodes/{name}/history?limit=50` | Get node health history |
| `POST` | `/api/nodes/{name}/check` | Force immediate health check |

### Maintenance Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/maintenance/schedule` | Get all scheduled operations |
| `POST` | `/api/maintenance/pruning` | Schedule node pruning |
| `POST` | `/api/maintenance/hermes-restart` | Schedule Hermes restart |
| `DELETE` | `/api/maintenance/{id}` | Cancel scheduled operation |
| `POST` | `/api/maintenance/run-now` | Execute operation immediately |
| `GET` | `/api/maintenance/logs?limit=100` | Get maintenance logs |
| `POST` | `/api/maintenance/prune-multiple` | Batch pruning operations |
| `POST` | `/api/maintenance/restart-multiple` | Batch Hermes restarts |
| `GET` | `/api/maintenance/status/{operation_id}` | Get operation status |
| `GET` | `/api/maintenance/summary` | Get operations summary |

### Hermes Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/hermes/instances` | Get all Hermes instances |
| `POST` | `/api/hermes/{name}/restart` | Restart specific Hermes instance |
| `GET` | `/api/hermes/{name}/status` | Get Hermes instance status |
| `POST` | `/api/hermes/restart-all` | Restart all Hermes instances |

### Configuration Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/config/nodes` | Get all node configurations |
| `PUT` | `/api/config/nodes/{name}` | Update node configuration |
| `GET` | `/api/config/hermes` | Get Hermes configurations |
| `GET` | `/api/config/servers` | Get server configurations |
| `POST` | `/api/config/reload` | Hot reload configurations |
| `POST` | `/api/config/validate` | Validate configuration |
| `GET` | `/api/config/files` | List configuration files |

### System Status

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/system/status` | Overall system health |
| `GET` | `/api/system/ssh-connections` | SSH connection pool status |
| `GET` | `/api/system/operations` | Running operations |
| `GET` | `/api/system/health` | Service health check |
| `GET` | `/api/system/connectivity` | Test server connectivity |
| `GET` | `/api/system/services` | All service statuses |

### Utility Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/docs` | API documentation |
| `GET` | `/api/version` | Version information |
| `GET` | `/health` | Simple health check |

## 🛠️ Production Deployment

Simply build and run the binary in your preferred environment:

```bash
# Build for production
cargo build --release

# Run with custom configuration
RUST_LOG=info ./target/release/nodes-manager

# Or run in background
nohup ./target/release/nodes-manager > /var/log/nodes-manager.log 2>&1 &
```

## 📊 Usage Examples

### Batch Operations

```bash
# Prune multiple nodes
curl -X POST http://localhost:8095/api/maintenance/prune-multiple \
  -H "Content-Type: application/json" \
  -d '{"node_names": ["node1", "node2", "node3"]}'

# Restart all Hermes instances
curl -X POST http://localhost:8095/api/hermes/restart-all

# Check system status
curl http://localhost:8095/api/system/status | jq
```

### Scheduling Operations

```bash
# Schedule weekly pruning
curl -X POST http://localhost:8095/api/maintenance/pruning \
  -H "Content-Type: application/json" \
  -d '{
    "operation_type": "pruning",
    "target_name": "osmosis-node",
    "schedule": "0 0 12 * * 1"
  }'
```

### Monitoring

```bash
# Get all node health
curl http://localhost:8095/api/nodes/health | jq

# Get specific node history
curl "http://localhost:8095/api/nodes/osmosis-1/history?limit=10" | jq
```

## 🔧 Development

### Building from Source

```bash
# Development build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Check code formatting
cargo fmt --check

# Run clippy lints
cargo clippy
```

### Project Structure

```
blockchain-nodes-manager/
├── src/
│   ├── config/         # Configuration management
│   ├── database.rs     # SQLite database operations
│   ├── health/         # Health monitoring system
│   ├── scheduler/      # Maintenance scheduling
│   ├── ssh/           # SSH connection management
│   ├── web/           # Web server and API handlers
│   └── main.rs        # Application entry point
├── config/            # Configuration files
├── static/            # Web interface assets
└── data/             # Runtime data (databases, logs)
```

## 🚨 Alarm System

The system can send webhooks to external systems (like n8n) when nodes become unhealthy:

```json
{
  "timestamp": "2025-08-11T10:00:00Z",
  "alarm_type": "node_health",
  "severity": "high",
  "node_name": "osmosis-mainnet-1",
  "message": "Node is not responding",
  "details": {
    "current_block": 15228588,
    "catching_up": true,
    "network": "osmosis-1"
  }
}
```

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Code Style

- Follow Rust standard formatting (`cargo fmt`)
- Run clippy lints (`cargo clippy`)
- Add tests for new functionality
- Update documentation for API changes

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- Built with [Axum](https://github.com/tokio-rs/axum) web framework
- Uses [SQLx](https://github.com/launchbadge/sqlx) for database operations
- SSH operations powered by [async-ssh2-tokio](https://github.com/TatriX/async-ssh2-tokio)
- Scheduling with [tokio-cron-scheduler](https://github.com/mvniekerk/tokio-cron-scheduler)

## 📞 Support

- Create an [Issue](https://github.com/your-username/blockchain-nodes-manager/issues) for bug reports
- Start a [Discussion](https://github.com/your-username/blockchain-nodes-manager/discussions) for questions
- Check the [Wiki](https://github.com/your-username/blockchain-nodes-manager/wiki) for detailed guides

---

**Made with ❤️ for the blockchain infrastructure community**
