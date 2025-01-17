//! Logic related to the main indexing loop performed by Graphix:
//!  1. Query `indexingStatuses` for all indexers.
//!  2. Query PoIs for recent common blocks across all indexers.
//!  3. Store the PoIs in the database.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use futures::stream::FuturesUnordered;
use futures::StreamExt;
use graphix_common_types::GraphNodeCollectedVersion;
use graphix_indexer_client::{
    IndexerClient, IndexerId, IndexingStatus, PoiRequest, ProofOfIndexing, SubgraphDeployment,
};
use tracing::*;

use crate::block_choice::BlockChoicePolicy;
use crate::PrometheusMetrics;

/// Queries all `indexingStatuses` for all the given indexers.
#[instrument(skip_all)]
pub async fn query_indexing_statuses(
    indexers: &[Arc<dyn IndexerClient>],
    metrics: &PrometheusMetrics,
) -> Vec<IndexingStatus> {
    let indexers_count = indexers.len();
    debug!(
        indexers_count = indexers_count,
        "Querying indexing statuses..."
    );

    let indexing_statuses_results = indexers
        .iter()
        .map(|indexer| async move { (indexer.clone(), indexer.clone().indexing_statuses().await) })
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .await;

    assert_eq!(indexing_statuses_results.len(), indexers.len());

    let mut indexing_statuses = vec![];
    let mut query_successes = 0;
    let mut query_failures = 0;

    for (indexer, query_result) in indexing_statuses_results {
        match query_result {
            Ok(statuses) => {
                query_successes += 1;
                metrics
                    .indexing_statuses_requests
                    .get_metric_with_label_values(&[&indexer.address_string(), "1"])
                    .unwrap()
                    .inc();

                debug!(
                    indexer_id = %indexer.address_string(),
                    statuses = %statuses.len(),
                    "Successfully queried indexing statuses"
                );
                indexing_statuses.extend(statuses);
            }

            Err(error) => {
                query_failures += 1;
                metrics
                    .indexing_statuses_requests
                    .get_metric_with_label_values(&[&indexer.address_string(), "0"])
                    .unwrap()
                    .inc();

                debug!(
                    indexer_id = %indexer.address_string(),
                    %error,
                    "Failed to query indexing statuses"
                );
            }
        }
    }

    assert_eq!(query_failures + query_successes, indexers.len());

    info!(
        indexers_count,
        indexing_statuses = indexing_statuses.len(),
        %query_successes,
        %query_failures,
        "Finished querying indexing statuses for all indexers"
    );

    indexing_statuses
}

/// Queries all `indexers` for their `graph-node` versions.
#[instrument(skip_all)]
pub async fn query_graph_node_versions(
    indexers: &[Arc<dyn IndexerClient>],
    _metrics: &PrometheusMetrics,
) -> HashMap<Arc<dyn IndexerClient>, anyhow::Result<GraphNodeCollectedVersion>> {
    let span = span!(Level::TRACE, "query_graph_node_versions");
    let _enter_span = span.enter();

    info!("Querying graph-node versions...");

    let graph_node_versions_results = indexers
        .iter()
        .map(|indexer| async move { (indexer.clone(), indexer.clone().version().await) })
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .await;

    assert_eq!(graph_node_versions_results.len(), indexers.len());

    let mut versions = HashMap::new();

    for (indexer, version_result) in graph_node_versions_results {
        match &version_result {
            Ok(version) => {
                trace!(
                    indexer_id = %indexer.address_string(),
                    version = ?version.version,
                    commit = ?version.commit,
                    "Successfully queried graph-node version"
                );
            }
            Err(error) => {
                trace!(
                    indexer_id = %indexer.address_string(),
                    %error,
                    "Failed to query graph-node version"
                );
            }
        }

        versions.insert(indexer, version_result);
    }

    info!(
        indexers = versions.len(),
        "Finished querying graph-node versions for all indexers"
    );

    versions
}

#[instrument(skip_all)]
pub async fn query_proofs_of_indexing(
    indexing_statuses: Vec<IndexingStatus>,
    block_choice_policy: BlockChoicePolicy,
) -> Vec<ProofOfIndexing> {
    info!("Query POIs for recent common blocks across indexers");

    // Identify all indexers
    let indexers = indexing_statuses
        .iter()
        .map(|status| status.indexer.clone())
        .collect::<HashSet<_>>();

    // Identify all deployments
    let deployments: HashSet<SubgraphDeployment> = HashSet::from_iter(
        indexing_statuses
            .iter()
            .map(|status| status.deployment.clone()),
    );

    // Group indexing statuses by deployment
    let statuses_by_deployment: HashMap<SubgraphDeployment, Vec<&IndexingStatus>> =
        HashMap::from_iter(deployments.iter().map(|deployment| {
            (
                deployment.clone(),
                indexing_statuses
                    .iter()
                    .filter(|status| status.deployment.eq(deployment))
                    .collect(),
            )
        }));

    // For each deployment, chooose a block on which to query the Poi
    let latest_blocks: HashMap<SubgraphDeployment, Option<u64>> =
        HashMap::from_iter(deployments.iter().map(|deployment| {
            (
                deployment.clone(),
                statuses_by_deployment.get(deployment).and_then(|statuses| {
                    block_choice_policy.choose_block(statuses.iter().copied())
                }),
            )
        }));

    // Fetch POIs for the most recent common blocks
    indexers
        .iter()
        .map(|indexer| async {
            let poi_requests = latest_blocks
                .iter()
                .filter(|(deployment, &block_number)| {
                    statuses_by_deployment
                        .get(*deployment)
                        .expect("bug in matching deployments to latest blocks and indexers")
                        .iter()
                        .any(|status| {
                            status.indexer.eq(indexer)
                                && Some(status.latest_block.number) >= block_number
                        })
                })
                .filter_map(|(deployment, block_number)| {
                    block_number.map(|block_number| PoiRequest {
                        deployment: deployment.clone(),
                        block_number,
                    })
                })
                .collect::<Vec<_>>();

            let pois = indexer.clone().proofs_of_indexing(poi_requests).await;

            debug!(
                id = %indexer.address_string(), pois = %pois.len(),
                "Successfully queried POIs from indexer"
            );

            pois
        })
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
}
