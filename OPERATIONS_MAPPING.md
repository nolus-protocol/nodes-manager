# Operations Mapping - Complete Reference

## Overview

This document maps all operations from UI → Handler → Service → Alerts to ensure complete coverage.

**Version**: 1.5.0  
**Last Updated**: 2025-10-24

---

## Operation Flow

```
UI Button → API Endpoint → Handler → Service → AlertService → n8n → Slack
```

---

## 1. Node Pruning Operation

### UI
- **Button**: Prune (wrench icon)
- **Function**: `app.executeNodePruning(nodeName)`
- **Condition**: `config.pruning_enabled`
- **API Call**: `POST /api/maintenance/nodes/{node}/prune`

### Backend
- **Handler**: `execute_manual_node_pruning()` in `handlers.rs:461`
- **Service**: `MaintenanceService.execute_immediate_operation("pruning", node)`
- **Infrastructure**: `HttpAgentManager.execute_node_pruning()`

### Alerts Sent
1. **Start**: `Maintenance` / `Info` - "Manual pruning operation started"
2. **Success**: `Maintenance` / `Info` - "Pruning completed successfully"
3. **Failure**: `Maintenance` / `Critical` - "Pruning failed: {error}"

### n8n Handling
- Start: ❌ Absorbed (Info)
- Success: ❌ Absorbed (Info)
- Failure: ✅ Sent to Slack (Critical)

**Status**: ✅ Complete - UI, Handler, Service, Alerts all working

---

## 2. Snapshot Creation Operation

### UI
- **Button**: Snapshot (camera icon)
- **Function**: `app.executeCreateSnapshot(nodeName)`
- **Condition**: `config.snapshots_enabled`
- **API Call**: `POST /api/snapshots/{node}/create`

### Backend
- **Handler**: `create_snapshot()` in `handlers.rs:494`
- **Service**: `MaintenanceService.execute_immediate_operation("snapshot_creation", node)`
- **Infrastructure**: `HttpAgentManager.create_node_snapshot()`

### Alerts Sent
1. **Start**: `Maintenance` / `Info` - "Manual snapshot_creation operation started"
2. **Success**: `Maintenance` / `Info` - "Snapshot creation completed successfully"
3. **Failure**: `Maintenance` / `Critical` - "Snapshot creation failed: {error}"

### n8n Handling
- Start: ❌ Absorbed (Info)
- Success: ❌ Absorbed (Info)
- Failure: ✅ Sent to Slack (Critical)

**Status**: ✅ Complete - UI, Handler, Service, Alerts all working

---

## 3. Node Restart Operation

### UI
- **Button**: ⚠️ **MISSING** - Need to add
- **Function**: ⚠️ **MISSING** - Need to create `app.executeNodeRestart(nodeName)`
- **Condition**: Should be available for all nodes
- **API Call**: `POST /api/maintenance/nodes/{node}/restart`

### Backend
- **Handler**: `execute_manual_node_restart()` in `handlers.rs:403`
- **Service**: `MaintenanceService.execute_immediate_operation("node_restart", node)`
- **Infrastructure**: `HttpAgentManager.restart_node()`

### Alerts Sent
1. **Start**: `Maintenance` / `Info` - "Manual node_restart operation started"
2. **Success**: `Maintenance` / `Info` - "Node restart completed successfully"
3. **Failure**: `Maintenance` / `Critical` - "Node restart failed: {error}"

### n8n Handling
- Start: ❌ Absorbed (Info)
- Success: ❌ Absorbed (Info)
- Failure: ✅ Sent to Slack (Critical)

**Status**: ⚠️ **INCOMPLETE** - Handler and Service work, but UI button missing

---

## 4. Snapshot Restore Operation

### UI
- **Button**: Restore (upload icon)
- **Function**: `app.executeManualRestore(nodeName)`
- **Condition**: `config.auto_restore_enabled`
- **API Call**: `POST /api/snapshots/{node}/restore`

### Backend
- **Handler**: `execute_manual_restore_from_latest()` in `handlers.rs:619`
- **Service**: `SnapshotService.restore_from_snapshot(node)`
- **Infrastructure**: `SnapshotManager.restore_from_snapshot()` → `HttpAgentManager.restore_node_from_snapshot()`

### Alerts Sent
1. **Completion**: `Snapshot` / `Info` - "Network snapshot restored successfully: {filename}"
2. **Failure**: `Snapshot` / `Critical` - "Snapshot restore failed: {error}"

### n8n Handling
- Success: ✅ Sent to Slack (Important state change)
- Failure: ✅ Sent to Slack (Critical)

**Status**: ✅ Complete - UI, Handler, Service, Alerts all working

---

## 5. State Sync Operation

### UI
- **Button**: State Sync (sync icon)
- **Function**: `app.executeStateSync(nodeName)`
- **Condition**: `config.state_sync_enabled`
- **API Call**: `POST /api/state-sync/{node}/execute`

### Backend
- **Handler**: `execute_manual_state_sync()` in `handlers.rs:652`
- **Service**: `StateSyncService.execute_state_sync(node)`
- **Infrastructure**: `HttpAgentManager.execute_state_sync()`

### Alerts Sent
1. **Start**: `Maintenance` / `Info` - "State sync started"
2. **Success**: `Maintenance` / `Info` - "State sync completed successfully"
3. **Failure**: `Maintenance` / `Critical` - "State sync failed: {error}"

### n8n Handling
- Start: ❌ Absorbed (Info)
- Success: ❌ Absorbed (Info)
- Failure: ✅ Sent to Slack (Critical)

**Status**: ✅ Complete - UI, Handler, Service, Alerts all working

---

## 6. Hermes Restart Operation

### UI
- **Button**: Restart Hermes (refresh icon)
- **Function**: `app.executeHermesRestart(hermesName)`
- **Condition**: Always available for Hermes instances
- **API Call**: `POST /api/maintenance/hermes/{hermes}/restart`

### Backend
- **Handler**: `execute_manual_hermes_restart()` in `handlers.rs:434`
- **Service**: `HermesService.restart_instance(hermes)`
- **Infrastructure**: `HttpAgentManager.restart_hermes()`

### Alerts Sent
1. **Start**: `Hermes` / `Info` - "Hermes restart started"
2. **Success**: `Hermes` / `Info` - "Hermes restart completed successfully"
3. **Failure**: `Hermes` / `Critical` - "Hermes restart failed: {error}"

### n8n Handling
- Start: ❌ Absorbed (Info)
- Success: ❌ Absorbed (Info)
- Failure: ✅ Sent to Slack (Critical)

**Status**: ✅ Complete - UI, Handler, Service, Alerts all working

---

## Background Operations (No UI Buttons)

### 7. Health Monitoring

**Source**: `HealthMonitor.check_all_nodes()` - runs every 90 seconds

**Alerts Sent**:
1. **Unhealthy**: `NodeHealth` / `Critical` - Progressive rate limiting (3 checks → 6h → 6h → 12h → 24h)
2. **Recovered**: `NodeHealth` / `Recovery` - "Node has recovered and is now healthy"

**n8n Handling**:
- Unhealthy: ✅ Sent to Slack (Critical)
- Recovered: ✅ Sent to Slack (Good news)

**Status**: ✅ Complete

---

### 8. Auto-Restore

**Source**: `HealthMonitor.monitor_auto_restore_triggers()` - checks unhealthy nodes for log patterns

**Alerts Sent**:
1. **Started**: `AutoRestore` / `Warning` - "Auto-restore STARTED due to corruption indicators"
2. **Completed**: `AutoRestore` / `Info` - "Auto-restore COMPLETED"
3. **Failed**: `AutoRestore` / `Critical` - "Auto-restore failed - manual intervention required"

**n8n Handling**:
- All: ✅ Sent to Slack (Important automated recovery)

**Status**: ✅ Complete

---

### 9. Log Pattern Detection

**Source**: `HealthMonitor.monitor_logs_per_node()` - checks configured log patterns

**Alerts Sent**:
1. **Pattern Match**: `LogPattern` / `Warning` - "Log pattern match detected"

**n8n Handling**:
- Pattern Match: ⚠️ Conditional (depends on pattern severity)

**Status**: ✅ Complete

---

### 10. Scheduled Operations

**Source**: `MaintenanceScheduler` - cron-based scheduling

**Operations Scheduled**:
- Pruning (via MaintenanceService)
- Snapshot creation (via MaintenanceService)
- Hermes restart (via HermesService)

**Alerts Sent**:
Same as manual operations above - all go through service layer

**n8n Handling**:
Same filtering as manual operations

**Status**: ✅ Complete

---

## Summary Statistics

| Operation | UI Button | Handler | Service | Alerts | n8n | Status |
|-----------|-----------|---------|---------|--------|-----|--------|
| Node Pruning | ✅ | ✅ | ✅ MaintenanceService | ✅ 3 alerts | ✅ | Complete |
| Snapshot Creation | ✅ | ✅ | ✅ MaintenanceService | ✅ 3 alerts | ✅ | Complete |
| Node Restart | ❌ | ✅ | ✅ MaintenanceService | ✅ 3 alerts | ✅ | **Missing UI** |
| Snapshot Restore | ✅ | ✅ | ✅ SnapshotService | ✅ 2 alerts | ✅ | Complete |
| State Sync | ✅ | ✅ | ✅ StateSyncService | ✅ 3 alerts | ✅ | Complete |
| Hermes Restart | ✅ | ✅ | ✅ HermesService | ✅ 3 alerts | ✅ | Complete |
| Health Check | N/A | N/A | ✅ HealthMonitor | ✅ 2 alerts | ✅ | Complete |
| Auto-Restore | N/A | N/A | ✅ HealthMonitor | ✅ 3 alerts | ✅ | Complete |
| Log Patterns | N/A | N/A | ✅ HealthMonitor | ✅ 1 alert | ✅ | Complete |

**Total Operations**: 9  
**Complete**: 8  
**Incomplete**: 1 (Node Restart - missing UI button)

---

## Alert Summary by Type

| Alert Type | Count | High-Value (Slack) | Low-Value (Absorbed) |
|------------|-------|-------------------|---------------------|
| Maintenance | 12 | 4 failures | 8 start/success |
| Snapshot | 2 | 2 (all) | 0 |
| Hermes | 3 | 1 failure | 2 start/success |
| NodeHealth | 2 | 2 (all) | 0 |
| AutoRestore | 3 | 3 (all) | 0 |
| LogPattern | 1 | 1 (conditional) | 0 |
| **TOTAL** | **23** | **13 (57%)** | **10 (43%)** |

---

## Action Items

### 1. Add Node Restart UI Button ⚠️ HIGH PRIORITY

**Location**: `static/index.html` - Add to node action buttons

**Required Changes**:
1. Add restart button to buttons object (around line 1595)
2. Add `executeNodeRestart()` function (around line 2219)
3. Add restart icon to icons object

**Why Important**: 
- Handler and service are complete and working
- Alerts are properly integrated
- Only UI access is missing
- Users may need to restart nodes without pruning/snapshot

### 2. Verify n8n Filter Logic ✅ READY

All 23 alert events are properly categorized and ready for n8n filtering:
- 13 high-value alerts → Send to Slack
- 10 low-value alerts → Absorb

---

## Testing Checklist

Before finalizing, test each operation:

- [ ] **Pruning**: Click UI button → Verify alerts sent (start/complete/fail)
- [ ] **Snapshot Creation**: Click UI button → Verify alerts sent
- [ ] **Node Restart**: ⚠️ Add UI button first → Test alerts
- [ ] **Snapshot Restore**: Click UI button → Verify alerts sent
- [ ] **State Sync**: Click UI button → Verify alerts sent
- [ ] **Hermes Restart**: Click UI button → Verify alerts sent
- [ ] **Health Check**: Wait for unhealthy state → Verify progressive alerts
- [ ] **Auto-Restore**: Trigger corruption pattern → Verify alerts
- [ ] **n8n Filtering**: Verify high-value reach Slack, low-value absorbed

---

## API Endpoints Reference

| Operation | Method | Endpoint | Handler | Service |
|-----------|--------|----------|---------|---------|
| Prune Node | POST | `/api/maintenance/nodes/{node}/prune` | execute_manual_node_pruning | MaintenanceService |
| Restart Node | POST | `/api/maintenance/nodes/{node}/restart` | execute_manual_node_restart | MaintenanceService |
| Restart Hermes | POST | `/api/maintenance/hermes/{hermes}/restart` | execute_manual_hermes_restart | HermesService |
| Create Snapshot | POST | `/api/snapshots/{node}/create` | create_snapshot | MaintenanceService |
| Restore Snapshot | POST | `/api/snapshots/{node}/restore` | execute_manual_restore_from_latest | SnapshotService |
| State Sync | POST | `/api/state-sync/{node}/execute` | execute_manual_state_sync | StateSyncService |

---

Last Updated: 2025-10-24  
Version: 1.5.0
