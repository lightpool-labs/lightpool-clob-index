use std::sync::Arc;

use crate::chain::ChainClient;
use crate::config::Config;
use crate::indexer::{IndexStore, SharedIndexStore, SharedIndexedBlockHead, new_head};

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub chain: Arc<ChainClient>,
    pub indexed_head: SharedIndexedBlockHead,
    pub index: SharedIndexStore,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let chain = Arc::new(ChainClient::new(&config.lightpool_rpc_url));
        Self {
            config,
            chain,
            indexed_head: new_head(),
            index: Arc::new(IndexStore::new()),
        }
    }
}
