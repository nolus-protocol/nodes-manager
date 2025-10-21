# Snapshot Optimization - External Providers

## Summary

Disabled local snapshot creation for Osmosis, Neutron, Osmo-test, and Pion nodes across all servers. These snapshots will be obtained from external providers instead.

## Time Savings

**Before** (with all snapshots):
- Osmosis snapshots: 3 servers × 2 hours = 6 hours
- Neutron snapshots: 2 servers × 2 hours = 4 hours
- Osmo-test snapshot: 1 server × 2 hours = 2 hours
- Pion snapshot: 1 server × 2 hours = 2 hours
- **Total snapshot time**: 14 hours across the week

**After** (external snapshots):
- **Total saved**: 14 hours of snapshot operations
- **Remaining snapshots**: Only Nolus network (pirin, vitosha, rila)

## Changes Made

### Enterprise (Wednesday)
- ✅ Osmosis: Pruning enabled, Snapshot **DISABLED**
- ✅ Neutron: Pruning enabled, Snapshot **DISABLED**
- ✅ Nolus: State sync enabled (no change)
- ⏱️ **Time saved**: 4 hours (2 snapshot operations removed)

### Discovery (Friday)
- ✅ Osmosis: Pruning enabled, Snapshot **DISABLED**
- ✅ Neutron: Pruning enabled, Snapshot **DISABLED**
- ✅ Nolus: State sync enabled (no change)
- ⏱️ **Time saved**: 4 hours (2 snapshot operations removed)

### Horizon (Thursday)
- ✅ Osmo-test: Pruning enabled, Snapshot **DISABLED**
- ✅ Pion: Pruning enabled, Snapshot **DISABLED**
- ✅ Vitosha & Rila: Pruning only (no snapshots)
- ⏱️ **Time saved**: 4 hours (2 snapshot operations removed)

### Voyager (Saturday)
- ✅ Osmosis: Pruning enabled, No snapshots configured
- ✅ Nolus-archive: Archive node (no pruning/snapshots)

## Updated Schedule

### Wednesday (Enterprise)
```
06:00-08:00  Osmosis pruning      ✅
10:00-12:00  Neutron pruning      ✅
14:00-15:00  Nolus state sync     ✅
16:00        Hermes restart       ✅
```
**Window**: 06:00-16:00 (10 hours) - Much more breathing room!

### Thursday (Horizon)
```
06:00-08:00  Vitosha pruning      ✅
08:00-10:00  Rila pruning         ✅
10:00-12:00  Osmo-test pruning    ✅
14:00-16:00  Pion pruning         ✅
18:00        Hermes restart       ✅
```
**Window**: 06:00-18:00 (12 hours) - Ends 3 hours earlier than before!

### Friday (Discovery)
```
06:00-08:00  Osmosis pruning      ✅
10:00-12:00  Neutron pruning      ✅
14:00-15:00  Nolus state sync     ✅
16:00        Hermes restart       ✅
```
**Window**: 06:00-16:00 (10 hours) - Much more breathing room!

## Benefits

✅ **Significant time savings**: 14 hours of snapshot operations removed  
✅ **Earlier completion**: All maintenance windows now finish earlier  
✅ **More breathing room**: Reduced risk of overlapping operations  
✅ **Simplified maintenance**: Fewer long-running operations  
✅ **External snapshots**: Can use optimized/fast snapshots from public providers  
✅ **Reduced load**: Less I/O and CPU usage during maintenance windows  

## External Snapshot Sources

For the disabled nodes, snapshots can be obtained from:

**Osmosis**:
- Polkachu: https://polkachu.com/tendermint_snapshots/osmosis
- Official: https://docs.osmosis.zone/networks/join-mainnet

**Neutron**:
- Polkachu: https://polkachu.com/tendermint_snapshots/neutron
- Official: https://docs.neutron.org/neutron/join-network

**Nolus** (still creating locally):
- ✅ Pirin, Vitosha, Rila continue to create snapshots
- These are the only snapshots we still need locally

## Configuration Files Changed

- ✅ `config/enterprise.toml` - Disabled osmosis & neutron snapshots
- ✅ `config/discovery.toml` - Disabled osmosis & neutron snapshots
- ✅ `config/horizon.toml` - Disabled osmo-test & pion snapshots
- ✅ `config/SCHEDULE.md` - Updated schedule documentation

## Verification

After deployment, verify:
1. Pruning operations still execute as scheduled
2. Hermes restarts happen after pruning completes
3. No snapshot operations for osmosis/neutron/osmo-test/pion
4. Only Nolus snapshots continue (pirin, vitosha, rila)

**Last Updated**: 2025-01-21
