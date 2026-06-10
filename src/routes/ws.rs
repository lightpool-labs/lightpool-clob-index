use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::indexer::OrderBookDelta;
use crate::orderbook::ensure_hydrated;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(ws_handler))
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

#[derive(Debug, Deserialize)]
struct WsRequest {
    op: String,
    channel: Option<String>,
    spot_market: Option<String>,
    depth: Option<u32>,
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut delta_rx: Option<broadcast::Receiver<OrderBookDelta>> = None;

    loop {
        if let Some(rx) = &mut delta_rx {
            tokio::select! {
                incoming = receiver.next() => {
                    if !handle_client_message(incoming, &state, &mut sender, &mut delta_rx).await {
                        break;
                    }
                }
                delta = rx.recv() => {
                    match delta {
                        Ok(delta) => {
                            if sender
                                .send(Message::Text(serde_json::to_string(&delta).unwrap_or_default().into()))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            delta_rx = None;
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                    }
                }
            }
        } else if !handle_client_message(receiver.next().await, &state, &mut sender, &mut delta_rx).await {
            break;
        }
    }
}

async fn handle_client_message(
    incoming: Option<Result<Message, axum::Error>>,
    state: &AppState,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    delta_rx: &mut Option<broadcast::Receiver<OrderBookDelta>>,
) -> bool {
    let Some(Ok(Message::Text(text))) = incoming else {
        return false;
    };

    let request = match serde_json::from_str::<WsRequest>(&text) {
        Ok(value) => value,
        Err(e) => {
            let _ = sender
                .send(Message::Text(
                    serde_json::json!({ "type": "error", "error": format!("invalid message: {e}") })
                        .to_string()
                        .into(),
                ))
                .await;
            return true;
        }
    };

    match request.op.as_str() {
        "subscribe" => {
            if request.channel.as_deref() != Some("orderbook") {
                return true;
            }
            let Some(spot_market) = request.spot_market.filter(|value| !value.trim().is_empty())
            else {
                return true;
            };
            let depth = request.depth.unwrap_or(10).clamp(1, 50);

            if let Err(error) = ensure_hydrated(state, &spot_market, depth).await {
                let _ = sender
                    .send(Message::Text(
                        serde_json::json!({ "type": "error", "error": error.to_string() })
                            .to_string()
                            .into(),
                    ))
                    .await;
                return true;
            }

            if let Some(snapshot) = state.book_store.ws_snapshot(&spot_market, depth).await {
                let _ = sender
                    .send(Message::Text(
                        serde_json::to_string(&snapshot).unwrap_or_default().into(),
                    ))
                    .await;
            }

            *delta_rx = Some(state.book_store.subscribe(&spot_market).await);
        }
        "unsubscribe" => {
            *delta_rx = None;
        }
        _ => {}
    }

    true
}
