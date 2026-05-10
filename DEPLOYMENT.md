# AEGIS Scheduler - Deployment Guide

## Table of Contents
1. [Pre-Deployment Checklist](#pre-deployment-checklist)
2. [Development Deployment (Docker Compose)](#development-deployment-docker-compose)
3. [Production Deployment (Kubernetes)](#production-deployment-kubernetes)
4. [Manual Deployment (Bare Metal)](#manual-deployment-bare-metal)
5. [Verification & Testing](#verification--testing)
6. [Post-Deployment Configuration](#post-deployment-configuration)
7. [Monitoring & Observability Setup](#monitoring--observability-setup)
8. [Scaling & Maintenance](#scaling--maintenance)

---

## Pre-Deployment Checklist

### Prerequisites Validation
```bash
# 1. Verify Rust installation
rustc --version  # Must be >= 1.75.0
cargo --version

# 2. Verify system dependencies
pkg-config --cflags --libs openssl
which cmake && which git

# 3. Verify network connectivity
ping -c 1 <other-node-1>
ping -c 1 <other-node-2>

# 4. Verify storage
df -h /var/lib/aegis  # At least 100GB available
mount | grep /var/lib/aegis  # Should be on SSD/NVMe

# 5. Verify ports are available
netstat -tlnp | grep -E ':(6000|6100|8000)'  # Should be empty
```

### Infrastructure Requirements Met
- [ ] All nodes meet hardware requirements (4+ cores, 16GB+ RAM)
- [ ] OS is Ubuntu 20.04+ or RHEL 8.x+
- [ ] Network latency between nodes < 10ms
- [ ] Storage directories created with correct permissions
- [ ] Firewall rules configured for consensus ports
- [ ] DNS resolution working for all node hostnames
- [ ] NTP synchronized across cluster (< 100ms skew)

---

## Development Deployment (Docker Compose)

### Quick Start (Single Node)

#### 1. Create docker-compose.yml

```yaml
version: '3.8'

services:
  aegis-node-1:
    build:
      context: .
      dockerfile: Dockerfile
      args:
        PROFILE: release
    image: aegis-scheduler:latest
    container_name: aegis-node-1
    hostname: aegis-node-1
    ports:
      - "6000:6000"
      - "8000:8000"
    environment:
      RUST_LOG: info,aegis_scheduler=debug
      AEGIS_NODE_ID: "node-1"
      AEGIS_BIND_ADDR: "0.0.0.0:6000"
      AEGIS_METRICS_ADDR: "0.0.0.0:8000"
      AEGIS_PEERS: "aegis-node-1:6000,aegis-node-2:6000,aegis-node-3:6000"
    volumes:
      - aegis-wal-1:/var/lib/aegis/wal
      - aegis-snapshots-1:/var/lib/aegis/snapshots
    networks:
      - aegis-network
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/health"]
      interval: 10s
      timeout: 5s
      retries: 3
      start_period: 30s
    restart: unless-stopped
    ulimits:
      nofile:
        soft: 65536
        hard: 65536

  aegis-node-2:
    build:
      context: .
      dockerfile: Dockerfile
      args:
        PROFILE: release
    image: aegis-scheduler:latest
    container_name: aegis-node-2
    hostname: aegis-node-2
    ports:
      - "6001:6000"
      - "8001:8000"
    environment:
      RUST_LOG: info,aegis_scheduler=debug
      AEGIS_NODE_ID: "node-2"
      AEGIS_BIND_ADDR: "0.0.0.0:6000"
      AEGIS_METRICS_ADDR: "0.0.0.0:8000"
      AEGIS_PEERS: "aegis-node-1:6000,aegis-node-2:6000,aegis-node-3:6000"
    volumes:
      - aegis-wal-2:/var/lib/aegis/wal
      - aegis-snapshots-2:/var/lib/aegis/snapshots
    networks:
      - aegis-network
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/health"]
      interval: 10s
      timeout: 5s
      retries: 3
      start_period: 30s
    restart: unless-stopped
    depends_on:
      - aegis-node-1
    ulimits:
      nofile:
        soft: 65536
        hard: 65536

  aegis-node-3:
    build:
      context: .
      dockerfile: Dockerfile
      args:
        PROFILE: release
    image: aegis-scheduler:latest
    container_name: aegis-node-3
    hostname: aegis-node-3
    ports:
      - "6002:6000"
      - "8002:8000"
    environment:
      RUST_LOG: info,aegis_scheduler=debug
      AEGIS_NODE_ID: "node-3"
      AEGIS_BIND_ADDR: "0.0.0.0:6000"
      AEGIS_METRICS_ADDR: "0.0.0.0:8000"
      AEGIS_PEERS: "aegis-node-1:6000,aegis-node-2:6000,aegis-node-3:6000"
    volumes:
      - aegis-wal-3:/var/lib/aegis/wal
      - aegis-snapshots-3:/var/lib/aegis/snapshots
    networks:
      - aegis-network
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/health"]
      interval: 10s
      timeout: 5s
      retries: 3
      start_period: 30s
    restart: unless-stopped
    depends_on:
      - aegis-node-1
    ulimits:
      nofile:
        soft: 65536
        hard: 65536

  prometheus:
    image: prom/prometheus:latest
    container_name: aegis-prometheus
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - prometheus-data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
    networks:
      - aegis-network
    restart: unless-stopped

networks:
  aegis-network:
    driver: bridge
    ipam:
      config:
        - subnet: 172.20.0.0/16

volumes:
  aegis-wal-1:
  aegis-wal-2:
  aegis-wal-3:
  aegis-snapshots-1:
  aegis-snapshots-2:
  aegis-snapshots-3:
  prometheus-data:
```

#### 2. Create Dockerfile

```dockerfile
# Build stage
FROM rust:1.75.0 as builder

WORKDIR /build

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    cmake \
    && rm -rf /var/lib/apt/lists/*

# Copy source
COPY . .

# Build
ARG PROFILE=release
RUN cargo build --${PROFILE}

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -m -u 1000 aegis

# Copy binary from builder
COPY --from=builder /build/target/release/aegis-scheduler /usr/local/bin/

# Create data directories
RUN mkdir -p /var/lib/aegis/{wal,snapshots} && \
    chown -R aegis:aegis /var/lib/aegis && \
    chmod 700 /var/lib/aegis

USER aegis

EXPOSE 6000 8000

ENTRYPOINT ["/usr/local/bin/aegis-scheduler"]
```

#### 3. Create prometheus.yml

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'aegis-node-1'
    static_configs:
      - targets: ['aegis-node-1:8000']
    
  - job_name: 'aegis-node-2'
    static_configs:
      - targets: ['aegis-node-2:8000']
    
  - job_name: 'aegis-node-3'
    static_configs:
      - targets: ['aegis-node-3:8000']
```

#### 4. Deploy

```bash
# Build and start the cluster
docker-compose up -d

# Verify nodes are running
docker-compose ps

# Check logs
docker-compose logs -f aegis-node-1

# Verify metrics endpoint
curl http://localhost:8000/metrics

# Verify cluster health
curl http://localhost:8000/health

# Access Prometheus
# Visit http://localhost:9090 in browser
```

#### 5. Stop and Cleanup

```bash
# Stop cluster
docker-compose down

# Remove volumes (destructive)
docker-compose down -v
```

---

## Production Deployment (Kubernetes)

### Prerequisites
- Kubernetes 1.24.0+ cluster running
- kubectl configured and authenticated
- Persistent volume provisioner configured
- Ingress controller installed (optional, for external access)

### Step 1: Create Namespace

```bash
kubectl create namespace aegis
kubectl label namespace aegis app=aegis-scheduler
```

### Step 2: Create ConfigMap for Configuration

```yaml
# config-map.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: aegis-config
  namespace: aegis
data:
  wal-fsync-interval: "100"  # fsync every 100 entries
  snapshot-interval: "10000"  # snapshot every 10k entries
  election-timeout-ms: "150"
  heartbeat-interval-ms: "50"
```

```bash
kubectl apply -f config-map.yaml
```

### Step 3: Create StatefulSet

```yaml
# statefulset.yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: aegis-scheduler
  namespace: aegis
spec:
  serviceName: aegis-scheduler
  replicas: 3
  selector:
    matchLabels:
      app: aegis-scheduler
  template:
    metadata:
      labels:
        app: aegis-scheduler
    spec:
      affinity:
        podAntiAffinity:
          preferredDuringSchedulingIgnoredDuringExecution:
            - weight: 100
              podAffinityTerm:
                labelSelector:
                  matchExpressions:
                    - key: app
                      operator: In
                      values:
                        - aegis-scheduler
                topologyKey: kubernetes.io/hostname
      
      initContainers:
        - name: init-data-dirs
          image: busybox
          command: ['sh', '-c', 'mkdir -p /var/lib/aegis/{wal,snapshots} && chmod 700 /var/lib/aegis']
          volumeMounts:
            - name: data
              mountPath: /var/lib/aegis

      containers:
        - name: aegis-scheduler
          image: aegis-scheduler:latest
          imagePullPolicy: IfNotPresent
          
          ports:
            - name: consensus
              containerPort: 6000
              protocol: TCP
            - name: metrics
              containerPort: 8000
              protocol: TCP
          
          env:
            - name: RUST_LOG
              value: "info,aegis_scheduler=debug"
            - name: AEGIS_NODE_ID
              valueFrom:
                fieldRef:
                  fieldPath: metadata.name
            - name: AEGIS_BIND_ADDR
              value: "0.0.0.0:6000"
            - name: AEGIS_METRICS_ADDR
              value: "0.0.0.0:8000"
            - name: AEGIS_PEERS
              value: "aegis-scheduler-0.aegis-scheduler:6000,aegis-scheduler-1.aegis-scheduler:6000,aegis-scheduler-2.aegis-scheduler:6000"
            - name: WAL_FSYNC_INTERVAL
              valueFrom:
                configMapKeyRef:
                  name: aegis-config
                  key: wal-fsync-interval
            - name: SNAPSHOT_INTERVAL
              valueFrom:
                configMapKeyRef:
                  name: aegis-config
                  key: snapshot-interval
          
          livenessProbe:
            httpGet:
              path: /health
              port: metrics
            initialDelaySeconds: 30
            periodSeconds: 10
            timeoutSeconds: 5
            failureThreshold: 3
          
          readinessProbe:
            httpGet:
              path: /health
              port: metrics
            initialDelaySeconds: 10
            periodSeconds: 5
            timeoutSeconds: 3
            failureThreshold: 2
          
          resources:
            requests:
              cpu: "2"
              memory: "8Gi"
            limits:
              cpu: "4"
              memory: "16Gi"
          
          volumeMounts:
            - name: data
              mountPath: /var/lib/aegis
          
          securityContext:
            runAsNonRoot: true
            runAsUser: 1000
            readOnlyRootFilesystem: true
            allowPrivilegeEscalation: false
            capabilities:
              drop:
                - ALL

      volumes:
        - name: tmp
          emptyDir: {}

  volumeClaimTemplates:
    - metadata:
        name: data
      spec:
        accessModes: ["ReadWriteOnce"]
        resources:
          requests:
            storage: 100Gi
```

```bash
kubectl apply -f statefulset.yaml
```

### Step 4: Create Headless Service

```yaml
# service.yaml
apiVersion: v1
kind: Service
metadata:
  name: aegis-scheduler
  namespace: aegis
  labels:
    app: aegis-scheduler
spec:
  clusterIP: None  # Headless service
  selector:
    app: aegis-scheduler
  ports:
    - name: consensus
      port: 6000
      targetPort: consensus
      protocol: TCP
    - name: metrics
      port: 8000
      targetPort: metrics
      protocol: TCP
---
# Client-facing service (optional)
apiVersion: v1
kind: Service
metadata:
  name: aegis-scheduler-client
  namespace: aegis
spec:
  type: LoadBalancer
  selector:
    app: aegis-scheduler
  ports:
    - name: metrics
      port: 8000
      targetPort: metrics
      protocol: TCP
```

```bash
kubectl apply -f service.yaml
```

### Step 5: Deploy and Verify

```bash
# Check pods are running
kubectl get pods -n aegis -w

# Check logs
kubectl logs -n aegis aegis-scheduler-0 -f

# Check services
kubectl get svc -n aegis

# Port-forward for testing
kubectl port-forward -n aegis svc/aegis-scheduler-client 8000:8000

# Verify cluster health
curl http://localhost:8000/health
curl http://localhost:8000/metrics
```

---

## Manual Deployment (Bare Metal)

### Step 1: Prepare Each Node

```bash
# On each node
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev cmake git

# Create aegis user
sudo useradd -m -s /bin/bash aegis

# Create data directories
sudo mkdir -p /var/lib/aegis/{wal,snapshots}
sudo chown aegis:aegis /var/lib/aegis
sudo chmod 700 /var/lib/aegis

# Configure system limits
sudo bash -c 'cat >> /etc/security/limits.conf' << 'EOF'
aegis soft nofile 65536
aegis hard nofile 65536
EOF

# Sync time
sudo systemctl enable ntp
sudo systemctl restart ntp
```

### Step 2: Build on Each Node

```bash
# Clone repository
cd /home/aegis
git clone <repository-url>
cd aegis-scheduler

# Build (release mode)
cargo build --release

# Binary location: target/release/aegis-scheduler
```

### Step 3: Create systemd Service

```bash
# On each node, create /etc/systemd/system/aegis-scheduler.service

[Unit]
Description=AEGIS Distributed Scheduler
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=aegis
Group=aegis
WorkingDirectory=/home/aegis/aegis-scheduler

Environment="RUST_LOG=info,aegis_scheduler=debug"
Environment="AEGIS_NODE_ID=node-1"
Environment="AEGIS_BIND_ADDR=0.0.0.0:6000"
Environment="AEGIS_METRICS_ADDR=0.0.0.0:8000"
Environment="AEGIS_PEERS=node-1:6000,node-2:6000,node-3:6000"

ExecStart=/home/aegis/aegis-scheduler/target/release/aegis-scheduler

Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

# Resource limits
LimitNOFILE=65536
LimitNPROC=65536

[Install]
WantedBy=multi-user.target
```

### Step 4: Start the Service

```bash
# On each node
sudo systemctl daemon-reload
sudo systemctl enable aegis-scheduler
sudo systemctl start aegis-scheduler

# Verify
sudo systemctl status aegis-scheduler
journalctl -u aegis-scheduler -f
```

### Step 5: Verify Cluster

```bash
# On each node
curl http://localhost:8000/health
curl http://localhost:8000/metrics | grep aegis_
```

---

## Verification & Testing

### Health Checks

```bash
# Check node health
curl -s http://<node>:8000/health | jq .

# Expected response:
# {
#   "status": "healthy",
#   "peers": {
#     "node-1": "healthy",
#     "node-2": "healthy", 
#     "node-3": "healthy"
#   },
#   "quorum": true,
#   "leader": "node-1"
# }
```

### Metrics Verification

```bash
# Check metrics
curl http://<node>:8000/metrics

# Look for key metrics:
# - aegis_consensus_leader_id (current leader)
# - aegis_consensus_term (raft term)
# - aegis_rpc_latency_ms (peer latency)
# - aegis_quorum_healthy_peers (quorum status)
# - aegis_wal_entries_total (log size)
```

### Consensus Validation

```bash
# Verify 3-node cluster has consensus
for node in node-1 node-2 node-3; do
  echo "=== $node ==="
  curl -s http://$node:8000/metrics | grep aegis_consensus_leader_id
done

# All should report the same leader
```

### Failure Injection Test

```bash
# Stop one node (should maintain quorum)
systemctl stop aegis-scheduler  # or docker-compose stop aegis-node-2

# Verify cluster still healthy
curl http://<healthy-node>:8000/health
# Should show 2/3 peers healthy, quorum: true

# Restart node
systemctl start aegis-scheduler

# Verify recovery
curl http://<node>:8000/health
# Should return to 3/3 peers healthy
```

---

## Post-Deployment Configuration

### Enable TLS (Production Recommended)

```bash
# Generate certificates on each node
openssl req -x509 -newkey rsa:4096 -keyout /var/lib/aegis/server.key \
  -out /var/lib/aegis/server.crt -days 365 -nodes

# Update environment variables in systemd service or docker-compose
AEGIS_TLS_CERT=/var/lib/aegis/server.crt
AEGIS_TLS_KEY=/var/lib/aegis/server.key
AEGIS_TLS_ENABLED=true
```

### Configure Log Rotation

```bash
# Create /etc/logrotate.d/aegis-scheduler
/var/log/aegis-scheduler/*.log {
  daily
  rotate 30
  compress
  delaycompress
  notifempty
  create 0640 aegis aegis
  postrotate
    systemctl reload aegis-scheduler > /dev/null 2>&1 || true
  endscript
}
```

### Setup Backup Strategy

```bash
# Backup WAL and snapshots daily
cat > /etc/cron.daily/aegis-backup << 'EOF'
#!/bin/bash
BACKUP_DIR=/backups/aegis
mkdir -p $BACKUP_DIR
rsync -av --delete /var/lib/aegis/wal/ $BACKUP_DIR/wal/
rsync -av --delete /var/lib/aegis/snapshots/ $BACKUP_DIR/snapshots/
EOF

chmod +x /etc/cron.daily/aegis-backup
```

---

## Monitoring & Observability Setup

### Prometheus Configuration

```yaml
# prometheus.yml additions
scrape_configs:
  - job_name: 'aegis-cluster'
    static_configs:
      - targets:
        - 'node-1:8000'
        - 'node-2:8000'
        - 'node-3:8000'
    scrape_interval: 15s
    scrape_timeout: 5s
```

### Grafana Dashboards

Key metrics to dashboard:
- `aegis_consensus_leader_id` — Current leader
- `aegis_quorum_healthy_peers` — Quorum status
- `aegis_rpc_latency_ms` — Peer latency
- `aegis_wal_entries_total` — Log size
- `aegis_snapshot_count` — Snapshot frequency
- `aegis_election_duration_ms` — Election timing

### Alert Rules

```yaml
# prometheus-alerts.yml
groups:
  - name: aegis
    rules:
      - alert: QuorumLost
        expr: aegis_quorum_healthy_peers < 2
        for: 1m
        annotations:
          summary: "Quorum lost on {{ $labels.instance }}"
      
      - alert: HighRPCLatency
        expr: aegis_rpc_latency_ms > 100
        for: 5m
        annotations:
          summary: "High RPC latency: {{ $value }}ms"
      
      - alert: WALGrowthTooFast
        expr: rate(aegis_wal_entries_total[1h]) > 1000
        for: 10m
        annotations:
          summary: "WAL growing too fast"
```

---

## Scaling & Maintenance

### Adding New Nodes to Cluster

```bash
# 1. Prepare new node (follow "Prepare Each Node" section)
# 2. Update AEGIS_PEERS on all nodes to include new node
# 3. Update load balancer/DNS
# 4. Restart all nodes gracefully (one at a time)
# 5. Verify health: curl http://<node>:8000/health
```

### Rolling Restart Procedure

```bash
# Graceful rolling restart (maintain quorum)
for node in node-1 node-2 node-3; do
  echo "Restarting $node..."
  systemctl restart aegis-scheduler@$node
  
  # Wait for recovery
  sleep 30
  
  # Verify health
  curl -f http://$node:8000/health || exit 1
done
```

### Backup & Recovery

```bash
# Create backup
tar czf aegis-backup-$(date +%Y%m%d).tar.gz \
  /var/lib/aegis/wal \
  /var/lib/aegis/snapshots

# Restore from backup
tar xzf aegis-backup-20260511.tar.gz -C /
systemctl restart aegis-scheduler
```

### Cluster Upgrade Procedure

```bash
# 1. Build new version
cargo build --release

# 2. Rolling restart with new binary
for node in node-1 node-2 node-3; do
  # Copy new binary
  scp target/release/aegis-scheduler aegis@$node:/home/aegis/bin/
  
  # Restart with new version
  ssh aegis@$node "systemctl restart aegis-scheduler"
  
  # Verify
  sleep 30
  curl http://$node:8000/health
done
```

---

## Troubleshooting

### Cluster Won't Start

```bash
# Check node logs
journalctl -u aegis-scheduler -n 100

# Common issues:
# - Port already in use: netstat -tlnp | grep 6000
# - Permission denied: ls -la /var/lib/aegis
# - DNS not resolving: nslookup <node-hostname>
```

### Lost Quorum

```bash
# Check peer health
curl http://<node>:8000/metrics | grep quorum

# If 2+ nodes down, cluster is unavailable
# Recovery: Restart failed nodes
systemctl restart aegis-scheduler
```

### High Replication Lag

```bash
# Check lag metrics
curl http://<node>:8000/metrics | grep replication_lag

# If > 1000ms, increase heartbeat frequency
# Update config and restart nodes
```

---

**Last Updated**: 2026-05-11  
**Project**: AEGIS Distributed AI Inference Scheduler  
**Version**: 1.0.0
