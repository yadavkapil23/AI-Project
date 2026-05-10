# AEGIS Phase 2: Week 3 - Distributed KV-Cache Coordination ⏳

## Objective

Extend the KV-cache allocator from single-node (Phase 1) to multi-node, enabling:
- **Global block ownership tracking** (which node owns which cache blocks)
- **Cross-node allocation** (choose best node, allocate remotely)
- **Failure recovery** (rebalance cache on node death)
- **Consistency validation** (all replicas agree on cache state)

---

## Architecture: From Single-Node to Distributed

### Phase 1 (Week 1-2): Single Node

```
┌──────────────────────────────┐
│   KVCacheAllocator (Node 1)  │
├──────────────────────────────┤
│ blocks: DashMap<id, KVBlock> │
│ lru_queue: VecDeque<BlockId> │
│ metrics: Latency, hits, miss │
└──────────────────────────────┘
```

**Problem**: No allocation strategy across nodes. All cache on one machine = limited capacity.

### Phase 2 Week 3: Distributed KV-Cache

```
┌─────────────────────────────────────────────────────────────┐
│              DistributedKVCache (Coordinator)                │
├─────────────────────────────────────────────────────────────┤
│ • node_map: BlockId → NodeId  (ownership tracking)         │
│ • node_allocators: Map<NodeId, RemoteAllocator>            │
│ • replication_log: AllocationEvents (audit trail)          │
│ • local_state_hash: BLAKE3(all allocations) (consistency)  │
└──────────────────────────────────────────────────────────────┘
         ↓                      ↓                      ↓
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│  Node 1          │  │  Node 2          │  │  Node 3          │
├──────────────────┤  ├──────────────────┤  ├──────────────────┤
│ Local blocks:    │  │ Local blocks:    │  │ Local blocks:    │
│ [1, 2, 5, 8]     │  │ [3, 4, 9, 10]    │  │ [6, 7, 11, 12]   │
│ capacity: 4GB    │  │ capacity: 4GB    │  │ capacity: 4GB    │
└──────────────────┘  └──────────────────┘  └──────────────────┘
```

---

## Week 3 Deliverables

### 1. DistributedKVCache Struct (~200 LOC)

```rust
pub struct DistributedKVCache {
    // Ownership: "block ID X is owned by node Y"
    node_map: Arc<DashMap<BlockId, NodeId>>,
    
    // Remote allocators for each peer
    node_allocators: Arc<DashMap<NodeId, RemoteAllocator>>,
    
    // Local allocator (for blocks on this node)
    local_allocator: Arc<Mutex<KVCacheAllocator>>,
    
    // Audit trail: all allocation decisions
    replication_log: Arc<ReplicatedLog>,
    
    // Consistency: hash of all active allocations
    state_hash: Arc<Mutex<Blake3Hash>>,
    
    // Metrics
    metrics: DistributedCacheMetrics,
}
```

### 2. Remote Allocation Client (~150 LOC)

```rust
pub struct RemoteAllocator {
    node_id: NodeId,
    grpc_client: AllocationServiceClient,
    node_addr: String,
    
    // Local cache of node's capacity
    known_capacity: Arc<Mutex<NodeCapacity>>,
    health_check: Arc<Mutex<HealthStatus>>,
}

impl RemoteAllocator {
    pub async fn allocate(&self, num_blocks: usize) -> Result<Vec<BlockHandle>> {
        // 1. Check health
        self.health_check().await?;
        
        // 2. RPC call to remote node
        let request = AllocationRequest {
            num_blocks: num_blocks as u32,
            trace_id: Uuid::new_v4().to_string(),
        };
        
        let response = self.grpc_client.allocate(request).await?;
        
        // 3. Update local capacity cache
        self.known_capacity.lock().free -= num_blocks;
        
        Ok(response.into())
    }
    
    pub async fn deallocate(&self, blocks: Vec<BlockId>) -> Result<()> {
        // RPC call to release blocks
        let request = DeallocationRequest { blocks };
        self.grpc_client.deallocate(request).await?;
        
        // Update capacity
        self.known_capacity.lock().free += blocks.len();
        Ok(())
    }
    
    async fn health_check(&self) -> Result<()> {
        let status = self.grpc_client.health_check().await?;
        *self.health_check.lock() = status.into();
        Ok(())
    }
}
```

### 3. Block Ownership Tracking (~100 LOC)

```rust
pub struct BlockOwnership {
    // Which node owns each block
    block_to_node: DashMap<BlockId, NodeId>,
    
    // Which blocks a node owns
    node_to_blocks: DashMap<NodeId, Vec<BlockId>>,
    
    // When ownership was assigned (for migration decisions)
    ownership_timestamps: DashMap<BlockId, Instant>,
}

impl BlockOwnership {
    pub fn register_block(&self, block_id: BlockId, node_id: NodeId) -> Result<()> {
        self.block_to_node.insert(block_id, node_id);
        self.node_to_blocks
            .entry(node_id)
            .or_insert_with(Vec::new)
            .push(block_id);
        self.ownership_timestamps.insert(block_id, Instant::now());
        Ok(())
    }
    
    pub fn owner_of(&self, block_id: BlockId) -> Result<NodeId> {
        self.block_to_node
            .get(&block_id)
            .map(|entry| entry.clone())
            .ok_or_else(|| anyhow!("Block {} not owned by any node", block_id))
    }
    
    /// When a node dies, find its blocks for migration
    pub fn blocks_owned_by(&self, node_id: &NodeId) -> Vec<BlockId> {
        self.node_to_blocks
            .get(node_id)
            .map(|entry| entry.clone())
            .unwrap_or_default()
    }
    
    /// Reassign blocks from dead node to healthy node
    pub fn migrate_blocks(&self, from: NodeId, to: NodeId, blocks: Vec<BlockId>) -> Result<()> {
        for block_id in blocks {
            self.block_to_node.insert(block_id, to);
        }
        Ok(())
    }
}
```

### 4. Node Selection Algorithm (~100 LOC)

Choose which node to allocate new blocks on based on:
1. **Available capacity** (free memory)
2. **Expected latency** (network distance, current load)
3. **Load balance** (distribute evenly)

```rust
pub struct NodeSelector {
    allocators: Arc<DashMap<NodeId, RemoteAllocator>>,
    metrics: Arc<NodeMetrics>,
}

impl NodeSelector {
    /// Select best node for allocation
    pub async fn choose_node(&self, num_blocks: usize) -> Result<NodeId> {
        let mut candidates = Vec::new();
        
        // Gather info about all nodes
        for entry in self.allocators.iter() {
            let node_id = entry.key().clone();
            let allocator = entry.value();
            
            // Health check
            if allocator.health_check().await.is_err() {
                continue;
            }
            
            let capacity = allocator.known_capacity.lock();
            
            // Must have enough space
            if capacity.free < num_blocks {
                continue;
            }
            
            let latency = self.metrics.get_latency(&node_id);
            let load = self.metrics.get_load(&node_id);
            
            candidates.push((
                node_id,
                Score {
                    capacity_ratio: (capacity.free as f32) / (capacity.total as f32),
                    latency_ms: latency,
                    load_percent: load,
                },
            ));
        }
        
        // Sort by: 1) available capacity, 2) latency, 3) load
        candidates.sort_by(|a, b| {
            let score_a = a.1.capacity_ratio * (1.0 - a.1.latency_ms / 100.0) * (1.0 - a.1.load_percent / 100.0);
            let score_b = b.1.capacity_ratio * (1.0 - b.1.latency_ms / 100.0) * (1.0 - b.1.load_percent / 100.0);
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        candidates
            .first()
            .map(|(node_id, _)| node_id.clone())
            .ok_or_else(|| anyhow!("No suitable node for allocation"))
    }
}
```

### 5. Failure Detection & Recovery (~150 LOC)

```rust
pub struct FailureDetector {
    allocators: Arc<DashMap<NodeId, RemoteAllocator>>,
    ownership: Arc<BlockOwnership>,
    replication_log: Arc<ReplicatedLog>,
    dead_nodes: Arc<DashSet<NodeId>>,
}

impl FailureDetector {
    /// Periodically check node health
    pub async fn health_check_loop(&self, check_interval: Duration) {
        loop {
            tokio::time::sleep(check_interval).await;
            
            for entry in self.allocators.iter() {
                let node_id = entry.key().clone();
                let allocator = entry.value();
                
                match allocator.health_check().await {
                    Ok(_) => {
                        // Node is alive, mark healthy
                        self.dead_nodes.remove(&node_id);
                    }
                    Err(e) => {
                        // Node is dead
                        info!("Node {} health check failed: {}", node_id, e);
                        self.dead_nodes.insert(node_id.clone());
                        
                        // Trigger recovery
                        let _ = self.recover_blocks_from_dead_node(&node_id).await;
                    }
                }
            }
        }
    }
    
    /// When node dies, find healthy node and migrate its blocks
    async fn recover_blocks_from_dead_node(&self, dead_node_id: &NodeId) -> Result<()> {
        // Find all blocks owned by dead node
        let blocks = self.ownership.blocks_owned_by(dead_node_id);
        
        info!("Recovering {} blocks from dead node {}", blocks.len(), dead_node_id);
        
        // Find healthy node with capacity
        let healthy_node = self.find_healthy_node_with_capacity(blocks.len()).await?;
        
        // Log the recovery action
        self.replication_log.append(LogEntry::BlocksMigrated {
            from_node: dead_node_id.clone(),
            to_node: healthy_node.clone(),
            blocks: blocks.clone(),
            timestamp: Instant::now(),
        })?;
        
        // Update ownership
        self.ownership.migrate_blocks(dead_node_id.clone(), healthy_node, blocks)?;
        
        info!("Successfully recovered {} blocks", blocks.len());
        Ok(())
    }
    
    async fn find_healthy_node_with_capacity(&self, required_blocks: usize) -> Result<NodeId> {
        for entry in self.allocators.iter() {
            let node_id = entry.key().clone();
            
            if self.dead_nodes.contains(&node_id) {
                continue;
            }
            
            let allocator = entry.value();
            if allocator.health_check().await.is_ok() {
                let capacity = allocator.known_capacity.lock();
                if capacity.free >= required_blocks {
                    return Ok(node_id);
                }
            }
        }
        
        Err(anyhow!("No healthy node with sufficient capacity"))
    }
}
```

### 6. Consistency Validation (~100 LOC)

```rust
pub struct ConsistencyValidator {
    ownership: Arc<BlockOwnership>,
    replication_log: Arc<ReplicatedLog>,
    state_hash: Arc<Mutex<Blake3Hash>>,
}

impl ConsistencyValidator {
    /// Verify all nodes agree on cache state
    pub async fn validate_consistency(&self, all_nodes: Vec<NodeId>) -> Result<()> {
        // Get local state hash
        let local_hash = *self.state_hash.lock();
        
        // Query each remote node for its state hash
        for node_id in all_nodes {
            let remote_hash = self.query_remote_hash(&node_id).await?;
            
            if local_hash != remote_hash {
                return Err(anyhow!(
                    "Consistency violation: local hash {} != remote hash {} on node {}",
                    local_hash,
                    remote_hash,
                    node_id
                ));
            }
        }
        
        info!("Consistency check passed");
        Ok(())
    }
    
    /// Recompute state hash after any allocation change
    pub fn update_state_hash(&self, ownership: &BlockOwnership) -> Result<()> {
        let mut hasher = blake3::Hasher::new();
        
        // Hash all ownership relationships
        let mut entries: Vec<_> = ownership.block_to_node.iter()
            .map(|ref_multi| (ref_multi.key().clone(), ref_multi.value().clone()))
            .collect();
        
        entries.sort_by_key(|e| e.0);
        
        for (block_id, node_id) in entries {
            hasher.update(format!("{}:{}", block_id, node_id).as_bytes());
        }
        
        *self.state_hash.lock() = hasher.finalize().into();
        Ok(())
    }
    
    async fn query_remote_hash(&self, node_id: &NodeId) -> Result<Blake3Hash> {
        // RPC call to get remote state hash
        // TODO: Implement gRPC endpoint
        todo!("Implement RemoteKVCache::get_state_hash() RPC")
    }
}
```

---

## Integration Points

### Update Scheduler (aegis-scheduler)

The existing `KVCacheAllocator` becomes a component of the distributed system:

```rust
// OLD: Single-node allocator
pub struct KVCacheAllocator {
    blocks: DashMap<BlockId, KVBlock>,
    lru_queue: VecDeque<BlockId>,
}

// NEW: Distributed wrapper
pub struct DistributedScheduler {
    // For requests on this node
    local_allocator: Arc<Mutex<KVCacheAllocator>>,
    
    // Coordinated across all nodes
    distributed_cache: Arc<DistributedKVCache>,
    
    // Tell speculative decode where blocks are
    block_router: Arc<BlockRouter>,
}

impl DistributedScheduler {
    pub async fn allocate(&self, request_id: &str, num_blocks: usize) -> Result<Vec<BlockHandle>> {
        // Ask distributed system to find best node
        let blocks = self.distributed_cache.allocate_global(request_id, num_blocks).await?;
        
        // Return handles that know where blocks actually live
        Ok(blocks)
    }
}
```

### Update Speculative Coordinator (aegis-speculative)

The coordinator needs to know where KV cache blocks are:

```rust
// Before: Doesn't matter where blocks are (all local)
let blocks = self.scheduler.allocate(num_blocks).await?;

// After: Speculative coordinator routes operations to correct node
let blocks = self.distributed_scheduler.allocate(request_id, num_blocks).await?;

for block in &blocks {
    match block.owner {
        BlockOwner::Local => {
            // Keep processing on this node
        }
        BlockOwner::Remote(node_id) => {
            // Send KV update to node_id
            self.send_kv_update(&block, kv_data, node_id).await?;
        }
    }
}
```

---

## Testing Strategy

### Unit Tests (~50 tests)

```bash
# Test ownership tracking
cargo test -p aegis-scheduler -- block_ownership

# Test node selection
cargo test -p aegis-scheduler -- node_selection

# Test failure recovery
cargo test -p aegis-scheduler -- failure_recovery

# Test consistency
cargo test -p aegis-scheduler -- consistency
```

### Integration Tests (~20 tests)

```bash
# 3-node Docker Compose
docker-compose up -d
cargo test -p aegis-scheduler --test distributed_integration -- --nocapture
```

### Failure Scenarios

1. **Node 1 dies, Node 2 + 3 recover its blocks**
   - Assert: All blocks reallocated
   - Assert: Consistency check passes

2. **Network partition (1 vs 2+3)**
   - Assert: Minority rejects allocations
   - Assert: Majority continues serving
   - After heal: Consistency restored

3. **Concurrent allocations race**
   - Assert: No double-allocation of same block
   - Assert: All allocations logged

---

## Success Criteria

✅ **Multi-node ownership tracking**
- [ ] All blocks have registered owner
- [ ] `owner_of(block_id)` returns correct node
- [ ] No orphaned blocks

✅ **Cross-node allocation**
- [ ] `allocate_global()` chooses least-loaded node
- [ ] Allocation succeeds when capacity exists
- [ ] Allocation fails gracefully when no capacity

✅ **Failure recovery**
- [ ] Dead node detection within 5 seconds
- [ ] Blocks migrated to healthy node within 10 seconds
- [ ] No data loss

✅ **Consistency validation**
- [ ] State hash computation correct
- [ ] All nodes agree on cache state
- [ ] Consistency check catches divergence

✅ **Integration with speculative decode**
- [ ] Speculative coordinator allocates blocks via distributed scheduler
- [ ] KV cache operations routed to correct node
- [ ] End-to-end acceptance rates unchanged

---

## Files to Create/Modify

### New Files

```
aegis-scheduler/src/
├── distributed.rs         (DistributedKVCache struct)
├── remote_allocator.rs    (RemoteAllocator client)
├── block_ownership.rs     (BlockOwnership tracking)
├── node_selector.rs       (NodeSelector algorithm)
├── failure_detector.rs    (FailureDetector + recovery)
├── consistency.rs         (ConsistencyValidator)
└── tests/
    └── distributed_integration.rs
```

### Modified Files

```
aegis-scheduler/src/
├── lib.rs                 (export new modules)
└── allocator.rs          (keep, becomes component of distributed)

aegis-speculative/src/
└── coordinator.rs        (update to use DistributedScheduler)
```

### gRPC Updates

```
aegis-proto/proto/
└── scheduling.proto      (add distributed RPC methods)
   ├── rpc AllocateGlobal()
   ├── rpc DeallocateGlobal()
   ├── rpc GetStateHash()
   └── rpc GetOwnership()
```

---

## Code Statistics

- **New code**: ~900 LOC (distributed coordination)
- **Modified code**: ~150 LOC (scheduler + speculative integration)
- **Tests**: ~70 new tests
- **gRPC methods**: 4 new

---

## Week 3 Timeline

| Phase | Time | Task |
|-------|------|------|
| 1 | Day 1-2 | Core DistributedKVCache + RemoteAllocator |
| 2 | Day 2-3 | BlockOwnership + NodeSelector |
| 3 | Day 3-4 | FailureDetector + ConsistencyValidator |
| 4 | Day 4-5 | Integration tests + Docker Compose testing |
| 5 | Day 5 | Documentation + metrics |

---

## Next: Week 4

Once distributed cache coordination works, Week 4 adds **distributed tracing**:
- Trace each allocation decision across nodes
- Visibility into where blocks went
- Latency attribution

This enables:
- Debugging cache allocation issues
- Performance bottleneck identification
- Audit trail for compliance

---

## Summary

Week 3 transforms the single-node KV cache allocator into a distributed, fault-tolerant system:

✅ **Ownership tracking**: Every block knows which node owns it
✅ **Allocation strategy**: Choose best node based on capacity/latency
✅ **Failure recovery**: Automatically rebalance blocks when node dies
✅ **Consistency**: All replicas agree on cache state

**Result**: Speculative decode can run on any node, with KV cache spread across the cluster. One node death doesn't lose cache data.

---

**Next**: Week 4 - OpenTelemetry Distributed Tracing
