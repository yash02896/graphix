use std::sync::Arc;
use std::{fmt::Debug, hash::Hash};

use anyhow::anyhow;
use async_trait::async_trait;

use crate::types::{IndexingStatus, POIRequest, ProofOfIndexing};

#[async_trait]
pub trait Indexer: Send + Sync + Debug {
    /// Uniquely identifies the indexer. This is relied on for the `Hash` and `Eq` impls.
    fn id(&self) -> &str;
    fn address(&self) -> Option<&[u8]>;

    async fn indexing_statuses(self: Arc<Self>) -> Result<Vec<IndexingStatus>, anyhow::Error>;

    async fn proofs_of_indexing(self: Arc<Self>, requests: Vec<POIRequest>)
        -> Vec<ProofOfIndexing>;

    /// Convenience wrapper around calling `proofs_of_indexing` for a single POI.
    async fn proof_of_indexing(
        self: Arc<Self>,
        request: POIRequest,
    ) -> Result<ProofOfIndexing, anyhow::Error> {
        let mut results = self.proofs_of_indexing(vec![request]).await;
        results
            .pop()
            .ok_or_else(|| anyhow!("no proof of indexing returned"))
    }
}

impl PartialEq for dyn Indexer {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Eq for dyn Indexer {}

impl Hash for dyn Indexer {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state)
    }
}

impl PartialOrd for dyn Indexer {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.id().partial_cmp(other.id())
    }
}

impl Ord for dyn Indexer {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id().cmp(other.id())
    }
}
