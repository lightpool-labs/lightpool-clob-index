use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::models::{Market, Order};

#[derive(Debug, Clone, Default)]
pub struct IndexedBlockHead {
    pub block_num: u64,
    pub digest: String,
    pub tx_count: usize,
    pub connected: bool,
}

pub type SharedIndexedBlockHead = Arc<RwLock<IndexedBlockHead>>;

#[derive(Debug, Clone)]
struct SpotMarketRef {
    market_id: Uuid,
    outcome: String,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredOrder {
    pub order: Order,
    pub user_address: String,
    pub chain_order_id: String,
    pub filled_raw: u64,
    pub size_raw: u64,
}

#[derive(Debug, Clone)]
struct QuestionEntry {
    question: String,
    slug: String,
    icon_url: Option<String>,
}

#[derive(Default)]
struct IndexStoreInner {
    markets: HashMap<Uuid, Market>,
    slug_to_market_id: HashMap<String, Uuid>,
    spot_to_market: HashMap<String, SpotMarketRef>,
    last_trade_price_by_spot: HashMap<String, u64>,
    orders: HashMap<Uuid, StoredOrder>,
    chain_order_index: HashMap<String, Uuid>,
    question_by_hash: HashMap<String, QuestionEntry>,
}

pub struct IndexStore {
    inner: RwLock<IndexStoreInner>,
}

pub type SharedIndexStore = Arc<IndexStore>;

impl IndexStore {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(IndexStoreInner::default()),
        }
    }

    pub async fn list_markets(&self) -> Vec<Market> {
        let inner = self.inner.read().await;
        let mut markets: Vec<Market> = inner.markets.values().cloned().collect();
        markets.sort_by_key(|m| m.resolution_deadline);
        markets
    }

    pub async fn get_market(&self, id: Uuid) -> Option<Market> {
        self.inner.read().await.markets.get(&id).cloned()
    }

    pub async fn get_event_by_slug(&self, slug: &str) -> Option<Market> {
        let inner = self.inner.read().await;
        inner
            .slug_to_market_id
            .get(slug)
            .and_then(|id| inner.markets.get(id).cloned())
    }

    pub async fn allocate_slug(&self, question: &str) -> String {
        let inner = self.inner.read().await;
        let existing_slugs: Vec<String> = inner.slug_to_market_id.keys().cloned().collect();
        crate::slug::allocate_unique_slug(&existing_slugs, question)
    }

    pub async fn list_orders_for_user(&self, user_address: &str) -> Vec<Order> {
        let inner = self.inner.read().await;
        inner
            .orders
            .values()
            .filter(|stored| stored.user_address.eq_ignore_ascii_case(user_address))
            .map(|stored| stored.order.clone())
            .collect()
    }

    pub async fn register_question(&self, question: &str, slug: &str, icon_url: Option<String>) {
        let hash = crate::chain::compute_question_hash(question);
        let key = hex::encode(hash);
        self.inner.write().await.question_by_hash.insert(
            key,
            QuestionEntry {
                question: question.to_string(),
                slug: slug.to_string(),
                icon_url,
            },
        );
    }

    pub async fn question_for_hash(&self, hash: &[u8; 32]) -> Option<String> {
        let key = hex::encode(hash);
        self.inner
            .read()
            .await
            .question_by_hash
            .get(&key)
            .map(|entry| entry.question.clone())
    }

    pub async fn slug_for_hash(&self, hash: &[u8; 32]) -> Option<String> {
        let key = hex::encode(hash);
        self.inner
            .read()
            .await
            .question_by_hash
            .get(&key)
            .map(|entry| entry.slug.clone())
            .filter(|slug| !slug.is_empty())
    }

    pub async fn icon_url_for_hash(&self, hash: &[u8; 32]) -> Option<String> {
        let key = hex::encode(hash);
        self.inner
            .read()
            .await
            .question_by_hash
            .get(&key)
            .and_then(|entry| entry.icon_url.clone())
    }

    fn remove_slug_mappings_for_market(inner: &mut IndexStoreInner, market_id: Uuid) {
        inner.slug_to_market_id.retain(|_, id| *id != market_id);
    }

    pub async fn position_token_specs(&self) -> Vec<(String, String)> {
        let inner = self.inner.read().await;
        let mut specs = Vec::new();

        for market in inner.markets.values() {
            specs.push(("YES".into(), market.yes_token.clone()));
            specs.push(("NO".into(), market.no_token.clone()));
        }

        specs
    }

    pub async fn upsert_market(&self, mut market: Market) {
        let mut inner = self.inner.write().await;
        if let Some(existing) = inner.markets.get(&market.id) {
            if market.icon_url.is_none() {
                market.icon_url = existing.icon_url.clone();
            }
            if market.slug.is_empty() {
                market.slug = existing.slug.clone();
            }
        }

        if !market.slug.is_empty() {
            Self::remove_slug_mappings_for_market(&mut inner, market.id);
            inner
                .slug_to_market_id
                .insert(market.slug.clone(), market.id);
        }

        inner
            .spot_to_market
            .insert(market.yes_spot_market.clone(), SpotMarketRef {
                market_id: market.id,
                outcome: "yes".into(),
            });
        inner
            .spot_to_market
            .insert(market.no_spot_market.clone(), SpotMarketRef {
                market_id: market.id,
                outcome: "no".into(),
            });
        inner.markets.insert(market.id, market);
    }

    pub async fn update_market_state(&self, market_address: &str, state: &str) {
        let mut inner = self.inner.write().await;
        for market in inner.markets.values_mut() {
            if market.market_address == market_address {
                market.state = state.to_string();
            }
        }
    }

    pub async fn lookup_spot_market(&self, spot_market: &str) -> Option<(Uuid, String)> {
        let inner = self.inner.read().await;
        if let Some(spot) = inner.spot_to_market.get(spot_market) {
            return Some((spot.market_id, spot.outcome.clone()));
        }
        if let Some(key) = contract_key_from_market_ref(spot_market) {
            if let Some(spot) = inner.spot_to_market.get(&key) {
                return Some((spot.market_id, spot.outcome.clone()));
            }
        }
        None
    }

    pub async fn record_last_trade_price(&self, spot_market: &str, price: u64) {
        let mut inner = self.inner.write().await;
        inner
            .last_trade_price_by_spot
            .insert(spot_market.to_string(), price);
        if let Some(key) = contract_key_from_market_ref(spot_market) {
            inner.last_trade_price_by_spot.insert(key, price);
        }
    }

    pub async fn last_trade_price(&self, spot_market: &str) -> Option<u64> {
        let inner = self.inner.read().await;
        if let Some(price) = inner.last_trade_price_by_spot.get(spot_market) {
            return Some(*price);
        }
        if let Some(key) = contract_key_from_market_ref(spot_market) {
            return inner.last_trade_price_by_spot.get(&key).copied();
        }
        None
    }

    pub async fn order_cancel_context(
        &self,
        order_id: Uuid,
        user_address: &str,
    ) -> Option<(Order, String, String)> {
        let inner = self.inner.read().await;
        let stored = inner.orders.get(&order_id)?;
        if !stored.user_address.eq_ignore_ascii_case(user_address) {
            return None;
        }
        if stored.order.status != "open" {
            return None;
        }
        let market = inner.markets.get(&stored.order.market_id)?;
        let spot_market = if stored.order.outcome == "yes" {
            market.yes_spot_market.clone()
        } else {
            market.no_spot_market.clone()
        };
        Some((
            stored.order.clone(),
            stored.chain_order_id.clone(),
            spot_market,
        ))
    }

    pub async fn insert_order(
        &self,
        order: Order,
        user_address: String,
        chain_order_id: String,
        size_raw: u64,
    ) {
        let stored = StoredOrder {
            order: order.clone(),
            user_address,
            chain_order_id: chain_order_id.clone(),
            filled_raw: 0,
            size_raw,
        };
        let mut inner = self.inner.write().await;
        inner.chain_order_index.insert(chain_order_id, order.id);
        inner.orders.insert(order.id, stored);
    }

    pub async fn update_order_cancelled(&self, chain_order_id: &str) {
        let mut inner = self.inner.write().await;
        let Some(order_id) = inner.chain_order_index.get(chain_order_id).copied() else {
            return;
        };
        if let Some(stored) = inner.orders.get_mut(&order_id) {
            stored.order.status = "cancelled".into();
        }
    }

    pub async fn update_order_fill(
        &self,
        chain_order_id: &str,
        fill_amount: u64,
        remaining_amount: u64,
        is_fully_filled: bool,
    ) {
        let mut inner = self.inner.write().await;
        let Some(order_id) = inner.chain_order_index.get(chain_order_id).copied() else {
            return;
        };
        let Some(stored) = inner.orders.get_mut(&order_id) else {
            return;
        };

        stored.filled_raw = stored.filled_raw.saturating_add(fill_amount);
        stored.order.status = if is_fully_filled || remaining_amount == 0 {
            "filled".into()
        } else {
            "open".into()
        };
    }
}

pub fn new_head() -> SharedIndexedBlockHead {
    Arc::new(RwLock::new(IndexedBlockHead::default()))
}

pub fn market_uuid(market_address: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_OID, market_address.as_bytes())
}

pub fn question_from_hash(hash: &[u8; 32]) -> String {
    let end = hash.iter().position(|&b| b == 0).unwrap_or(32);
    String::from_utf8_lossy(&hash[..end]).trim().to_string()
}

fn contract_key_from_market_ref(value: &str) -> Option<String> {
    let hex_body = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    if hex_body.len() < 16 {
        return None;
    }
    Some(format!("0x{}", &hex_body[..16]))
}
