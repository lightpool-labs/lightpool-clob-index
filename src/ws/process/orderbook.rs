use axum::extract::ws::Message;
use futures_util::stream::SplitSink;
use futures_util::SinkExt;

use crate::http::process::ensure_hydrated;
use crate::state::AppState;
use crate::ws::models::{ws_error, ws_subscribed, ws_unsubscribed, CHANNEL_ORDERBOOK_DELTA};
use crate::ws::process::WsSession;

pub async fn handle_subscribe(
    state: &AppState,
    sender: &mut SplitSink<axum::extract::ws::WebSocket, Message>,
    session: &mut WsSession,
    spot_market: &str,
    depth: u32,
) -> bool {
    if let Err(error) = ensure_hydrated(state, spot_market, depth).await {
        let _ = sender
            .send(Message::Text(ws_error(error.to_string()).into()))
            .await;
        return true;
    }

    if let Some(snapshot) = state.book_store.ws_snapshot(spot_market, depth).await {
        let text = serde_json::to_string(&snapshot).unwrap_or_default();
        if sender.send(Message::Text(text.into())).await.is_err() {
            return false;
        }
    }

    let rx = state.book_store.subscribe(spot_market).await;
    session.subscribe_orderbook(spot_market.to_string(), rx);

    let _ = sender
        .send(Message::Text(
            ws_subscribed(CHANNEL_ORDERBOOK_DELTA, spot_market).into(),
        ))
        .await;
    true
}

pub async fn handle_unsubscribe(
    sender: &mut SplitSink<axum::extract::ws::WebSocket, Message>,
    session: &mut WsSession,
    spot_market: Option<&str>,
) {
    session.cancel_channel(CHANNEL_ORDERBOOK_DELTA, spot_market);
    if let Some(key) = spot_market {
        let _ = sender
            .send(Message::Text(
                ws_unsubscribed(CHANNEL_ORDERBOOK_DELTA, key).into(),
            ))
            .await;
    }
}
