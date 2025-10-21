# Timezone Fix - Cron Scheduler Discrepancy

## 🔴 Critical Issue Identified

**Problem**: The frontend and backend interpret cron schedules in **different timezones**, causing scheduled tasks to execute at unexpected times.

**Observed Behavior**: 
- UI displays: "Next execution at 08:00 AM"
- Actual execution: Happens at 09:00 AM
- **Difference**: 1 hour discrepancy

---

## Root Cause Analysis

### Frontend Behavior (JavaScript)
**File**: `static/index.html` (lines 1437-1475)

```javascript
getNextRun(expression) {
    const now = new Date();
    const next = new Date(now);
    
    next.setUTCSeconds(...)   // Uses UTC!
    next.setUTCMinutes(...)   // Uses UTC!
    next.setUTCHours(...)     // Uses UTC!
    
    return next;  // Returns Date in UTC
}
```

**Frontend assumes**: All cron schedules are in **UTC timezone**.

### Backend Behavior (Rust)
**File**: `manager/src/scheduler/operations.rs` (line 188)

```rust
let job = Job::new_async(schedule.as_str(), move |_uuid, _scheduler| {
    // No timezone specified!
});
```

**Backend reality**: `tokio-cron-scheduler` uses **system local timezone** by default, NOT UTC!

### The Mismatch

| Component | Interprets Cron As | Result |
|-----------|-------------------|---------|
| **Frontend** | UTC | Displays time converted to browser's local timezone |
| **Backend** | System Local Time | Executes in server's local timezone |
| **Result** | ❌ MISMATCH | UI shows wrong time |

---

## Example Scenario

**Cron Schedule**: `0 0 5 * 5 *` (intended to run at 05:00)

### If Server is in EEST (UTC+3)

**Backend Interpretation**:
- Cron: `0 0 5 * 5 *`
- Interpreted as: **05:00 EEST (local time)**
- Actually runs at: **05:00 EEST** = **02:00 UTC**

**Frontend Calculation**:
- Assumes: `0 0 5 * 5 *` = **05:00 UTC**
- Browser in EEST: 05:00 UTC + 3 hours = **08:00 EEST**
- Displays: **"08:00 AM"**

**What Actually Happens**:
- Task executes: **05:00 EEST** (02:00 UTC)
- Browser shows: **08:00 EEST**
- **Difference**: 3 hours off!

### If Server is in a Different Timezone

If server is in a different timezone than your browser (e.g., server in UTC+4, browser in EEST UTC+3):
- This could explain the 1-hour discrepancy you observed

---

## Current Status

### Documentation Says
**File**: `config/SCHEDULE.md` (line 3)

> **Important:** All cron schedules execute in UTC. Local timezone is EEST (UTC+3) in summer, EET (UTC+2) in winter.

**This is INCORRECT!** Schedules do NOT execute in UTC by default.

### What's Really Happening

The backend executes cron schedules in **whatever timezone the system is configured to use**.

Common configurations:
- ✅ If system timezone = UTC: Everything works as expected
- ❌ If system timezone = EEST/EET: Schedules are off by 2-3 hours
- ❌ If system timezone = anything else: Unpredictable behavior

---

## The Fix

### Option 1: Force Backend to Use UTC (Recommended)

**Modify**: `manager/src/scheduler/operations.rs`

Add this import:
```rust
use tokio_cron_scheduler::JobSchedulerBuilder;
use chrono_tz::Tz;
```

Update the scheduler creation (line 25):
```rust
pub async fn new(
    database: Arc<Database>,
    http_manager: Arc<HttpAgentManager>,
    config: Arc<Config>,
    snapshot_manager: Arc<SnapshotManager>,
) -> Result<Self> {
    // Create scheduler with explicit UTC timezone
    let scheduler = JobSchedulerBuilder::new()
        .with_timezone(chrono_tz::UTC)  // Force UTC!
        .build()
        .await
        .map_err(|e| anyhow!("Failed to create JobScheduler: {}", e))?;

    Ok(Self {
        database,
        http_manager,
        _config: config,
        snapshot_manager,
        scheduler,
    })
}
```

Add dependency to `manager/Cargo.toml`:
```toml
[dependencies]
chrono-tz = "0.8"
```

**Advantages**:
- ✅ Matches documentation
- ✅ Matches frontend expectations
- ✅ Consistent across all deployments
- ✅ No DST confusion

### Option 2: Update Frontend to Use Local Time

**Modify**: `static/index.html` (lines 1440-1456)

Change all `setUTC*` methods to regular `set*` methods:
```javascript
getNextRun(expression) {
    const now = new Date();
    const next = new Date(now);
    
    next.setSeconds(...)    // Local time!
    next.setMinutes(...)    // Local time!
    next.setHours(...)      // Local time!
    
    return next;
}
```

**Advantages**:
- ✅ No backend changes needed
- ❌ Inconsistent across deployments with different timezones
- ❌ Confusing for operators (what timezone is "local"?)

### Option 3: Hybrid - Add Timezone Indicator to UI

Keep backend as-is, but update UI to show which timezone schedules use:

```javascript
// Add config
const CONFIG = {
    SCHEDULER_TIMEZONE: 'EEST',  // or fetch from API
    SCHEDULER_UTC_OFFSET: 3
};

// Update display
formatNextRun(cronSchedule) {
    // ... existing code ...
    return `<div class="next-run-time">${timeString}, ${dayString}, ${dateString} (${CONFIG.SCHEDULER_TIMEZONE})</div>`;
}
```

---

## Recommended Solution

**Use Option 1: Force UTC in Backend**

This is the best solution because:
1. ✅ Aligns with industry standards (cron schedules are typically UTC)
2. ✅ Matches your documentation (`SCHEDULE.md`)
3. ✅ Matches frontend implementation
4. ✅ Avoids DST confusion
5. ✅ Consistent across all servers regardless of system timezone

---

## Verification Steps

### Step 1: Check Current Server Timezone

SSH to the manager server and run:
```bash
date
timedatectl
echo $TZ
```

### Step 2: Check Rust Timezone

Add this to `scheduler/operations.rs` (temporary debug):
```rust
info!("Server timezone: {:?}", chrono::Local::now());
info!("UTC time: {:?}", chrono::Utc::now());
```

### Step 3: Compare Execution Times

1. Note what time the UI shows for next execution
2. Note what time the actual execution happens (check logs)
3. Calculate difference

### Step 4: Verify Fix

After implementing Option 1:
1. Restart manager service
2. Check logs for timezone info
3. Verify UI times match actual execution times

---

## Migration Guide

### Before Deploying Fix

**IMPORTANT**: All existing cron schedules are currently interpreted in **local time** (likely EEST/EET).

If you switch to UTC, you must **adjust all cron schedules**:

**Example**:
- **Old schedule** (interpreted as EEST): `0 0 9 * * 1` = Monday 09:00 EEST
- **New schedule** (interpreted as UTC): `0 0 6 * * 1` = Monday 06:00 UTC = Monday 09:00 EEST

**Formula**: `UTC_hour = EEST_hour - 3` (or `- 2` for EET in winter)

### Update SCHEDULE.md

After fixing, update `config/SCHEDULE.md` to be explicit:

```markdown
**IMPORTANT:** All cron schedules execute in **UTC timezone**.

The backend forces UTC timezone via `JobSchedulerBuilder::with_timezone(chrono_tz::UTC)`.
The frontend displays times converted to your browser's local timezone automatically.

Example:
- Cron: `0 0 6 * * 1` = Monday 06:00 UTC
- Displays in EEST browser: Monday 09:00 AM (06:00 + 3)
- Displays in EET browser: Monday 08:00 AM (06:00 + 2)
- Actually executes: Monday 06:00 UTC
```

---

## Testing the Fix

### Test Case 1: Simple Schedule

```toml
pruning_schedule = "0 0 10 * * 1"  # Monday 10:00 UTC
```

**Expected**:
- Backend executes: Monday 10:00 UTC
- UI in EEST: Shows "Monday 13:00" (10 + 3)
- UI in EET: Shows "Monday 12:00" (10 + 2)

### Test Case 2: Verify Logs

After execution, check logs:
```bash
grep "Executing scheduled pruning" /var/log/manager.log
```

Should show execution time matching UTC, not local.

---

## Summary

| Item | Current (Broken) | After Fix (Option 1) |
|------|-----------------|---------------------|
| **Backend interprets** | System local time (EEST/EET) | UTC (forced) |
| **Frontend interprets** | UTC | UTC |
| **Match?** | ❌ NO | ✅ YES |
| **UI shows correct time?** | ❌ NO (off by 1-3 hours) | ✅ YES |
| **Consistent across servers?** | ❌ NO (depends on system TZ) | ✅ YES |

---

## Action Items

- [ ] Verify current server timezone
- [ ] Implement Option 1 (force UTC in backend)
- [ ] Update all cron schedules to use UTC hours
- [ ] Update SCHEDULE.md documentation
- [ ] Test on staging environment
- [ ] Deploy to production
- [ ] Verify UI times match execution times

---

## References

- tokio-cron-scheduler docs: https://docs.rs/tokio-cron-scheduler/
- chrono-tz crate: https://docs.rs/chrono-tz/
- Backend scheduler: `manager/src/scheduler/operations.rs`
- Frontend scheduler: `static/index.html` (lines 1414-1500)
- Documentation: `config/SCHEDULE.md`

---

**Last Updated**: 2025-01-21
**Issue Severity**: 🔴 HIGH - Causes incorrect execution times for all scheduled maintenance
**Fix Complexity**: 🟡 MEDIUM - Requires code change + schedule updates