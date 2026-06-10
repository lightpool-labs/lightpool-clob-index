use serde::Serialize;

use crate::domain::Order;

#[derive(Debug, Serialize)]
pub struct CancelContextResponse {
    pub order: Order,
    pub chain_order_id: String,
    pub spot_market: String,
}
