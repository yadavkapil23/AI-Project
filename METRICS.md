# AEGIS Metrics Reference

Complete metric definitions for Phase 1 MVP.

## Metric Hierarchy

```
┌─ Gateway Metrics
│  ├─ Request Latency (histogram)
│  ├─ Queue Depth (gauge)
│  ├─ Active Streams (counter)
│  ├─ Auth Failures (counter)
│  └─ Rate Limited (counter)
│
├─ Scheduler Metrics
│  ├─ Cache Hit Rate (histogram)
│  ├─ Fragmentation (gauge)
│  ├─ Allocation Latency (histogram)
│  ├─ Evictions (counter)
│  └─ Block Accounting
│
├─ Speculative Metrics
│  ├─ Acceptance Rate (histogram)
│  ├─ Draft Length (histogram)
│  ├─ Rollback Count (counter)
│  └─ Speedup Factor (calculated)
│
├─ Safety Metrics
│  ├─ Violations (counter)
│  ├─ Fallbacks (counter)
│  ├─ Policy Checks (counter)
│  └─ Violation Rate (calculated)
│
└─ Audit Metrics
   ├─ Event Count (counter)
   ├─ Hash Latency (histogram)
   └─ Trail Size (gauge)
```

## Gateway Metrics

### request_latency_ms
- **Type**: Histogram
- **Unit**: milliseconds
- **Buckets**: [1, 5, 10, 50, 100, 500, 1000]
- **Meaning**: End-to-end inference request latency (client perspective)
- **Calculation**: wall-clock time from request receipt to final token
- **Example**: P50=8ms, P99=50ms indicates 99% of requests complete within 50ms
- **Target**: P99 < 500ms

### active_streams
- **Type**: Gauge (atomic counter)
- **Unit**: count
- **Meaning**: Number of in-flight inference requests
- **Use**: Capacity planning, detect backlog
- **Example**: Active=500 out of 1000 max = 50% utilization

### queue_depth
- **Type**: Gauge
- **Unit**: count
- **Meaning**: Requests waiting for processing slot
- **Use**: Early warning for overload
- **Example**: Depth=100 with max_concurrent=1000 = queue forming

### total_requests
- **Type**: Counter
- **Unit**: count
- **Meaning**: Cumulative inference requests received
- **Use**: Throughput = total_requests / elapsed_time

### total_completed
- **Type**: Counter
- **Unit**: count
- **Meaning**: Successfully processed requests
- **Use**: Success rate = total_completed / total_requests

### total_failed
- **Type**: Counter
- **Unit**: count
- **Meaning**: Failed/errored requests
- **Example**: Out of memory, timeout, auth failure

### total_rate_limited
- **Type**: Counter
- **Unit**: count
- **Meaning**: Requests rejected by rate limiter
- **Use**: Detect if rate limit too low

### avg_latency_ms
- **Type**: Calculated (sum of latencies / count)
- **Unit**: milliseconds
- **Meaning**: Mean latency across all requests
- **Use**: Overall performance trend

### p99_latency_ms
- **Type**: Calculated (99th percentile)
- **Unit**: milliseconds
- **Meaning**: Latency of slowest 1% of requests
- **Use**: SLA compliance checking
- **Why P99**: More representative than max, less sensitive to outliers

## Scheduler (KV Cache) Metrics

### kv_cache_hit_rate
- **Type**: Histogram
- **Unit**: ratio [0.0, 1.0]
- **Buckets**: [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 0.95, 1.0]
- **Meaning**: Percentage of cache allocation reuse
- **Calculation**: cache_hits / (cache_hits + cache_misses)
- **Example**: 0.75 = 75% of allocations found existing KV blocks
- **Target**: > 0.7 (good locality)

### kv_cache_fragmentation
- **Type**: Gauge (percentage)
- **Unit**: ratio [0.0, 1.0]
- **Meaning**: Free memory / total memory
- **Calculation**: free_blocks / total_blocks
- **Example**: 0.05 = 5% fragmentation (95% utilization)
- **Target**: < 0.05
- **Why it matters**: Wasted space reduces effective cache capacity

### total_allocations
- **Type**: Counter
- **Unit**: count
- **Meaning**: Total KV blocks allocated since startup
- **Use**: Throughput calculation

### total_deallocations
- **Type**: Counter
- **Unit**: count
- **Meaning**: Total KV blocks freed
- **Use**: Should ≈ allocations (steady state)

### total_evictions
- **Type**: Counter
- **Unit**: count
- **Meaning**: Blocks evicted by LRU/LFU policy
- **Use**: Indicates cache pressure
- **High values**: Frequent eviction suggests insufficient cache

### avg_allocation_latency_us
- **Type**: Histogram
- **Unit**: microseconds
- **Meaning**: Time to allocate a single KV block
- **Buckets**: [1, 5, 10, 50, 100, 500]
- **Target**: < 10 µs
- **Why**: Allocation on hot path, should be near-instant

### allocation_latency_percentiles
- **P50**: Median allocation time
- **P99**: 99th percentile allocation time
- **Max**: Maximum observed latency
- **Use**: Detect allocation slowdowns

## Speculative Decoding Metrics

### speculative_acceptance_rate
- **Type**: Histogram
- **Unit**: ratio [0.0, 1.0]
- **Buckets**: [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 0.95, 1.0]
- **Meaning**: Percentage of draft tokens approved by verifier
- **Calculation**: accepted_tokens / total_draft_tokens
- **Example**: 0.8 = verifier approves 80% of drafted tokens
- **Target**: > 0.75

### draft_length
- **Type**: Histogram
- **Unit**: count
- **Meaning**: Number of tokens drafted before verification
- **Buckets**: [1, 2, 4, 8, 16, 32]
- **Example**: Avg draft_length = 5 tokens
- **Adaptive**: Increases if acceptance > 0.85, decreases if < 0.7

### total_rollbacks
- **Type**: Counter
- **Unit**: count
- **Meaning**: Verification failures requiring rollback
- **Calculation**: count of accept=false decisions
- **High values**: Verifier disagrees frequently, lower draft_length

### speculative_speedup
- **Type**: Calculated
- **Unit**: ratio
- **Formula**: (draft_tokens + accepted_tokens) / (draft_passes + 1)
- **Example**: 4 draft + 3 accepted = 7 / 2 = 3.5x speedup
- **Meaning**: How many times faster than single-token generation
- **Target**: 2-4x

## Safety Monitor Metrics

### policy_violations
- **Type**: Counter
- **Unit**: count
- **Meaning**: Policy constraint violations detected
- **Example**: Tool execution without auth
- **Use**: Detect malicious or erroneous behavior

### safety_fallbacks
- **Type**: Counter
- **Unit**: count
- **Meaning**: Fallback actions triggered
- **Example**: Reject request, use safer model
- **Use**: How often safety overrides performance

### total_policy_checks
- **Type**: Counter
- **Unit**: count
- **Meaning**: Total policy evaluations
- **Use**: Baseline for violation rate

### violation_rate
- **Type**: Calculated
- **Unit**: ratio [0.0, 1.0]
- **Formula**: violations / total_checks
- **Example**: 0.01 = 1% of policy checks fail
- **Target**: < 0.001 (< 0.1%)

### policy_check_latency
- **Type**: Histogram
- **Unit**: microseconds
- **Meaning**: Time to evaluate a single policy
- **Target**: < 10 µs
- **Why**: On hot path, should not impact latency

## Audit Engine Metrics

### audit_events_total
- **Type**: Counter
- **Unit**: count
- **Meaning**: Total audit events recorded
- **Example**: Request received, token generated, rollback, etc.

### audit_hash_latency_us
- **Type**: Histogram
- **Unit**: microseconds
- **Buckets**: [1, 5, 10, 50, 100, 500]
- **Meaning**: Time to hash a single event (BLAKE3)
- **Expected**: 2-10 µs per event

### audit_trail_size
- **Type**: Gauge
- **Unit**: bytes
- **Meaning**: Current size of audit log in memory
- **Use**: Memory usage tracking

### hash_verification_latency
- **Type**: Histogram
- **Unit**: milliseconds
- **Meaning**: Time to verify entire audit trail
- **Calculation**: Replay all events and recompute hashes
- **Use**: Recovery RTO (recovery time objective)

## Consensus Metrics

### replicated_log_size
- **Type**: Gauge
- **Unit**: count
- **Meaning**: Number of entries in distributed log
- **Use**: Detect log growth

### log_replay_latency_ms
- **Type**: Histogram
- **Unit**: milliseconds
- **Meaning**: Time to replay all log entries
- **Use**: Failure recovery time

### consensus_latency_ms
- **Type**: Histogram
- **Unit**: milliseconds
- **Meaning**: Time for state to replicate across nodes (Phase 2)

## Composite Metrics

### Effective Throughput
```
Throughput = total_completed / elapsed_time  [tokens/sec]
```

### Cache Efficiency
```
Cache_Efficiency = (1 - fragmentation) * hit_rate  [0.0-1.0]
```
- Example: (1 - 0.05) * 0.75 = 0.7125 = 71.25% effective use

### End-to-End Speedup
```
E2E_Speedup = (no_spec_latency / spec_latency)
```
- Example: 20ms → 8ms = 2.5x speedup

### Safety Overhead
```
Safety_Overhead = policy_check_latency / request_latency  [%]
```

## Metric Collection Strategy

### Sampling
- Histograms use fixed buckets (no reservoir sampling)
- Counters are atomic operations (zero cost)
- Gauges updated on every state change

### Retention
- In-memory ring buffer (last 10k events)
- Prometheus scrape interval = 15 seconds
- Phase 2: persistent storage

### Export
- OpenTelemetry integration (Phase 2)
- Prometheus `/metrics` endpoint
- JSON export via HTTP API (Phase 2)

## Performance Impact

Metric recording overhead:
| Operation | Latency | Impact |
|-----------|---------|--------|
| Counter increment | 10-50 ns | < 0.1% |
| Histogram record | 100-500 ns | < 1% |
| Gauge update | 50-200 ns | < 0.5% |
| Hash latency record | 500 ns | included in hash time |

**Total overhead**: < 5% of request latency

## Interpreting Metrics

### High Request Latency
- Check: active_streams (is queue full?)
- Check: speculative_acceptance_rate (is verification causing delay?)
- Check: cache_fragmentation (is allocation slow?)

### Low Acceptance Rate
- Check: draft_length (may be too aggressive)
- Check: verifier quality (model mismatch?)
- Action: Reduce draft_length, retrain verifier

### High Fragmentation
- Check: block_size (may be too large)
- Check: eviction_frequency (too much churn)
- Action: Adjust eviction policy, increase cache

### Policy Violations
- Check: violation_type (auth, sequence, other?)
- Check: client_id (specific client misbehaving?)
- Action: Block client, adjust policy

## Alerting Rules (SRE)

```yaml
- alert: HighRequestLatency
  expr: gateway_request_latency_p99 > 500  # milliseconds
  
- alert: LowAcceptanceRate
  expr: speculative_acceptance_rate < 0.7
  
- alert: HighFragmentation
  expr: kv_cache_fragmentation > 0.1  # 10%
  
- alert: PolicyViolations
  expr: safety_violations_total > 100  # per hour
```

## Further Reading

- [Prometheus best practices](https://prometheus.io/docs/practices/naming/)
- [OpenTelemetry spec](https://opentelemetry.io/docs/specs/otel/)
- [Criterion.rs benchmarking](https://bheisler.github.io/criterion.rs/book/)
