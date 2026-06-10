use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use lightpool_sdk::{parse_token_contract, Address};
use serde::Deserialize;
use std::str::FromStr;

use crate::chain::{format_price_pieces, format_token_amount};
use crate::error::{AppError, AppResult};
use crate::models::{BookLevel, BookResponse, MarketInfoResponse};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/:spot_market/book", get(get_book))
        .route("/:spot_market/info", get(get_market_info))
}

#[derive(Debug, Deserialize)]
pub struct SpotQuery {
    pub account: String,
    pub depth: Option<u32>,
}

async fn parse_account(account: &str) -> AppResult<Address> {
    Address::from_str(account.trim())
        .map_err(|e| AppError::BadRequest(format!("invalid account: {e}")))
}

async fn parse_spot_market(spot_market: &str) -> AppResult<lightpool_sdk::ContractAddress> {
    parse_token_contract(spot_market)
        .map_err(|e| AppError::BadRequest(format!("invalid spot market: {e}")))
}

async fn get_book(
    State(state): State<AppState>,
    Path(spot_market): Path<String>,
    Query(query): Query<SpotQuery>,
) -> AppResult<Json<BookResponse>> {
    let account = parse_account(&query.account).await?;
    let spot_market = parse_spot_market(&spot_market).await?;
    let depth = query.depth.unwrap_or(10);

    let book = state.chain.get_book(account, spot_market, depth).await?;
    let last_trade_price = if let Some(price) = state.index.last_trade_price(&spot_market.to_string()).await {
        Some(format_price_pieces(price))
    } else {
        let market_info = state.chain.get_market_info(account, spot_market).await?;
        market_info.last_price.map(|price| format_price_pieces(price))
    };

    Ok(Json(BookResponse {
        bids: book
            .best_bids
            .into_iter()
            .map(|level| BookLevel {
                price: format_price_pieces(level.price),
                size: format_token_amount(level.total_quantity),
            })
            .collect(),
        asks: book
            .best_asks
            .into_iter()
            .map(|level| BookLevel {
                price: format_price_pieces(level.price),
                size: format_token_amount(level.total_quantity),
            })
            .collect(),
        last_trade_price,
    }))
}

async fn get_market_info(
    State(state): State<AppState>,
    Path(spot_market): Path<String>,
    Query(query): Query<SpotQuery>,
) -> AppResult<Json<MarketInfoResponse>> {
    let account = parse_account(&query.account).await?;
    let spot_market = parse_spot_market(&spot_market).await?;

    let info = state.chain.get_market_info(account, spot_market).await?;

    Ok(Json(MarketInfoResponse {
        last_price: info.last_price.map(|price| format_price_pieces(price)),
        state: info.state.to_string(),
        min_order_size: format_token_amount(info.min_order_size),
        tick_size: format_token_amount(info.tick_size),
        maker_fee_bps: info.maker_fee_bps,
        taker_fee_bps: info.taker_fee_bps,
        allow_market_orders: info.allow_market_orders,
    }))
}
