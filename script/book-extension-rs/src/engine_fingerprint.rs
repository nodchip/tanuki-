use std::collections::BTreeMap;

use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize)]
pub struct EngineFingerprintInput {
    pub engine_sha256: String,
    pub hash_mb: usize,
    pub threads: usize,
    pub multipv: usize,
    pub extra: BTreeMap<String, Option<String>>,
    pub corpus_nodes: u64,
    pub search_timeout_sec: f64,
    pub engine_config_revision: u64,
}

pub fn compute_engine_fingerprint(
    input: &EngineFingerprintInput,
) -> Result<String, serde_json::Error> {
    let encoded = serde_json::to_vec(input)?;
    Ok(format!("{:x}", Sha256::digest(encoded)))
}
