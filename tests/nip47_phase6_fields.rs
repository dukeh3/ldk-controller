/// Integration tests for Phase 6 NIP-47 fields:
/// - GetBalanceResponse: lightning_balance, onchain_balance
/// - LookupInvoiceResponse: payment_method
/// - ListTransactionsRequest: payment_method filter
///
/// Uses two in-process LDK nodes on regtest with containerized bitcoind.
use std::net::TcpListener;
use std::str::FromStr;
use std::time::Duration;

use ldk_controller::lightning::{LdkService, LdkServiceConfig};
use ldk_node::bitcoin::secp256k1::PublicKey;
use ldk_node::bitcoin::Network;
use ldk_node::lightning::ln::msgs::SocketAddress;
use ldk_node::lightning_invoice::{Bolt11InvoiceDescription, Description};
use ldk_node::payment::{PaymentDirection, PaymentStatus};
use ldk_node::Builder;
use nwc::nostr::nips::nip47::PaymentMethod;

#[path = "common/mod.rs"]
mod common;

fn unique_storage_dir(prefix: &str) -> String {
    format!(
        "/tmp/{prefix}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    )
}

fn free_local_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    port
}

/// Shared two-node test harness: Alice (LdkService) + Bob (raw ldk_node).
/// Returns (alice, bob_node, bitcoind) with an open, ready channel and
/// both a bolt11 and keysend payment already completed (A→B each).
struct TwoNodeHarness {
    alice: std::sync::Arc<LdkService>,
    bob: ldk_node::Node,
    _bitcoind: common::bitcoind::BitcoindHarness,
}

impl TwoNodeHarness {
    async fn setup() -> Self {
        let bitcoind = common::bitcoind::BitcoindHarness::start().await;
        let miner_address = bitcoind.get_new_address().await;
        bitcoind.mine_blocks(101, &miner_address).await;

        let port_a = free_local_port();
        let port_b = free_local_port();
        let addr_b = SocketAddress::from_str(&format!("127.0.0.1:{port_b}"))
            .expect("valid socket address");

        let alice_cfg = LdkServiceConfig {
            network: "regtest".to_string(),
            bitcoind_rpc_host: bitcoind.rpc_host().to_string(),
            bitcoind_rpc_port: bitcoind.rpc_port(),
            bitcoind_rpc_user: bitcoind.rpc_user().to_string(),
            bitcoind_rpc_password: bitcoind.rpc_password().to_string(),
            ldk_storage_dir: unique_storage_dir("phase6-alice"),
            ldk_listen_addr: Some(format!("127.0.0.1:{port_a}")),
        };
        let alice = LdkService::start_from_config(&alice_cfg).expect("start alice");

        let mut bob_builder = Builder::new();
        bob_builder.set_network(Network::Regtest);
        bob_builder.set_chain_source_bitcoind_rpc(
            bitcoind.rpc_host().to_string(),
            bitcoind.rpc_port(),
            bitcoind.rpc_user().to_string(),
            bitcoind.rpc_password().to_string(),
        );
        bob_builder
            .set_listening_addresses(vec![addr_b.clone()])
            .expect("set bob listen addr");
        bob_builder.set_storage_dir_path(unique_storage_dir("phase6-bob"));
        let bob = bob_builder.build().expect("build bob");
        bob.start().expect("start bob");

        // Fund Alice.
        let alice_addr = alice.new_onchain_address().expect("alice address");
        bitcoind.send_to_address(&alice_addr, 0.05).await;
        bitcoind.mine_blocks(1, &miner_address).await;

        wait_for_sync(&alice, 5_000_000_000, Duration::from_secs(20)).await;

        // Open channel with push so Bob has inbound liquidity to pay Alice back.
        let bob_pubkey = bob.node_id().to_string();
        alice
            .open_channel(&bob_pubkey, &format!("127.0.0.1:{port_b}"), 2_000_000, Some(500_000_000))
            .expect("open channel");

        bitcoind.mine_blocks(6, &miner_address).await;
        wait_for_channel_ready(&alice, &bob, Duration::from_secs(45), &bitcoind, &miner_address)
            .await;

        // --- Bolt11 payment: Alice -> Bob ---
        let bob_desc = Bolt11InvoiceDescription::Direct(
            Description::new("bolt11-a-to-b".to_string()).expect("desc"),
        );
        let bob_invoice = bob
            .bolt11_payment()
            .receive(100_000, &bob_desc, 3600)
            .expect("bob invoice");
        alice
            .pay_invoice(&bob_invoice.to_string(), None)
            .expect("alice pays bob bolt11");

        // --- Keysend payment: Alice -> Bob ---
        let bob_pk = PublicKey::from_str(&bob_pubkey).expect("parse bob pubkey");
        alice
            .pay_keysend(&bob_pk.to_string(), 50_000)
            .expect("alice keysends bob");

        Self {
            alice,
            bob,
            _bitcoind: bitcoind,
        }
    }

    fn stop(self) {
        self.bob.stop().expect("stop bob");
        self.alice.stop().expect("stop alice");
    }
}

async fn wait_for_sync(alice: &LdkService, min_balance_msat: u64, timeout: Duration) {
    let start = tokio::time::Instant::now();
    loop {
        alice.sync_wallets().expect("alice sync");
        if alice.get_balance_msat().expect("alice balance") >= min_balance_msat {
            return;
        }
        assert!(start.elapsed() <= timeout, "funding sync timeout");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn wait_for_channel_ready(
    alice: &LdkService,
    bob: &ldk_node::Node,
    timeout: Duration,
    bitcoind: &common::bitcoind::BitcoindHarness,
    miner_address: &str,
) {
    let start = tokio::time::Instant::now();
    loop {
        alice.sync_wallets().expect("alice sync");
        bob.sync_wallets().expect("bob sync");

        let alice_ready = alice.has_ready_channel_with(&bob.node_id().to_string());
        let bob_ready = bob
            .list_channels()
            .iter()
            .any(|c| c.counterparty_node_id.to_string() == alice.node_id() && c.is_channel_ready);

        if alice_ready && bob_ready {
            return;
        }

        assert!(
            start.elapsed() <= timeout,
            "channel did not become ready in time"
        );
        bitcoind.mine_blocks(1, miner_address).await;
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

// ---------------------------------------------------------------------------
// Test 1: get_balance split fields
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_balance_returns_split_lightning_and_onchain() {
    let _guard = common::test_guard();
    let harness = TwoNodeHarness::setup().await;

    harness.alice.sync_wallets().expect("alice sync");
    let bal = harness.alice.get_balance().expect("get_balance");

    // After opening a channel and funding on-chain, both should be populated.
    assert!(
        bal.lightning_msat > 0,
        "lightning_msat should be > 0 after channel open, got {}",
        bal.lightning_msat
    );
    // Alice funded 0.05 BTC but only used 0.02 BTC for channel, so change remains.
    assert!(
        bal.onchain_msat > 0,
        "onchain_msat should be > 0 (change from channel funding), got {}",
        bal.onchain_msat
    );
    assert_eq!(
        bal.total_msat,
        bal.lightning_msat + bal.onchain_msat,
        "total should equal lightning + onchain"
    );

    println!(
        "Balance: total={}msat, lightning={}msat, onchain={}msat",
        bal.total_msat, bal.lightning_msat, bal.onchain_msat
    );

    harness.stop();
}

// ---------------------------------------------------------------------------
// Test 2: get_balance shifts after payment
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_balance_shifts_after_payment() {
    let _guard = common::test_guard();
    let harness = TwoNodeHarness::setup().await;

    harness.alice.sync_wallets().expect("sync");
    let bal_after = harness.alice.get_balance().expect("balance after payments");

    // Alice opened 2M sats channel with 500k push, then paid 100k + 50k bolt11/keysend.
    // Lightning balance should be less than 1.5M sats (1_500_000_000 msat) minus payments.
    // We just verify it's positive and less than the full channel amount.
    let channel_alice_side_msat = 1_500_000_000u64; // 2M - 500k push = 1.5M sats
    assert!(
        bal_after.lightning_msat < channel_alice_side_msat,
        "lightning balance should be reduced by payments: {}",
        bal_after.lightning_msat
    );
    assert!(
        bal_after.lightning_msat > 0,
        "lightning balance should still be positive"
    );

    println!(
        "After payments: lightning={}msat (started ~{channel_alice_side_msat}msat)",
        bal_after.lightning_msat
    );

    harness.stop();
}

// ---------------------------------------------------------------------------
// Test 3: lookup_invoice bolt11 has payment_method
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lookup_invoice_bolt11_has_payment_method() {
    let _guard = common::test_guard();
    let harness = TwoNodeHarness::setup().await;

    // Find a bolt11 outbound payment from Alice.
    let bolt11_payment = harness
        .alice
        .node()
        .list_payments()
        .into_iter()
        .find(|p| {
            p.direction == PaymentDirection::Outbound
                && matches!(p.kind, ldk_node::payment::PaymentKind::Bolt11 { .. })
                && p.status == PaymentStatus::Succeeded
        })
        .expect("should have a bolt11 outbound payment");

    // Extract payment hash and look it up.
    let hash = match &bolt11_payment.kind {
        ldk_node::payment::PaymentKind::Bolt11 { hash, .. } => {
            hash.0.iter().map(|b| format!("{b:02x}")).collect::<String>()
        }
        _ => unreachable!(),
    };

    let details = harness
        .alice
        .lookup_payment_by_hash(&hash)
        .expect("lookup bolt11 payment");

    // Build the LookupInvoiceResponse and check payment_method.
    let response = ldk_controller::payment_details_to_lookup_response(&details);
    assert_eq!(
        response.payment_method,
        Some(PaymentMethod::Bolt11),
        "bolt11 payment should have payment_method=Bolt11"
    );

    harness.stop();
}

// ---------------------------------------------------------------------------
// Test 4: lookup_invoice keysend has payment_method
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lookup_invoice_keysend_has_payment_method() {
    let _guard = common::test_guard();
    let harness = TwoNodeHarness::setup().await;

    // Find a spontaneous (keysend) outbound payment from Alice.
    let keysend_payment = harness
        .alice
        .node()
        .list_payments()
        .into_iter()
        .find(|p| {
            p.direction == PaymentDirection::Outbound
                && matches!(p.kind, ldk_node::payment::PaymentKind::Spontaneous { .. })
                && p.status == PaymentStatus::Succeeded
        })
        .expect("should have a keysend outbound payment");

    let hash = match &keysend_payment.kind {
        ldk_node::payment::PaymentKind::Spontaneous { hash, .. } => {
            hash.0.iter().map(|b| format!("{b:02x}")).collect::<String>()
        }
        _ => unreachable!(),
    };

    let details = harness
        .alice
        .lookup_payment_by_hash(&hash)
        .expect("lookup keysend payment");

    let response = ldk_controller::payment_details_to_lookup_response(&details);
    assert_eq!(
        response.payment_method,
        Some(PaymentMethod::Keysend),
        "keysend payment should have payment_method=Keysend"
    );

    harness.stop();
}

// ---------------------------------------------------------------------------
// Test 5: list_transactions returns payment_method on every entry
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_transactions_all_entries_have_payment_method() {
    let _guard = common::test_guard();
    let harness = TwoNodeHarness::setup().await;

    let payments = harness.alice.list_payments_filtered(
        None, None, None, None, None, None, None,
    );
    assert!(
        payments.len() >= 2,
        "should have at least 2 payments (bolt11 + keysend), got {}",
        payments.len()
    );

    for payment in &payments {
        let response = ldk_controller::payment_details_to_lookup_response(payment);
        assert!(
            response.payment_method.is_some(),
            "every payment should have payment_method set, missing for {:?}",
            payment.kind
        );
    }

    harness.stop();
}

// ---------------------------------------------------------------------------
// Test 6: list_transactions filter by Bolt11 excludes keysend
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_transactions_filter_bolt11_excludes_keysend() {
    let _guard = common::test_guard();
    let harness = TwoNodeHarness::setup().await;

    let bolt11_only = harness.alice.list_payments_filtered(
        None, None, None, None, None, None, Some(PaymentMethod::Bolt11),
    );
    assert!(
        !bolt11_only.is_empty(),
        "should have at least one bolt11 payment"
    );

    for payment in &bolt11_only {
        assert!(
            matches!(
                payment.kind,
                ldk_node::payment::PaymentKind::Bolt11 { .. }
                    | ldk_node::payment::PaymentKind::Bolt11Jit { .. }
            ),
            "filtered list should only contain bolt11 payments, got {:?}",
            payment.kind
        );
    }

    // Verify keysend is excluded.
    let has_keysend = bolt11_only.iter().any(|p| {
        matches!(p.kind, ldk_node::payment::PaymentKind::Spontaneous { .. })
    });
    assert!(!has_keysend, "bolt11 filter should exclude keysend payments");

    harness.stop();
}

// ---------------------------------------------------------------------------
// Test 7: list_transactions filter by Bolt12 returns empty (no bolt12 payments)
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_transactions_filter_bolt12_returns_empty() {
    let _guard = common::test_guard();
    let harness = TwoNodeHarness::setup().await;

    let bolt12_only = harness.alice.list_payments_filtered(
        None, None, None, None, None, None, Some(PaymentMethod::Bolt12),
    );
    assert!(
        bolt12_only.is_empty(),
        "no bolt12 payments were made, should be empty, got {}",
        bolt12_only.len()
    );

    harness.stop();
}
