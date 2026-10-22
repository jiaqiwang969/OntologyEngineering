//! Same source pools and budget, different priority; never changes applicability.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pool {
    pub label: String,
    pub pages: Vec<Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replay {
    pub schema: String,
    pub query: String,
    pub context_sha256: String,
    pub provider: String,
    pub index_identity: Value,
    pub limit: usize,
    pub pools: Vec<Pool>,
}
impl Replay {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "oev2.retrieval-pools/1"
                && !self.query.trim().is_empty()
                && self.context_sha256.len() == 64
                && !self.provider.is_empty()
                && self.index_identity["sha256"]
                    .as_str()
                    .is_some_and(|s| s.len() == 64),
            "retrieval_replay_identity_invalid"
        );
        ensure!(
            (1..=128).contains(&self.limit) && self.pools.len() <= 32,
            "retrieval_replay_budget"
        );
        let mut labels = BTreeSet::new();
        for pool in &self.pools {
            ensure!(
                !pool.label.is_empty()
                    && labels.insert(&pool.label)
                    && pool.pages.len() <= 128
                    && pool
                        .pages
                        .iter()
                        .all(|p| p["id"].as_str().is_some_and(|s| !s.is_empty())),
                "retrieval_replay_pool_invalid"
            );
        }
        Ok(())
    }
}
pub fn select_retrieval_pools(replay: &Replay, order: &str) -> Result<Vec<Value>> {
    replay.validate()?;
    ensure!(
        ["context_first", "query_first"].contains(&order),
        "retrieval_order_unsupported"
    );
    let mut pools: Vec<_> = replay.pools.iter().collect();
    if order == "query_first" {
        // Stable order within every existing pool; broad context remains available.
        pools.sort_by_key(|p| usize::from(p.label != "query_and_object"));
    }
    let mut selected = Vec::new();
    let mut ids = BTreeSet::new();
    for slot in 0..replay.limit {
        for pool in &pools {
            if let Some(page) = pool.pages.get(slot)
                && ids.insert(page["id"].as_str().unwrap().to_owned())
            {
                selected.push(page.clone());
            }
            if selected.len() >= replay.limit {
                break;
            }
        }
        if selected.len() >= replay.limit {
            break;
        }
    }
    Ok(selected)
}
