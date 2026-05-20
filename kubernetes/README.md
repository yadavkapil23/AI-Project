# Kubernetes Manifests for AEGIS Scheduler

This directory contains Kubernetes manifests for deploying AEGIS distributed scheduler.

## File Structure

| File | Purpose |
|------|---------|
| `namespace.yaml` | Kubernetes namespace and labels |
| `configmap.yaml` | Configuration parameters |
| `statefulset.yaml` | Main AEGIS scheduler deployment (3 replicas) |
| `service.yaml` | Kubernetes services (headless, client-facing, metrics) |
| `rbac.yaml` | ServiceAccount and RBAC policies |
| `network-policy.yaml` | Network policies for security |
| `hpa.yaml` | Horizontal Pod Autoscaler and disruption budget |

## Prerequisites

- Kubernetes 1.24.0+ cluster
- `kubectl` configured with cluster access
- Container runtime (Docker, containerd, CRI-O)
- Persistent volume provisioner (for StatefulSet storage)

## Quick Start

### 1. Create Namespace and ConfigMap

```bash
kubectl apply -f namespace.yaml
kubectl apply -f configmap.yaml
```

### 2. Create RBAC

```bash
kubectl apply -f rbac.yaml
```

### 3. Create Services

```bash
kubectl apply -f service.yaml
```

### 4. Create Network Policies (Optional)

```bash
kubectl apply -f network-policy.yaml
```

### 5. Deploy StatefulSet

```bash
kubectl apply -f statefulset.yaml
```

### 6. Create HPA (Optional)

```bash
kubectl apply -f hpa.yaml
```

### All at Once

```bash
kubectl apply -f ./
```

## Deployment Steps (Detailed)

### Step 1: Verify Prerequisites

```bash
# Check Kubernetes version
kubectl version --short

# Check available storage classes
kubectl get storageclass

# Check node resources
kubectl describe nodes
```

### Step 2: Create Image (Local Registry)

```bash
# Build Docker image
docker build -t aegis-scheduler:latest .

# Push to registry (or use locally for testing)
docker tag aegis-scheduler:latest <registry>/aegis-scheduler:latest
docker push <registry>/aegis-scheduler:latest
```

### Step 3: Deploy Manifests

```bash
# Create everything
kubectl apply -f ./

# Monitor deployment
kubectl get pods -n aegis -w

# Check pod logs
kubectl logs -n aegis aegis-scheduler-0 -f
```

### Step 4: Verify Cluster

```bash
# Check pods are running
kubectl get pods -n aegis

# Check services
kubectl get svc -n aegis

# Check StatefulSet
kubectl get statefulset -n aegis

# Check PVCs
kubectl get pvc -n aegis
```

## Accessing the Cluster

### Port Forwarding

```bash
# Forward metrics port
kubectl port-forward -n aegis svc/aegis-scheduler-client 8000:8000

# Forward consensus port
kubectl port-forward -n aegis svc/aegis-scheduler-client 6000:6000

# Access metrics
curl http://localhost:8000/metrics
curl http://localhost:8000/health
```

### LoadBalancer (if supported)

```bash
# Get external IP
kubectl get svc -n aegis aegis-scheduler-client

# Access via external IP
curl http://<EXTERNAL-IP>:8000/health
```

## Monitoring

### Prometheus Integration

Add to Prometheus scrape config:

```yaml
scrape_configs:
  - job_name: 'aegis-scheduler'
    kubernetes_sd_configs:
      - role: pod
        namespaces:
          names:
            - aegis
    relabel_configs:
      - source_labels: [__meta_kubernetes_pod_label_app]
        action: keep
        regex: aegis-scheduler
      - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_port]
        action: keep
        regex: "8000"
```

### Grafana Dashboards

Import dashboards showing:
- `aegis_consensus_leader_id`
- `aegis_quorum_healthy_peers`
- `aegis_rpc_latency_ms`
- `aegis_wal_entries_total`
- `aegis_snapshot_count`

## Configuration

### Adjust Replica Count

Edit `statefulset.yaml`:

```yaml
spec:
  replicas: 5  # Change from 3 to desired count
```

### Adjust Resource Limits

Edit `statefulset.yaml` `resources` section:

```yaml
resources:
  requests:
    cpu: "4"
    memory: "16Gi"
  limits:
    cpu: "8"
    memory: "32Gi"
```

### Adjust Storage Size

Edit `statefulset.yaml` `volumeClaimTemplates`:

```yaml
volumeClaimTemplates:
  - metadata:
      name: data
    spec:
      resources:
        requests:
          storage: 500Gi  # Change to desired size
```

### Change Storage Class

Edit `statefulset.yaml`:

```yaml
volumeClaimTemplates:
  - metadata:
      name: data
    spec:
      storageClassName: fast-ssd  # Or your storage class
```

## Troubleshooting

### Pods not starting

```bash
# Check pod events
kubectl describe pod -n aegis aegis-scheduler-0

# Check logs
kubectl logs -n aegis aegis-scheduler-0 --previous

# Check PVC status
kubectl get pvc -n aegis
```

### Network connectivity issues

```bash
# Check service DNS
kubectl run -it --rm debug --image=busybox --restart=Never -- \
  nslookup aegis-scheduler.aegis.svc.cluster.local

# Test connectivity between pods
kubectl exec -it -n aegis aegis-scheduler-0 -- \
  curl aegis-scheduler-1.aegis-scheduler:6000
```

### High resource usage

```bash
# Check resource metrics
kubectl top pods -n aegis

# Check node resources
kubectl top nodes

# Adjust limits in statefulset.yaml and reapply
```

### Storage issues

```bash
# Check PVC status
kubectl get pvc -n aegis

# Check PV status
kubectl get pv

# Check storage class
kubectl get storageclass
```

## Scaling

### Add Nodes to Cluster

```bash
# Update replicas in statefulset.yaml
kubectl apply -f statefulset.yaml

# Monitor rollout
kubectl rollout status -n aegis statefulset/aegis-scheduler -w
```

### Remove Nodes from Cluster

```bash
# Scale down replicas
kubectl scale statefulset -n aegis aegis-scheduler --replicas=2

# Wait for graceful shutdown
```

## Backup & Recovery

### Backup PVCs

```bash
# Snapshot PVCs
for pvc in $(kubectl get pvc -n aegis -o jsonpath='{.items[*].metadata.name}'); do
  kubectl exec -n aegis <pod> -- \
    tar czf /tmp/backup-$pvc.tar.gz /var/lib/aegis
  kubectl cp aegis/<pod>:/tmp/backup-$pvc.tar.gz .
done
```

### Restore from Backup

```bash
# Copy backup into pod
kubectl cp backup.tar.gz aegis/<pod>:/tmp/

# Extract
kubectl exec -n aegis <pod> -- \
  tar xzf /tmp/backup.tar.gz -C /
```

## Cleanup

### Delete Entire Deployment

```bash
# Delete all resources
kubectl delete namespace aegis

# Or delete individually
kubectl delete -f ./
```

### Delete Specific Resources

```bash
# Delete StatefulSet (keeps PVCs)
kubectl delete statefulset -n aegis aegis-scheduler

# Delete PVCs (destructive)
kubectl delete pvc -n aegis -l app=aegis-scheduler

# Delete services
kubectl delete svc -n aegis -l app=aegis-scheduler
```

## Best Practices

1. **Always use at least 3 replicas** for consensus quorum
2. **Use PersistentVolumes** on high-performance storage (SSD/NVMe)
3. **Enable Pod Disruption Budgets** to prevent quorum loss during maintenance
4. **Use Network Policies** to restrict traffic
5. **Monitor metrics** continuously
6. **Regular backups** of WAL and snapshots
7. **Rolling updates** to maintain quorum during upgrades
8. **Separate namespaces** for different environments

## Notes

- StatefulSet ensures ordered, predictable pod names for consensus
- Headless service allows DNS-based peer discovery
- PVC provides durability across pod restarts
- RBAC limits pod permissions
- Network policies restrict unauthorized access
- HPA can scale up but consensus typically runs fixed size

## References

- [Kubernetes StatefulSets](https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/)
- [Persistent Volumes](https://kubernetes.io/docs/concepts/storage/persistent-volumes/)
- [Network Policies](https://kubernetes.io/docs/concepts/services-networking/network-policies/)
- [RBAC](https://kubernetes.io/docs/reference/access-authn-authz/rbac/)

---

**Last Updated**:   
**Project**: AEGIS Distributed AI Inference Scheduler
