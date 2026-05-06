#![warn(clippy::pedantic)]
#![allow(clippy::too_many_lines)]

//! Seed generator derived from `substrate/frame/proxy/src/tests.rs`.
//!
//! Each seed encodes one test scenario as a sequence of SCALE-encoded
//! `(bool, u8, RuntimeCall)` tuples for the rococo fuzzer.
//!
//! ## Key translation notes
//!
//! ### Account mapping
//! Test accounts are u64 (1, 2, 3, ...).  Fuzzer accounts are AccountId32 with
//! 5 fixed addresses [0;32]..[4;32].  Mapping used here:
//!
//! | Test account | Fuzzer origin | Fuzzer `AccountId32` |
//! |---|---|---|
//! | 1 | 0 | [0;32] |
//! | 2 | 1 | [1;32] |
//! | 3 | 2 | [2;32] |
//! | 4 | 3 | [3;32] |
//! | 5 / 6 | 4 | [4;32] |
//!
//! Fuzzer accounts are funded with `1 << 60`, so deposit/transfer amounts in
//! the original tests (which use small numbers like 10) are scaled up to
//! ensure they fit comfortably without exhausting balances.
//!
//! ### `ProxyType`
//! Rococo uses its own `ProxyType` enum (`Any`, `NonTransfer`, `Governance`,
//! `IdentityJudgement`, `CancelProxy`, `Auction`, `Society`, `OnDemandOrdering`).
//! The original substrate tests use a mock `ProxyType` (`Any`, `JustTransfer`,
//! `JustUtility`); we map those to rococo equivalents:
//! - `Any` → `Any`
//! - `JustTransfer` → no rococo equivalent (Balances is excluded from all rococo
//!   non-Any types). For filtering tests we use `NonTransfer` (which excludes
//!   Balances) to demonstrate the filter mechanism.
//! - `JustUtility` → no exact equivalent; `NonTransfer` allows Utility.
//!
//! ### `CallHasher`
//! Rococo uses `BlakeTwo256`, so call hashes are `H256(blake2_256(call.encode()))`.
//!
//! ### Block / extrinsic index
//! The fuzzer dispatches calls via `extrinsic.dispatch(...)` directly, bypassing
//! `Executive::apply_extrinsic`, so `System::extrinsic_index()` is always
//! `None → 0`.  Block numbering starts at 1 and only advances when `advance_block`
//! is set to `true` on a tuple.
//!
//! ### Pure proxy address
//! Computed pure-proxy addresses must be derived using the same height /
//! ext_index that will be observed at dispatch time inside the fuzzer.  For the
//! seeds below, we always create the pure account in block 1 with ext_index 0.

use codec::Encode;
use rococo_runtime::{ProxyType, RuntimeCall};
use sp_core::{blake2_256, H256};
use sp_runtime::{AccountId32, MultiAddress};
use std::{fs, path::Path};

// ─── helpers ─────────────────────────────────────────────────────────────────

type AccountId = AccountId32;
type BlockNumber = u32;

/// Fuzzer account at a given index (matches main.rs: `[i; 32].into()` for i in 0..5).
fn account(idx: u8) -> AccountId {
    [idx; 32].into()
}

/// Lookup wrapper for `AccountIdLookupOf<T>` arguments.
fn lookup(idx: u8) -> MultiAddress<AccountId, ()> {
    MultiAddress::Id(account(idx))
}

fn enc(advance_block: bool, origin: u8, call: RuntimeCall) -> Vec<u8> {
    (advance_block, origin, call).encode()
}

/// `H256(blake2_256(call.encode()))` — matches rococo's `T::CallHasher::hash_of(&call)`.
fn call_hash(call: &RuntimeCall) -> H256 {
    H256::from(blake2_256(&call.encode()))
}

/// Standard inner call used as the "proxied" call: transfer 1 UNIT to account(4).
fn inner_transfer_call() -> RuntimeCall {
    RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
        dest: lookup(4),
        value: UNITS,
    })
}

/// Compute a pure-proxy address the same way the pallet does, assuming the
/// `create_pure` call lands at the given block height with extrinsic index 0
/// (matches the fuzzer's `extrinsic_index().unwrap_or_default() == 0`).
fn pure_account(
    spawner: &AccountId,
    proxy_type: &ProxyType,
    index: u16,
    height: BlockNumber,
    ext_index: u32,
) -> AccountId {
    // Mirrors `pallet_proxy::Pallet::pure_account`:
    //     entropy = blake2_256(("modlpy/proxy____", who, height, ext_index, proxy_type, index).encode())
    let entropy = (
        b"modlpy/proxy____",
        spawner,
        height,
        ext_index,
        proxy_type,
        index,
    )
        .using_encoded(blake2_256);
    AccountId32::new(entropy)
}

const UNITS: u128 = 1_000_000_000_000;

// ─── entry point ─────────────────────────────────────────────────────────────

fn main() {
    let out_dir = Path::new("seedgen");
    fs::create_dir_all(out_dir).expect("create seedgen dir");

    let seeds = build_seeds();
    for (name, data) in &seeds {
        let path = out_dir.join(format!("{name}.seed"));
        fs::write(&path, data).expect("write seed");
    }
    println!("wrote {} seeds to {}", seeds.len(), out_dir.display());
}

// ─── seeds ───────────────────────────────────────────────────────────────────

fn build_seeds() -> Vec<(String, Vec<u8>)> {
    let mut seeds: Vec<(String, Vec<u8>)> = Vec::new();

    // =========================================================================
    // proxying_works (basic): add_proxy + proxy
    //
    // 0 delegates to 1 with proxy_type=Any/delay=0; 1 then proxies a transfer
    // on behalf of 0. Also exercises Duplicate (re-add) and CallFiltered paths
    // by trying a non-permitted inner call through a NonTransfer proxy.
    // =========================================================================
    {
        let mut data = Vec::new();
        let inner = inner_transfer_call();

        // 0 → 1 with proxy_type Any, delay 0
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(1),
                proxy_type: ProxyType::Any,
                delay: 0,
            }),
        ));

        // 1 calls proxy on behalf of 0 → executes transfer (Ok)
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: lookup(0),
                force_proxy_type: None,
                call: Box::new(inner.clone()),
            }),
        ));

        // 4 (not a proxy) tries → NotProxy
        data.extend(enc(
            false,
            4,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: lookup(0),
                force_proxy_type: None,
                call: Box::new(inner.clone()),
            }),
        ));

        // 1 with force_proxy_type=NonTransfer → NotProxy (not registered with that type)
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: lookup(0),
                force_proxy_type: Some(ProxyType::NonTransfer),
                call: Box::new(inner),
            }),
        ));

        seeds.push(("proxy_proxying_works".to_string(), data));
    }

    // =========================================================================
    // add_remove_proxies_works
    //
    // Add the same proxy twice → Duplicate; mix proxy_types; remove with wrong
    // type → NotFound; remove all four; re-add self → NoSelfProxy.
    // =========================================================================
    {
        let mut data = Vec::new();

        // First proxy: 0 → 1 with Any/0
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(1),
                proxy_type: ProxyType::Any,
                delay: 0,
            }),
        ));

        // Duplicate (same params) → Duplicate
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(1),
                proxy_type: ProxyType::Any,
                delay: 0,
            }),
        ));

        // Same delegate but different proxy_type → OK (counted as different)
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(1),
                proxy_type: ProxyType::NonTransfer,
                delay: 0,
            }),
        ));

        // 0 → 2 with Any
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(2),
                proxy_type: ProxyType::Any,
                delay: 0,
            }),
        ));

        // 0 → 3 with CancelProxy
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(3),
                proxy_type: ProxyType::CancelProxy,
                delay: 0,
            }),
        ));

        // remove_proxy with wrong type → NotFound
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::remove_proxy {
                delegate: lookup(2),
                proxy_type: ProxyType::NonTransfer,
                delay: 0,
            }),
        ));

        // remove_proxy with correct type → OK
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::remove_proxy {
                delegate: lookup(3),
                proxy_type: ProxyType::CancelProxy,
                delay: 0,
            }),
        ));

        // remove the remaining proxies
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::remove_proxy {
                delegate: lookup(2),
                proxy_type: ProxyType::Any,
                delay: 0,
            }),
        ));
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::remove_proxy {
                delegate: lookup(1),
                proxy_type: ProxyType::Any,
                delay: 0,
            }),
        ));
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::remove_proxy {
                delegate: lookup(1),
                proxy_type: ProxyType::NonTransfer,
                delay: 0,
            }),
        ));

        // self proxy → NoSelfProxy
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(0),
                proxy_type: ProxyType::Any,
                delay: 0,
            }),
        ));

        seeds.push(("proxy_add_remove_proxies".to_string(), data));
    }

    // =========================================================================
    // announcement_works
    //
    // Two proxies (0→2 and 1→2) with delay=1; account(2) announces calls for
    // each principal.
    // =========================================================================
    {
        let mut data = Vec::new();

        // 0 → 2 with Any/1
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(2),
                proxy_type: ProxyType::Any,
                delay: 1,
            }),
        ));
        // 1 → 2 with Any/1
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(2),
                proxy_type: ProxyType::Any,
                delay: 1,
            }),
        ));

        // 2 announces for 0
        data.extend(enc(
            false,
            2,
            RuntimeCall::Proxy(pallet_proxy::Call::announce {
                real: lookup(0),
                call_hash: H256::from([1u8; 32]),
            }),
        ));
        // 2 announces for 1
        data.extend(enc(
            false,
            2,
            RuntimeCall::Proxy(pallet_proxy::Call::announce {
                real: lookup(1),
                call_hash: H256::from([2u8; 32]),
            }),
        ));

        seeds.push(("proxy_announcement_works".to_string(), data));
    }

    // =========================================================================
    // remove_announcement_works
    //
    // Setup as announcement_works; then remove_announcement with wrong hash
    // (NotFound), then with correct hash (Ok).
    // =========================================================================
    {
        let mut data = Vec::new();

        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(2),
                proxy_type: ProxyType::Any,
                delay: 1,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(2),
                proxy_type: ProxyType::Any,
                delay: 1,
            }),
        ));
        data.extend(enc(
            false,
            2,
            RuntimeCall::Proxy(pallet_proxy::Call::announce {
                real: lookup(0),
                call_hash: H256::from([1u8; 32]),
            }),
        ));
        data.extend(enc(
            false,
            2,
            RuntimeCall::Proxy(pallet_proxy::Call::announce {
                real: lookup(1),
                call_hash: H256::from([2u8; 32]),
            }),
        ));

        // remove with wrong hash → NotFound
        data.extend(enc(
            false,
            2,
            RuntimeCall::Proxy(pallet_proxy::Call::remove_announcement {
                real: lookup(0),
                call_hash: H256::from([0u8; 32]),
            }),
        ));
        // remove with correct hash → Ok
        data.extend(enc(
            false,
            2,
            RuntimeCall::Proxy(pallet_proxy::Call::remove_announcement {
                real: lookup(0),
                call_hash: H256::from([1u8; 32]),
            }),
        ));

        seeds.push(("proxy_remove_announcement".to_string(), data));
    }

    // =========================================================================
    // reject_announcement_works
    //
    // Same setup; reject_announcement called by `real` (not the delegate).
    // =========================================================================
    {
        let mut data = Vec::new();

        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(2),
                proxy_type: ProxyType::Any,
                delay: 1,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(2),
                proxy_type: ProxyType::Any,
                delay: 1,
            }),
        ));
        data.extend(enc(
            false,
            2,
            RuntimeCall::Proxy(pallet_proxy::Call::announce {
                real: lookup(0),
                call_hash: H256::from([1u8; 32]),
            }),
        ));
        data.extend(enc(
            false,
            2,
            RuntimeCall::Proxy(pallet_proxy::Call::announce {
                real: lookup(1),
                call_hash: H256::from([2u8; 32]),
            }),
        ));

        // 0 rejects with wrong hash → NotFound
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::reject_announcement {
                delegate: lookup(2),
                call_hash: H256::from([0u8; 32]),
            }),
        ));
        // 3 (not real of any announcement) rejects → NotFound
        data.extend(enc(
            false,
            3,
            RuntimeCall::Proxy(pallet_proxy::Call::reject_announcement {
                delegate: lookup(2),
                call_hash: H256::from([1u8; 32]),
            }),
        ));
        // 0 rejects correct (its own) → Ok
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::reject_announcement {
                delegate: lookup(2),
                call_hash: H256::from([1u8; 32]),
            }),
        ));

        seeds.push(("proxy_reject_announcement".to_string(), data));
    }

    // =========================================================================
    // announcer_must_be_proxy
    //
    // 1 tries to announce on behalf of 0 without being a registered proxy → NotProxy.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::announce {
                real: lookup(0),
                call_hash: H256::from([0u8; 32]),
            }),
        ));
        seeds.push(("proxy_announcer_must_be_proxy".to_string(), data));
    }

    // =========================================================================
    // calling_proxy_doesnt_remove_announcement
    //
    // Add proxy with delay=0; announce; then proxy() executes and the
    // announcement remains in storage.
    // =========================================================================
    {
        let mut data = Vec::new();
        let inner = inner_transfer_call();
        let h = call_hash(&inner);

        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(1),
                proxy_type: ProxyType::Any,
                delay: 0,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::announce {
                real: lookup(0),
                call_hash: h,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: lookup(0),
                force_proxy_type: None,
                call: Box::new(inner),
            }),
        ));

        seeds.push((
            "proxy_calling_proxy_doesnt_remove_announcement".to_string(),
            data,
        ));
    }

    // =========================================================================
    // delayed_requires_pre_announcement +
    // proxy_announced_removes_announcement_and_returns_deposit
    //
    // add_proxy(delay=1); proxy() → Unannounced; proxy_announced (no announce yet)
    // → Unannounced; announce; advance block; proxy_announced → Ok.
    // =========================================================================
    {
        let mut data = Vec::new();
        let inner = inner_transfer_call();
        let h = call_hash(&inner);

        // 0 → 1 with delay=1
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(1),
                proxy_type: ProxyType::Any,
                delay: 1,
            }),
        ));
        // proxy() with no announcement → Unannounced
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: lookup(0),
                force_proxy_type: None,
                call: Box::new(inner.clone()),
            }),
        ));
        // proxy_announced before announcing → Unannounced
        data.extend(enc(
            false,
            3,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy_announced {
                delegate: lookup(1),
                real: lookup(0),
                force_proxy_type: None,
                call: Box::new(inner.clone()),
            }),
        ));
        // 1 announces the call hash
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::announce {
                real: lookup(0),
                call_hash: h,
            }),
        ));
        // proxy_announced before delay elapsed → Unannounced
        data.extend(enc(
            false,
            3,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy_announced {
                delegate: lookup(1),
                real: lookup(0),
                force_proxy_type: None,
                call: Box::new(inner.clone()),
            }),
        ));
        // advance block (now block 2 ≥ height(1) + delay(1))
        data.extend(enc(
            true,
            3,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy_announced {
                delegate: lookup(1),
                real: lookup(0),
                force_proxy_type: None,
                call: Box::new(inner),
            }),
        ));

        seeds.push(("proxy_delayed_announced".to_string(), data));
    }

    // =========================================================================
    // filtering_works (adapted to rococo proxy types)
    //
    // 0 delegates to 1 (Any), 2 (NonTransfer), 3 (CancelProxy).
    // - proxy(1, transfer) → Ok (Any allows everything)
    // - proxy(2, transfer) → CallFiltered (NonTransfer excludes Balances)
    // - proxy(3, transfer) → CallFiltered (CancelProxy only allows reject_announcement)
    // - proxy(2, system::remark) → Ok (NonTransfer allows System)
    // - proxy(2, utility::batch[transfer]) → batch exec but inner filtered
    // =========================================================================
    {
        let mut data = Vec::new();

        // setup
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(1),
                proxy_type: ProxyType::Any,
                delay: 0,
            }),
        ));
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(2),
                proxy_type: ProxyType::NonTransfer,
                delay: 0,
            }),
        ));
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(3),
                proxy_type: ProxyType::CancelProxy,
                delay: 0,
            }),
        ));

        let transfer = inner_transfer_call();

        // Any → Ok
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: lookup(0),
                force_proxy_type: None,
                call: Box::new(transfer.clone()),
            }),
        ));
        // NonTransfer with Balances → ProxyExecuted(Err(CallFiltered))
        data.extend(enc(
            false,
            2,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: lookup(0),
                force_proxy_type: None,
                call: Box::new(transfer.clone()),
            }),
        ));
        // CancelProxy with Balances → ProxyExecuted(Err(CallFiltered))
        data.extend(enc(
            false,
            3,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: lookup(0),
                force_proxy_type: None,
                call: Box::new(transfer.clone()),
            }),
        ));

        // Inside Utility::batch — the filter is applied recursively to inner calls.
        let batch = RuntimeCall::Utility(pallet_utility::Call::batch {
            calls: vec![transfer.clone()],
        });
        // Any → batch executes
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: lookup(0),
                force_proxy_type: None,
                call: Box::new(batch.clone()),
            }),
        ));
        // NonTransfer → outer batch allowed but inner Balances filtered (BatchInterrupted)
        data.extend(enc(
            false,
            2,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: lookup(0),
                force_proxy_type: None,
                call: Box::new(batch),
            }),
        ));

        // remove_proxies via NonTransfer → CallFiltered (escalation prevention).
        let remove_all = RuntimeCall::Proxy(pallet_proxy::Call::remove_proxies {});
        data.extend(enc(
            false,
            2,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: lookup(0),
                force_proxy_type: None,
                call: Box::new(remove_all.clone()),
            }),
        ));
        // remove_proxies via Any → Ok (clears all of 0's proxies)
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: lookup(0),
                force_proxy_type: None,
                call: Box::new(remove_all),
            }),
        ));

        seeds.push(("proxy_filtering_works".to_string(), data));
    }

    // =========================================================================
    // pure_works  (create_pure / proxy via pure / kill_pure)
    //
    // create_pure(0, Any, 0, 0) creates a deterministic pure account.  We then:
    // - fund the pure account with a balance transfer,
    // - call proxy(0, real=pure, transfer) which dispatches the inner transfer
    //   from the pure account,
    // - call proxy(0, real=pure, kill_pure(...)) which dispatches kill_pure
    //   from the pure account itself (the only valid origin) and removes the
    //   pure proxy.
    //
    // The fuzzer always observes height=1, ext_index=0 at create_pure time,
    // so the pure address is computed using those values.
    // =========================================================================
    {
        let mut data = Vec::new();

        // 0 creates a pure account (Any, delay=0, index=0).
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::create_pure {
                proxy_type: ProxyType::Any,
                delay: 0,
                index: 0,
            }),
        ));

        // Compute the deterministic pure address (block 1, ext_index 0).
        let pure = pure_account(&account(0), &ProxyType::Any, 0, 1, 0);

        // Fund the pure account so it can transfer.
        data.extend(enc(
            false,
            0,
            RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
                dest: MultiAddress::Id(pure.clone()),
                value: 5 * UNITS,
            }),
        ));

        // 0 (the spawner / sole delegate) proxies a transfer through the pure account.
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: MultiAddress::Id(pure.clone()),
                force_proxy_type: None,
                call: Box::new(inner_transfer_call()),
            }),
        ));

        // Wrong spawner → kill_pure fails with NoPermission.
        let kill_call_wrong = RuntimeCall::Proxy(pallet_proxy::Call::kill_pure {
            spawner: lookup(1), // wrong: actual spawner was account(0)
            proxy_type: ProxyType::Any,
            index: 0,
            height: 1,
            ext_index: 0,
        });
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: MultiAddress::Id(pure.clone()),
                force_proxy_type: None,
                call: Box::new(kill_call_wrong),
            }),
        ));

        // Correct kill_pure (from the pure account, dispatched via proxy).
        let kill_call = RuntimeCall::Proxy(pallet_proxy::Call::kill_pure {
            spawner: lookup(0),
            proxy_type: ProxyType::Any,
            index: 0,
            height: 1,
            ext_index: 0,
        });
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: MultiAddress::Id(pure),
                force_proxy_type: None,
                call: Box::new(kill_call),
            }),
        ));

        seeds.push(("proxy_pure_create_use_kill".to_string(), data));
    }

    // =========================================================================
    // poke_deposit_works_for_proxy_deposits
    // poke_deposit_charges_fee_when_deposit_unchanged
    //
    // We can't change deposit constants from the fuzzer, so the poke just
    // exercises the unchanged-deposit (Pays::Yes) path.
    // =========================================================================
    {
        let mut data = Vec::new();

        // 0 adds a proxy → reserves deposit
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(1),
                proxy_type: ProxyType::Any,
                delay: 0,
            }),
        ));

        // 0 pokes its own deposit (no change) → Pays::Yes
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::poke_deposit {}),
        ));

        // Account with no proxies/announcements → still Ok (Pays::Yes, no event).
        data.extend(enc(
            false,
            4,
            RuntimeCall::Proxy(pallet_proxy::Call::poke_deposit {}),
        ));

        seeds.push(("proxy_poke_deposit_basic".to_string(), data));
    }

    // =========================================================================
    // poke_deposit_updates_both_proxy_and_announcement_deposits
    //
    // 1 has proxy storage AND announcement storage; pokes both.
    // =========================================================================
    {
        let mut data = Vec::new();

        // 1 adds a proxy (1 → 0 with Any)
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(0),
                proxy_type: ProxyType::Any,
                delay: 0,
            }),
        ));
        // 0 → 1 (so 1 is a proxy of 0 with delay=1, allowing announcements)
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                delegate: lookup(1),
                proxy_type: ProxyType::Any,
                delay: 1,
            }),
        ));
        // 1 announces a call for 0 (creates announcement deposit on 1)
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::announce {
                real: lookup(0),
                call_hash: H256::from([7u8; 32]),
            }),
        ));
        // 1 pokes; both deposits exist but values unchanged → Pays::Yes
        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::poke_deposit {}),
        ));

        seeds.push(("proxy_poke_deposit_both".to_string(), data));
    }

    // =========================================================================
    // proxy_through_utility_batch
    //
    // Use Utility::batch_all to register two proxies atomically, then exercise
    // proxy() on one of them.  This puts a Utility wrapper around proxy state
    // changes — a useful seed because the fuzzer recursively filters through
    // batch calls.
    // =========================================================================
    {
        let add1 = RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
            delegate: lookup(1),
            proxy_type: ProxyType::Any,
            delay: 0,
        });
        let add2 = RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
            delegate: lookup(2),
            proxy_type: ProxyType::NonTransfer,
            delay: 0,
        });

        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::batch_all {
                calls: vec![add1, add2],
            }),
        ));

        data.extend(enc(
            false,
            1,
            RuntimeCall::Proxy(pallet_proxy::Call::proxy {
                real: lookup(0),
                force_proxy_type: None,
                call: Box::new(inner_transfer_call()),
            }),
        ));

        seeds.push(("proxy_via_utility_batch".to_string(), data));
    }

    // =========================================================================
    // proxy_remove_proxies
    //
    // Add a few proxies, then call remove_proxies (clears all in one shot).
    // =========================================================================
    {
        let mut data = Vec::new();
        for d in [1u8, 2, 3] {
            data.extend(enc(
                false,
                0,
                RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                    delegate: lookup(d),
                    proxy_type: ProxyType::Any,
                    delay: 0,
                }),
            ));
        }
        data.extend(enc(
            false,
            0,
            RuntimeCall::Proxy(pallet_proxy::Call::remove_proxies {}),
        ));
        seeds.push(("proxy_remove_proxies".to_string(), data));
    }

    seeds
}
