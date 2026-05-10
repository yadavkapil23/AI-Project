// gRPC server: serves the SchedulingService RPCs over the wire.
//
// Wraps a `DistributedKVCache` and exposes the four cross-node coordination
// methods (AllocateGlobal, DeallocateGlobal, GetStateHash, HealthCheck).

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{info, warn};

use aegis_proto::scheduling::scheduling_service_server::{
    SchedulingService, SchedulingServiceServer,
};
use aegis_proto::scheduling::{
    health_response::Status as HealthStatusEnum, AllocateRequest, AllocateResponse,
    DeallocateRequest, DeallocateResponse, HealthRequest, HealthResponse, StateHashRequest,
    StateHashResponse,
};

use crate::distributed::DistributedKVCache;

/// SchedulingServiceImpl: tonic server that proxies into a DistributedKVCache.
pub struct SchedulingServiceImpl {
    cache: Arc<DistributedKVCache>,
    node_id: String,
    started: Instant,
    /// Monotonic state epoch — bumped on each successful allocate/deallocate.
    epoch: Arc<AtomicU64>,
}

impl SchedulingServiceImpl {
    pub fn new(node_id: impl Into<String>, cache: Arc<DistributedKVCache>) -> Self {
        Self {
            cache,
            node_id: node_id.into(),
            started: Instant::now(),
            epoch: Arc::new(AtomicU64::new(0)),
        }
    }

    fn bump_epoch(&self) -> u64 {
        self.epoch.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn current_epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }
}

#[tonic::async_trait]
impl SchedulingService for SchedulingServiceImpl {
    async fn allocate_global(
        &self,
        request: Request<AllocateRequest>,
    ) -> Result<Response<AllocateResponse>, Status> {
        let req = request.into_inner();
        let request_id = if req.request_id.is_empty() {
            "rpc".to_string()
        } else {
            req.request_id
        };

        // Allocate locally on this node — the caller already picked us as the
        // remote target via NodeSelector, so we don't recurse into the global
        // allocator (which would re-enter the network layer).
        let handles = match self
            .cache
            .allocate_local_for_caller(&request_id, &req.caller_node_id, req.num_blocks as usize)
            .await
        {
            Ok(h) => h,
            Err(e) => {
                warn!("AllocateGlobal failed on {}: {:#}", self.node_id, e);
                return Ok(Response::new(AllocateResponse {
                    block_ids: vec![],
                    free_blocks: 0,
                    total_blocks: 0,
                    ok: false,
                    error: format!("{:#}", e),
                }));
            }
        };

        let stats = self.cache.local_stats();
        self.bump_epoch();

        Ok(Response::new(AllocateResponse {
            block_ids: handles.iter().map(|h| h.block_id as u64).collect(),
            free_blocks: stats.free_blocks as u32,
            total_blocks: stats.total_blocks as u32,
            ok: true,
            error: String::new(),
        }))
    }

    async fn deallocate_global(
        &self,
        request: Request<DeallocateRequest>,
    ) -> Result<Response<DeallocateResponse>, Status> {
        let req = request.into_inner();
        let blocks: Vec<usize> = req.block_ids.iter().map(|&b| b as usize).collect();

        match self.cache.deallocate(blocks).await {
            Ok(()) => {
                self.bump_epoch();
                let stats = self.cache.local_stats();
                Ok(Response::new(DeallocateResponse {
                    ok: true,
                    error: String::new(),
                    free_blocks: stats.free_blocks as u32,
                }))
            }
            Err(e) => Ok(Response::new(DeallocateResponse {
                ok: false,
                error: format!("{:#}", e),
                free_blocks: 0,
            })),
        }
    }

    async fn get_state_hash(
        &self,
        _request: Request<StateHashRequest>,
    ) -> Result<Response<StateHashResponse>, Status> {
        let hash = self.cache.get_state_hash();
        Ok(Response::new(StateHashResponse {
            state_hash: hash.as_bytes().to_vec(),
            epoch: self.current_epoch(),
            num_owned_blocks: self.cache.num_owned_blocks() as u32,
        }))
    }

    async fn health_check(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        let stats = self.cache.local_stats();
        let serving = self.cache.health_check().await.is_ok();
        Ok(Response::new(HealthResponse {
            status: if serving {
                HealthStatusEnum::Serving as i32
            } else {
                HealthStatusEnum::NotServing as i32
            },
            node_id: self.node_id.clone(),
            free_blocks: stats.free_blocks as u32,
            total_blocks: stats.total_blocks as u32,
            uptime_secs: self.started.elapsed().as_secs(),
        }))
    }
}

/// Start the scheduling gRPC server on `addr`. Returns when the server exits.
pub async fn serve(addr: SocketAddr, service: SchedulingServiceImpl) -> Result<()> {
    info!("scheduling gRPC server listening on {}", addr);
    Server::builder()
        .add_service(SchedulingServiceServer::new(service))
        .serve(addr)
        .await
        .map_err(|e| anyhow::anyhow!("scheduling server error: {}", e))
}

/// Variant that listens until `shutdown` resolves. Useful in integration tests.
pub async fn serve_with_shutdown<F>(
    addr: SocketAddr,
    service: SchedulingServiceImpl,
    shutdown: F,
) -> Result<()>
where
    F: std::future::Future<Output = ()>,
{
    info!("scheduling gRPC server (with shutdown) listening on {}", addr);
    Server::builder()
        .add_service(SchedulingServiceServer::new(service))
        .serve_with_shutdown(addr, shutdown)
        .await
        .map_err(|e| anyhow::anyhow!("scheduling server error: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allocator::KVCacheAllocator;
    use std::sync::Arc;

    fn make_service() -> SchedulingServiceImpl {
        let alloc = Arc::new(KVCacheAllocator::new(1024 * 1024, 16 * 1024).unwrap());
        let cache = Arc::new(DistributedKVCache::new("test-srv".to_string(), alloc));
        SchedulingServiceImpl::new("test-srv", cache)
    }

    #[tokio::test]
    async fn test_allocate_then_state_hash() {
        let svc = make_service();

        let resp = svc
            .allocate_global(Request::new(AllocateRequest {
                request_id: "r1".into(),
                num_blocks: 4,
                caller_node_id: "client".into(),
            }))
            .await
            .unwrap()
            .into_inner();

        assert!(resp.ok, "allocate_global should succeed: {:?}", resp.error);
        assert_eq!(resp.block_ids.len(), 4);

        let h = svc
            .get_state_hash(Request::new(StateHashRequest {
                caller_node_id: "client".into(),
            }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(h.state_hash.len(), 32);
        assert!(h.epoch >= 1);
    }

    #[tokio::test]
    async fn test_health_check_serving() {
        let svc = make_service();
        let resp = svc
            .health_check(Request::new(HealthRequest {
                caller_node_id: "client".into(),
            }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(resp.status, HealthStatusEnum::Serving as i32);
        assert_eq!(resp.node_id, "test-srv");
        assert!(resp.total_blocks > 0);
    }

    #[tokio::test]
    async fn test_deallocate_unknown_block_is_idempotent() {
        let svc = make_service();
        let resp = svc
            .deallocate_global(Request::new(DeallocateRequest {
                block_ids: vec![9999],
                caller_node_id: "client".into(),
            }))
            .await
            .unwrap()
            .into_inner();
        // Deallocating an unknown block is a no-op in our model — must not panic.
        assert!(resp.ok);
    }
}
