use std::str::FromStr;

use lightpool_sdk::{parse_token_contract, Address};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

fn parse_query_account(value: &str) -> Address {
    Address::from_str(value.trim()).unwrap_or_else(|error| {
        tracing::warn!(
            query_account = %value.trim(),
            error = %error,
            "invalid QUERY_ACCOUNT, falling back to zero address"
        );
        Address::ZERO
    })
}

pub async fn ensure_hydrated(state: &AppState, spot_market: &str, depth: u32) -> AppResult<()> {
    if state.book_store.is_hydrated(spot_market).await {
        return Ok(());
    }

    let account = parse_query_account(&state.config.query_account);
    let spot = parse_token_contract(spot_market)
        .map_err(|e| AppError::BadRequest(format!("invalid spot market: {e}")))?;

    let chain_book = state.chain.get_book(account, spot, depth).await?;
    let last_trade_price = if let Some(price) = state.index.last_trade_price(spot_market).await {
        Some(price)
    } else {
        let market_info = state.chain.get_market_info(account, spot).await?;
        market_info.last_price
    };

    state
        .book_store
        .hydrate_from_chain(spot_market, &chain_book, last_trade_price)
        .await;

    Ok(())
}
