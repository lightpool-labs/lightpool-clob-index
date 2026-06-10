mod processor;
mod store;

use std::time::Duration;

use lightpool_sdk::{Message, Subscription, WebSocketClient};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub use processor::index_order_created;
pub use store::{
    market_uuid, IndexStore, IndexedBlockHead, SharedIndexStore, SharedIndexedBlockHead, new_head,
};

use crate::error::{AppError, AppResult};

use processor::process_block;

pub fn spawn(
    ws_url: String,
    head: SharedIndexedBlockHead,
    index: SharedIndexStore,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match run_once(&ws_url, head.clone(), index.clone()).await {
                Ok(()) => {
                    tracing::warn!("indexer stream ended, reconnecting in 5s");
                }
                Err(e) => {
                    tracing::error!("indexer error: {e}, reconnecting in 5s");
                }
            }

            {
                let mut state = head.write().await;
                state.connected = false;
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })
}

async fn run_once(
    ws_url: &str,
    head: SharedIndexedBlockHead,
    index: SharedIndexStore,
) -> AppResult<()> {
    let mut client = WebSocketClient::new(Some(ws_url.to_string()))
        .await
        .map_err(|e| AppError::Internal(format!("create ws client: {e}")))?;

    let (sender, mut receiver) = mpsc::unbounded_channel();
    let subscription_id = client
        .subscribe(Subscription::NewBlocks, sender)
        .await
        .map_err(|e| AppError::Internal(format!("subscribe NewBlocks: {e}")))?;

    tracing::info!(subscription_id, "indexer subscribed to NewBlocks");

    {
        let mut state = head.write().await;
        state.connected = true;
    }

    while let Some(message) = receiver.recv().await {
        match message {
            Message::NewBlock(block) => {
                let block_num = block.block_num;
                let digest = hex::encode(block.digest.as_bytes());
                let tx_count = block.transaction_outputs.len();

                tracing::info!(block_num, tx_count, "processing block");

                process_block(&index, block).await;

                let mut state = head.write().await;
                state.block_num = block_num;
                state.digest = digest;
                state.tx_count = tx_count;
            }
            Message::Error(err) => {
                return Err(AppError::Internal(format!("ws error: {err}")));
            }
        }
    }

    Ok(())
}
