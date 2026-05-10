// aegis-scheduler-node: standalone scheduler node binary.
//
// Boots:
//   - a local KV-cache allocator (size configurable via env)
//   - a DistributedKVCache wrapping it
//   - the scheduling gRPC server on $AEGIS_GRPC_ADDR (default 0.0.0.0:50051)
//
// Peers can be pre-registered via $AEGIS_PEERS (comma-separated id=addr:port pairs):
//   AEGIS_PEERS="node-2=node-2:50051,node-3=node-3:50051"

use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::{info, warn};

use aegis_scheduler::{
    serve_scheduling, DistributedKVCache, KVCacheAllocator, SchedulingServiceImpl,
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let node_id = env::var("AEGIS_NODE_ID").unwrap_or_else(|_| "node-1".to_string());
    let addr: SocketAddr = env::var("AEGIS_GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".to_string())
        .parse()
        .context("AEGIS_GRPC_ADDR must be a valid SocketAddr like 0.0.0.0:50051")?;

    let cache_bytes: usize = env::var("AEGIS_CACHE_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(64 * 1024 * 1024); // 64 MiB default for tests
    let block_size: usize = env::var("AEGIS_BLOCK_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(16 * 1024); // 16 KiB

    let allocator = Arc::new(
        KVCacheAllocator::new(cache_bytes, block_size)
            .context("failed to build local KVCacheAllocator")?,
    );
    let cache = Arc::new(DistributedKVCache::new(node_id.clone(), allocator));

    if let Ok(peers) = env::var("AEGIS_PEERS") {
        for entry in peers.split(',').filter(|s| !s.is_empty()) {
            match entry.split_once('=') {
                Some((peer_id, peer_addr)) => {
                    let cap = cache_bytes / block_size;
                    if let Err(e) =
                        cache.register_peer(peer_id.trim().to_string(), peer_addr.trim().to_string(), cap)
                    {
                        warn!("failed to register peer {}: {:#}", peer_id, e);
                    } else {
                        info!("registered peer {} at {}", peer_id, peer_addr);
                    }
                }
                None => warn!("malformed AEGIS_PEERS entry: {}", entry),
            }
        }
    }

    info!(
        "starting AEGIS scheduler node id={} addr={} cache_bytes={} block_size={}",
        node_id, addr, cache_bytes, block_size
    );

    let service = SchedulingServiceImpl::new(node_id, cache);
    serve_scheduling(addr, service).await
}
