use chrono::NaiveDateTime;
use diesel::{Insertable, Queryable};

use super::schema::*;

#[derive(Debug, Insertable, Queryable)]
#[table_name = "proofs_of_indexing"]
pub struct ProofOfIndexing {
    pub timestamp: NaiveDateTime,
    pub indexer: String,
    pub deployment: String,
    pub block_number: i64,
    pub block_hash: Option<String>,
    pub proof_of_indexing: String,
}

#[derive(Debug, Insertable, Queryable)]
#[table_name = "poi_cross_check_reports"]
pub struct POICrossCheckReport {
    pub timestamp: NaiveDateTime,
    pub indexer1: String,
    pub indexer2: String,
    pub deployment: String,
    pub block_number: i64,
    pub block_hash: Option<String>,
    pub proof_of_indexing1: String,
    pub proof_of_indexing2: String,
}