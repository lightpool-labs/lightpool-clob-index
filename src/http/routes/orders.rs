use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use lightpool_sdk::spot_events::OrderCreatedEvent;
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::Order;
use crate::error::{AppError, AppResult};
use crate::http::models::CancelContextResponse;
use crate::indexer::{apply_order_created_to_book, index_order_created, publish_user_order_created};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_orders))
        .route("/:id/cancel-context", get(cancel_context))
        .route("/:id/cancelled", post(mark_cancelled))
        .route("/index/from-event", post(index_from_event))
}

#[derive(Debug, Deserialize)]
pub struct ListOrdersQuery {
    pub user_address: String,
}

#[derive(Debug, Deserialize)]
pub struct CancelContextQuery {
    pub user_address: String,
}

async fn list_orders(
    State(state): State<AppState>,
    Query(query): Query<ListOrdersQuery>,
) -> Json<Vec<Order>> {
    let mut orders = state
        .index
        .list_orders_for_user(&query.user_address)
        .await;

    for order in &mut orders {
        if order.question.is_empty() || order.event_slug.is_empty() {
            if let Some(market) = state.index.get_market(order.market_id).await {
                if order.question.is_empty() {
                    order.question = market.question;
                }
                if order.event_slug.is_empty() {
                    order.event_slug = market.slug;
                }
            }
        }
    }

    Json(orders)
}

async fn cancel_context(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<CancelContextQuery>,
) -> AppResult<Json<CancelContextResponse>> {
    let (order, chain_order_id, spot_market) = state
        .index
        .order_cancel_context(id, &query.user_address)
        .await
        .ok_or_else(|| AppError::NotFound(format!("open order {id} not found")))?;

    Ok(Json(CancelContextResponse {
        order,
        chain_order_id,
        spot_market,
    }))
}

async fn mark_cancelled(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<CancelContextQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let (_, chain_order_id, _) = state
        .index
        .order_cancel_context(id, &query.user_address)
        .await
        .ok_or_else(|| AppError::NotFound(format!("open order {id} not found")))?;

    state.index.update_order_cancelled(&chain_order_id).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn index_from_event(
    State(state): State<AppState>,
    Json(event): Json<OrderCreatedEvent>,
) -> AppResult<Json<Order>> {
    let chain_order_id = event.order_id.to_string();
    let is_new = !state.index.has_chain_order(&chain_order_id).await;
    if is_new {
        apply_order_created_to_book(&state.book_store, 0, &event).await;
    }

    let order = index_order_created(&state.index, event)
        .await
        .ok_or_else(|| AppError::Internal("failed to index order from event".into()))?;
    if is_new {
        publish_user_order_created(&state.user_hub, &state.index, &chain_order_id, 0).await;
    }
    Ok(Json(order))
}
