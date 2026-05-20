# AEGIS Scheduler - Prerequisites & System Requirements

## Overview
This document outlines all system, software, and infrastructure requirements for deploying AEGIS distributed AI inference scheduler.

---

## 1. System Requirements

### Minimum Hardware (Single Node Development)
- **CPU**: 2+ cores (4+ cores recommended)
- **RAM**: 4GB minimum, 8GB+ recommended
- **Storage**: 10GB minimum (SSD strongly recommended for WAL performance)
- **Network**: 10Mbps network connectivity minimum

### Production Deployment (3+ Node Cluster)
- **CPU**: 4+ cores per node (8+ cores recommended)
- **RAM**: 16GB per node minimum (32GB+ recommended)
- **Storage**: 100GB+ SSD per node (NVMe ideal for write-ahead logs)
- **Network**: 1Gbps+ inter-node connectivity, <10ms latency preferred

### Operating System
- **Linux**: Ubuntu 20.04 LTS, 22.04 LTS, or RHEL 8.x+ (primary)
- **macOS**: 12.0+ (development only, not production)
- **Windows**: WSL2 Ubuntu 22.04 (development only, not production)

---

## 2. Software Dependencies

### Core Runtime
- **Rust**: 1.75.0 or later (MSRV: 1.70.0)
  - Install: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
  - Verify: `rustc --version && cargo --version`

- **Tokio Async Runtime**: 1.35.0+ (included in Cargo.toml)
- **gRPC (Tonic)**: 0.11.0+ (included in Cargo.toml)

### Required System Libraries
```bash
# Ubuntu/Debian
sudo apt-get update && sudo apt-get install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  cmake \
  git

# RHEL/CentOS
sudo yum groupinstall -y "Development Tools" && \
sudo yum install -y \
  openssl-devel \
  cmake \
  git
```

### Monitoring & Observability (Optional but Recommended)
- **OpenTelemetry Collector**: 0.88.0+ (for metrics/traces)
- **Prometheus**: 2.45.0+ (metrics storage)
- **Jaeger**: 1.45.0+ (distributed tracing)
- **Grafana**: 10.0.0+ (visualization)

---

## 3. Network Requirements

### Ports (Per Node)
| Port | Protocol | Purpose | Required |
|------|----------|---------|----------|
| 6000-6100 | TCP | gRPC Consensus RPC | Yes |
| 8000 | TCP | Metrics/Health HTTP | Yes |
| 4317 | TCP | OpenTelemetry gRPC (collector) | Optional |
| 4318 | TCP | OpenTelemetry HTTP (collector) | Optional |

### Network Configuration
- **Firewall Rules**: Open inter-node communication on consensus ports
- **DNS**: Stable DNS resolution for node discovery (hostname-based or service mesh)
- **Load Balancer**: Optional - for client load distribution
- **Network Partitions**: System handles, but <5% packet loss tolerance recommended

### Example Firewall Rules (iptables)
```bash
# Allow inter-node consensus traffic
sudo iptables -A INPUT -p tcp --dport 6000:6100 -s <cluster-subnet> -j ACCEPT

# Allow metrics scrape
sudo iptables -A INPUT -p tcp --dport 8000 -s <monitoring-subnet> -j ACCEPT
```

---

## 4. Container Runtime (Docker/Kubernetes)

### Docker
- **Version**: 20.10.0+ (24.0.0+ recommended)
- **Compose**: 2.0.0+
- **Install**: [Official Docker Docs](https://docs.docker.com/engine/install/)

### Kubernetes (Optional for HA Deployment)
- **Version**: 1.24.0+ (1.27.0+ recommended)
- **Container Runtime**: Docker, containerd, or CRI-O
- **CNI Plugin**: Flannel, Calico, or Weave (for overlay networking)
- **Storage Class**: Must support persistent volumes (local-path, EBS, NFS, etc.)

### Kubernetes Add-ons (Recommended)
- **Prometheus Operator**: For metrics collection
- **Jaeger Operator**: For tracing
- **Ingress Controller**: Nginx or Traefik for external access

---

## 5. Build Dependencies

### Cargo Workspace
Located in `Cargo.toml`:

**Core Crates:**
- `tokio` 1.35+ — async runtime
- `tonic` 0.11+ — gRPC framework
- `prost` 0.12+ — protobuf serialization
- `serde` 1.0+ — serialization
- `blake3` 1.5+ — hashing for state consistency
- `tracing` 0.1+ — observability
- `opentelemetry` 0.21+ — OpenTelemetry integration

**Testing & Development:**
- `tokio-test` — async testing utilities
- `criterion` — benchmarking
- `proptest` — property-based testing
- `tempfile` — temporary file management

### Workspace Structure
```
.
├── Cargo.toml                    (workspace root)
├── scheduler/                    (core scheduling engine)
├── consensus/                    (consensus protocol)
├── audit/                        (audit trail engine)
├── gateway/                      (API gateway)
├── inference-backends/           (backend drivers)
├── aegis-scheduler/              (distributed KV cache)
└── benchmarks/                   (performance benchmarks)
```

---

## 6. File System & Storage

### Write-Ahead Log (WAL)
- **Location**: Configurable, default: `/var/lib/aegis/wal/`
- **Filesystem**: ext4, XFS, or BTRFS (F2FS not recommended)
- **Permissions**: 700 (rwx------)
- **Capacity**: Grows with cluster load; plan 1GB/day per node baseline

### Snapshots
- **Location**: `/var/lib/aegis/snapshots/`
- **Frequency**: Automatic every N entries (configurable)
- **Retention**: Keep last 3 snapshots by default
- **Size**: ~10-50MB per snapshot (depends on state size)

### Configuration Files
- **Location**: `/etc/aegis/`
- **Permissions**: 640 (rw-r-----)
- **Format**: YAML

### Data Durability
- **fsync**: Enabled by default (configurable via WalConfig)
- **Corruption Detection**: BLAKE3 hashing on all snapshots
- **Recovery**: Automatic on node startup

---

## 7. Deployment Environment Checklist

### Pre-Deployment Verification
- [ ] All 3+ nodes running supported OS
- [ ] Rust 1.75.0+ installed on each node
- [ ] System libraries installed (build-essential, openssl-dev, etc.)
- [ ] Inter-node network connectivity verified (<10ms latency)
- [ ] Consensus ports (6000-6100) open and firewall rules applied
- [ ] Metrics port (8000) accessible to monitoring system
- [ ] Storage directories created with proper permissions:
  ```bash
  sudo mkdir -p /var/lib/aegis/{wal,snapshots}
  sudo chown aegis:aegis /var/lib/aegis
  sudo chmod 700 /var/lib/aegis
  ```
- [ ] DNS resolves all node hostnames
- [ ] NTP synchronized across all nodes (< 100ms skew)
- [ ] CPU frequency scaling disabled (for benchmarking consistency)
  ```bash
  sudo bash -c 'echo performance | tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor'
  ```

### Development Setup (Single Node)
- [ ] Docker installed (for containerized development)
- [ ] `docker-compose` available
- [ ] 4GB+ RAM available
- [ ] 10GB+ storage available

### Production Setup (3+ Node HA Cluster)
- [ ] Kubernetes 1.24.0+ or manual orchestration tool
- [ ] Persistent volume provisioner configured (EBS, NFS, etc.)
- [ ] Load balancer or ingress controller for client traffic
- [ ] Monitoring stack deployed (Prometheus + Grafana)
- [ ] Centralized logging configured (ELK, Loki, or Datadog)
- [ ] Backup strategy defined for WAL and snapshots

---

## 8. Optional Services

### Observability Stack (Recommended for Production)
| Service | Version | Purpose | CPU | RAM |
|---------|---------|---------|-----|-----|
| Prometheus | 2.45.0+ | Metrics storage | 2 cores | 4GB |
| Grafana | 10.0.0+ | Visualization | 1 core | 2GB |
| Jaeger | 1.45.0+ | Distributed tracing | 2 cores | 4GB |
| OpenTelemetry Collector | 0.88.0+ | Metrics/logs aggregation | 1 core | 2GB |

### Persistence & Backup (Recommended)
| Tool | Purpose |
|------|---------|
| `rsync` or `restic` | WAL/snapshot backups |
| S3-compatible storage | Off-site backup destination |
| `pg_dump` alternative | State snapshots (optional) |

### Infrastructure as Code (IaC)
- **Terraform**: For infrastructure provisioning
- **Ansible**: For configuration management
- **Helm**: For Kubernetes deployments

---

## 9. Security Requirements

### Network Security
- [ ] TLS 1.2+ enabled for all gRPC connections (mTLS recommended)
- [ ] firewall blocking unauthorized ports
- [ ] Network policies in Kubernetes (if applicable)
- [ ] Private subnets for cluster communication

### OS Security
- [ ] SELinux or AppArmor policies applied
- [ ] Regular OS security updates
- [ ] SSH key-based authentication only (no passwords)
- [ ] Sudo access restricted to deployment automation

### Application Security
- [ ] RBAC enabled for API access
- [ ] Audit logging enabled and centralized
- [ ] Secrets management (HashiCorp Vault, K8s Secrets, etc.)
- [ ] Regular security scanning of container images

---

## 10. Performance Tuning (Optional)

### Linux Kernel Parameters
```bash
# Increase file descriptor limits
sudo bash -c 'echo "* soft nofile 65536" >> /etc/security/limits.conf'
sudo bash -c 'echo "* hard nofile 65536" >> /etc/security/limits.conf'

# Optimize TCP for low-latency
sudo sysctl -w net.ipv4.tcp_tw_reuse=1
sudo sysctl -w net.ipv4.tcp_nodelay=1
sudo sysctl -w net.ipv4.tcp_fastopen=3

# Increase backlog
sudo sysctl -w net.core.somaxconn=4096
sudo sysctl -w net.ipv4.tcp_max_syn_backlog=4096
```

### Tokio Runtime Tuning
- Workers: 1 per CPU core (configurable via environment)
- Max blocking threads: 512
- Thread stack size: 2MB (default)

---

## 11. Support & Resources

| Resource | Location |
|----------|----------|
| Build Instructions | DEPLOYMENT.md |
| Configuration Guide | OPERATIONAL_RUNBOOKS.md |
| Troubleshooting | OPERATIONAL_RUNBOOKS.md (Troubleshooting section) |
| Architecture | ARCHITECTURE.md |
| Source Code | `/scheduler` |

---

## 12. Quick Start Verification

```bash
# Verify Rust installation
rustc --version  # Should be 1.75.0+
cargo --version

# Verify system libraries
pkg-config --cflags --libs openssl

# Verify Docker (if using containers)
docker --version
docker-compose --version

# Clone and build
git clone <repository-url>
cd aegis-scheduler
cargo build --release

# Run tests
cargo test --release

# Build docs
cargo doc --release --no-deps
```

---

**Last Updated**:   
**Project**: AEGIS Distributed AI Inference Scheduler  
**Version**: 1.0.0 (Production Ready)
