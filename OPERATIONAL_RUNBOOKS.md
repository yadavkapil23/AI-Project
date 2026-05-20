# AEGIS Consensus System - Operational Runbooks

**Version**: 1.0  

**System**: Distributed consensus with Raft-inspired algorithm  

---

## Table of Contents

1. [Normal Operations](#normal-operations)
2. [Failure Scenarios](#failure-scenarios)
3. [Maintenance & Upgrades](#maintenance--upgrades)
4. [Monitoring & Alerting](#monitoring--alerting)
5. [Troubleshooting](#troubleshooting)

---

## Normal Operations

### Cluster Startup

**Prerequisites**:
- N nodes ready (recommend 3, 5, or 7 for production)
- Network connectivity between all nodes
- Persistent storage available for WAL

**Procedure**:
```
1. Start all nodes simultaneously or staggered (< 30s apart)
2. Nodes will begin leader election
3. Wait for leader to be elected (< 500ms)
4. Verify quorum: healthy_count > N/2
5. Monitor health_status for confirmation
```

**Success Indicators**:
- ✅ One leader elected
- ✅ All followers replicating
- ✅ Quorum status: true
- ✅ No errors in logs

### Normal Operation Checklist

- [ ] All nodes reporting healthy
- [ ] Leader is responsive
- [ ] All followers in replication
- [ ] Quorum maintained
- [ ] Metrics showing < 5ms latency
- [ ] Success rate > 99%
- [ ] No failed_attempts on any peer

---

## Failure Scenarios

### Scenario 1: Single Node Failure

**Detection**:
- RPC timeout to node (default: 5000ms)
- After max_retries (3): node marked unhealthy
- Detection time: < 100ms after failure occurs

**Automatic Response**:
```
Node fails
  ↓
RPC timeouts detected (< 5s)
  ↓
Peer marked unhealthy
  ↓
Cluster continues (quorum check: passes if N > 2)
  ↓
No leader election needed (not leader)
```

**Action Required**: None (automatic recovery)

**Recovery**:
```
Node recovers
  ↓
RPC succeeds
  ↓
Health state restored
  ↓
Automatic catch-up via log replication
  ↓
Consistency restored (< 1 second)
```

**Verification**:
- [ ] health_status shows node healthy
- [ ] State hashes match across cluster
- [ ] No data loss (WAL preserved state)

---

### Scenario 2: Leader Failure

**Detection**:
- Leadership timeout (heartbeat stops)
- Followers detect missing heartbeats
- Election starts automatically

**Automatic Response**:
```
Leader fails
  ↓
Followers detect timeout (< 150ms)
  ↓
New election starts
  ↓
Followers vote for candidate
  ↓
New leader elected (< 500ms)
  ↓
Replication resumes
```

**Action Required**: None (automatic election)

**During Election**:
- Cluster stops accepting writes (quorum check)
- No data loss possible
- Recovery time: < 500ms

**Verification**:
- [ ] New leader elected
- [ ] Quorum still maintains
- [ ] Write operations resume
- [ ] No data corruption

---

### Scenario 3: Multiple Node Failures (Cascading)

**Detection per Node**:
- Each node failure detected independently
- After max_retries: marked unhealthy

**Response**:

**If > N/2 nodes fail**: Lost quorum
```
Node A fails
  ↓
Node B fails
  ↓
Node C fails (out of 5)
  ↓
3 healthy, 2 failed: 3 > 2.5 = QUORUM OK
  ↓
Continue normal operation
```

**If ≤ N/2 nodes healthy**: Lost quorum
```
4 nodes fail (out of 5)
  ↓
1 healthy: 1 ≤ 2.5 = QUORUM LOST
  ↓
Cluster stops accepting writes
  ↓
Wait for recovery
```

**Action Required**:
- [ ] Check node health: `health_status()`
- [ ] Determine root cause (network? hardware?)
- [ ] Initiate recovery for failed nodes
- [ ] Monitor recovery progress

**Recovery**:
- Recover nodes in sequence (avoid thundering herd)
- Each recovery restores one node
- Quorum restored when needed threshold reached
- Catch-up automatic for recovered nodes

---

### Scenario 4: Network Partition

**Detection**:
- Nodes in partition A can't reach nodes in partition B
- Health tracking shows peers unreachable
- After max_retries: peers marked unhealthy

**Behavior**:

**Majority Partition** (> N/2 healthy):
```
Partition occurs
  ↓
Majority has quorum: Continue operations
  ↓
Writes accepted (safe)
  ↓
Minority partition isolated
```

**Minority Partition** (≤ N/2 healthy):
```
Partition occurs
  ↓
Minority lost quorum: Block writes
  ↓
Automatic quorum check prevents writes
  ↓
Wait for partition heal
```

**Action Required**:
- [ ] Identify network issue
- [ ] Check connectivity between nodes
- [ ] Fix network connection
- [ ] Monitor partition healing

**Healing**:
```
Network healed
  ↓
Peers reachable again
  ↓
Health state restored
  ↓
Minority catches up via replication
  ↓
Consistency verified (state hash match)
```

---

## Maintenance & Upgrades

### Rolling Restart (Zero Downtime)

**For N-node cluster, can restart N-1 at a time while maintaining quorum**

**Procedure**:
```
1. Choose node to restart (start with followers, not leader)
2. Stop node gracefully (allow in-flight RPCs to complete)
3. Monitor: Verify cluster maintains quorum
4. Wait: Automatic health state change (< 100ms)
5. Restart node
6. Wait: Automatic recovery and catch-up (< 1 second)
7. Repeat for next node
```

**Example (5-node cluster)**:
```
Before: 5 healthy, quorum = 3
Stop node-1: 4 healthy > 2.5 ✓
Stop node-2: 3 healthy > 2.5 ✓
Stop node-3: 2 healthy ≤ 2.5 ✗ (STOP HERE)

Restart node-1: 3 healthy > 2.5 ✓
Restart node-2: 4 healthy > 2.5 ✓
Restart node-3: 5 healthy > 2.5 ✓
```

**Verification**:
- [ ] Before restart: health_status shows all healthy
- [ ] During restart: quorum maintained
- [ ] After restart: node health restored
- [ ] State hashes match across cluster

### Configuration Changes

**Steps**:
1. Update configuration on one node
2. Restart node (falls into rolling restart procedure)
3. Repeat for all other nodes
4. Verify changes applied cluster-wide

### Software Upgrade

**Same as rolling restart**:
1. Stop oldest node
2. Deploy new version
3. Restart node
4. Wait for recovery
5. Repeat for next node

**Verification**:
- [ ] All nodes upgraded
- [ ] All nodes healthy
- [ ] Quorum maintained throughout
- [ ] No data loss

---

## Monitoring & Alerting

### Key Metrics to Monitor

**Cluster Health**:
- `healthy_peers`: Should equal total_peers
- `has_quorum`: Should be true
- `failed_attempts`: Should be 0 for all peers

**Performance**:
- `average_latency_ms`: Should be < 50ms
- `success_rate`: Should be > 99%
- `timeout_count`: Should be rare

**Node Health**:
- `last_heartbeat_ms`: Should be < 30s
- `rpc_count`: Should be increasing
- `consecutive_failures`: Should be 0

### Alert Thresholds

| Alert | Threshold | Action |
|-------|-----------|--------|
| Lost Quorum | has_quorum = false | Page on-call, investigate |
| High Latency | avg_latency > 100ms | Investigate network |
| Failed Peers | failed_attempts > 0 | Check peer status |
| Low Success Rate | success_rate < 95% | Investigate issues |
| Cascading Failures | 2+ failures | Check hardware/network |

---

## Troubleshooting

### Problem: Cluster Lost Quorum

**Symptoms**:
- `has_quorum()` returns false
- Writes being rejected
- No leader can be elected

**Investigation**:
```
1. Check health_status() for all nodes
2. Count healthy_peers: Is it > N/2?
3. Check network connectivity
4. Check node logs for failures
```

**Solution**:
```
If network partition:
  - Fix network connection
  - Monitor automatic healing
  
If node hardware failure:
  - Investigate failed node
  - Replace hardware
  - Restart node
  
If cascading failures:
  - Check for common cause (power loss? network issue?)
  - Recover one node at a time
  - Monitor quorum restoration
```

### Problem: High Latency

**Symptoms**:
- `average_latency_ms` > 100ms
- Slow write operations

**Investigation**:
```
1. Check network connectivity
2. Check node CPU/memory usage
3. Check WAL disk I/O latency
4. Review recent log entries for errors
```

**Solution**:
```
If network issue:
  - Check bandwidth
  - Reduce other network traffic
  - Investigate packet loss
  
If resource constraint:
  - Scale up node resources
  - Reduce load temporarily
  - Monitor recovery
  
If disk I/O:
  - Use faster disk
  - Reduce fsync frequency (trade-off: durability)
  - Monitor WAL size
```

### Problem: Failed Catch-Up After Recovery

**Symptoms**:
- Node recovered but state doesn't match others
- Persistent consistency errors

**Investigation**:
```
1. Check state_hash() on recovered node vs others
2. Check log entries on recovered node
3. Review recovery logs for errors
```

**Solution**:
```
1. Verify network connectivity is stable
2. Restart recovery process
3. Monitor log replication
4. Verify consistency before resuming operations
```

### Problem: Leader Not Elected

**Symptoms**:
- No leader in cluster
- Elections seem to fail repeatedly

**Investigation**:
```
1. Check if quorum exists
2. Check election logs
3. Verify network connectivity
```

**Solution**:
```
If quorum exists:
  - Wait for election timeout (< 500ms)
  - Monitor automatic election
  
If no quorum:
  - Recover failed nodes first
  - Then leader election will proceed
```

---

## Quick Reference

### Health Check Command
```
GET /health
Response:
{
  "is_running": true,
  "peer_count": 5,
  "healthy_peers": 5,
  "has_quorum": true,
  "metrics": {
    "avg_latency_ms": 25,
    "success_rate": 0.9999
  }
}
```

### Maintenance Decision Tree

```
Need to do maintenance?
  ├─ Rolling restart?
  │   └─ Can take N-1 nodes offline
  │       └─ Proceed with rolling restart
  ├─ Need to upgrade?
  │   └─ Same as rolling restart
  ├─ Need to change config?
  │   └─ Do rolling restart with config changes
  └─ Emergency shutdown?
      └─ Make sure recovery can restore state
```

### Common Failure Recovery Times

| Failure | Detection | Recovery | Total |
|---------|-----------|----------|-------|
| Single node | < 100ms | < 100ms | < 200ms |
| Leader | < 150ms | < 500ms | < 650ms |
| Network partition | < 1s | < 5s | < 6s |
| Cascading (gradual) | < 100ms each | < 100ms each | Varies |

---

## Support & Escalation

**Level 1 (Operational)**:
- Quorum check
- Node health verification
- Basic restart procedures

**Level 2 (Engineering)**:
- Network investigation
- Performance analysis
- Recovery procedure optimization

**Level 3 (Architecture)**:
- Design changes
- Capacity planning
- Major system changes

---

**For questions, refer to metrics, logs, and health_status()**

