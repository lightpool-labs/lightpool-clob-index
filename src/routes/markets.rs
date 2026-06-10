use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::{
    AllocateSlugRequest, MarketQuery, MarketSortOrder, MarketsPageResponse,
    RegisterQuestionRequest, SlugResponse, DEFAULT_MARKETS_PAGE_LIMIT, MAX_MARKETS_ID_BATCH,
    MAX_MARKETS_PAGE_LIMIT, MAX_MARKETS_SLUG_BATCH,
};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(query_markets))
        .route("/slug/:slug", get(get_market_by_slug))
        .route("/index/register-question", post(register_question))
        .route("/index/allocate-slug", post(allocate_slug))
        .route("/index/position-token-specs", get(position_token_specs))
}

#[derive(Debug, Deserialize)]
pub struct QueryMarketsParams {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub slug: Option<String>,
    pub slugs: Option<String>,
    pub market_ids: Option<String>,
    pub market_addresses: Option<String>,
    pub state: Option<String>,
    pub order: Option<String>,
    pub ascending: Option<bool>,
}

fn parse_csv(value: Option<String>) -> Vec<String> {
    value
        .map(|items| {
            items
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn parse_uuid_csv(value: Option<String>) -> AppResult<Vec<Uuid>> {
    let items = parse_csv(value);
    if items.len() > MAX_MARKETS_ID_BATCH {
        return Err(AppError::BadRequest(format!(
            "market_ids accepts at most {MAX_MARKETS_ID_BATCH} ids"
        )));
    }

    items
        .into_iter()
        .map(|item| {
            Uuid::parse_str(&item)
                .map_err(|error| AppError::BadRequest(format!("invalid market id '{item}': {error}")))
        })
        .collect()
}

fn build_market_query(params: QueryMarketsParams) -> AppResult<MarketQuery> {
    let slugs = parse_csv(params.slugs);
    if slugs.len() > MAX_MARKETS_SLUG_BATCH {
        return Err(AppError::BadRequest(format!(
            "slugs accepts at most {MAX_MARKETS_SLUG_BATCH} values"
        )));
    }

    let market_addresses = parse_csv(params.market_addresses);
    if market_addresses.len() > MAX_MARKETS_ID_BATCH {
        return Err(AppError::BadRequest(format!(
            "market_addresses accepts at most {MAX_MARKETS_ID_BATCH} values"
        )));
    }

    let limit = params
        .limit
        .unwrap_or(DEFAULT_MARKETS_PAGE_LIMIT)
        .clamp(1, MAX_MARKETS_PAGE_LIMIT);
    let offset = params.offset.unwrap_or(0);

    Ok(MarketQuery {
        limit,
        offset,
        slug: params
            .slug
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        slugs,
        market_ids: parse_uuid_csv(params.market_ids)?,
        market_addresses,
        state: params
            .state
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        order: MarketSortOrder::parse(params.order.as_deref()),
        ascending: params.ascending.unwrap_or(true),
    })
}

async fn query_markets(
    State(state): State<AppState>,
    Query(params): Query<QueryMarketsParams>,
) -> AppResult<Json<MarketsPageResponse>> {
    let query = build_market_query(params)?;
    let limit = query.limit;
    let offset = query.offset;
    let (markets, total) = state.index.query_markets(query).await;

    Ok(Json(MarketsPageResponse {
        markets,
        total,
        limit,
        offset,
    }))
}

async fn get_market_by_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<Json<crate::models::Market>> {
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
