# LDK-Controller: NIP-47 + NIP-XX Compliance Plan

This plan maps the current ldk-controller implementation against the proposed
NIP-47 (NWC) and NIP-XX (NNC) specifications and defines the work required to
reach full compliance.

## Current State

- **Source**: `/home/rene/git/ldk-controller/`
- **LDK Node**: v0.7
- **Nostr SDK**: custom fork `dukeh3/nostr`
- **Encryption**: NIP-04 only
- **NWC event kinds**: 13194 (info), 23194 (request), 23195 (response)
- **NNC event kinds**: 23196 (request), 23197 (response) — non-standard
- **Notifications**: Not implemented
- **Tests**: 82 test functions across 67 files

---

## Phase 1 — Align Existing Implementation

Goal: Make what already works conform to the specs. No new features.

### 1.1 Fix NNC event kinds

Change control channel from non-standard 23196/23197 to spec kinds:

| Current | NIP-XX Spec | Use |
|---------|-------------|-----|
| 23196   | **23198**   | NNC request |
| 23197   | **23199**   | NNC response |
| —       | **23200**   | NNC notification (new) |

Files: `lib.rs` (constants `CONTROL_REQUEST_KIND`, `CONTROL_RESPONSE_KIND`),
all test files referencing these kinds.

**Tests affected:**
- `control_kind_roundtrip.rs` (5 tests) — uses `CONTROL_REQUEST_KIND` / `CONTROL_RESPONSE_KIND`
- `e2e_blackbox_container.rs` → `e2e_control_list_channels_roundtrip` — sends kind 23196
- `control_open_channel_roundtrip.rs` — uses control kinds
- `control_list_channels_with_open_channel.rs` — uses control kinds
- `control_connect_disconnect_peer_roundtrip.rs` — uses control kinds
- `control_channel_payments_scenario.rs` — uses control kinds

All above must update kind constants from 23196/23197 to 23198/23199.

### 1.2 Publish NNC info event (kind 13198)

On startup, publish a replaceable kind 13198 event listing supported NNC
methods, alongside the existing kind 13194 NWC info event.

Files: `lib.rs` (`run_nwc_service`)

**Tests affected:** None directly (no tests assert on NNC info event).

**New tests needed:**
- `nnc_info_event_published.rs` — verify kind 13198 is published on startup
  with correct NNC method list

### 1.3 Align `list_channels` response

Current `LdkChannelInfo` has 3 fields. NIP-XX requires:

```
id, short_channel_id, peer_pubkey, state, is_private,
local_balance, remote_balance, capacity, funding_txid,
funding_output_index
```

LDK Node's `list_channels()` returns `ChannelDetails` which has all of these.
Expand `LdkChannelInfo` and the serialization.

Files: `lightning/ldk_service.rs` (`LdkChannelInfo`, `list_channels`)

**Tests affected:**
- `control_kind_roundtrip.rs` → `control_allowed_when_method_listed_returns_channels_array`
  — asserts on result shape, needs updated field names
- `control_list_channels_with_open_channel.rs` — asserts `counterparty_pubkey`,
  needs field rename to `peer_pubkey` and new field assertions
- `control_open_channel_roundtrip.rs` — asserts on list_channels result after open
- `control_channel_payments_scenario.rs` — uses list_channels result
- `e2e_blackbox_container.rs` → `e2e_control_list_channels_roundtrip` — asserts array

### 1.4 Align `list_peers` response

Current `LdkPeerInfo` returns `{node_id, address, is_persisted, is_connected}`.
NIP-XX requires `{pubkey, address, connected, alias, num_channels}`.

Add `num_channels` (count from `list_channels`) and rename fields.

Files: `lightning/ldk_service.rs` (`LdkPeerInfo`, `list_peers`)

**Tests affected:**
- `control_kind_roundtrip.rs` → `control_allowed_list_peers_returns_array`
  — asserts on result shape
- `control_connect_disconnect_peer_roundtrip.rs` — asserts `node_id` field,
  needs rename to `pubkey`

### 1.5 Align `open_channel` params and response + update NIP-XX spec

Current params: `{pubkey, host, port, capacity_sats, push_msat}`
NIP-XX params: `{pubkey, amount, push_amount, private, host, close_address, notify}`

Response: return empty `result: {}` on acceptance. The `funding_txid` is not
available synchronously from `ldk_node.open_channel()` (it returns a
`UserChannelId`, the txid arrives later via `Event::ChannelPending`). The
funding txid is delivered via the `channel_opened` notification (kind 23200).

**NIP-XX spec updated**: removed `funding_txid` from `open_channel` response,
added note that it is delivered asynchronously via notification.

Files: `lib.rs` (`OpenChannelParams`, `process_control_request`),
`lightning/ldk_service.rs` (`open_channel`),
`/home/rene/git/nips/XX.md` (done)

**Tests affected:**
- `control_open_channel_roundtrip.rs` — sends `{pubkey, host, port, capacity_sats}`,
  needs updated to `{pubkey, amount, host, ...}`, response assertion changes
- `control_channel_payments_scenario.rs` — same param changes

### 1.6 Align `close_channel` params and response + update NIP-XX spec

Current params: `{channel_id, force}`
NIP-XX params: `{id, force, close_address, notify}`

Response: return empty `result: {}` on acceptance. The `closing_txid` is
delivered via the `channel_closed` notification (kind 23200) once confirmed.

**NIP-XX spec updated**: removed `closing_txid` from `close_channel` response,
added note that it is delivered asynchronously via notification.

Files: `lib.rs` (`CloseChannelParams`, `process_control_request`),
`lightning/ldk_service.rs` (`close_channel`),
`/home/rene/git/nips/XX.md` (done)

**Tests affected:**
- `control_open_channel_roundtrip.rs` — sends `{channel_id, force}`,
  needs updated to `{id, force}`

### 1.7 Align `connect_peer` params

Current: `{pubkey, host, port}` (separate host and port)
NIP-XX: `{pubkey, host}` where host is `"ip:port"`

Files: `lib.rs` (`ConnectPeerParams`)

**Tests affected:**
- `control_connect_disconnect_peer_roundtrip.rs` — sends separate `host`
  and `port`, needs combined `host: "ip:port"`
- `control_open_channel_roundtrip.rs` — if it uses connect_peer

### 1.8 Fix `get_balance` response

Current: returns only on-chain balance as `balance`.
NIP-47: `{ balance, lightning_balance, onchain_balance }`

Add channel balance calculation from `list_channels()` balances.

Files: `lib.rs` (`GetBalanceHandler`), `lightning/ldk_service.rs`

**Tests affected:**
- `nwc_get_balance_roundtrip.rs` — asserts `balance` field
- `nwc_ldk_integration/get_balance_after_onchain_funding.rs` — asserts balance
  equals funding amount, needs update for new response fields
- `e2e_blackbox_container.rs` → `e2e_nwc_get_info_get_balance_roundtrip`

**New tests needed:**
- Test that `lightning_balance` reflects channel balances
- Test that `balance` = `lightning_balance` + `onchain_balance`

### 1.9 Add `"ALL"` special key in access control

NIP-XX spec: `"ALL"` in `methods` or `control` map grants access to all
methods of that type. Currently not checked.

Files: `lib.rs` (`verify_access`, `authorize_control_method`)

**Tests affected:** None (no existing tests use `"ALL"`).

**New tests needed:**
- `nwc_all_methods_grant.rs` — grant with `"ALL"` key allows any NWC method
- `control_all_methods_grant.rs` — grant with `"ALL"` key allows any NNC method
- `nwc_all_with_rate_limit.rs` — `"ALL"` with rate limit applies to every method

### 1.10 Move `new_onchain_address` from NNC to NWC

NIP-47 has `make_new_address`. Remove from `SUPPORTED_CONTROL_METHODS`,
add as NWC method.

Files: `lib.rs` (add `MakeNewAddressHandler`, update `SUPPORTED_METHODS`)

**Tests affected:** None currently test `new_onchain_address` via the control
channel directly.

**New tests needed:**
- `nwc_make_new_address_roundtrip.rs` — test as NWC method

### 1.11 Remove `get_channel` control method

Not in NIP-XX spec. Consumers should use `list_channels` and filter
client-side. Remove or keep as non-standard extension.

**Tests affected:**
- `control_open_channel_roundtrip.rs` — uses `get_channel` after open.
  Replace with `list_channels` + filter.

### 1.12 Update `get_info` response

- Populate `notifications` field with supported notification types
- Add new methods to `SUPPORTED_METHODS` list as they are implemented
- Report `block_height` from LDK Node (currently hardcoded to 0)

Files: `lib.rs` (`GetInfoHandler`)

**Tests affected:**
- `nwc_get_info_roundtrip.rs` — asserts methods list, will need new entries
- `nwc_get_info_allowed.rs` — asserts on response content
- `nwc_ldk_integration/get_info_returns_ldk_identity.rs` — asserts network + pubkey
- `access_grant_get_info.rs` — asserts methods list

**New tests needed:**
- Assert `notifications` field is populated
- Assert `block_height` > 0 when LDK service is attached

---

## Phase 2 — New NWC Methods

Goal: Implement the NIP-47 methods that don't exist yet.

### 2.1 `pay_onchain`

Send on-chain payment. LDK Node has `onchain_payment().send_to_address()`.

```
Request:  { address, amount (sats), feerate (sat/vB) }
Response: { txid }
```

Need: new Method variant in nwc crate fork, handler in lib.rs, LdkService
method.

**New tests needed:**
- `nwc_pay_onchain_roundtrip.rs` — mock roundtrip
- `nwc_ldk_integration/pay_onchain_happy_path.rs` — real LDK: fund wallet,
  send on-chain, verify txid
- `nwc_pay_onchain_insufficient_balance.rs` — error case

### 2.2 `make_new_address` (from Phase 1.10)

Already implemented as control method. Wrap as NWC handler with NIP-47
response format:

```
Response: { address, type }
```

### 2.3 `pay_offer` (BOLT-12)

LDK Node 0.7 has `bolt12_payment().send()`.

```
Request:  { offer, amount (msats), payer_note }
Response: { preimage, fees_paid }
```

Need: new Method variant + ResponseResult in nwc crate fork.

**New tests needed:**
- `nwc_pay_offer_roundtrip.rs` — mock roundtrip
- `nwc_ldk_integration/pay_offer_happy_path.rs` — real LDK with BOLT-12

### 2.4 `make_offer` (BOLT-12)

LDK Node 0.7 has `bolt12_payment().receive()`.

```
Request:  { amount (msats), description }
Response: { offer, description, amount }
```

Need: new Method variant + ResponseResult in nwc crate fork.

**New tests needed:**
- `nwc_make_offer_roundtrip.rs` — mock roundtrip
- `nwc_ldk_integration/make_offer_happy_path.rs` — real LDK

### 2.5 `lookup_offer`

NIP-47 requires tracking per-offer payment stats. LDK Node doesn't do this
natively.

Options:
- Maintain a local offers table (SQLite or in-memory map)
- Track `offer -> [payments]` when payments arrive

```
Response: { offer, description, amount, active, num_payments_received, total_received }
```

**New tests needed:**
- `nwc_lookup_offer_roundtrip.rs`
- `nwc_lookup_offer_not_found.rs`

### 2.6 `lookup_address`

NIP-47 requires per-address transaction history. LDK Node doesn't track this.

Options:
- Query bitcoind RPC (`listreceivedbyaddress`, `gettransaction`)
- Maintain local address table

```
Response: { address, type, total_received, transactions: [...] }
```

**New tests needed:**
- `nwc_lookup_address_roundtrip.rs`
- `nwc_lookup_address_not_found.rs`

### 2.7 Implement real `lookup_invoice`

Currently a stub. Use LDK Node's payment list to find matching payment by
hash or bolt11 string.

Files: `lib.rs` (`LookupInvoiceHandler`), `lightning/ldk_service.rs`

**Tests affected:**
- `nwc_lookup_invoice_roundtrip.rs` — currently tests against stub data,
  needs rewrite for real implementation

**New tests needed:**
- `nwc_ldk_integration/lookup_invoice_after_payment.rs` — make invoice,
  pay it, lookup by payment_hash, verify settled state
- `nwc_lookup_invoice_not_found.rs` — NOT_FOUND error case

### 2.8 Implement real `list_transactions`

Currently a stub returning empty. Use LDK Node's `list_payments()` with
filtering by type, time range, limit/offset.

Need to support `payment_method` filter (`bolt11`, `bolt12`, `onchain`,
`keysend`).

Files: `lib.rs` (`ListTransactionsHandler`), `lightning/ldk_service.rs`

**Tests affected:**
- `nwc_list_transactions_roundtrip.rs` — currently tests against empty stub,
  needs rewrite

**New tests needed:**
- `nwc_ldk_integration/list_transactions_after_payments.rs` — make payments,
  list them, verify entries with correct fields
- `nwc_list_transactions_filter_by_type.rs` — test `type` filter
- `nwc_list_transactions_pagination.rs` — test `limit`/`offset`

---

## Phase 3 — Notification Infrastructure

Goal: Build the event-driven notification pipeline for both NWC and NNC.

### 3.1 LDK event monitoring loop

LDK Node provides `wait_next_event()` (blocking) and `event_handled()`.
Create a background task with a blocking event loop:

```rust
loop {
    let event = node.wait_next_event();
    // map event → notification, dispatch to subscribers
    node.event_handled();
}
```

No polling needed — `wait_next_event()` blocks until an event is available.

Relevant LDK events:
- `PaymentReceived` → NWC `payment_received`
- `PaymentSuccessful` → NWC `payment_sent`
- `ChannelReady` → NNC `channel_opened`
- `ChannelClosed` → NNC `channel_closed`

**New tests needed:**
- `notification_event_loop_starts.rs` — verify event loop spawns with LDK service

### 3.2 Per-client subscription state

Data structure to track which clients are subscribed to which notification
types. Support for:

- Default: notify creator of their own operations
- Explicit subscription via `subscribe_notifications`
- Opt-out via `"notify": false` on individual requests

**New tests needed:**
- `subscription_state_default_notify_creator.rs` — client that makes an
  invoice gets `payment_received` when it's paid
- `subscription_state_opt_out.rs` — `"notify": false` suppresses notification
- `subscription_state_explicit_subscribe.rs` — subscribed client gets all
  events of that type, even for other clients' operations

### 3.3 `subscribe_notifications` handler (NWC)

```
Request:  { types: ["payment_received", "payment_sent"] }
Response: {}
```

Empty types array unsubscribes.

**New tests needed:**
- `nwc_subscribe_notifications_roundtrip.rs` — subscribe and unsubscribe
- `nwc_subscribe_then_receive_notification.rs` — subscribe, trigger payment,
  verify kind 23197 notification received

### 3.4 `subscribe_notifications` handler (NNC)

Same pattern but for NNC notification types:
```
Request:  { types: ["channel_opened", "channel_closed"] }
Response: {}
```

**New tests needed:**
- `nnc_subscribe_notifications_roundtrip.rs`
- `nnc_subscribe_channel_opened_notification.rs` — subscribe, open channel,
  verify kind 23200 notification with funding_txid

### 3.5 NWC notification publishing (kind 23197)

When a subscribed event occurs, encrypt the notification payload and publish
as kind 23197 to the relay, p-tagged to each subscribed client.

**New tests needed:**
- `nwc_payment_received_notification.rs` — make invoice on Alice, pay from
  Bob, verify Alice receives kind 23197 `payment_received`
- `nwc_payment_sent_notification.rs` — pay invoice, verify `payment_sent`
  notification

### 3.6 NNC notification publishing (kind 23200)

Same for NNC notifications — kind 23200, p-tagged + e-tagged (reference
original request).

**New tests needed:**
- `nnc_channel_opened_notification.rs` — open channel with `notify: true`,
  wait for confirmation, verify kind 23200 with channel details + funding_txid
- `nnc_channel_closed_notification.rs` — close channel, verify notification
  with closing_txid

### 3.7 Hold invoice notifications

Implement `hold_invoice_accepted` notification when LDK reports an accepted
hold invoice HTLC.

**New tests needed:**
- `nwc_hold_invoice_accepted_notification.rs` — make hold invoice, have payer
  lock it, verify `hold_invoice_accepted` notification

---

## Phase 4 — NIP-44 Encryption

Goal: Replace NIP-04 with NIP-44 and add negotiation.

### 4.1 Add NIP-44 to nwc crate fork

The `dukeh3/nostr` fork needs NIP-44 encrypt/decrypt functions accessible
from the nwc crate. The upstream nostr-sdk already supports NIP-44; may
need to merge upstream changes.

### 4.2 Add `encryption` tag to info events

Both kind 13194 (NWC) and kind 13198 (NNC) info events should include:
```json
["encryption", "nip44_v2 nip04"]
```

**Tests affected:**
- `nwc_get_info_roundtrip.rs` — may need to check encryption tag on info event
- `e2e_blackbox_container.rs` → `e2e_nwc_get_info_get_balance_roundtrip`

**New tests needed:**
- `nwc_info_event_encryption_tag.rs` — verify encryption tag present and correct

### 4.3 Request encryption negotiation

Read the `encryption` tag from incoming requests. Decrypt accordingly.
Respond with the same encryption scheme the client used.

**Tests affected:**
All NWC and NNC roundtrip tests currently use NIP-04. They will continue to
work (NIP-04 remains supported for requests). New tests should use NIP-44.

**New tests needed:**
- `nwc_nip44_request_roundtrip.rs` — send NIP-44 encrypted request, verify
  NIP-44 encrypted response
- `nwc_nip04_backward_compat.rs` — send NIP-04 request (no encryption tag),
  verify NIP-04 response still works
- `nwc_unsupported_encryption_error.rs` — send with unknown encryption tag,
  verify UNSUPPORTED_ENCRYPTION error

### 4.4 Notification encryption

Publish notifications encrypted with NIP-44 only (kind 23197 for NWC,
kind 23200 for NNC). We will NOT publish NIP-04 notifications (kind 23196).
Legacy NIP-04 clients that don't support NIP-44 will not receive
notifications.

**New tests needed:**
- `nwc_notification_nip44_only.rs` — verify notifications are kind 23197
  (NIP-44) and no kind 23196 (NIP-04) is published

---

## Phase 5 — Advanced NNC Methods

Goal: Implement the remaining NIP-XX node control methods.

### 5.1 `get_channel_fees`

Read per-channel fee config. LDK Node exposes channel config through
`ChannelDetails.config`. Map to NIP-XX format:

```
Response: { fees: [{ id, short_channel_id, peer_pubkey, base_fee_msat, fee_rate, min_htlc_msat, max_htlc_msat }] }
```

**New tests needed:**
- `control_get_channel_fees_roundtrip.rs` — open channel, query fees
- `control_get_channel_fees_specific_channel.rs` — query single channel by id

### 5.2 `set_channel_fees`

Update channel fee policy. LDK Node has `update_channel_config()`.

```
Request:  { id, base_fee_msat, fee_rate, min_htlc_msat, max_htlc_msat }
Response: {}
```

**New tests needed:**
- `control_set_channel_fees_roundtrip.rs` — set fees, then get_channel_fees
  to verify
- `control_set_channel_fees_not_found.rs` — NOT_FOUND error for bad channel id

### 5.3 `get_pending_htlcs`

Extract in-flight HTLCs from channel details. LDK's `ChannelDetails` has
inbound/outbound HTLC info.

```
Response: { htlcs: [{ channel_id, direction, amount, hash_lock, expiry_height }] }
```

**New tests needed:**
- `control_get_pending_htlcs_empty.rs` — no HTLCs in flight
- `control_get_pending_htlcs_with_hold_invoice.rs` — hold invoice creates
  pending HTLC

### 5.4 `get_forwarding_history`

LDK emits `Event::PaymentForwarded`. Need persistent storage to record
these events and query them later.

Options:
- SQLite table for forwarding events
- In-memory ring buffer (loses data on restart)

```
Response: { forwards: [{ incoming_channel_id, outgoing_channel_id, incoming_amount, outgoing_amount, fee_earned, settled_at }] }
```

**New tests needed:**
- `control_get_forwarding_history_empty.rs`
- `control_get_forwarding_history_after_routing.rs` — requires 3-node setup
  (A→B→C) to generate a forwarding event

### 5.5 `list_network_nodes`

LDK Node exposes `network_graph()` which returns a `ReadOnlyNetworkGraph`.
Iterate nodes with pagination.

```
Response: { nodes: [{ pubkey, alias, color, num_channels, total_capacity, addresses, last_update }] }
```

**New tests needed:**
- `control_list_network_nodes_roundtrip.rs`

### 5.6 `get_network_stats`

Aggregate from `network_graph()`:
```
Response: { num_nodes, num_channels, total_capacity, avg_channel_size, max_channel_size }
```

**New tests needed:**
- `control_get_network_stats_roundtrip.rs`

### 5.7 `get_network_node`

Single node lookup from `network_graph()`:
```
Response: { pubkey, alias, color, num_channels, total_capacity, addresses, last_update, features }
```

**New tests needed:**
- `control_get_network_node_roundtrip.rs`
- `control_get_network_node_not_found.rs`

### 5.8 `get_network_channel`

Single channel lookup from `network_graph()`:
```
Response: { short_channel_id, capacity, node1_pubkey, node2_pubkey, node1_policy, node2_policy }
```

**New tests needed:**
- `control_get_network_channel_roundtrip.rs`
- `control_get_network_channel_not_found.rs`

### 5.9 `estimate_route_fee`

No direct LDK Node API. Would need access to the underlying `DefaultRouter`
or `NetworkGraph` to probe routes.

Possible approach: use `bolt11_payment().send_probes()` with a dummy invoice.

**New tests needed:**
- `control_estimate_route_fee_roundtrip.rs`
- `control_estimate_route_fee_no_route.rs`

### 5.10 `query_routes`

Same underlying challenge. Need direct access to LDK's `find_route()`.
LDK Node 0.7 doesn't expose this publicly.

Options:
- Fork LDK Node to expose router
- Wait for upstream API
- Use `send_probes()` as approximation

**New tests needed:**
- `control_query_routes_roundtrip.rs`
- `control_query_routes_not_found.rs`

---

## Phase 6 — NWC Crate Fork Updates

All new NIP-47 methods need corresponding types in `dukeh3/nostr`:

### 6.1 New Method variants

Add to `Method` enum:
- `PayOffer`
- `MakeOffer`
- `LookupOffer`
- `PayOnchain`
- `MakeNewAddress`
- `LookupAddress`
- `SubscribeNotifications`

### 6.2 New RequestParams variants

Add parameter structs for each new method.

### 6.3 New ResponseResult variants

Add response structs:
- `PayOfferResponse { preimage, fees_paid }`
- `MakeOfferResponse { offer, description, amount }`
- `LookupOfferResponse { offer, description, amount, active, num_payments_received, total_received }`
- `PayOnchainResponse { txid }`
- `MakeNewAddressResponse { address, type }`
- `LookupAddressResponse { address, type, total_received, transactions }`
- `SubscribeNotificationsResponse {}`

### 6.4 GetBalanceResponse update

Add `lightning_balance` and `onchain_balance` optional fields.

### 6.5 ListTransactions update

Add `payment_method` field to transaction entries and filter parameter.

**Tests needed (in nwc crate):**
- JSON roundtrip tests for each new Method variant
- JSON roundtrip tests for each new RequestParams / ResponseResult
- Verify `Method::Unknown(String)` still works for forward compatibility

---

## Test Impact Summary

### Existing tests that MUST be updated

| Test file | Reason | Phase |
|-----------|--------|-------|
| `control_kind_roundtrip.rs` (5 tests) | Event kind 23196→23198, 23197→23199 | 1.1 |
| `e2e_blackbox_container.rs` → `e2e_control_list_channels_roundtrip` | Event kind change | 1.1 |
| `control_open_channel_roundtrip.rs` | Kind change, param rename, response change, remove `get_channel` | 1.1, 1.3, 1.5, 1.6, 1.11 |
| `control_list_channels_with_open_channel.rs` | Kind change, field renames | 1.1, 1.3 |
| `control_connect_disconnect_peer_roundtrip.rs` | Kind change, peer field renames, host:port merge | 1.1, 1.4, 1.7 |
| `control_channel_payments_scenario.rs` | Kind change, param renames | 1.1, 1.5 |
| `nwc_get_info_roundtrip.rs` | Methods list grows | 1.12 |
| `nwc_get_info_allowed.rs` | Methods list grows | 1.12 |
| `access_grant_get_info.rs` | Methods list grows | 1.12 |
| `nwc_get_balance_roundtrip.rs` | Response adds lightning/onchain fields | 1.8 |
| `nwc_ldk_integration/get_balance_after_onchain_funding.rs` | Response field changes | 1.8 |
| `nwc_lookup_invoice_roundtrip.rs` | Stub → real implementation | 2.7 |
| `nwc_list_transactions_roundtrip.rs` | Stub → real implementation | 2.8 |

### New test files needed

| Test file | What it tests | Phase |
|-----------|---------------|-------|
| `nnc_info_event_published.rs` | Kind 13198 published on startup | 1.2 |
| `nwc_all_methods_grant.rs` | `"ALL"` key grants all NWC methods | 1.9 |
| `control_all_methods_grant.rs` | `"ALL"` key grants all NNC methods | 1.9 |
| `nwc_make_new_address_roundtrip.rs` | make_new_address as NWC method | 1.10 |
| `nwc_get_info_notifications_field.rs` | notifications list in get_info | 1.12 |
| `nwc_pay_onchain_roundtrip.rs` | pay_onchain mock roundtrip | 2.1 |
| `nwc_ldk_integration/pay_onchain_happy_path.rs` | Real on-chain payment | 2.1 |
| `nwc_pay_offer_roundtrip.rs` | BOLT-12 pay mock | 2.3 |
| `nwc_make_offer_roundtrip.rs` | BOLT-12 offer creation mock | 2.4 |
| `nwc_lookup_offer_roundtrip.rs` | Offer lookup | 2.5 |
| `nwc_lookup_address_roundtrip.rs` | Address lookup | 2.6 |
| `nwc_ldk_integration/lookup_invoice_after_payment.rs` | Real lookup after pay | 2.7 |
| `nwc_ldk_integration/list_transactions_after_payments.rs` | Real list after payments | 2.8 |
| `nwc_subscribe_notifications_roundtrip.rs` | Subscribe/unsubscribe | 3.3 |
| `nwc_subscribe_then_receive_notification.rs` | E2E: subscribe → pay → notify | 3.3 |
| `nnc_subscribe_notifications_roundtrip.rs` | NNC subscribe/unsubscribe | 3.4 |
| `nwc_payment_received_notification.rs` | Kind 23197 on payment receipt | 3.5 |
| `nwc_payment_sent_notification.rs` | Kind 23197 on payment sent | 3.5 |
| `nnc_channel_opened_notification.rs` | Kind 23200 on channel open | 3.6 |
| `nnc_channel_closed_notification.rs` | Kind 23200 on channel close | 3.6 |
| `nwc_nip44_request_roundtrip.rs` | NIP-44 encrypted request/response | 4.3 |
| `nwc_nip04_backward_compat.rs` | NIP-04 still works | 4.3 |
| `nwc_notification_nip44_only.rs` | No kind 23196 published | 4.4 |
| `control_get_channel_fees_roundtrip.rs` | Fee query | 5.1 |
| `control_set_channel_fees_roundtrip.rs` | Fee update + verify | 5.2 |
| `control_get_pending_htlcs_empty.rs` | No HTLCs | 5.3 |
| `control_get_forwarding_history_empty.rs` | No forwards | 5.4 |
| `control_list_network_nodes_roundtrip.rs` | Graph node listing | 5.5 |
| `control_get_network_stats_roundtrip.rs` | Graph stats | 5.6 |
| `control_get_network_node_roundtrip.rs` | Single node lookup | 5.7 |
| `control_get_network_channel_roundtrip.rs` | Single channel lookup | 5.8 |
| `control_estimate_route_fee_roundtrip.rs` | Fee estimation | 5.9 |
| `control_query_routes_roundtrip.rs` | Route query | 5.10 |

---

## Summary Matrix

| Method                    | Spec   | Current      | Phase | Tests exist | Tests needed |
|---------------------------|--------|--------------|-------|-------------|--------------|
| `get_info`                | NIP-47 | Partial      | 1.12  | 5           | 1            |
| `get_balance`             | NIP-47 | Partial      | 1.8   | 3           | 2            |
| `pay_invoice`             | NIP-47 | Working      | —     | 4           | 0            |
| `pay_keysend`             | NIP-47 | Working      | —     | 4           | 0            |
| `make_invoice`            | NIP-47 | Working      | —     | 3           | 0            |
| `lookup_invoice`          | NIP-47 | Stub         | 2.7   | 1 (stub)    | 2            |
| `list_transactions`       | NIP-47 | Stub         | 2.8   | 1 (stub)    | 3            |
| `make_hold_invoice`       | NIP-47 | Stub         | —     | 1 (stub)    | 0            |
| `cancel_hold_invoice`     | NIP-47 | Stub         | —     | 1 (stub)    | 0            |
| `settle_hold_invoice`     | NIP-47 | Stub         | —     | 1 (stub)    | 0            |
| `pay_offer`               | NIP-47 | Missing      | 2.3   | 0           | 2            |
| `make_offer`              | NIP-47 | Missing      | 2.4   | 0           | 2            |
| `lookup_offer`            | NIP-47 | Missing      | 2.5   | 0           | 2            |
| `pay_onchain`             | NIP-47 | Missing      | 2.1   | 0           | 3            |
| `make_new_address`        | NIP-47 | Control only | 1.10  | 0           | 1            |
| `lookup_address`          | NIP-47 | Missing      | 2.6   | 0           | 2            |
| `subscribe_notifications` | NIP-47 | Missing      | 3.3   | 0           | 2            |
| `list_channels`           | NIP-XX | Partial      | 1.3   | 4           | 0            |
| `open_channel`            | NIP-XX | Partial      | 1.5   | 2           | 0            |
| `close_channel`           | NIP-XX | Partial      | 1.6   | 1           | 0            |
| `list_peers`              | NIP-XX | Partial      | 1.4   | 2           | 0            |
| `connect_peer`            | NIP-XX | Partial      | 1.7   | 1           | 0            |
| `disconnect_peer`         | NIP-XX | Working      | —     | 1           | 0            |
| `get_channel_fees`        | NIP-XX | Missing      | 5.1   | 0           | 2            |
| `set_channel_fees`        | NIP-XX | Missing      | 5.2   | 0           | 2            |
| `get_forwarding_history`  | NIP-XX | Missing      | 5.4   | 0           | 2            |
| `get_pending_htlcs`       | NIP-XX | Missing      | 5.3   | 0           | 2            |
| `estimate_route_fee`      | NIP-XX | Missing      | 5.9   | 0           | 2            |
| `query_routes`            | NIP-XX | Missing      | 5.10  | 0           | 2            |
| `list_network_nodes`      | NIP-XX | Missing      | 5.5   | 0           | 1            |
| `get_network_stats`       | NIP-XX | Missing      | 5.6   | 0           | 1            |
| `get_network_node`        | NIP-XX | Missing      | 5.7   | 0           | 2            |
| `get_network_channel`     | NIP-XX | Missing      | 5.8   | 0           | 2            |
| `subscribe_notifications` | NIP-XX | Missing      | 3.4   | 0           | 2            |
| NIP-44 encryption         | Both   | Missing      | 4     | 0           | 3            |
| Notification pipeline     | Both   | Missing      | 3     | 0           | 8            |
