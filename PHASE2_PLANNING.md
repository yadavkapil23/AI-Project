# AEGIS Phase 2: Strategic Planning

## Overview

Phase 2 transforms AEGIS from an **infrastructure skeleton** (Phase 1) into a **production-ready inference system** that can actually run real models at scale.

**Phase 1** = Infrastructure + metrics + correctness  
**Phase 2** = Models + distribution + production ops

---

## What We Have (Phase 1)

✅ **Complete infrastructure**:
- gRPC gateway with auth/rate limiting
- KV cache allocator with scheduling
- Speculative decoding coordinator
- Safety policy enforcement
- Cryptographic audit trails
- Distributed state skeleton
- Comprehensive metrics

❌ **What's Missing**:
- Real LLM models
- Multi-node coordination
- Production auth/TLS
- Adaptive resource management
- Persistence layer
- Load testing

---

## Phase 2: What We Need to Do

### 6 Major Work Streams

#### 1. **Real LLM Model Integration** ⭐ (HIGHEST IMPACT)
**Problem**: Currently using synthetic token generation  
**Solution**: Wire real inference engines

**Options**:

| Option | Effort | Quality | Flexibility | Time |
|--------|--------|---------|-------------|------|
| **vLLM** | High | Excellent | High | 2-3 weeks |
| **llama.cpp** | Medium | Good | Medium | 1-2 weeks |
| **Mock** | Low | N/A | None | 2 days |
| **Abstraction Layer** | Medium | Excellent | Max | 2 weeks |

**What vLLM gives you**:
- ✅ Continuous batching (2-4x throughput improvement)
- ✅ KV cache-aware scheduling (native paging)
- ✅ Multiple model support (Llama, Qwen, Mistral, etc.)
- ✅ Hardware optimizations (Flash-Attention, etc.)
- ✗ Python dependency (need FFI layer)

**What llama.cpp gives you**:
- ✅ Pure C++ (easy to integrate)
- ✅ GGUF format (quantized models)
- ✅ CPU inference (no CUDA required)
- ✅ Small binary size
- ✗ No continuous batching (slower)
- ✗ Single model at a time

**What Abstraction Layer gives you**:
- ✅ Switch between backends at runtime
- ✅ Test with mock, deploy with vLLM
- ✅ Support both GPU and CPU inference
- ✗ More code to maintain

---

#### 2. **Multi-Node Raft Consensus** (CRITICAL FOR SCALE)
**Problem**: Currently single-node only  
**Solution**: Implement distributed state via Raft

**What you gain**:
- ✅ Horizontal scaling (add nodes, increase capacity)
- ✅ Fault tolerance (node fails, cluster survives)
- ✅ Load balancing (requests distributed)
- ✅ Shared KV cache (reuse across nodes)

**Architecture**:
```
Client Request
     ↓
API Gateway (round-robin across nodes)
     ↓
Node 1, Node 2, Node 3 (Raft cluster)
     ↓
Shared KV Cache (replicated state)
     ↓
Token generation (coordinated)
```

**Implementation approach**:
1. Replace single-node log with `tokio-raft` or `async-raft`
2. Add cluster discovery (static or service discovery)
3. Implement leader election + log replication
4. Add snapshot mechanism for recovery
5. Benchmark: measure replication latency

**Complexity**: HIGH (~4-5 weeks)

---

#### 3. **Production Auth & TLS** (SECURITY)
**Problem**: Stub auth, no encryption  
**Solution**: Production-grade security

**Required**:
- ✅ mTLS (client certificates)
- ✅ OAuth2 integration (delegated auth)
- ✅ Rate limiting per client/model
- ✅ Audit logging (who did what)
- ✅ Credential rotation

**Implementation**:
```rust
// Phase 1 (stub):
if token.starts_with("bearer-") {
    Ok(())
}

// Phase 2 (production):
let claims = decode_jwt(&token)?;
validate_signature(&claims)?;
check_scopes(&claims, &required_scopes)?;
audit_log(&claims, &action)?;
```

**Complexity**: MEDIUM (~2-3 weeks)

---

#### 4. **Adaptive KV Cache Sizing** (PERFORMANCE)
**Problem**: Fixed cache size, suboptimal under varying load  
**Solution**: Dynamic allocation based on workload

**How it works**:
```
Monitor:
  - Queue depth
  - Cache hit rate
  - Memory pressure
  
Adapt:
  - Increase block size if hit rate > 80%
  - Decrease if fragmentation > 10%
  - Redistribute across models
  - Evict cold models
  
Measure:
  - Hit rate impact
  - Latency improvement
  - Memory efficiency
```

**Complexity**: MEDIUM (~3 weeks)

---

#### 5. **Persistence & Recovery** (RELIABILITY)
**Problem**: No disk storage, audit trails lost on crash  
**Solution**: Durable audit + checkpointing

**Required**:
- ✅ Append-only audit log to disk
- ✅ State snapshots (e.g., every 1000 events)
- ✅ Recovery from latest snapshot
- ✅ Disk I/O optimization (batch writes)

**Complexity**: MEDIUM (~2-3 weeks)

---

#### 6. **Load Testing & Observability** (VALIDATION)
**Problem**: Don't know behavior under load  
**Solution**: Realistic benchmark framework

**Required**:
- ✅ Load generator (configurable request patterns)
- ✅ Latency tracking (P50, P95, P99, P99.9)
- ✅ Throughput measurement
- ✅ Resource utilization tracking
- ✅ Grafana dashboards

**Complexity**: MEDIUM (~2-3 weeks)

---

## Strategic Options

### Option A: "Full Production Stack" (8-10 weeks)
```
Phase 2a (4 weeks): vLLM integration + real models
Phase 2b (3 weeks): Raft consensus + multi-node
Phase 2c (2 weeks): Auth/TLS + persistence
Phase 2d (1 week): Load testing + dashboards
```

**Result**: Production-ready system  
**Cost**: 8-10 weeks of engineering  
**Risk**: High (everything integrated)

### Option B: "Model-First" (4-5 weeks)
```
Phase 2a (2 weeks): vLLM integration
Phase 2b (1 week): Basic load testing
Phase 2c (1-2 weeks): Auth/TLS
```

**Result**: Working system with real models  
**Cost**: 4-5 weeks  
**Risk**: No distributed scaling yet  
**Use case**: Single machine deployment

### Option C: "Infrastructure-First" (6-7 weeks)
```
Phase 2a (3 weeks): Raft consensus
Phase 2b (2 weeks): vLLM integration
Phase 2c (1-2 weeks): Load testing
```

**Result**: Scalable infrastructure  
**Cost**: 6-7 weeks  
**Risk**: vLLM integration complexity  
**Use case**: Distributed system from day 1

### Option D: "Incremental" (Pick & Choose)
```
Pick each component independently:
- Start with vLLM
- Then add Raft if needed
- Then add TLS as deployment nears
```

**Result**: Flexible, pragmatic  
**Cost**: Variable (3-10 weeks depending on scope)  
**Risk**: Low (validate each step)

---

## My Recommendation: Option B + Progressive Expansion

### Phase 2a: "Get Models Working" (2 weeks)
**Goal**: AEGIS running real LLMs

```
1. Implement vLLM integration layer
   - Create inference backend trait
   - Wrap vLLM server API calls
   - Add model loading/unloading
   
2. Update speculative coordinator
   - Use real vLLM for draft/verify
   - Measure actual speedup
   - Verify metrics accuracy
   
3. Test end-to-end
   - Run e2e_inference benchmark
   - Compare Phase 1 metrics vs reality
   - Validate safety constraints
```

**Deliverable**: Running Llama-2-7B (or similar) through AEGIS  
**Metrics**: Real latency, throughput, acceptance rates  
**Time**: 2 weeks

### Phase 2b: "Add Distribution" (3 weeks, if needed)
**Goal**: Scale beyond single node

```
1. Implement Raft consensus
   - Replace single-node log with Raft
   - Add cluster coordination
   - Test failover
   
2. Load balancing
   - Add request router
   - Implement round-robin
   - Track per-node capacity
   
3. Distributed cache
   - Coordinate KV allocation across nodes
   - Implement cache invalidation
   - Test consistency
```

**Deliverable**: 3-node cluster running coordinated inference  
**Metrics**: Node failure recovery, distributed throughput  
**Time**: 3 weeks

### Phase 2c: "Production Hardening" (2-3 weeks, parallel)
**Goal**: Ready for real users

```
1. Auth/TLS
   - Add mTLS support
   - Implement OAuth2
   - Add rate limiting per client
   
2. Persistence
   - Write audit logs to disk
   - Implement snapshotting
   - Test recovery
   
3. Monitoring
   - Grafana dashboards
   - Alert rules
   - Health checks
```

**Deliverable**: System ready for production deployment  
**Metrics**: Security posture, MTTR, observability  
**Time**: 2-3 weeks

---

## Technical Decisions to Make Now

### 1. Model Backend
**Question**: vLLM or llama.cpp?

**vLLM**:
- ✅ Continuous batching (2-4x faster)
- ✅ More models supported
- ✗ Requires Python + CUDA/ROCm
- ✗ More complex FFI layer

**llama.cpp**:
- ✅ Pure C++ (simpler)
- ✅ GGUF quantization
- ✗ No batching (slower)
- ✗ Limited to llama family

**Recommendation**: **vLLM** for maximum throughput, **llama.cpp** for simplicity. Start with vLLM, add llama.cpp as fallback.

### 2. Consensus Approach
**Question**: Full Raft or simplified consensus?

**Full Raft**:
- ✅ Proven, well-tested
- ✅ Production-grade
- ✗ Complex implementation
- ✗ More testing needed

**Simplified (e.g., consensus on leader only)**:
- ✅ Simpler code
- ✗ Less fault-tolerant
- ✗ Not true distributed

**Recommendation**: Go **full Raft** using `async-raft` crate. It's well-maintained and reduces implementation risk.

### 3. Persistence Strategy
**Question**: Write-through or write-ahead?

**Write-through** (audit log after confirmed):
- ✅ Strong consistency
- ✗ Higher latency

**Write-ahead** (log before execution):
- ✅ Low latency
- ✗ Can have duplicates on crash

**Recommendation**: **Write-ahead log** for audit trail (durability), **async checkpoint** for recovery (performance).

### 4. Load Pattern
**Question**: What workload to optimize for?

**Option A**: High throughput (100s of concurrent requests)  
**Option B**: Low latency (1-2 requests, fast response)  
**Option C**: Mixed (realistic production mix)  

**Recommendation**: **Mixed**. Design for burst traffic + sustained load.

---

## Resource Estimates

### Personnel
- **1 Senior Systems Engineer**: Full Phase 2 (8-10 weeks)
- **OR 2 Engineers**: Parallel streams (4-5 weeks)

### Infrastructure
- **GPU for testing**: 1x A100 or 2x RTX 4090
- **CPU cluster**: 3 nodes for testing Raft
- **Storage**: 100GB for model weights

### Time Breakdown
```
vLLM Integration:        2-3 weeks
Raft Consensus:          3-4 weeks
Auth/TLS:                1-2 weeks
Persistence:             1-2 weeks
Load Testing:            1-2 weeks
Integration/Testing:     1-2 weeks
─────────────────────────────────
Total:                   8-10 weeks (sequential)
Or:                      4-5 weeks (with 2 engineers)
```

---

## Success Metrics (Phase 2)

### Functionality
- [ ] Real Llama model (7B-13B) running through AEGIS
- [ ] Speculative decoding working with real verifier
- [ ] Multi-node cluster with Raft consensus
- [ ] Production auth/TLS enabled
- [ ] Audit logs persisted to disk

### Performance
- [ ] P99 latency: < 200ms (10 tokens)
- [ ] Throughput: > 100 req/sec
- [ ] Cache hit rate: > 70%
- [ ] Speculative speedup: 2-4x
- [ ] Model inference latency: < 50ms/token

### Reliability
- [ ] MTBF: > 720 hours
- [ ] Recovery time: < 10 seconds
- [ ] No data loss on node failure
- [ ] Consistent state across cluster

### Operations
- [ ] Deployed in containers
- [ ] Grafana dashboards
- [ ] Alert rules configured
- [ ] Runbook documentation

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|-----------|
| vLLM integration complex | HIGH | Start with llama.cpp, add vLLM later |
| Raft consensus bugs | HIGH | Use proven crate (async-raft), extensive testing |
| Performance regression | MEDIUM | Benchmark at each stage |
| Model OOM issues | MEDIUM | Implement model swapping |
| Network partition during Raft | MEDIUM | Implement split-brain prevention |

---

## Go/No-Go Decision Points

### Checkpoint 1: vLLM Integration (Week 2)
**Go**: Real model + metrics match expectations  
**No-Go**: > 30% latency increase vs Phase 1  
**Decision**: Continue to Raft or stop

### Checkpoint 2: Raft Cluster (Week 5)
**Go**: 3-node cluster stable, <100ms sync latency  
**No-Go**: Consensus issues, data inconsistency  
**Decision**: Continue to production hardening or reduce scope

### Checkpoint 3: Full System (Week 8)
**Go**: All success metrics met, ready for staging  
**No-Go**: Critical bugs found  
**Decision**: Deploy or iterate

---

## Next Steps: Decision Framework

### To Decide Now:

1. **Scope**: Which option (A/B/C/D)?
2. **Timeline**: 4 weeks? 8 weeks? 10 weeks?
3. **Model**: vLLM or llama.cpp?
4. **Distribution**: Single node or multi-node?
5. **Team**: 1 engineer or 2?

### To Decide After vLLM Integration:

1. **Raft**: Do we need distributed state?
2. **Auth**: Priority vs other hardening?
3. **Persistence**: Write-through or write-ahead?

---

## Phase 2 Architecture

```
┌──────────────────────────────────────────────────┐
│          AEGIS Phase 2 System                    │
└──────────────────────────────────────────────────┘
              ↓
    ┌─────────────────────┐
    │  Load Balancer      │
    │ (request routing)   │
    └──────────┬──────────┘
               ↓
    ┌──────────────────────────────────┐
    │  Raft Cluster (3 nodes)          │
    │  ┌────────┬────────┬────────┐    │
    │  │ Node 1 │ Node 2 │ Node 3 │    │
    │  └────────┴────────┴────────┘    │
    │      ↓       ↓       ↓           │
    │  vLLM Servers (inference)        │
    │  Safety Monitors                 │
    │  Audit Engines                   │
    └──────────────────────────────────┘
               ↓
    ┌──────────────────────┐
    │  Shared KV Cache     │
    │ (distributed via Raft)
    └──────────────────────┘
               ↓
    ┌──────────────────────┐
    │  Persistence Layer   │
    │ (audit + snapshots)  │
    └──────────────────────┘
```

---

## Summary: What You Need to Decide

1. **What's your priority**?
   - Models working (2 weeks)
   - Distributed system (6 weeks)
   - Production-ready (8-10 weeks)

2. **What's your constraint**?
   - Time budget
   - Team size
   - Available hardware

3. **What's your deployment target**?
   - Single powerful machine
   - 3-5 node cluster
   - Cloud-native infrastructure

**Once you decide, I'll:**
- Create detailed implementation roadmap
- Set up Phase 2 project structure
- Build components incrementally
- Validate at each checkpoint
- Document thoroughly

---

**Ready to decide?**
