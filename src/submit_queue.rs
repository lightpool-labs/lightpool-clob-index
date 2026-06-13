use std::sync::Arc;

use lightpool_sdk::lightpool_types::SignedTransaction;
use lightpool_sdk::types::SubmitTransactionResponse;
use tokio::sync::{mpsc, oneshot};

use crate::chain::ChainClient;
use crate::error::{AppError, AppResult};

struct SubmitJob {
    tx: SignedTransaction,
    respond_to: oneshot::Sender<AppResult<SubmitTransactionResponse>>,
}

#[derive(Clone)]
pub struct SubmitQueue {
    sender: mpsc::Sender<SubmitJob>,
}

impl SubmitQueue {
    pub fn spawn(chain: Arc<ChainClient>, capacity: usize) -> Self {
        let (sender, mut receiver) = mpsc::channel::<SubmitJob>(capacity);

        tokio::spawn(async move {
            while let Some(job) = receiver.recv().await {
                let result = chain.submit_transaction(job.tx).await;
                if job.respond_to.send(result).is_err() {
                    tracing::warn!("submit queue client dropped before response was sent");
                }
            }
            tracing::error!("submit queue worker stopped");
        });

        tracing::info!("submit queue worker started capacity={capacity}");
        Self { sender }
    }

    pub async fn submit(&self, tx: SignedTransaction) -> AppResult<SubmitTransactionResponse> {
        let (respond_to, response_rx) = oneshot::channel();
        self.sender
            .send(SubmitJob { tx, respond_to })
            .await
            .map_err(|_| AppError::ServiceUnavailable("submit queue unavailable".into()))?;

        response_rx
            .await
            .map_err(|_| AppError::Internal("submit queue worker dropped".into()))?
    }
}
