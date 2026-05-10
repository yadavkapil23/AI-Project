#!/bin/bash

# Start 3-node AEGIS cluster via Docker Compose

set -e

echo "================================"
echo "Starting AEGIS 3-Node Cluster"
echo "================================"

# Check docker-compose
if ! command -v docker-compose &> /dev/null; then
    echo "Error: docker-compose not found"
    exit 1
fi

# Start cluster
cd "$(dirname "$0")/../docker"

echo "Building images..."
docker-compose build

echo "Starting services..."
docker-compose up -d

echo "Waiting for services to be healthy..."
sleep 10

# Check health
echo ""
echo "Cluster Health Status:"
echo "====================

for i in 1 2 3; do
    port=$((9000 + i))
    if curl -s "http://localhost:${port}/health" > /dev/null 2>&1; then
        echo "✓ Node $i is healthy"
    else
        echo "✗ Node $i is not responding"
    fi
done

echo ""
echo "Services Running:"
echo "================"
docker-compose ps

echo ""
echo "Access Points:"
echo "=============="
echo "• Load Balanced gRPC:  localhost:50050"
echo "• Node 1 gRPC:         localhost:50051"
echo "• Node 2 gRPC:         localhost:50052"
echo "• Node 3 gRPC:         localhost:50053"
echo ""
echo "• Prometheus:          http://localhost:9090"
echo "• Jaeger Tracing:      http://localhost:16686"
echo ""
echo "To stop cluster: docker-compose down"
echo "To view logs:    docker-compose logs -f <service>"
