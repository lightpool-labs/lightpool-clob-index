use std::net::SocketAddr;

use bytes::Bytes;
use futures_util::SinkExt;
use lightpool_sdk::lightpool_types::SignedTransaction;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug)]
pub struct MempoolClient {
    addr: SocketAddr,
}

impl MempoolClient {
    pub fn new(addr: &str) -> AppResult<Self> {
        let addr = addr
            .parse::<SocketAddr>()
            .map_err(|error| AppError::Internal(format!("invalid mempool address: {error}")))?;
        Ok(Self { addr })
    }

    pub async fn submit_transaction(&self, tx: &SignedTransaction) -> AppResult<()> {
        let stream = TcpStream::connect(self.addr)
            .await
            .map_err(|error| AppError::Internal(format!("mempool connect failed: {error}")))?;

        let mut transport = Framed::new(stream, LengthDelimitedCodec::new());
        let tx_bytes = bincode::serialize(tx)
            .map_err(|error| AppError::Internal(format!("serialize transaction: {error}")))?;

        transport
            .send(Bytes::from(tx_bytes))
            .await
            .map_err(|error| AppError::Internal(format!("mempool send failed: {error}")))?;

        Ok(())
    }
}
