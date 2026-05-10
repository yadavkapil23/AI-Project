# AEGIS Phase 2: Detailed Implementation Roadmap

## Your Vision (Confirmed ✅)

**"SERIOUS systems/inference infrastructure project"**
- ✅ Infrastructure-first (6-7 weeks)
- ✅ Correctness > throughput
- ✅ Rollback safety as core feature
- ✅ Real distributed coordination
- ✅ Production-grade observability
- ✅ llama.cpp integration (backend-agnostic)
- ✅ Docker Compose → Kubernetes progression
- ✅ No UI/demo features

---

## Phase 2 Execution Plan

### Weekly Breakdown (6-7 weeks)

#### **Week 1: Foundation & Infrastructure Setup**

**Deliverables**:
- [ ] llama.cpp FFI layer (Rust wrapper)
- [ ] InferenceBackend trait (abstraction)
- [ ] Docker Compose for 3-node local cluster
- [ ] Simplified replicated log (NOT full Raft yet)

**Code**:

```
aegis/
├── inference-backends/       (NEW)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── traits.rs         (InferenceBackend trait)
│       ├── llama_cpp.rs      (llama.cpp wrapper)
│       └── mock_backend.rs   (for testing)
│
├── distributed/              (NEW)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── replicated_log.rs (simplified)
│       ├── node.rs           (cluster node)
│       └── coordinator.rs    (sync)
│
├── docker/                   (NEW)
│   ├── Dockerfile
│   ├── docker-compose.yml    (3-node setup)
│   └── entrypoint.sh
```

**Testing**:
```bash
cargo test -p aegis-inference-backends
docker-compose up -d
./scripts/test-local-cluster.sh
```

**Metrics**:
- [ ] Backend latency (ms/token)
- [ ] Token throughput (tokens/sec)
- [ ] FFI overhead (% of latency)

---

#### **Week 2: Speculative Decoding with Real Models**

**Deliverables**:
- [ ] Update SpeculativeCoordinator to use real backend
- [ ] Verify draft/verifier coordination
- [ ] Rollback safety validation
- [ ] Acceptance rate metrics

**Code Changes**:

```rust
// OLD (Phase 1): Synthetic tokens
fn generate_draft(&self, _request_id: &str) -> Result<Vec<Token>> {
    // Fake token generation
    Ok(vec![...])
}

// NEW (Phase 2): Real inference
fn generate_draft(&self, request_id: &str, prompt: &str) -> Result<Vec<Token>> {
    let tokens = self.backend.generate(
        prompt,
        self.draft_length,
        temperature=0.7,
    ).await?;
    
    self.metrics.record_draft_tokens(tokens.len());
    Ok(tokens)
}
```

**Testing**:
```bash
cargo test -p aegis-speculative -- --nocapture
cargo run --example speculative_e2e
```

**Benchmarks**:
- [ ] Draft latency vs token count
- [ ] Verify latency
- [ ] Acceptance rate (should be > 75%)
- [ ] Rollback frequency

**Metrics to track**:
- Draft latency percentiles (P50, P95, P99)
- Verifier latency
- Acceptance rate distribution
- Rollback overhead (ms)

---

#### **Week 3: Distributed KV-Cache Coordination**

**Deliverables**:
- [ ] Extend scheduler for multi-node
- [ ] Cache block ownership tracking
- [ ] Block migration on node failure
- [ ] Consistency validation

**Code Changes**:

```rust
// Phase 1: Single node
struct KVCacheAllocator {
    blocks: DashMap<usize, KVBlock>,
}

// Phase 2: Distributed
struct DistributedKVCache {
    local_blocks: DashMap<usize, KVBlock>,
    node_map: DashMap<usize, NodeId>,  // block → node owner
    replication_log: ReplicatedLog,     // audit trail
}

impl DistributedKVCache {
    async fn allocate_global(
        &self,
        request_id: &str,
        num_blocks: usize,
    ) -> Result<Vec<BlockHandle>> {
        // Find best node for allocation
        let node = self.choose_node()?;
        
        // Allocate remotely
        let blocks = node.allocate(num_blocks).await?;
        
        // Log to replicated log
        self.replication_log.append(LogEntry::BlocksAllocated {
            blocks: blocks.clone(),
            node: node.id,
        })?;
        
        Ok(blocks)
    }
}
```

**Testing**:
- [ ] Allocate blocks across 3 nodes
- [ ] Verify ownership tracking
- [ ] Simulate node failure, recover blocks
- [ ] Consistency check (all replicas agree)

**Benchmarks**:
- [ ] Cross-node allocation latency
- [ ] Cache hit rate (with replication)
- [ ] Replication overhead

---

#### **Week 4: OpenTelemetry Distributed Tracing**

**Deliverables**:
- [ ] Trace context propagation (across nodes)
- [ ] Span generation for each operation
- [ ] Full request path visualization
- [ ] Distributed trace export

**Code Structure**:

```rust
// Phase 1: Local traces only
pub async fn execute(&self, request: InferenceRequest) -> Result<Response> {
    let span = span!(Level::INFO, "execute");
    let _guard = span.enter();
    
    // ... all in single span
}

// Phase 2: Distributed spans
pub async fn execute(&self, request: InferenceRequest) -> Result<Response> {
    let trace_id = request.trace_id.clone();
    let root_span = span!(
        parent: None,
        target: "aegis::execute",
        trace_id = %trace_id,
        "execute_root"
    );
    let _guard = root_span.enter();
    
    // Gateway span
    let gateway_span = span!(
        parent: &root_span,
        "gateway::authenticate"
    );
    self.gateway.authenticate(&request, &gateway_span).await?;
    
    // Speculative span
    let spec_span = span!(
        parent: &root_span,
        "speculative::draft"
    );
    let tokens = self.speculative.generate_draft(..., &spec_span).await?;
    
    // KV scheduler span
    let kv_span = span!(
        parent: &root_span,
        "kv::allocate"
    );
    let blocks = self.scheduler.allocate(..., &kv_span).await?;
    
    // ... more spans
}
```

**Trace Format**:
```
Root Span (request-uuid)
├── Gateway Span (auth + rate limit)
│   ├── Token validation span
│   └── Rate limit check span
├── KV Allocation Span
│   ├── Node selection span
│   ├── Remote allocation span (on Node 2)
│   └── Replication log span
├── Speculative Decoding Span
│   ├── Draft generation span (on Node 1)
│   ├── Verify span (on Node 3)
│   ├── Rollback span (if needed)
│   └── Commit span
├── Audit Span
│   └── Hash computation span
└── Response Span (streaming)
```

**Testing**:
```bash
cargo test -p aegis-telemetry
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 cargo run --release
```

**Visualization** (Phase 2 later):
- Jaeger trace UI (see full request path)
- Flamegraph (bottleneck identification)
- Latency attribution (which span is slow)

---

#### **Week 5: Simplified Replicated Log + Consistency**

**Deliverables**:
- [ ] Multi-node replicated log
- [ ] Quorum-based consistency (not full Raft yet)
- [ ] Failover handling
- [ ] Log snapshot + recovery

**Design** (Simplified, NOT Full Raft):

```rust
// Simplified approach: Leader-based, quorum writes
pub struct ReplicatedLog {
    // If leader
    leader: Option<NodeId>,
    followers: Vec<NodeId>,
    pending_acks: usize,
    
    // Local log
    entries: Vec<LogEntry>,
    snapshots: Vec<Snapshot>,
}

impl ReplicatedLog {
    /// Append entry, wait for quorum ack
    pub async fn append(&self, entry: LogEntry) -> Result<u64> {
        if self.is_leader() {
            // Broadcast to followers
            let futures: Vec<_> = self.followers.iter()
                .map(|node| node.append_async(&entry))
                .collect();
            
            // Wait for quorum (>50%)
            let quorum_size = (self.followers.len() + 1) / 2 + 1;
            let acks = futures::future::select_ok(futures).await?;
            
            if acks.len() >= quorum_size {
                self.entries.push(entry.clone());
                Ok(self.entries.len() as u64)
            } else {
                Err(anyhow!("Quorum not reached"))
            }
        } else {
            // Forward to leader
            self.leader.append(entry).await
        }
    }
    
    /// Recover from snapshots + replay log
    pub async fn recover(&self) -> Result<State> {
        // Find latest snapshot
        let latest = self.snapshots.last()?;
        let mut state = latest.restore()?;
        
        // Replay entries after snapshot
        for entry in &self.entries[latest.log_index..] {
            state.apply(&entry)?;
        }
        
        Ok(state)
    }
}
```

**Testing**:
- [ ] Single leader, all followers up
- [ ] Leader failure, follower takes over
- [ ] Partition: leader on minority (must reject)
- [ ] Partition heals (catch-up replication)
- [ ] Snapshot + recovery

**Failure Scenarios**:
```
Scenario 1: Normal operation (3 nodes, 1 leader)
├─ Node 1 (leader)
├─ Node 2 (follower)
└─ Node 3 (follower)
→ All writes to leader, replicate to followers

Scenario 2: Leader failure
├─ Node 1 (DEAD)
├─ Node 2 (new leader, quorum with Node 3)
└─ Node 3 (follower)
→ Redirect to Node 2, reject writes to Node 1

Scenario 3: Network partition
├─ Part A: Node 1 (leader, minority)
│  └─ REJECTS writes (can't reach quorum)
└─ Part B: Node 2 + 3 (majority)
   └─ Node 2 becomes leader, ACCEPTS writes
→ After heal: Node 1 catches up from Node 2
```

**Metrics**:
- [ ] Replication latency (time for quorum ack)
- [ ] Log size growth
- [ ] Snapshot frequency
- [ ] Recovery time (from snapshot)

---

#### **Week 6: Integration + Benchmarks**

**Deliverables**:
- [ ] Full e2e with llama.cpp + distributed runtime
- [ ] Multi-node benchmarks
- [ ] Rollback overhead measurement
- [ ] Cache efficiency validation

**Benchmarks to Run**:

```bash
# 1. Single-node baseline (for comparison)
cargo bench --bench single_node_baseline -- --verbose

# 2. 3-node distributed
docker-compose up -d
cargo bench --bench distributed_3node -- --verbose

# 3. Failover scenario
./scripts/kill-node-1.sh
cargo bench --bench failover_recovery -- --verbose

# 4. Speculative decode with real models
cargo bench --bench speculative_with_llama -- --verbose

# 5. KV cache efficiency
cargo bench --bench kv_multi_node -- --verbose
```

**Benchmark Output Template**:

```
=== Single Node Baseline ===
End-to-end latency (10 tokens):
  P50: 45 ms
  P99: 120 ms
Cache hit rate: 78%
Token throughput: 220 tokens/sec

=== 3-Node Distributed ===
End-to-end latency (10 tokens):
  P50: 52 ms (15% overhead)
  P99: 145 ms
Replication latency: 5 ms
Cache hit rate: 76% (accounting for distributed)
Token throughput: 190 tokens/sec (15% reduction due to coordination)

=== Failover ===
Detection time: 2 seconds
Recovery time: 5 seconds
Lost requests: 0 (queued during failover)
Total downtime: 7 seconds

=== Speculative with llama.cpp ===
Draft latency: 30 ms (4 tokens)
Verify latency: 35 ms (4 tokens)
Acceptance rate: 81%
Speedup: 2.1x
Total latency: 40 ms (vs 65 ms serial)

=== KV Cache Multi-Node ===
Hit rate (3 nodes): 76%
Allocation latency: 8 ms (cross-node)
Fragmentation: 3%
Reuse across nodes: 65%
```

---

#### **Week 7: Docker Compose Setup + Kubernetes Prep**

**Deliverables**:
- [ ] Working docker-compose.yml (3-node cluster)
- [ ] Startup/shutdown scripts
- [ ] Health check endpoints
- [ ] Kubernetes manifests (for Phase 2.5)

**Docker Compose File**:

```yaml
version: '3.8'

services:
  # Shared storage for models
  model-store:
    image: minio/minio:latest
    environment:
      MINIO_ROOT_USER: admin
      MINIO_ROOT_PASSWORD: password
    ports:
      - "9000:9000"

  # AEGIS Node 1 (Leader)
  node1:
    build:
      context: .
      dockerfile: docker/Dockerfile
    environment:
      NODE_ID: "node-1"
      NODE_ADDR: "0.0.0.0:50051"
      PEERS: "node-2:50052,node-3:50053"
      LLAMA_MODEL_PATH: "/models/llama-7b.gguf"
      RUST_LOG: "info,aegis=debug"
    ports:
      - "50051:50051"  # gRPC
      - "9001:9001"    # Prometheus
    volumes:
      - ./models:/models:ro
      - node1-data:/var/lib/aegis
    depends_on:
      - model-store

  # AEGIS Node 2 (Follower)
  node2:
    build:
      context: .
      dockerfile: docker/Dockerfile
    environment:
      NODE_ID: "node-2"
      NODE_ADDR: "0.0.0.0:50052"
      PEERS: "node-1:50051,node-3:50053"
      LLAMA_MODEL_PATH: "/models/llama-7b.gguf"
      RUST_LOG: "info,aegis=debug"
    ports:
      - "50052:50052"
      - "9002:9001"
    volumes:
      - ./models:/models:ro
      - node2-data:/var/lib/aegis
    depends_on:
      - model-store

  # AEGIS Node 3 (Follower)
  node3:
    build:
      context: .
      dockerfile: docker/Dockerfile
    environment:
      NODE_ID: "node-3"
      NODE_ADDR: "0.0.0.0:50053"
      PEERS: "node-1:50051,node-2:50052"
      LLAMA_MODEL_PATH: "/models/llama-7b.gguf"
      RUST_LOG: "info,aegis=debug"
    ports:
      - "50053:50053"
      - "9003:9001"
    volumes:
      - ./models:/models:ro
      - node3-data:/var/lib/aegis
    depends_on:
      - model-store

  # Load balancer
  nginx:
    image: nginx:alpine
    volumes:
      - ./docker/nginx.conf:/etc/nginx/nginx.conf:ro
    ports:
      - "50050:50050"  # Load-balanced gRPC
    depends_on:
      - node1
      - node2
      - node3

  # Monitoring
  prometheus:
    image: prom/prometheus:latest
    volumes:
      - ./docker/prometheus.yml:/etc/prometheus/prometheus.yml:ro
    ports:
      - "9090:9090"
    depends_on:
      - node1
      - node2
      - node3

  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "16686:16686"  # UI
      - "4317:4317"    # OTLP receiver

volumes:
  node1-data:
  node2-data:
  node3-data:
```

**Scripts**:

```bash
# Start cluster
./scripts/start-cluster.sh

# Run health check
./scripts/health-check.sh

# Kill a node (for failover test)
./scripts/kill-node.sh <node-id>

# Restart node
./scripts/restart-node.sh <node-id>

# Full test suite
./scripts/test-all.sh
```

---

## Architecture Diagram (Phase 2)

```
┌─────────────────────────────────────────────────────────────────┐
│                     Phase 2 Architecture                        │
└─────────────────────────────────────────────────────────────────┘

                    ┌──────────────────┐
                    │  Load Balancer   │
                    │   (nginx)        │
                    └────────┬─────────┘
                             │
         ┌───────────────────┼───────────────────┐
         │                   │                   │
         ▼                   ▼                   ▼
    ┌─────────┐         ┌─────────┐         ┌─────────┐
    │ Node 1  │         │ Node 2  │         │ Node 3  │
    │(Leader) │         (Follower)│         (Follower)│
    └────┬────┘         └────┬────┘         └────┬────┘
         │                   │                   │
    ┌────┴───────────────────┼───────────────────┴────┐
    │   Simplified Replicated Log (Quorum-based)     │
    │   • Append-only entries                        │
    │   • Quorum acks (>50% nodes)                   │
    │   • Snapshot + recovery                        │
    │   • Leader election on failure                 │
    └──────────────────┬──────────────────────────────┘
                       │
    ┌──────────────────┴──────────────────┐
    │                                     │
    ▼                                     ▼
┌─────────────────┐               ┌──────────────────┐
│  Per-Node:      │               │  Shared via Log: │
├─────────────────┤               ├──────────────────┤
│ • Gateway       │               │ • KV block       │
│ • Auth/Rate Lim │               │   ownership      │
│ • llama.cpp     │               │ • Execution      │
│ • Speculative   │               │   state          │
│ • Safety        │               │ • Audit trail    │
│ • Audit         │               │                  │
│ • Local metrics │               │                  │
└─────────────────┘               └──────────────────┘

         ▼

┌─────────────────────────────────────────────────────────┐
│   OpenTelemetry Tracing (Distributed Spans)            │
│   • Root trace ID propagates to all nodes              │
│   • Each operation creates child span                   │
│   • Spans sent to Jaeger collector                      │
└─────────────────────────────────────────────────────────┘

         ▼

┌─────────────────────────────────────────────────────────┐
│   Metrics + Observability Stack                        │
│   • Prometheus (metrics scrape)                        │
│   • Jaeger (distributed traces)                        │
│   • Grafana (dashboards - Phase 2.5)                   │
└─────────────────────────────────────────────────────────┘
```

---

## Rollback Safety (Core Feature)

**Why rollback matters**:
- Speculative decode generates wrong tokens → must undo
- Inconsistency detected → must revert to last safe state
- Node failure → must recover consistent state

**Implementation**:

```rust
pub struct Checkpoint {
    log_index: u64,
    kv_blocks_allocated: Vec<usize>,
    kv_blocks_freed: Vec<usize>,
    audit_hash: [u8; 32],
    timestamp: u64,
}

impl Rollback {
    /// Create checkpoint before speculative execution
    pub async fn create_checkpoint(&self, request_id: &str) -> Result<Checkpoint> {
        Ok(Checkpoint {
            log_index: self.log.last_index(),
            kv_blocks_allocated: self.cache.allocated_blocks(),
            kv_blocks_freed: vec![],  // None yet
            audit_hash: self.audit.current_hash(),
            timestamp: now_ns(),
        })
    }
    
    /// Rollback to checkpoint on verification failure
    pub async fn rollback_to_checkpoint(
        &self,
        request_id: &str,
        checkpoint: &Checkpoint,
    ) -> Result<()> {
        // 1. Restore KV cache state
        self.cache.free_blocks(&checkpoint.kv_blocks_allocated)?;
        
        // 2. Reset log to checkpoint
        self.log.truncate_to(checkpoint.log_index)?;
        
        // 3. Verify audit trail integrity
        let current_hash = self.audit.current_hash();
        if current_hash != checkpoint.audit_hash {
            return Err(anyhow!("Audit trail mismatch after rollback"));
        }
        
        // 4. Log rollback event
        self.audit.record(AuditEvent {
            event_type: "ROLLBACK".to_string(),
            request_id: request_id.to_string(),
            checkpoint_index: checkpoint.log_index,
        })?;
        
        self.metrics.record_rollback();
        Ok(())
    }
}
```

**Rollback Scenarios**:

| Scenario | Action | Recovery Time |
|----------|--------|----------------|
| Speculative verify fails | Rollback KV + log to checkpoint | < 10 ms |
| Node crashes during spec | Restart node, recover from log | < 5 sec |
| Inconsistency detected | Revert to snapshot | < 10 sec |
| Entire cluster down | Recover from disk snapshots | < 30 sec |

---

## Testing Strategy (Phase 2)

### Unit Tests (Per-Module)
```bash
cargo test -p aegis-inference-backends
cargo test -p aegis-distributed
cargo test -p aegis-speculative -- --nocapture
cargo test -p aegis-scheduler
```

### Integration Tests
```bash
# Local cluster via docker-compose
./scripts/start-cluster.sh
cargo test --test integration_tests -- --test-threads=1

# Failover scenarios
./scripts/test-failover.sh

# Rollback validation
./scripts/test-rollback-safety.sh
```

### Benchmarks
```bash
cargo bench --bench single_node
cargo bench --bench distributed_3node
cargo bench --bench speculative_llama
cargo bench --bench kv_distributed
cargo bench --bench failover_overhead
```

---

## Success Metrics (Phase 2 Completion)

### Functionality ✅
- [ ] llama.cpp integration working
- [ ] Speculative decode with real models
- [ ] 3-node cluster operational
- [ ] Quorum-based replication working
- [ ] Failover + recovery tested
- [ ] Rollback safety validated

### Performance ✅
- [ ] P99 latency: < 150ms (10 tokens)
- [ ] Throughput: > 150 tokens/sec (shared across 3 nodes)
- [ ] Replication latency: < 10ms
- [ ] Cache hit rate: > 70%
- [ ] Failover detection: < 2 sec
- [ ] Recovery time: < 5 sec

### Observability ✅
- [ ] OpenTelemetry spans for all operations
- [ ] Distributed traces visible in Jaeger
- [ ] Prometheus metrics scraping
- [ ] Latency attribution working
- [ ] Bottleneck identification possible

### Reliability ✅
- [ ] Zero data loss on node failure
- [ ] Consistent state across replicas
- [ ] Rollback safety verified
- [ ] Audit trail integrity proven
- [ ] Recovery from snapshot works

---

## Deliverables Checklist

### Code
- [ ] `aegis-inference-backends` crate (llama.cpp FFI)
- [ ] `aegis-distributed` crate (replication + consensus)
- [ ] Updated `aegis-runtime` (multi-node orchestration)
- [ ] Updated `aegis-speculative` (real model integration)
- [ ] Updated `aegis-scheduler` (distributed KV)
- [ ] Updated `aegis-telemetry` (distributed tracing)

### Infrastructure
- [ ] Docker Compose setup (3-node local)
- [ ] Start/stop/health-check scripts
- [ ] Failover test scripts
- [ ] Rollback test scripts
- [ ] Model downloading script

### Documentation
- [ ] Phase 2 architecture (updated ARCHITECTURE.md)
- [ ] Deployment guide (docker-compose + kubectl)
- [ ] Operational runbook (troubleshooting, recovery)
- [ ] Metrics guide (what to monitor)
- [ ] Distributed tracing guide (how to debug)

### Benchmarks
- [ ] Single-node baseline
- [ ] 3-node distributed
- [ ] Failover overhead
- [ ] Speculative with llama.cpp
- [ ] KV distributed efficiency
- [ ] Rollback overhead

### Tests
- [ ] 50+ unit tests
- [ ] 10+ integration tests
- [ ] 5+ failover scenarios
- [ ] 3+ rollback test cases
- [ ] 100% critical path coverage

---

## Implementation Start Date

**Ready to begin Week 1?**

Next steps:
1. Create `aegis-inference-backends` crate
2. Write llama.cpp FFI wrapper
3. Design InferenceBackend trait
4. Set up docker-compose template
5. Begin distributed log implementation

---

**Let's build the real thing.** ✅
