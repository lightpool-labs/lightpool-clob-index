use axum::{extract::State, routing::post, Json, Router};

use crate::error::{AppError, AppResult};
use crate::http::models::{SubmitTxRequest, SubmitTxResponse};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/submit", post(submit_transaction))
}

async fn submit_transaction(
    State(state): State<AppState>,
    Json(body): Json<SubmitTxRequest>,
) -> AppResult<Json<SubmitTxResponse>> {
    let response = state.submit_queue.submit(body.tx).await?;

    if !response.receipt.is_success() {
        return Err(AppError::Internal(format!(
            "transaction failed: {:?}",
            response.receipt.status
        )));
    }

    Ok(Json(SubmitTxResponse {
        digest: response.digest,
        receipt: response.receipt,
    }))
}
