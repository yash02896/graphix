use std::time::Duration;

use crate::prelude::{Indexer, IndexingStatus};
use eventuals::*;
use futures::stream::FuturesOrdered;
use futures::StreamExt;
use tracing::*;

pub fn indexing_statuses<I>(indexers: Eventual<Vec<I>>) -> Eventual<Vec<IndexingStatus<I>>>
where
    I: Indexer + 'static,
{
    join((indexers, timer(Duration::from_secs(20))))
        .map(|(indexers, _)| query_indexing_statuses(indexers))
}

pub async fn query_indexing_statuses<I>(indexers: Vec<I>) -> Vec<IndexingStatus<I>>
where
    I: Indexer,
{
    info!("Query indexing statuses");

    indexers
        .iter()
        .map(|indexer| indexer.clone().indexing_statuses())
        .collect::<FuturesOrdered<_>>()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .zip(indexers)
        .filter_map(skip_errors)
        .flatten()
        .collect()
}

fn skip_errors<I>(
    result: (Result<Vec<IndexingStatus<I>>, anyhow::Error>, I),
) -> Option<Vec<IndexingStatus<I>>>
where
    I: Indexer,
{
    let url = result.1.urls().status.to_string();
    match result.0 {
        Ok(indexing_statuses) => {
            info!(
                id = %result.1.id(), %url, statuses = %indexing_statuses.len(),
                "Successfully queried indexing statuses"
            );

            Some(indexing_statuses)
        }
        Err(error) => {
            warn!(
                id = %result.1.id(), %url, %error,
                "Failed to query indexing statuses"
            );
            None
        }
    }
}
