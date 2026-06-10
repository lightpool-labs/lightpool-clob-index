use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct BookLevel {
    pub price: String,
    pub size: String,
}

#[derive(Debug, Serialize)]
pub struct BookResponse {
    pub sequence: u64,
    pub bids: Vec<BookLevel>,
    pub asks: Vec<BookLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_trade_price: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub id: Uuid,
    pub slug: String,
    pub question: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    pub market_address: String,
    pub collateral_token: String,
    pub yes_token: String,
    pub no_token: String,
    pub yes_spot_market: String,
    pub no_spot_market: String,
    pub state: String,
    pub resolution_deadline: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: Uuid,
    pub market_id: Uuid,
    #[serde(default)]
    pub event_slug: String,
    pub question: String,
    pub outcome: String,
    pub side: String,
    pub price: String,
    pub size: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceEntry {
    pub token: String,
    pub symbol: String,
    pub total: String,
    pub locked: String,
    pub available: String,
}

#[derive(Debug, Deserialize)]
pub struct BalancesRequest {
    pub tokens: Vec<BalanceTokenSpec>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BalanceTokenSpec {
    pub symbol: String,
    pub address: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterQuestionRequest {
    pub question: String,
    pub slug: String,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AllocateSlugRequest {
    pub question: String,
}

#[derive(Debug, Serialize)]
pub struct SlugResponse {
    pub slug: String,
}

#[derive(Debug, Deserialize)]
pub struct SubmitTxRequest {
    pub tx: lightpool_sdk::lightpool_types::SignedTransaction,
}

#[derive(Debug, Serialize)]
pub struct SubmitTxResponse {
    pub digest: String,
    pub receipt: lightpool_sdk::TransactionReceipt,
}

#[derive(Debug, Serialize)]
pub struct MarketInfoResponse {
    pub last_price: Option<String>,
    pub state: String,
    pub min_order_size: String,
    pub tick_size: String,
    pub maker_fee_bps: u16,
    pub taker_fee_bps: u16,
    pub allow_market_orders: bool,
}

#[derive(Debug, Serialize)]
pub struct CancelContextResponse {
    pub order: Order,
    pub chain_order_id: String,
    pub spot_market: String,
}
