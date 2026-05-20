# AEGIS Scheduler - Deployment Checklist

Use this checklist to verify all prerequisites and deployment steps before going to production.

---

## Phase 1: Pre-Deployment Planning

### Infrastructure Planning
- [ ] Determined deployment model (Docker Compose / Kubernetes / Bare Metal)
- [ ] Identified cluster size (3+, preferably odd number)
- [ ] Planned node placement across availability zones/regions
- [ ] Verified network topology and inter-node latency (<10ms)
- [ ] Identified storage infrastructure (local SSDs, EBS, NFS, etc.)
- [ ] Planned backup strategy and recovery procedures
- [ ] Identified monitoring and logging infrastructure
- [ ] Determined security requirements (TLS, mTLS, firewall rules)
- [ ] Planned capacity (CPU, memory, storage) with 20% headroom

### Team Preparation
- [ ] Trained operations team on AEGIS architecture
- [ ] Prepared runbooks (see OPERATIONAL_RUNBOOKS.md)
- [ ] Defined on-call procedures and escalation paths
- [ ] Identified primary and backup administrators
- [ ] Created communication channels for incident response

---

## Phase 2: Environment Preparation

### Operating System Setup (Linux)
- [ ] OS version: Ubuntu 20.04+ or RHEL 8.x+
- [ ] Kernel: 5.10.0+ (check: `uname -r`)
- [ ] System clocks synchronized via NTP (< 100ms skew)
  ```bash
  timedatectl status
  ```
- [ ] SELinux or AppArmor configured appropriately
- [ ] Firewall enabled with correct rules
- [ ] SSH hardened (key-based auth only, no password)
- [ ] Sudo access restricted to deployment tool/user

### Storage Setup
- [ ] Storage directories created:
  ```bash
  mkdir -p /var/lib/aegis/{wal,snapshots}
  ```
- [ ] Permissions set correctly:
  ```bash
  chown aegis:aegis /var/lib/aegis
  chmod 700 /var/lib/aegis
  ```
- [ ] Storage filesystem verified (ext4, XFS, BTRFS)
- [ ] Storage capacity verified (minimum 100GB per node)
- [ ] Storage performance tested (IOPS, throughput)
- [ ] Backup destination configured

### Network Setup
- [ ] Inter-node connectivity verified:
  ```bash
  ping -c 1 <node-2>
  ping -c 1 <node-3>
  ```
- [ ] Latency measured and < 10ms:
  ```bash
  ping -c 100 <node> | grep avg
  ```
- [ ] Firewall rules applied for:
  - Consensus ports (6000-6100)
  - Metrics port (8000)
  - Optional: OpenTelemetry ports (4317, 4318)
- [ ] DNS resolution working for node hostnames:
  ```bash
  nslookup node-1
  nslookup node-2
  nslookup node-3
  ```
- [ ] Load balancer configured (if applicable)
- [ ] Network policies configured (Kubernetes only)

### System Performance Tuning
- [ ] File descriptor limits increased:
  ```bash
  ulimit -n 65536
  ```
- [ ] TCP tuning applied:
  ```bash
  sysctl -a | grep tcp_tw_reuse
  sysctl -a | grep tcp_nodelay
  ```
- [ ] CPU frequency scaling: performance mode
  ```bash
  cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
  ```
- [ ] Swappiness disabled or minimized:
  ```bash
  sysctl vm.swappiness
  ```

---

## Phase 3: Software Installation

### Build Tools
- [ ] Rust 1.75.0+ installed:
  ```bash
  rustc --version
  ```
- [ ] Cargo available:
  ```bash
  cargo --version
  ```
- [ ] System libraries installed:
  ```bash
  pkg-config --cflags --libs openssl
  which cmake
  ```

### Container Runtime (if using Docker/Kubernetes)
- [ ] Docker 20.10.0+ installed:
  ```bash
  docker --version
  docker run hello-world
  ```
- [ ] Docker Compose 2.0.0+ installed:
  ```bash
  docker-compose --version
  ```
- [ ] Docker daemon running:
  ```bash
  systemctl status docker
  ```
- [ ] Docker resource limits configured

### Kubernetes (if using Kubernetes)
- [ ] Kubernetes 1.24.0+ available:
  ```bash
  kubectl version --short
  ```
- [ ] kubectl configured with cluster context:
  ```bash
  kubectl cluster-info
  ```
- [ ] Storage class available:
  ```bash
  kubectl get storageclass
  ```
- [ ] Ingress controller installed (if needed)
- [ ] Network CNI plugin installed (Flannel, Calico, Weave)

### Optional: Monitoring Stack
- [ ] Prometheus 2.45.0+ available (if applicable)
- [ ] Grafana 10.0.0+ available (if applicable)
- [ ] OpenTelemetry Collector 0.88.0+ available (if applicable)

---

## Phase 4: Code Preparation

### Build Verification
- [ ] Source code cloned:
  ```bash
  git clone <repository-url>
  cd aegis-scheduler
  ```
- [ ] Branch/tag verified:
  ```bash
  git log --oneline | head -5
  ```
- [ ] Build succeeds:
  ```bash
  cargo build --release
  ```
- [ ] Tests pass:
  ```bash
  cargo test --release 2>&1 | tail -10
  ```
- [ ] Binary size reasonable:
  ```bash
  ls -lh target/release/aegis-scheduler
  ```

### Docker Image Build (if applicable)
- [ ] Dockerfile reviewed
- [ ] Docker image builds:
  ```bash
  docker build -t aegis-scheduler:latest .
  ```
- [ ] Image size reasonable:
  ```bash
  docker images aegis-scheduler
  ```
- [ ] Image pushed to registry (if applicable)

### Documentation Review
- [ ] DEPLOYMENT.md reviewed
- [ ] PREREQUISITES.md reviewed
- [ ] OPERATIONAL_RUNBOOKS.md reviewed
- [ ] Configuration files reviewed

---

## Phase 5: Configuration

### Environment Variables
- [ ] Node IDs defined (node-1, node-2, node-3, ...)
- [ ] Bind addresses configured (0.0.0.0:6000)
- [ ] Metrics addresses configured (0.0.0.0:8000)
- [ ] Peer list configured:
  ```
  AEGIS_PEERS=node-1:6000,node-2:6000,node-3:6000
  ```
- [ ] Logging level set appropriately (info for production)

### Consensus Configuration
- [ ] Election timeout: 150ms
- [ ] Heartbeat interval: 50ms
- [ ] RPC timeout: 1000ms
- [ ] Retry configuration: 3 retries with exponential backoff

### WAL Configuration
- [ ] fsync interval: 100 entries
- [ ] Max segment size: 1GB
- [ ] Data directory: /var/lib/aegis/wal
- [ ] Backup strategy defined

### Snapshot Configuration
- [ ] Snapshot interval: 10,000 entries
- [ ] Snapshot retention: 3 snapshots
- [ ] Data directory: /var/lib/aegis/snapshots

### TLS Configuration (Production)
- [ ] Certificates generated or obtained
- [ ] TLS enabled in configuration
- [ ] Certificate paths configured
- [ ] Certificate expiration scheduled in calendar

---

## Phase 6: Deployment

### Docker Compose Deployment
- [ ] docker-compose.yml configured correctly
- [ ] Dockerfile reviewed
- [ ] prometheus.yml configured
- [ ] Networks created:
  ```bash
  docker-compose up -d
  ```
- [ ] Containers running:
  ```bash
  docker-compose ps
  ```

### Kubernetes Deployment
- [ ] Namespace created:
  ```bash
  kubectl apply -f kubernetes/namespace.yaml
  ```
- [ ] ConfigMap created:
  ```bash
  kubectl apply -f kubernetes/configmap.yaml
  ```
- [ ] RBAC applied:
  ```bash
  kubectl apply -f kubernetes/rbac.yaml
  ```
- [ ] Services created:
  ```bash
  kubectl apply -f kubernetes/service.yaml
  ```
- [ ] StatefulSet deployed:
  ```bash
  kubectl apply -f kubernetes/statefulset.yaml
  ```
- [ ] Pods running:
  ```bash
  kubectl get pods -n aegis
  ```

### Bare Metal Deployment
- [ ] User `aegis` created on each node
- [ ] Binary compiled on each node:
  ```bash
  cargo build --release
  ```
- [ ] systemd service created on each node
- [ ] Service enabled:
  ```bash
  systemctl enable aegis-scheduler
  systemctl start aegis-scheduler
  ```

---

## Phase 7: Verification & Testing

### Health Checks
- [ ] All nodes report healthy:
  ```bash
  curl http://<node>:8000/health
  ```
- [ ] Quorum established (expected output: `"quorum": true`)
- [ ] Leader elected (one node should report leader status)
- [ ] All nodes report same leader

### Metrics Verification
- [ ] Metrics endpoint responds:
  ```bash
  curl http://<node>:8000/metrics
  ```
- [ ] Key metrics present:
  - `aegis_consensus_leader_id`
  - `aegis_quorum_healthy_peers`
  - `aegis_rpc_latency_ms`
  - `aegis_wal_entries_total`

### Consensus Validation
- [ ] 3-node cluster formed
- [ ] Leader election works
- [ ] All nodes synchronized
- [ ] Log replication working

### Network Testing
- [ ] Latency acceptable (< 50ms p99)
- [ ] Packet loss < 0.1%
- [ ] No port timeouts

### Failure Injection Test
- [ ] Stop one node:
  ```bash
  systemctl stop aegis-scheduler  # or docker stop aegis-node-2
  ```
- [ ] Cluster remains healthy:
  ```bash
  curl http://<healthy-node>:8000/health
  ```
- [ ] Verify `quorum: true` and `healthy_peers: 2`
- [ ] Restart node:
  ```bash
  systemctl start aegis-scheduler
  ```
- [ ] Node rejoins cluster

### Performance Baseline
- [ ] Establish baseline metrics:
  - RPC latency (average, p50, p99)
  - Election time
  - Log replication lag
- [ ] Record in monitoring system
- [ ] Set up alerts for anomalies

---

## Phase 8: Monitoring & Observability

### Prometheus Setup
- [ ] Prometheus targets scraping:
  ```bash
  curl http://prometheus:9090/api/v1/targets | jq .
  ```
- [ ] Metrics being collected
- [ ] Data retention: 30 days

### Grafana Setup
- [ ] Grafana accessible
- [ ] Prometheus datasource added
- [ ] Dashboard created with key metrics
- [ ] Alerts configured

### Logging Setup
- [ ] Logs being collected (journalctl, docker logs, etc.)
- [ ] Centralized logging configured (if applicable)
- [ ] Log retention policy defined

### Alert Rules
- [ ] QuorumLost alert configured
- [ ] HighRPCLatency alert configured
- [ ] WALGrowthTooFast alert configured
- [ ] LeaderElectionTimeout alert configured
- [ ] AlertManager configured to route alerts

---

## Phase 9: Backup & Recovery

### Backup Strategy
- [ ] Backup schedule defined (daily minimum)
- [ ] Backup destination configured
- [ ] Backup script created:
  ```bash
  tar czf aegis-backup-$(date +%Y%m%d).tar.gz \
    /var/lib/aegis/wal \
    /var/lib/aegis/snapshots
  ```
- [ ] Backup retention policy (30 days minimum)
- [ ] Backup tested and verified

### Recovery Testing
- [ ] Recovery procedure documented
- [ ] Recovery tested in non-production environment
- [ ] RTO measured (recovery time objective)
- [ ] RPO measured (recovery point objective)

### Disaster Recovery Plan
- [ ] Multi-region strategy (if applicable)
- [ ] Data center failover procedures
- [ ] Communication plan during incidents

---

## Phase 10: Security

### TLS/Encryption
- [ ] TLS certificates valid
- [ ] Certificate expiration dates tracked
- [ ] mTLS enabled (optional, production recommended)

### Access Control
- [ ] RBAC configured (Kubernetes)
- [ ] Service accounts created
- [ ] Secret management in place (for certificates, API keys)

### Network Security
- [ ] Firewall rules applied
- [ ] Network policies enforced (Kubernetes)
- [ ] Unnecessary ports closed

### Audit Logging
- [ ] Audit logging enabled
- [ ] Logs sent to central location
- [ ] Access logging configured

---

## Phase 11: Documentation

### Operational Documentation
- [ ] Runbooks up-to-date (OPERATIONAL_RUNBOOKS.md)
- [ ] Troubleshooting guide available
- [ ] Configuration documented
- [ ] Known issues documented

### Team Knowledge
- [ ] Team trained on operational procedures
- [ ] Team trained on troubleshooting
- [ ] Escalation procedures documented
- [ ] On-call schedule established

### Change Management
- [ ] Change log started
- [ ] Release notes prepared
- [ ] Deployment procedures documented
- [ ] Rollback procedures documented

---

## Phase 12: Go/No-Go Decision

### Operational Readiness
- [ ] All items in this checklist completed ✓
- [ ] Monitoring and alerting verified ✓
- [ ] Backup and recovery tested ✓
- [ ] Security verified ✓
- [ ] Documentation complete ✓
- [ ] Team trained ✓

### Sign-Off
- [ ] Infrastructure owner sign-off: ______________________
- [ ] Operations owner sign-off: ______________________
- [ ] Security owner sign-off (if applicable): ______________________
- [ ] Project manager approval: ______________________

### Deployment Authorization
- [ ] Approved for production deployment: YES / NO
- [ ] Date approved: _____________________
- [ ] Approval authority: _____________________

---

## Phase 13: Post-Deployment (First Week)

### Monitoring
- [ ] Metrics being collected (24+ hours)
- [ ] No unexpected patterns observed
- [ ] Performance baseline matches predictions
- [ ] No alert anomalies

### Operational Review
- [ ] Daily health check completed
- [ ] No incidents or issues
- [ ] Logs reviewed for errors
- [ ] Metrics trending normally

### Adjustment
- [ ] Alert thresholds tuned if necessary
- [ ] Scaling policies adjusted if needed
- [ ] Backup frequency verified
- [ ] Documentation updated based on operational experience

---

## Troubleshooting Quick Reference

| Issue | Diagnosis | Resolution |
|-------|-----------|-----------|
| Pods not starting | `kubectl describe pod` | Check logs, storage, resources |
| Quorum lost | `curl .../metrics` look for `healthy_peers < 2` | Restart failed nodes |
| High latency | Check network latency | Verify network, add more resources |
| Storage full | `df -h /var/lib/aegis` | Increase storage or adjust retention |
| Metrics missing | Check Prometheus targets | Verify service, port forwarding |

---

**Checklist Version**: 1.0  
**Last Updated**:   
**Project**: AEGIS Distributed AI Inference Scheduler
