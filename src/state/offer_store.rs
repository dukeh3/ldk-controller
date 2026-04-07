use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

#[derive(Debug, Clone)]
pub struct OfferRecord {
    pub offer: String,
    pub description: String,
    pub amount_msat: u64,
    pub active: bool,
    pub num_payments_received: u64,
    pub total_received_msat: u64,
}

static OFFER_STORE: OnceLock<RwLock<HashMap<String, OfferRecord>>> = OnceLock::new();

fn store() -> &'static RwLock<HashMap<String, OfferRecord>> {
    OFFER_STORE.get_or_init(|| RwLock::new(HashMap::new()))
}

pub fn insert_offer(offer: String, description: String, amount_msat: u64) {
    let mut map = store().write().expect("offer store lock poisoned");
    map.insert(
        offer.clone(),
        OfferRecord {
            offer,
            description,
            amount_msat,
            active: true,
            num_payments_received: 0,
            total_received_msat: 0,
        },
    );
}

pub fn get_offer(offer: &str) -> Option<OfferRecord> {
    let map = store().read().expect("offer store lock poisoned");
    map.get(offer).cloned()
}

#[allow(dead_code)]
pub fn record_payment(offer: &str, amount_msat: u64) {
    let mut map = store().write().expect("offer store lock poisoned");
    if let Some(record) = map.get_mut(offer) {
        record.num_payments_received += 1;
        record.total_received_msat += amount_msat;
    }
}

#[allow(dead_code)]
pub fn clear() {
    let mut map = store().write().expect("offer store lock poisoned");
    map.clear();
}
