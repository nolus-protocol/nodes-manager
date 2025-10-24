# Alert Webhook Events - Complete Reference

## Overview

This document lists **ALL webhook alert events** triggered by the nodes-manager system. Use this to configure your n8n automation to decide which alerts should send Slack notifications and which should be absorbed/filtered.

## Alert Structure

Every webhook POST contains:

```json
{
  "timestamp": "2025-10-24T13:45:00Z",
  "alert_type": "NodeHealth | AutoRestore | Snapshot | Hermes | LogPattern | Maintenance",
  "severity": "Critical | Warning | Info | Recovery",
  "node_name": "full-node-3",
  "message": "Human-readable message",
  "server_host": "production-server-1",
  "details": { /* Additional context */ }
}
```

---

## 1. Node Health Alerts (`AlertType::NodeHealth`)

**Source**: `manager/src/health/monitor.rs`  
**Trigger**: Health check system (every 90 seconds by default)  
**Rate Limiting**: Progressive (3 checks → 6h → 6h → 12h → 24h)

### 1.1 Node Health - Critical (Unhealthy)

**When**: Node becomes unhealthy (RPC fails, block height not progressing, or catching up)  
**Severity**: `Critical`  
**Rate Limited**: Yes (see progressive schedule below)  
**Example Message**: `"Node health check failed"`, `"Block height not progressing"`

**Progressive Alert Schedule**:
- **Check 1-2**: No alert (silent)
- **Check 3**: First alert sent (after 3 consecutive unhealthy checks)
- **+6 hours**: Second alert
- **+6 hours**: Third alert (12h total)
- **+12 hours**: Fourth alert (24h total)
- **+24 hours**: Fifth alert (48h total)
- **+24 hours**: Subsequent alerts every 24h

**Recommendation**: ✅ **Send to Slack** - These are critical issues requiring immediate attention

```json
{
  "alert_type": "NodeHealth",
  "severity": "Critical",
  "message": "Node health check failed",
  "details": {
    "rpc_url": "http://...",
    "block_height": 12345,
    "is_catching_up": false,
    "network": "pirin-1"
  }
}
```

### 1.2 Node Health - Recovery

**When**: Node recovers from unhealthy state  
**Severity**: `Recovery`  
**Rate Limited**: No (sent once per recovery)  
**Example Message**: `"Node has recovered and is now healthy"`

**Recommendation**: ✅ **Send to Slack** - Good news that issues are resolved

```json
{
  "alert_type": "NodeHealth",
  "severity": "Recovery",
  "message": "Node has recovered and is now healthy",
  "details": {
    "rpc_url": "http://...",
    "block_height": 12450,
    "network": "pirin-1"
  }
}
```

### 1.3 ETL Service Health - Critical

**When**: ETL service (like indexer) becomes unhealthy  
**Severity**: `Critical`  
**Rate Limited**: Yes (same progressive schedule as nodes)  
**Example Message**: `"ETL service health check failed: Connection refused"`

**Recommendation**: ⚠️ **Conditional** - Send to Slack if ETL service is critical to operations

```json
{
  "alert_type": "NodeHealth",
  "severity": "Critical",
  "node_name": "etl:indexer-service",
  "message": "ETL service health check failed: Connection refused",
  "details": {
    "service_url": "http://...:8080/health",
    "response_time_ms": 5000,
    "status_code": null
  }
}
```

### 1.4 ETL Service Health - Recovery

**When**: ETL service recovers  
**Severity**: `Recovery`  
**Rate Limited**: No  
**Example Message**: `"Node has recovered and is now healthy"` (node_name will be `etl:service-name`)

**Recommendation**: 📊 **Optional** - Informational, can be logged only

---

## 2. Auto-Restore Alerts (`AlertType::AutoRestore`)

**Source**: `manager/src/health/monitor.rs`  
**Trigger**: Log file patterns detected indicating database corruption  
**Rate Limiting**: 2-hour cooldown between restore attempts per node

### 2.1 Auto-Restore Started

**When**: Auto-restore process begins after detecting corruption trigger words in logs  
**Severity**: `Warning`  
**Rate Limited**: Yes (2h cooldown per node)  
**Example Message**: `"Auto-restore STARTED for full-node-3 due to corruption indicators"`

**Recommendation**: ✅ **Send to Slack** - Important operation that needs visibility

```json
{
  "alert_type": "AutoRestore",
  "severity": "Warning",
  "message": "Auto-restore STARTED for full-node-3 due to corruption indicators",
  "details": {
    "trigger_words": ["UPGRADE NEEDED", "wrong Block.Header.AppHash"],
    "status": "starting"
  }
}
```

### 2.2 Auto-Restore Completed

**When**: Auto-restore successfully completes snapshot restoration  
**Severity**: `Info`  
**Rate Limited**: No (one per operation)  
**Example Message**: `"Auto-restore COMPLETED for full-node-3 - node should be syncing from restored state"`

**Recommendation**: ✅ **Send to Slack** - Confirmation that recovery succeeded

```json
{
  "alert_type": "AutoRestore",
  "severity": "Info",
  "message": "Auto-restore COMPLETED for full-node-3 - node should be syncing from restored state",
  "details": {
    "trigger_words": ["UPGRADE NEEDED"],
    "status": "completed",
    "snapshot_filename": "pirin-1_20250121_17154420"
  }
}
```

### 2.3 Auto-Restore Failed

**When**: Auto-restore fails to complete  
**Severity**: `Critical`  
**Rate Limited**: No  
**Example Message**: `"CRITICAL: Auto-restore failed for full-node-3 - manual intervention required"`

**Recommendation**: 🚨 **URGENT - Send to Slack** - Requires immediate manual intervention

```json
{
  "alert_type": "AutoRestore",
  "severity": "Critical",
  "message": "CRITICAL: Auto-restore failed for full-node-3 - manual intervention required",
  "details": {
    "error_message": "Snapshot not found",
    "trigger_words": ["UPGRADE NEEDED"]
  }
}
```

---

## 3. Maintenance Operation Alerts (`AlertType::Maintenance`)

**Source**: `manager/src/services/maintenance_service.rs`  
**Trigger**: Manual operations via UI/API or scheduled operations via cron  
**Rate Limiting**: None (one alert per operation start/complete/fail)

### 3.1 Pruning Started

**When**: Node pruning operation begins (manual or scheduled)  
**Severity**: `Info`  
**Rate Limited**: No  
**Example Message**: `"Manual pruning operation started for full-node-3"`

**Recommendation**: 📊 **Absorb** - Routine operation, log only

```json
{
  "alert_type": "Maintenance",
  "severity": "Info",
  "message": "Manual pruning operation started for full-node-3",
  "details": {
    "operation_id": "uuid-here",
    "operation_type": "pruning",
    "status": "started"
  }
}
```

### 3.2 Pruning Completed

**When**: Pruning successfully completes  
**Severity**: `Info`  
**Rate Limited**: No  
**Example Message**: `"Pruning completed successfully for full-node-3"`

**Recommendation**: 📊 **Absorb** - Success confirmation, log only

```json
{
  "alert_type": "Maintenance",
  "severity": "Info",
  "message": "Pruning completed successfully for full-node-3",
  "details": {
    "operation_id": "uuid-here",
    "operation_type": "pruning",
    "status": "completed"
  }
}
```

### 3.3 Pruning Failed

**When**: Pruning operation fails  
**Severity**: `Critical`  
**Rate Limited**: No  
**Example Message**: `"Pruning failed for full-node-3: Disk space full"`

**Recommendation**: ⚠️ **Send to Slack** - Indicates an issue that needs attention

```json
{
  "alert_type": "Maintenance",
  "severity": "Critical",
  "message": "Pruning failed for full-node-3: Disk space full",
  "details": {
    "operation_id": "uuid-here",
    "operation_type": "pruning",
    "status": "failed",
    "error_message": "Disk space full"
  }
}
```

### 3.4 Snapshot Creation Started

**When**: Manual or scheduled snapshot creation begins  
**Severity**: `Info`  
**Rate Limited**: No  
**Example Message**: `"Manual snapshot_creation operation started for full-node-3"`

**Recommendation**: 📊 **Absorb** - Routine operation, log only

### 3.5 Snapshot Creation Completed

**When**: Snapshot creation succeeds  
**Severity**: `Info`  
**Rate Limited**: No  
**Example Message**: `"Snapshot creation completed successfully for full-node-3"`

**Recommendation**: 📊 **Absorb** - Success confirmation, log only

### 3.6 Snapshot Creation Failed

**When**: Snapshot creation fails  
**Severity**: `Critical`  
**Rate Limited**: No  
**Example Message**: `"Snapshot creation failed for full-node-3: Insufficient disk space"`

**Recommendation**: ⚠️ **Send to Slack** - May impact recovery capability

### 3.7 Node Restart Started

**When**: Manual or scheduled node restart begins  
**Severity**: `Info`  
**Rate Limited**: No  
**Example Message**: `"Manual node_restart operation started for full-node-3"`

**Recommendation**: 📊 **Absorb** - Routine operation, log only

### 3.8 Node Restart Completed

**When**: Node restart succeeds  
**Severity**: `Info`  
**Rate Limited**: No  
**Example Message**: `"Node restart completed successfully for full-node-3"`

**Recommendation**: 📊 **Absorb** - Success confirmation, log only

### 3.9 Node Restart Failed

**When**: Node restart fails  
**Severity**: `Critical`  
**Rate Limited**: No  
**Example Message**: `"Node restart failed for full-node-3: Service not responding"`

**Recommendation**: 🚨 **Send to Slack** - Critical issue requiring immediate attention

### 3.10 State Sync Started

**When**: Manual state sync operation begins  
**Severity**: `Info`  
**Rate Limited**: No  
**Example Message**: `"State sync started for full-node-3"`

**Recommendation**: 📊 **Absorb** - Routine operation, log only

### 3.11 State Sync Completed

**When**: State sync succeeds  
**Severity**: `Info`  
**Rate Limited**: No  
**Example Message**: `"State sync completed successfully for full-node-3"`

**Recommendation**: 📊 **Absorb** - Success confirmation, log only

### 3.12 State Sync Failed

**When**: State sync operation fails  
**Severity**: `Critical`  
**Rate Limited**: No  
**Example Message**: `"State sync failed for full-node-3: RPC sources unreachable"`

**Recommendation**: 🚨 **Send to Slack** - Critical issue, node cannot sync

---

## 4. Snapshot Restoration Alerts (`AlertType::Snapshot`)

**Source**: `manager/src/snapshot/manager.rs`  
**Trigger**: Manual snapshot restore operations via UI/API  
**Rate Limiting**: None

### 4.1 Snapshot Restore Completed

**When**: Manual snapshot restore succeeds  
**Severity**: `Info`  
**Rate Limited**: No  
**Example Message**: `"Network snapshot restored successfully for full-node-3: pirin-1_20250121_17154420"`

**Recommendation**: ✅ **Send to Slack** - Important operation that changes node state

```json
{
  "alert_type": "Snapshot",
  "severity": "Info",
  "message": "Network snapshot restored successfully for full-node-3: pirin-1_20250121_17154420",
  "details": {
    "operation_type": "snapshot_restore",
    "operation_status": "completed",
    "snapshot_filename": "pirin-1_20250121_17154420",
    "network": "pirin-1",
    "connection_type": "http_agent"
  }
}
```

### 4.2 Snapshot Restore Failed

**When**: Manual snapshot restore fails  
**Severity**: `Critical`  
**Rate Limited**: No  
**Example Message**: `"Snapshot restore failed for full-node-3: Snapshot file corrupted"`

**Recommendation**: 🚨 **Send to Slack** - Failed recovery operation needs attention

```json
{
  "alert_type": "Snapshot",
  "severity": "Critical",
  "message": "Snapshot restore failed for full-node-3: Snapshot file corrupted",
  "details": {
    "operation_type": "snapshot_restore",
    "operation_status": "failed",
    "error_message": "Snapshot file corrupted",
    "network": "pirin-1",
    "connection_type": "http_agent"
  }
}
```

---

## 5. Hermes Relayer Alerts (`AlertType::Hermes`)

**Source**: `manager/src/services/hermes_service.rs`  
**Trigger**: Manual or scheduled Hermes restart operations  
**Rate Limiting**: None

### 5.1 Hermes Restart Started

**When**: Hermes relayer restart begins  
**Severity**: `Info`  
**Rate Limited**: No  
**Example Message**: `"Hermes restart started for hermes-relayer-1"`

**Recommendation**: 📊 **Absorb** - Routine operation, log only

```json
{
  "alert_type": "Hermes",
  "severity": "Info",
  "message": "Hermes restart started for hermes-relayer-1",
  "details": {
    "operation_type": "hermes_restart",
    "hermes_name": "hermes-relayer-1",
    "status": "started"
  }
}
```

### 5.2 Hermes Restart Completed

**When**: Hermes restart succeeds  
**Severity**: `Info`  
**Rate Limited**: No  
**Example Message**: `"Hermes restart completed successfully for hermes-relayer-1"`

**Recommendation**: 📊 **Absorb** - Success confirmation, log only

### 5.3 Hermes Restart Failed

**When**: Hermes restart fails  
**Severity**: `Critical`  
**Rate Limited**: No  
**Example Message**: `"Hermes restart failed for hermes-relayer-1: Service failed to start"`

**Recommendation**: ⚠️ **Send to Slack** - May impact IBC relay functionality

```json
{
  "alert_type": "Hermes",
  "severity": "Critical",
  "message": "Hermes restart failed for hermes-relayer-1: Service failed to start",
  "details": {
    "operation_type": "hermes_restart",
    "hermes_name": "hermes-relayer-1",
    "status": "failed",
    "error_message": "Service failed to start"
  }
}
```

---

## 6. Log Pattern Alerts (`AlertType::LogPattern`)

**Source**: `manager/src/health/monitor.rs`  
**Trigger**: Configured log patterns detected in node logs  
**Rate Limiting**: None (sent every time pattern is detected during health check)

### 6.1 Log Pattern Detected

**When**: Configured regex pattern matches in node logs  
**Severity**: `Warning`  
**Rate Limited**: No  
**Example Message**: `"Log pattern match detected"`

**Recommendation**: ⚠️ **Conditional** - Depends on your log patterns
- **Critical patterns** (errors, panics): Send to Slack
- **Info patterns** (debugging): Absorb/log only

```json
{
  "alert_type": "LogPattern",
  "severity": "Warning",
  "message": "Log pattern match detected",
  "details": {
    "log_path": "/var/log/full-node-3",
    "log_output": "...matched log lines...",
    "patterns": ["ERROR", "PANIC"]
  }
}
```

---

## Alert Filtering Recommendations

### 🚨 CRITICAL - Always Send to Slack

| Alert Type | Severity | Event | Reason |
|------------|----------|-------|--------|
| NodeHealth | Critical | Node unhealthy | Service degradation |
| AutoRestore | Critical | Auto-restore failed | Manual intervention required |
| Maintenance | Critical | Any operation failed | Indicates infrastructure issue |
| Snapshot | Critical | Restore failed | Recovery capability compromised |
| Hermes | Critical | Restart failed | IBC relay affected |

### ✅ IMPORTANT - Send to Slack

| Alert Type | Severity | Event | Reason |
|------------|----------|-------|--------|
| NodeHealth | Recovery | Node recovered | Confirm issue resolved |
| AutoRestore | Warning | Auto-restore started | Important operation visibility |
| AutoRestore | Info | Auto-restore completed | Confirm recovery succeeded |
| Snapshot | Info | Restore completed | Important state change |

### ⚠️ CONDITIONAL - Your Decision

| Alert Type | Severity | Event | Consider |
|------------|----------|-------|----------|
| LogPattern | Warning | Pattern matched | Depends on pattern criticality |
| Hermes | Critical | Restart failed | Depends on relayer importance |
| Maintenance | Critical | Pruning failed | May be recoverable |

### 📊 INFORMATIONAL - Absorb/Log Only

| Alert Type | Severity | Event | Reason |
|------------|----------|-------|--------|
| Maintenance | Info | Operation started | Routine operation |
| Maintenance | Info | Operation completed | Expected success |
| Hermes | Info | Restart started/completed | Routine operation |

---

## Alert Frequency Guidelines

### High Frequency (Multiple per hour)
- **Maintenance operations** (if scheduled frequently)
- **Log pattern alerts** (depends on patterns)

### Medium Frequency (Every 90 seconds during issues)
- **Node health checks** (but rate-limited progressively)

### Low Frequency (Rare events)
- **Auto-restore operations** (2h cooldown)
- **Critical failures** (should be rare in healthy system)
- **Recovery notifications** (one per incident)

---

## n8n Filtering Strategy

### Recommended n8n Filter Logic

```javascript
// Example n8n function node for filtering

// ALWAYS send to Slack
if (severity === "Critical" || severity === "Recovery") {
  return true;
}

// Auto-restore events
if (alert_type === "AutoRestore") {
  return true; // All auto-restore events are important
}

// Snapshot restore (manual operations)
if (alert_type === "Snapshot") {
  return true; // Important state changes
}

// Maintenance operations - only failures
if (alert_type === "Maintenance") {
  return severity === "Critical"; // Only failed operations
}

// Hermes - only failures
if (alert_type === "Hermes") {
  return severity === "Critical"; // Only failed restarts
}

// Log patterns - depends on your configuration
if (alert_type === "LogPattern") {
  // Custom logic based on patterns or node names
  return message.includes("PANIC") || message.includes("FATAL");
}

// Default: absorb Info-level maintenance notifications
return false;
```

### Summary Statistics

**Total Alert Event Types**: 27 different alert scenarios  
**Recommended for Slack**: 14 events (52%)  
**Recommended to Absorb**: 13 events (48%)

**By Severity**:
- Critical: 9 event types → ✅ All to Slack
- Warning: 3 event types → ⚠️ Conditional
- Info: 11 event types → 📊 Mostly absorb
- Recovery: 2 event types → ✅ All to Slack

---

## Testing Alerts

To test your n8n filtering, you can trigger test alerts:

```bash
# The AlertService has a test_webhook method (not exposed via API yet)
# You can manually trigger operations to test:

# Test maintenance alerts
curl -X POST http://localhost:8080/api/maintenance/nodes/{node}/prune

# Test health alerts - wait for health check cycle or manually induce failure

# Check logs for alert webhook posts
tail -f logs/manager.log | grep "Sending alert"
```

---

## Configuration

**Webhook URL**: Set in `config/main.toml`:
```toml
alarm_webhook_url = "https://your-n8n-instance/webhook/nodes-manager-alerts"
```

**Rate Limiting Constants**: Defined in `manager/src/constants/alerts.rs`:
- `FIRST_ALERT_AFTER_CHECKS = 3`
- `SECOND_ALERT_INTERVAL_HOURS = 6`
- `THIRD_ALERT_INTERVAL_HOURS = 6`
- `FOURTH_ALERT_INTERVAL_HOURS = 12`
- `SUBSEQUENT_ALERT_INTERVAL_HOURS = 24`

---

## Appendix: Alert Flow Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    Alert Event Occurs                        │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│              AlertService.send_*_alert()                     │
│  • Checks rate limiting (if applicable)                     │
│  • Constructs JSON payload                                  │
│  • Posts to webhook URL                                     │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│                     n8n Webhook                              │
│  • Receives POST with alert JSON                            │
│  • Applies filtering logic                                  │
│  • Routes to Slack or logs/absorbs                          │
└─────────────────────────────────────────────────────────────┘
```

Last Updated: 2025-10-24
Version: 1.5.0
