use lightpool_sdk::event_contract_events::{
    EventContractBurnedEvent, EventContractCreatedEvent, EventContractMintedEvent,
    EventContractRedeemedEvent, EventContractResolvedEvent,
};
use lightpool_sdk::spot_events::{
    OrderCancelledEvent, OrderCreatedEvent, OrderEventType, OrderFilledEvent,
    parse_spot_event_data,
};
use lightpool_sdk::token_events::{
    TokenCreatedEvent, TokenMintedEvent, TransferEvent, parse_event_data,
};
use lightpool_sdk::{EventData, EventType, ExecutionStatus, TransactionEvent, VerifiedBlock};
use lightpool_sdk::lightpool_types::TransactionResult;
use uuid::Uuid;

use crate::chain::{format_price_pieces, format_token_amount};
use crate::domain::{Market, Order};
use crate::ws::process::SharedUserEventHub;

use super::book_store::SharedBookStore;
use super::store::{market_uuid, question_from_hash, IndexStore, SharedIndexStore};

pub async fn process_block(
    store: &SharedIndexStore,
    book_store: &SharedBookStore,
    user_hub: &SharedUserEventHub,
    block: VerifiedBlock,
) {
    let block_num = block.block_num;

    for tx_result in block.transaction_outputs {
        log_tx_result(&tx_result);

        if !tx_result.is_success() {
            continue;
        }

        for event in &tx_result.receipt.events {
            let EventType::Call(action_name) = &event.event_type else {
                continue;
            };

            tracing::info!(action = action_name.as_str(), detail = %format_event_detail(event), "processing tx event");

            match action_name.as_str() {
                "event_contract_created" => {
                    if let EventData::Bytes(data) = &event.data {
                        match bincode::deserialize::<EventContractCreatedEvent>(data) {
                            Ok(created) => {
                                index_market_created(store, created).await;
                            }
                            Err(e) => {
                                tracing::warn!(
                                    error = %e,
                                    "failed to decode event_contract_created"
                                );
                            }
                        }
                    }
                }
                "event_contract_resolved" => {
                    if let EventData::Bytes(data) = &event.data {
                        if let Ok(resolved) =
                            bincode::deserialize::<EventContractResolvedEvent>(data)
                        {
                            store
                                .update_market_state(
                                    &resolved.market_address.to_string(),
                                    "Resolved",
                                )
                                .await;
                        }
                    }
                }
                "order_created" => {
                    if let EventData::Bytes(data) = &event.data {
                        if let Ok(created) = bincode::deserialize::<OrderCreatedEvent>(data) {
                            let chain_order_id = created.order_id.to_string();
                            if !store.has_chain_order(&chain_order_id).await {
                                apply_order_created_to_book(book_store, block_num, &created).await;
                                index_order_created(store, created).await;
                                publish_user_order_created(
                                    user_hub,
                                    store,
                                    &chain_order_id,
                                    block_num,
                                )
                                .await;
                            }
                        }
                    }
                }
                "order_cancelled" => {
                    if let EventData::Bytes(data) = &event.data {
                        if let Ok(cancelled) = bincode::deserialize::<OrderCancelledEvent>(data) {
                            let chain_order_id = cancelled.order_id.to_string();
                            if let Some(spot_market) =
                                store.spot_market_for_chain_order(&chain_order_id).await
                            {
                                book_store
                                    .apply_cancelled(
                                        &spot_market,
                                        cancelled.side,
                                        cancelled.price,
                                        cancelled.cancelled_amount,
                                        block_num,
                                    )
                                    .await;
                            }
                            store.update_order_cancelled(&chain_order_id).await;
                            publish_user_order_cancelled(user_hub, store, &chain_order_id, block_num)
                                .await;
                        }
                    }
                }
                "order_filled" => {
                    if let EventData::Bytes(data) = &event.data {
                        if let Ok(filled) = bincode::deserialize::<OrderFilledEvent>(data) {
                            let chain_order_id = filled.order_id.to_string();
                            let spot_market =
                                crate::spot_market::normalize_spot_market_key(&filled.market.to_string());
                            store
                                .record_last_trade_price(&spot_market, filled.price)
                                .await;
                            book_store
                                .apply_filled(
                                    &spot_market,
                                    filled.side,
                                    filled.price,
                                    filled.fill_amount,
                                    block_num,
                                    filled.price,
                                )
                                .await;
                            store
                                .update_order_fill(
                                    &chain_order_id,
                                    filled.fill_amount,
                                    filled.remaining_amount,
                                    filled.is_fully_filled,
                                )
                                .await;
                            publish_user_order_filled(
                                user_hub,
                                store,
                                &chain_order_id,
                                &spot_market,
                                filled.price,
                                filled.fill_amount,
                                filled.remaining_amount,
                                filled.is_fully_filled,
                                filled.side,
                                block_num,
                            )
                            .await;
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn log_tx_result(tx_result: &TransactionResult) {
    let digest = hex::encode(tx_result.transaction_digest().as_bytes());
    let sender = tx_result.sender().to_string();
    let block_num = tx_result.receipt.block_num;

    match &tx_result.receipt.status {
        ExecutionStatus::Failure(msg) => {
            tracing::info!(
                tx_digest = %digest,
                block_num,
                sender = %sender,
                success = false,
                error = msg.as_str(),
                "tx failed"
            );
            return;
        }
        ExecutionStatus::Success => {}
    }

    let event_summaries: Vec<String> = tx_result
        .receipt
        .events
        .iter()
        .map(format_event_detail)
        .collect();

    tracing::info!(
        tx_digest = %digest,
        block_num,
        sender = %sender,
        success = true,
        event_count = event_summaries.len(),
        events = event_summaries.join(" | "),
        "tx committed"
    );
}

fn event_action_name(event_type: &EventType) -> &str {
    match event_type {
        EventType::Call(name) => name.as_str(),
        EventType::Transfer => "transfer",
        EventType::System => "system",
        EventType::Custom(name) => name.as_str(),
    }
}

fn format_event_detail(event: &TransactionEvent) -> String {
    let action = event_action_name(&event.event_type);

    if let Some(data) = parse_event_data(&event.event_type, &event.data) {
        return format!("{action}: {data}");
    }

    if let Some(data) = parse_spot_event_data(&event.event_type, &event.data) {
        return format!("{action}: {data}");
    }

    let EventData::Bytes(bytes) = &event.data else {
        return format!("{action}: (no payload)");
    };

    match action {
        "event_contract_created" => {
            if let Ok(e) = bincode::deserialize::<EventContractCreatedEvent>(bytes) {
                let question = question_from_hash(&e.question_hash);
                return format!(
                    "event_contract_created: question={} market={} yes={} no={} collateral={} deadline={} state={}",
                    question,
                    e.market_address,
                    e.yes_token,
                    e.no_token,
                    e.collateral_token,
                    e.resolution_deadline,
                    e.state,
                );
            }
        }
        "event_contract_minted" => {
            if let Ok(e) = bincode::deserialize::<EventContractMintedEvent>(bytes) {
                return format!(
                    "event_contract_minted: market={} user={} amount={}",
                    e.market_address,
                    e.user,
                    format_token_amount(e.amount),
                );
            }
        }
        "event_contract_burned" => {
            if let Ok(e) = bincode::deserialize::<EventContractBurnedEvent>(bytes) {
                return format!(
                    "event_contract_burned: market={} user={} amount={}",
                    e.market_address,
                    e.user,
                    format_token_amount(e.amount),
                );
            }
        }
        "event_contract_resolved" => {
            if let Ok(e) = bincode::deserialize::<EventContractResolvedEvent>(bytes) {
                return format!(
                    "event_contract_resolved: market={} outcome={}",
                    e.market_address, e.outcome
                );
            }
        }
        "event_contract_redeemed" => {
            if let Ok(e) = bincode::deserialize::<EventContractRedeemedEvent>(bytes) {
                return format!(
                    "event_contract_redeemed: market={} user={} amount={}",
                    e.market_address,
                    e.user,
                    format_token_amount(e.amount),
                );
            }
        }
        "token_created" => {
            if let Ok(e) = bincode::deserialize::<TokenCreatedEvent>(bytes) {
                return format!(
                    "token_created: symbol={} name={} supply={} token={} to={} mintable={}",
                    e.symbol,
                    e.name,
                    format_token_amount(e.total_supply),
                    e.token_address,
                    e.to,
                    e.mintable,
                );
            }
        }
        "token_minted" => {
            if let Ok(e) = bincode::deserialize::<TokenMintedEvent>(bytes) {
                return format!(
                    "token_minted: token={} amount={} to={}",
                    e.token_address,
                    format_token_amount(e.amount),
                    e.to,
                );
            }
        }
        "order_created" => {
            if let Ok(e) = bincode::deserialize::<OrderCreatedEvent>(bytes) {
                let side = match e.side {
                    lightpool_sdk::OrderSide::Buy => "buy",
                    lightpool_sdk::OrderSide::Sell => "sell",
                };
                return format!(
                    "order_created: id={} side={} size={} market={} creator={}",
                    e.order_id,
                    side,
                    format_token_amount(e.amount),
                    e.market,
                    e.creator,
                );
            }
        }
        "order_cancelled" => {
            if let Ok(e) = bincode::deserialize::<OrderCancelledEvent>(bytes) {
                return format!(
                    "order_cancelled: id={} side={:?} amount={}",
                    e.order_id,
                    e.side,
                    format_token_amount(e.cancelled_amount),
                );
            }
        }
        "order_filled" => {
            if let Ok(e) = bincode::deserialize::<OrderFilledEvent>(bytes) {
                return format!(
                    "order_filled: id={} price={} fill={} remaining={} market={}",
                    e.order_id,
                    format_price_pieces(e.price),
                    format_token_amount(e.fill_amount),
                    format_token_amount(e.remaining_amount),
                    e.market,
                );
            }
        }
        _ => {}
    }

    if let EventType::Transfer = &event.event_type {
        if let Ok(e) = bincode::deserialize::<TransferEvent>(bytes) {
            return format!(
                "transfer: token={} from={} to={} amount={}",
                e.token,
                e.from,
                e.to,
                format_token_amount(e.amount),
            );
        }
    }

    format!("{action}: (undecoded)")
}

pub async fn apply_order_created_to_book(
    book_store: &SharedBookStore,
    block_num: u64,
    created: &OrderCreatedEvent,
) {
    let OrderEventType::Limit { price, .. } = &created.order_type else {
        return;
    };

    let spot_market =
        crate::spot_market::normalize_spot_market_key(&created.market.to_string());

    book_store
        .apply_created(
            &spot_market,
            created.side,
            *price,
            created.amount,
            block_num,
        )
        .await;
}

async fn index_market_created(store: &SharedIndexStore, created: EventContractCreatedEvent) {
    let market_address = created.market_address.to_string();
    let question = store
        .question_for_hash(&created.question_hash)
        .await
        .unwrap_or_else(|| question_from_hash(&created.question_hash));
    let slug = store
        .slug_for_hash(&created.question_hash)
        .await
        .unwrap_or_else(|| crate::slug::slug_from_question(&question));
    let icon_url = store.icon_url_for_hash(&created.question_hash).await;

    let market = Market {
        id: market_uuid(&market_address),
        slug,
        question,
        icon_url,
        market_address,
        collateral_token: created.collateral_token.to_string(),
        yes_token: created.yes_token.to_string(),
        no_token: created.no_token.to_string(),
        yes_spot_market: created.yes_spot_market.to_string(),
        no_spot_market: created.no_spot_market.to_string(),
        state: created.state.to_string(),
        resolution_deadline: created.resolution_deadline,
    };

    tracing::info!(
        market_id = %market.id,
        slug = %market.slug,
        question = %market.question,
        market_address = %market.market_address,
        "indexed event contract market"
    );

    store.upsert_market(market).await;
}

pub async fn index_order_created(store: &SharedIndexStore, created: OrderCreatedEvent) -> Option<Order> {
    let spot_market = created.market.to_string();
    let Some((market_id, outcome)) = store.lookup_spot_market(&spot_market).await else {
        tracing::debug!(spot_market, "order_created for unknown spot market");
        return None;
    };

    let price_raw = match &created.order_type {
        OrderEventType::Limit { price, .. } => *price,
        OrderEventType::Market { .. } => 0,
        OrderEventType::Trigger { limit_price, .. } => *limit_price,
    };

    let side = match created.side {
        lightpool_sdk::OrderSide::Buy => "buy",
        lightpool_sdk::OrderSide::Sell => "sell",
    };

    let chain_order_id = created.order_id.to_string();
    let question = store
        .get_market(market_id)
        .await
        .map(|market| market.question)
        .unwrap_or_default();
    let market_slug = store
        .get_market(market_id)
        .await
        .map(|market| market.slug)
        .unwrap_or_default();
    let order = Order {
        id: Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("{market_id}:{chain_order_id}").as_bytes(),
        ),
        market_id,
        market_slug,
        question,
        outcome,
        side: side.into(),
        price: format_price_pieces(price_raw),
        size: format_token_amount(created.amount),
        status: "open".into(),
    };

    tracing::info!(
        order_id = chain_order_id,
        market_id = %market_id,
        user = %created.creator,
        "indexed order"
    );

    store
        .insert_order(
            order.clone(),
            created.creator.to_string(),
            chain_order_id,
            created.amount,
        )
        .await;

    Some(order)
}

pub async fn publish_user_order_created(
    user_hub: &SharedUserEventHub,
    store: &SharedIndexStore,
    chain_order_id: &str,
    block_num: u64,
) {
    let Some((order, user_address, _)) = store.stored_order_by_chain_id(chain_order_id).await else {
        return;
    };
    user_hub
        .publish_order(
            "placement",
            &user_address,
            chain_order_id,
            order,
            block_num,
        )
        .await;
}

async fn publish_user_order_cancelled(
    user_hub: &SharedUserEventHub,
    store: &SharedIndexStore,
    chain_order_id: &str,
    block_num: u64,
) {
    let Some((order, user_address, _)) = store.stored_order_by_chain_id(chain_order_id).await else {
        return;
    };
    user_hub
        .publish_order(
            "cancellation",
            &user_address,
            chain_order_id,
            order,
            block_num,
        )
        .await;
}

async fn publish_user_order_filled(
    user_hub: &SharedUserEventHub,
    store: &SharedIndexStore,
    chain_order_id: &str,
    spot_market: &str,
    price_raw: u64,
    fill_amount_raw: u64,
    remaining_amount_raw: u64,
    is_fully_filled: bool,
    side: lightpool_sdk::OrderSide,
    block_num: u64,
) {
    let Some((order, user_address, _)) = store.stored_order_by_chain_id(chain_order_id).await else {
        return;
    };

    let side_str = match side {
        lightpool_sdk::OrderSide::Buy => "buy",
        lightpool_sdk::OrderSide::Sell => "sell",
    };

    user_hub
        .publish_trade(
            &user_address,
            chain_order_id,
            order.id,
            &order.market_slug,
            &order.outcome,
            side_str,
            &format_price_pieces(price_raw),
            &format_token_amount(fill_amount_raw),
            &format_token_amount(remaining_amount_raw),
            is_fully_filled,
            spot_market,
            block_num,
        )
        .await;

    let event = "update";
    user_hub
        .publish_order(event, &user_address, chain_order_id, order, block_num)
        .await;
}
