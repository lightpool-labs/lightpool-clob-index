use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};

use crate::error::{AppError, AppResult};
use crate::models::{AllocateSlugRequest, Market, RegisterQuestionRequest, SlugResponse};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_markets))
        .route("/slug/:slug", get(get_market_by_slug))
        .route("/index/register-question", post(register_question))
        .route("/index/allocate-slug", post(allocate_slug))
        .route("/index/position-token-specs", get(position_token_specs))
}

async fn list_markets(State(state): State<AppState>) -> Json<Vec<Market>> {
    Json(state.index.list_markets().await)
}

async fn get_market_by_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<Json<Market>> {
    state
        .index
        .get_event_by_slug(&slug)
        .await
        .ok_or_else(|| AppError::NotFound(format!("market {slug} not found")))
        .map(Json)
}

async fn register_question(
    State(state): State<AppState>,
    Json(body): Json<RegisterQuestionRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if body.question.trim().is_empty() {
        return Err(AppError::BadRequest("question is required".into()));
    }
    if body.slug.trim().is_empty() {
        return Err(AppError::BadRequest("slug is required".into()));
    }
    let icon_url = body
        .icon_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    state
        .index
        .register_question(body.question.trim(), body.slug.trim(), icon_url)
        .await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn allocate_slug(
    State(state): State<AppState>,
    Json(body): Json<AllocateSlugRequest>,
) -> AppResult<Json<SlugResponse>> {
    if body.question.trim().is_empty() {
        return Err(AppError::BadRequest("question is required".into()));
    }
    let slug = state.index.allocate_slug(body.question.trim()).await;
    Ok(Json(SlugResponse { slug }))
}

async fn position_token_specs(
    State(state): State<AppState>,
) -> Json<Vec<crate::models::BalanceTokenSpec>> {
    let specs = state.index.position_token_specs().await;
    Json(
        specs
            .into_iter()
            .map(|(symbol, address)| crate::models::BalanceTokenSpec { symbol, address })
            .collect(),
    )
}
