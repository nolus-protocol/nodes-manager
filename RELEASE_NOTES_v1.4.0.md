# Release v1.4.0 - State Sync Bug Fixes & UI Improvements

## 🎯 Critical Bug Fixes

### Agent
- **Fixed: State sync never completing** - The `catching_up` status detection was failing due to trailing characters in grep output (`false}` instead of `false`). State sync now correctly detects when a node has finished syncing.

### Manager  
- **Fixed: State sync config path error** - Changed from `pruning_deploy_path` (which included `/data` subdirectory) to `deploy_path` (home directory). Config path now correctly resolves to `{deploy_path}/config/config.toml` instead of the incorrect `{deploy_path}/data/config/config.toml`.

- **Fixed: Only one RPC server sent to agent** - CometBFT requires at least 2 RPC servers for state sync redundancy. Now sends ALL configured RPC servers instead of just the first one.

- **Fixed: State sync shows as "catching up" instead of "in maintenance"** - Web handler now properly calls `HttpAgentManager::execute_state_sync()` which creates maintenance windows and tracks operations correctly.

### Configuration
- **Changed: Default state sync timeout** - Increased from 10 minutes to 30 minutes to accommodate chains with large state sizes (20-50GB).

- **Fixed: RPC server TOML formatting** - Removed double-quoting and duplication of RPC servers list in generated config.toml.

## ✨ Features & Improvements

### UI
- **Removed: Active Operations Panel** - Eliminated the entire "Active Operations" feature as it was unused and the cancel button didn't actually stop agent operations. Cleaner UI with 245 lines of code removed.

### Testing
- **Added: Comprehensive state sync test suite** - 14 new tests protecting against regressions:
  - Path configuration validation
  - Config path construction
  - Multiple RPC server handling
  - Error scenarios
  - Regression tests for all fixed bugs

### Code Quality
- **Refactored: Path configuration system** - Clearer semantics with `deploy_path` representing home directory. All operations now use consistent path handling.
  
- **Removed: Duplicate code paths** - Eliminated `StateSyncManager` duplicate, all operations now flow through `HttpAgentManager` for consistent tracking.

- **Documentation: Updated CLAUDE.md** - Added comprehensive path configuration section with examples.

## 📊 Test Results

- ✅ 149 tests passing
- ✅ All CI/CD checks passing
- ✅ Critical regression tests in place

## 🔧 Breaking Changes

None - All changes are backward compatible. Existing config files work without modification due to auto-derivation.

## 📦 Deployment

### Manager (Required)
```bash
# Contains critical state sync fixes
cargo build --release -p manager
scp target/release/manager server:/path/to/manager
systemctl restart nodes-manager
```

### Agent (Required)
```bash
# Contains state sync completion detection fix
cargo build --release -p agent
scp target/release/agent server:/path/to/agent
systemctl restart agent
```

## 🐛 Known Issues

- **Cancel button removed**: Agent operations cannot be cancelled mid-execution. This is a design limitation - operations run to completion or timeout. The cancel UI has been removed to avoid confusion.

## 📝 Full Changelog

### Manager Changes
- Renamed `pruning_deploy_path` → `deploy_path` throughout codebase
- Updated state sync to use `deploy_path` for config path construction
- Fixed RPC parameter fetching to return all configured servers
- Updated web handler to use HttpAgentManager for proper tracking
- Increased default state sync timeout to 30 minutes
- Removed unused StateSyncManager from web layer
- Removed Active Operations Panel from UI (245 lines)

### Agent Changes
- Fixed state sync completion detection (trim braces from grep output)
- Fixed RPC servers TOML formatting (no double-quoting)

### Testing
- Added state_sync_integration_tests.rs (456 lines)
- Updated manual_operations_integration.rs (replaced placeholders)
- Enhanced mock_rpc.rs with new methods
- Added comprehensive test documentation

### Documentation
- Updated README.md with new path configuration examples
- Updated CLAUDE.md with Path Configuration System section
- Added test improvement plans and summaries

## 🎉 Contributors

Special thanks to @kostovster for thorough testing and finding critical bugs!

## 📅 Release Date

2025-01-18

## 🔗 Links

- [Full Diff](https://github.com/nolus-protocol/nodes-manager/compare/v1.3.0...v1.4.0)
- [Documentation](https://github.com/nolus-protocol/nodes-manager/blob/main/README.md)
- [Issue Tracker](https://github.com/nolus-protocol/nodes-manager/issues)
