pub mod accounts;
pub mod health;
pub mod markets;
pub mod orders;
pub mod spot;
pub mod tx;
pub mod ws;

use axum::Router;

use crate::state::AppState;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .nest("/health", health::router())
        .nest("/markets", markets::router())
        .nest("/spot", spot::router())
        .nest("/accounts", accounts::router())
        .nest("/orders", orders::router())
        .nest("/tx", tx::router())
        .nest("/ws", ws::router())
}
