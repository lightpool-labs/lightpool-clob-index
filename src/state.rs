use std::sync::Arc;

use crate::chain::ChainClient;
use crate::config::Config;
use crate::indexer::{
    BookStore, IndexStore, SharedBookStore, SharedIndexStore, SharedIndexedBlockHead, new_head,
};
use crate::submit_queue::SubmitQueue;
use crate::ws::process::{SharedUserEventHub, UserEventHub};

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub chain: Arc<ChainClient>,
    pub submit_queue: SubmitQueue,
    pub indexed_head: SharedIndexedBlockHead,
    pub index: SharedIndexStore,
    pub book_store: SharedBookStore,
    pub user_hub: SharedUserEventHub,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let chain = Arc::new(ChainClient::new(&config.lightpool_rpc_url));
        let submit_queue = SubmitQueue::spawn(chain.clone(), config.submit_queue_capacity);
        Self {
            config,
            chain,
            submit_queue,
            indexed_head: new_head(),
            index: Arc::new(IndexStore::new()),
            book_store: Arc::new(BookStore::new()),
            user_hub: Arc::new(UserEventHub::new()),
        }
    }
}
