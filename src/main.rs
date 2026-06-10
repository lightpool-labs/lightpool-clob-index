mod chain;
mod config;
mod error;
mod indexer;
mod models;
mod routes;
mod slug;
mod state;

use std::net::SocketAddr;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(
            |_| "lightpool_clob_index=debug,tower_http=debug".into(),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env();
    let state = AppState::new(config.clone());

    if config.enable_indexer {
        let ws_url = config.lightpool_ws_url.clone();
        let head = state.indexed_head.clone();
        let index = state.index.clone();
        let _indexer_handle = indexer::spawn(ws_url, head, index);
        tracing::info!("block indexer started");
    } else {
        tracing::info!("block indexer disabled");
    }

    let app = Router::new()
        .nest("/api", routes::api_router())
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("invalid listen address");

    tracing::info!("lightpool-clob-index listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind");
    axum::serve(listener, app).await.expect("server failed");
}
