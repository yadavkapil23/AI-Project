# AEGIS - Getting Started Guide

Quick reference for new team members. Start here, then dive into detailed docs.

---

## What is AEGIS in 30 Seconds?

**AEGIS** = Smart load balancer + coordinator for AI inference across multiple GPU servers

**Key insight**: Prevents failure, balances load, maintains state consistency

**Example**: ChatGPT with 20 GPU servers that automatically recover from failures

---

## What Problem Does It Solve?

### Before AEGIS ❌
```
GPU Server 1 crashes
→ 500 pending requests LOST
→ Customers angry 😠
→ Manual recovery takes 30 minutes
→ Lost revenue: $5,000+
```

### After AEGIS ✅
```
GPU Server 1 crashes
→ AEGIS detects in 95ms
→ Requests redirected to Server 2
→ Automatic recovery <500ms
→ Zero requests lost
→ Customers see 100ms delay (unnoticeable)
```

---

## How Does It Work? (Simple Version)

```
Client Request
    ↓
AEGIS: "Which server should handle this?"
    ↓
AEGIS decides → sends to Server A
    ↓
Server A processes
    ↓
Server A shares result with all other servers
    ↓
Response back to client
```

**The magic**: If Server A crashes, Server B has the exact same state and can take over.

---

## Architecture in Plain English

Think of AEGIS as 5 layers of a building:

```
🏢 Layer 5: Front Desk (receives client requests)
📋 Layer 4: Manager (makes decisions, everyone agrees)
💾 Layer 3: Shared Storage (everyone has copy of data)
🔒 Layer 2: Backup System (nothing is ever lost)
🌐 Layer 1: Phones (servers talk to each other)
```

Each layer builds on the one below. Remove one = whole building fails.

---

## Get Started in 5 Minutes

### Option 1: Try it Locally (Fastest)

```bash
# 1. Navigate to project
cd /path/to/aegis-scheduler

# 2. Start 3-node cluster with Docker
docker-compose up -d

# 3. Check if it's running
curl http://localhost:8000/health

# 4. View metrics
curl http://localhost:8000/metrics | grep aegis_

# 5. View dashboard
# Open http://localhost:3000 in browser (Grafana)

# 6. Stop it
docker-compose down
```

### Option 2: Run Tests

```bash
# All tests
cargo test --release

# Specific test suite (watch failures auto-recover)
cargo test --test failure_recovery_tests

# Watch chaos testing (node failures injected)
cargo test --test chaos_tests -- --nocapture
```

### Option 3: Read Code

```bash
# Key files to understand
scheduler/src/lib.rs                    # Overview
scheduler/src/consensus_grpc_server.rs  # Networking
scheduler/src/persistence.rs            # Durability
consensus/src/lib.rs                    # Decision-making
```

---

## Key Concepts (5 Minute Read)

### 1. **Consensus** = All nodes agree on one truth
```
Problem: Three servers, network unreliable. Who's in charge?
Solution: Vote! Whoever gets majority wins.
Result: Only one decision at a time, no conflicts
```

### 2. **Replication** = Copy data to all servers
```
Server A decides something
→ Server A tells Server B "Remember this"
→ Server A tells Server C "Remember this"
→ Now if Server A dies, B and C have the data
```

### 3. **Quorum** = Majority votes
```
With 5 servers:
  3+ votes = "We can make decisions"
  2 or less = "We don't have quorum, wait"
  
If split: 3 servers in one group, 2 in another
  Group A (3): Has quorum, continues ✓
  Group B (2): No quorum, stops ✗
  
Result: Only one group acts, no conflicts!
```

### 4. **Durability** = Never lose data
```
Decision made: "Request 123 assigned to Server A"
1. Write to disk (FSYNC - wait for disk confirmation)
2. Then tell servers about it
3. Power fails? Read disk on restart, recover all decisions

No data lost! ✓
```

### 5. **Failover** = Automatic recovery
```
Server A is leader
Server B misses heartbeat (150ms, suspicious)
Server B: "Maybe A is dead, I'm running for leader"
Server C votes for B
Server B: "I'm new leader! Everyone follow me"

Server A actually was dead = smooth recovery ✓
Server A still alive = A says "I'm still leader", wins ✓
```

---

## Important Files to Know

### Documentation (Start Here)
| File | Time | Purpose |
|------|------|---------|
| `PROJECT_OVERVIEW.md` | 15 min | Complete explanation (you are here) |
| `ARCHITECTURE.md` | 20 min | Detailed design |
| `DEPLOYMENT.md` | 30 min | How to deploy |
| `OPERATIONAL_RUNBOOKS.md` | 20 min | How to fix problems |

### Code (Then Read This)
| File | What It Does |
|------|--------------|
| `scheduler/src/lib.rs` | Main module, exports everything |
| `scheduler/src/consensus_grpc_server.rs` | Networking & RPC |
| `scheduler/src/persistence.rs` | Durable log (WAL) |
| `consensus/src/lib.rs` | Consensus algorithm |
| `scheduler/src/failure_detector.rs` | Health checking |

### Config (For Deployment)
| File | Purpose |
|------|---------|
| `docker-compose.yml` | Local development |
| `Dockerfile` | Container image |
| `kubernetes/` | Production deployment |
| `PREREQUISITES.md` | What you need |

### Tests (Learn by Example)
| File | Tests |
|------|-------|
| `tests/chaos_tests.rs` | Failure injection (60+ scenarios) |
| `tests/failure_recovery_tests.rs` | Recovery procedures |
| `tests/network_hardening_tests.rs` | Network resilience |
| `tests/state_machine_*.rs` | Data consistency |

---

## Typical Scenarios

### Scenario 1: Your First Day

```
1. Read this document (GETTING_STARTED.md) ← You are here
2. Read PROJECT_OVERVIEW.md (full explanation)
3. Run docker-compose to see it working
4. Read ARCHITECTURE.md to understand layers
5. Look at scheduler/src/lib.rs, skim the code
6. Ask questions! ← This is expected and good
```

### Scenario 2: You Need to Fix a Bug

```
1. Read the failing test (understand what broke)
2. Read ARCHITECTURE.md (find which layer)
3. Find the code (grep for function name)
4. Fix it
5. Run cargo test (verify fix)
6. Submit for review
```

### Scenario 3: You're Deploying to Production

```
1. Read PREREQUISITES.md (do you have all dependencies?)
2. Read DEPLOYMENT.md (which deployment method?)
3. Read DEPLOYMENT_CHECKLIST.md (verify everything)
4. Choose: Docker / Kubernetes / Bare Metal
5. Follow deployment guide
6. Read OPERATIONAL_RUNBOOKS.md (how to operate)
```

### Scenario 4: System is Down

```
1. Check health: curl http://<node>:8000/health
2. Check metrics: curl http://<node>:8000/metrics
3. Look for "quorum_healthy_peers" < 2 (quorum lost = BAD)
4. Check logs: docker logs or journalctl
5. Find relevant section in OPERATIONAL_RUNBOOKS.md
6. Follow recovery procedure
7. Verify with health endpoint again
```

---

## Critical Things to Know

### ⚠️ Never Do This
- [ ] Don't kill all 3 nodes at once (no recovery possible)
- [ ] Don't delete /var/lib/aegis/ directory (data loss)
- [ ] Don't run with 2 nodes only (no quorum tolerance)
- [ ] Don't ignore "quorum lost" alerts (system stopping)

### ✅ Always Do This
- [ ] Monitor the health endpoint regularly
- [ ] Keep 3+ nodes running
- [ ] Backup WAL and snapshots daily
- [ ] Review logs for errors
- [ ] Test recovery procedures regularly

### 📊 Metrics to Watch
```
aegis_quorum_healthy_peers
  → < 2 = ALERT! QUORUM LOST = BAD
  → 3 = Normal

aegis_rpc_latency_ms (p99)
  → > 100ms = Something slow
  → > 500ms = Node likely dead

aegis_consensus_leader_id
  → Should stay same (unless node dies)
  → Changing constantly = unstable network

aegis_wal_entries_total
  → Should grow steadily
  → Growing too fast = high write load
  → Not growing = no activity

aegis_election_duration_ms
  → Should be < 500ms
  → > 1s = network or CPU slow
```

---

## Troubleshooting Quick Guide

### "Cluster reports quorum lost"
```
Problem: Less than 2 healthy nodes
Solution:
  1. Check which nodes are down: curl http://<node>:8000/health
  2. Restart down nodes: systemctl restart aegis-scheduler
  3. Wait 30 seconds for recovery
  4. Verify: curl http://<node>:8000/health
```

### "Node not joining cluster"
```
Problem: New node can't talk to others
Solution:
  1. Check firewall: is port 6000 open?
  2. Check DNS: can nodes resolve each other?
  3. Check AEGIS_PEERS env var: is it correct?
  4. Check logs: docker logs <node> or journalctl
```

### "High latency (>500ms)"
```
Problem: Requests slow
Solution:
  1. Check network: ping between nodes < 10ms?
  2. Check CPU: is server CPU maxed out?
  3. Check memory: is server out of RAM?
  4. Check storage: is disk full?
  5. Check metrics: any failed requests?
```

### "Can't recover from failure"
```
Problem: Failed node doesn't come back
Solution:
  1. Check logs: docker logs <node>
  2. Check disk: is /var/lib/aegis accessible?
  3. Check permissions: can aegis user read files?
  4. Check connectivity: can node reach others on port 6000?
  5. If stuck: wipe disk and restart (data recovers from WAL)
```

---

## Common Questions

**Q: How many nodes do I need?**
A: Minimum 3. Odd numbers better (3, 5, 7).

**Q: What if a node is slow?**
A: AEGIS will timeout and use other nodes. No problem.

**Q: Can I add nodes without downtime?**
A: Yes, but requires brief maintenance to update peer list.

**Q: How do I know it's working?**
A: curl http://localhost:8000/health should show quorum: true

**Q: What's the latency overhead?**
A: 5-10ms per request. Mostly network.

**Q: Is data safe?**
A: Yes. Write-ahead log ensures durability. Even power loss doesn't lose data.

**Q: Can I scale to 100 nodes?**
A: Theoretically yes (architecture supports it). Tested to 5 nodes.

**Q: What if all nodes crash?**
A: State is durable on disk. Restart all nodes, system recovers automatically.

---

## Learning Path (Recommended Order)

### Week 1: Understand the System
- [ ] Read PROJECT_OVERVIEW.md (this file, extended)
- [ ] Read ARCHITECTURE.md
- [ ] Run docker-compose locally
- [ ] Watch nodes in Grafana dashboard

### Week 2: Understand the Code
- [ ] Read scheduler/src/lib.rs
- [ ] Read consensus/src/lib.rs
- [ ] Read key test files
- [ ] Run tests locally

### Week 3: Understand Deployment
- [ ] Read DEPLOYMENT.md
- [ ] Read PREREQUISITES.md
- [ ] Try Docker setup
- [ ] Try Kubernetes setup

### Week 4: Understand Operations
- [ ] Read OPERATIONAL_RUNBOOKS.md
- [ ] Read DEPLOYMENT_CHECKLIST.md
- [ ] Practice failure scenarios
- [ ] Learn metrics monitoring

### Month 2+: Specialize
- [ ] Choose specialty: Dev / Ops / Architecture / Testing
- [ ] Deep dive into relevant components
- [ ] Contribute to project

---

## Resources at Your Fingertips

### Quick Links
- **Architecture**: [ARCHITECTURE.md](ARCHITECTURE.md)
- **Deployment**: [DEPLOYMENT.md](DEPLOYMENT.md)
- **Operations**: [OPERATIONAL_RUNBOOKS.md](OPERATIONAL_RUNBOOKS.md)
- **Prerequisites**: [PREREQUISITES.md](PREREQUISITES.md)
- **Checklist**: [DEPLOYMENT_CHECKLIST.md](DEPLOYMENT_CHECKLIST.md)

### Useful Commands
```bash
# Check health
curl http://localhost:8000/health | jq .

# Check metrics
curl http://localhost:8000/metrics

# View logs
docker logs aegis-node-1
journalctl -u aegis-scheduler -f

# Run tests
cargo test --release
cargo test --test chaos_tests

# Start local cluster
docker-compose up -d

# Stop local cluster
docker-compose down
```

---

## Next Steps

1. **Read** PROJECT_OVERVIEW.md (the full detailed version)
2. **Run** docker-compose locally and explore
3. **Ask** your team lead any questions
4. **Read** ARCHITECTURE.md to go deeper
5. **Explore** the code in scheduler/src/

---

## Who to Ask

- **Architecture questions** → Read ARCHITECTURE.md or ask design lead
- **Code questions** → Read code comments or ask tech lead
- **Deployment questions** → Read DEPLOYMENT.md or ask ops lead
- **General questions** → Ask your team lead or project manager

---

## Good Resources for Background Knowledge

Want to understand distributed systems better? Read these (optional):

1. **Raft Consensus**: [raft.github.io](http://raft.github.io) (15 min read)
2. **Consensus Explained**: "Raft: Understandable Consensus Algorithm" (video, 20 min)
3. **Distributed Systems**: "The Phoenix Project" (book, but business-focused)
4. **Practical**: "Designing Data-Intensive Applications" (Chapter 8-9, optional)

**Don't feel pressured to read all of this!** AEGIS handles consensus internally. You just need to know:
- **More nodes** = More reliable
- **Quorum needed** = Majority decides
- **Leader coordinates** = One decision maker
- **Automatic recovery** = Failures handled

---

## Welcome! 🚀

You're joining a team building production-grade distributed systems. The codebase is:
- ✅ Well-tested (365+ tests)
- ✅ Well-documented (comprehensive guides)
- ✅ Well-architected (5-layer design)
- ✅ Production-ready (deployed with confidence)

Take your time learning. Ask questions. You'll be productive in a week. Expert in a month.

Let's build something great! 💪

---

**Project**: AEGIS Distributed AI Inference Scheduler  
**Status**: Production Ready  
**Last Updated**: 
