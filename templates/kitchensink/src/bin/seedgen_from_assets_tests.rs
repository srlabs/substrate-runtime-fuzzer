#![warn(clippy::pedantic)]
#![allow(clippy::too_many_lines)]

//! Seed generator derived from `substrate/frame/assets/src/tests.rs`.
//!
//! Each seed encodes one full test scenario as a sequence of SCALE-encoded
//! `(bool, u8, RuntimeCall)` tuples.  The fuzzer will use these as a starting
//! corpus and mutate them to explore deeper code paths.
//!
//! Translation notes
//! -----------------
//! * `force_create(root, id, owner, is_sufficient, min_balance)` → replaced by
//!   `create(signed(0), id, lookup(0), min_balance)`.  Assets created this way
//!   are non-sufficient; tests that relied on sufficiency still produce useful
//!   seeds because the fuzzer mutates them.
//! * Test account integer `N` → fuzzer origin `N-1` (0-indexed), account `[N-1;32]`.
//!   Exceptions: test accounts 10/20 (minting targets) → origins 1/2.
//! * `set_reserves` is filtered by the fuzzer – skipped in all seeds.
//! * Root-only calls (`force_create`, `force_set_metadata`, `force_clear_metadata`,
//!   `force_asset_status`) are omitted; signed equivalents are used where possible.
//! * All five fuzzer accounts ([0;32]…[4;32]) start with 10_000_000 DOLLARS,
//!   so every deposit (AssetDeposit=100 DOLLARS, ApprovalDeposit=1 DOLLAR,
//!   MetadataDeposit, AccountDeposit=1 DOLLAR) is comfortably covered.

use codec::{Compact, Encode};
use kitchensink_runtime::RuntimeCall;
use node_primitives::AccountIndex;
use sp_runtime::MultiAddress;
use std::{fs, path::Path};

// ── helpers ──────────────────────────────────────────────────────────────────

type AccountId = sp_runtime::AccountId32;

/// Account `[idx; 32]` – mirrors the fuzzer's account list.
fn account(idx: u8) -> AccountId {
    [idx; 32].into()
}

/// `MultiAddress::Id([idx; 32])` – used wherever a call takes `AccountIdLookupOf<T>`.
/// The kitchensink runtime's Lookup is `Indices`, so the second type param is `AccountIndex = u32`.
fn lookup(idx: u8) -> MultiAddress<AccountId, AccountIndex> {
    MultiAddress::Id(account(idx))
}

/// Encode one extrinsic in fuzzer format: `(advance_block, origin, call)`.
fn enc(advance_block: bool, origin: u8, call: RuntimeCall) -> Vec<u8> {
    (advance_block, origin, call).encode()
}

// ── entry point ──────────────────────────────────────────────────────────────

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

// ── seed scenarios ────────────────────────────────────────────────────────────

#[allow(clippy::cast_lossless)]
fn build_seeds() -> Vec<(String, Vec<u8>)> {
    let mut seeds: Vec<(String, Vec<u8>)> = Vec::new();

    // =========================================================================
    // lifecycle_should_work
    //
    // Full asset lifecycle: create → set_metadata → mint × 2 → freeze_asset →
    // start_destroy → destroy_accounts → destroy_approvals → finish_destroy,
    // repeated twice.  `set_reserves` is skipped (filtered by the fuzzer).
    // =========================================================================
    {
        let mut data = Vec::new();

        // create asset 1, admin = account(0)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        // set_metadata: name=[0], symbol=[0], decimals=12
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::set_metadata {
            id: Compact(1u32),
            name: vec![0u8],
            symbol: vec![0u8],
            decimals: 12,
        })));
        // mint 100 to account(1), account(2)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(1),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(2),
            amount: 100u128,
        })));
        // destroy sequence
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::freeze_asset {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::start_destroy {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::destroy_accounts {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::destroy_approvals {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::finish_destroy {
            id: Compact(1u32),
        })));

        // second round – recreate asset 1
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::set_metadata {
            id: Compact(1u32),
            name: vec![0u8],
            symbol: vec![0u8],
            decimals: 12,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(1),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(2),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::freeze_asset {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::start_destroy {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::destroy_accounts {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::destroy_approvals {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::finish_destroy {
            id: Compact(1u32),
        })));

        seeds.push(("assets_lifecycle_should_work".to_string(), data));
    }

    // =========================================================================
    // normal_asset_create_and_destroy_callbacks_should_work
    //
    // Minimal create → destroy cycle.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::start_destroy {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::destroy_accounts {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::destroy_approvals {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::finish_destroy {
            id: Compact(1u32),
        })));
        seeds.push(("assets_create_and_destroy_callbacks".to_string(), data));
    }

    // =========================================================================
    // partial_destroy_should_work
    //
    // Mint to 5 accounts (RemoveItemsLimit=1000 so one destroy_accounts call
    // handles all of them in the real runtime, unlike the mock which caps at 5).
    // Exercise multiple destroy_accounts + destroy_approvals calls.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        for target in 0u8..5 {
            data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
                id: Compact(1u32),
                beneficiary: lookup(target),
                amount: 10u128,
            })));
        }
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::freeze_asset {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::start_destroy {
            id: Compact(1u32),
        })));
        // call multiple times as the test does
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::destroy_accounts {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::destroy_accounts {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::destroy_approvals {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::destroy_approvals {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::finish_destroy {
            id: Compact(1u32),
        })));
        seeds.push(("assets_partial_destroy_should_work".to_string(), data));
    }

    // =========================================================================
    // destroy_should_refund_approvals
    //
    // Create approvals for three delegates, then destroy the asset; the
    // destroy_approvals step should unreserve all approval deposits.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        // three approvals from account(0) to accounts 1, 2, 3
        for delegate in 1u8..=3 {
            data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::approve_transfer {
                id: Compact(1u32),
                delegate: lookup(delegate),
                amount: 50u128,
            })));
        }
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::freeze_asset {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::start_destroy {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::destroy_accounts {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::destroy_approvals {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::finish_destroy {
            id: Compact(1u32),
        })));
        seeds.push(("assets_destroy_should_refund_approvals".to_string(), data));
    }

    // =========================================================================
    // basic_minting_should_work
    //
    // create two assets, mint to multiple accounts.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(2u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(1),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(2u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        seeds.push(("assets_basic_minting_should_work".to_string(), data));
    }

    // =========================================================================
    // querying_total_supply_should_work
    //
    // mint → transfer chain → burn.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 50u128,
        })));
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(2),
            amount: 31u128,
        })));
        // burn all of account(2)'s balance
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::burn {
            id: Compact(1u32),
            who: lookup(2),
            amount: u128::MAX,
        })));
        seeds.push(("assets_querying_total_supply_should_work".to_string(), data));
    }

    // =========================================================================
    // transferring_amount_below_available_balance_should_work
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 50u128,
        })));
        seeds.push(("assets_transferring_amount_below_available".to_string(), data));
    }

    // =========================================================================
    // transferring_enough_to_kill_source_when_keep_alive_should_fail
    //
    // transfer_keep_alive: attempts that exceed keep-alive threshold fail;
    // the successful call leaves min_balance in the source.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 10u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        // would kill source (100 - 91 = 9 < 10), should fail
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer_keep_alive {
            id: Compact(1u32),
            target: lookup(1),
            amount: 91u128,
        })));
        // leaves exactly min_balance, should succeed
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer_keep_alive {
            id: Compact(1u32),
            target: lookup(1),
            amount: 90u128,
        })));
        seeds.push(("assets_transfer_keep_alive".to_string(), data));
    }

    // =========================================================================
    // min_balance_should_work
    //
    // Accounts that drop below min_balance are reaped: by transfer, force_transfer,
    // burn, and transfer_approved.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 10u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        // death by transfer: 100 - 91 = 9 < 10 → account(0) reaped
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 91u128,
        })));
        // death by force_transfer: account(1) is now at 100 → transfer 91 out
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::force_transfer {
            id: Compact(1u32),
            source: lookup(1),
            dest: lookup(0),
            amount: 91u128,
        })));
        // death by burn: account(0) at 100 → burn 91
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::burn {
            id: Compact(1u32),
            who: lookup(0),
            amount: 91u128,
        })));
        // death by transfer_approved: re-mint, approve, transfer_approved
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::approve_transfer {
            id: Compact(1u32),
            delegate: lookup(1),
            amount: 100u128,
        })));
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer_approved {
            id: Compact(1u32),
            owner: lookup(0),
            destination: lookup(2),
            amount: 91u128,
        })));
        seeds.push(("assets_min_balance_should_work".to_string(), data));
    }

    // =========================================================================
    // transferring_frozen_user_should_not_work
    //
    // freeze an account → transfer FROM it fails, TO it succeeds → thaw → both work.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(1),
            amount: 100u128,
        })));
        // freeze account(1)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::freeze {
            id: Compact(1u32),
            who: lookup(1),
        })));
        // can transfer TO frozen account(1)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 50u128,
        })));
        // cannot transfer FROM frozen account(1) – will fail with Frozen
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(0),
            amount: 25u128,
        })));
        // thaw account(1)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::thaw {
            id: Compact(1u32),
            who: lookup(1),
        })));
        // transfer now succeeds
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(0),
            amount: 25u128,
        })));
        seeds.push(("assets_transferring_frozen_user".to_string(), data));
    }

    // =========================================================================
    // transferring_frozen_asset_should_not_work
    //
    // freeze_asset → transfer fails → thaw_asset → transfer works.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::freeze_asset {
            id: Compact(1u32),
        })));
        // fails: AssetNotLive
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 50u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::thaw_asset {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 50u128,
        })));
        seeds.push(("assets_transferring_frozen_asset".to_string(), data));
    }

    // =========================================================================
    // transferring_from_blocked_account_should_not_work
    //
    // block → transfer from blocked fails → thaw → transfer works.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        // block account(0) (freezer = account(0) itself)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::block {
            id: Compact(1u32),
            who: lookup(0),
        })));
        // fails: Frozen (blocked behaves like frozen for outgoing transfers)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 50u128,
        })));
        // thaw (admin = account(0))
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::thaw {
            id: Compact(1u32),
            who: lookup(0),
        })));
        // now succeeds
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 50u128,
        })));
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(0),
            amount: 50u128,
        })));
        seeds.push(("assets_transferring_from_blocked".to_string(), data));
    }

    // =========================================================================
    // transferring_to_blocked_account_should_not_work
    //
    // block an account → transfer TO it fails → thaw → succeeds.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(1),
            amount: 100u128,
        })));
        // block account(0) as transfer target
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::block {
            id: Compact(1u32),
            who: lookup(0),
        })));
        // fails: TokenError::Blocked
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(0),
            amount: 50u128,
        })));
        // thaw
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::thaw {
            id: Compact(1u32),
            who: lookup(0),
        })));
        // both directions now work
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(0),
            amount: 50u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 50u128,
        })));
        seeds.push(("assets_transferring_to_blocked".to_string(), data));
    }

    // =========================================================================
    // approval_lifecycle_works
    //
    // create → mint → approve_transfer → transfer_approved (partial) →
    // cancel_approval.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        // account(0) approves account(1) for 50
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::approve_transfer {
            id: Compact(1u32),
            delegate: lookup(1),
            amount: 50u128,
        })));
        // account(1) transfers 40 from account(0) to account(2)
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer_approved {
            id: Compact(1u32),
            owner: lookup(0),
            destination: lookup(2),
            amount: 40u128,
        })));
        // account(0) cancels remaining 10-unit approval
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::cancel_approval {
            id: Compact(1u32),
            delegate: lookup(1),
        })));
        seeds.push(("assets_approval_lifecycle_works".to_string(), data));
    }

    // =========================================================================
    // transfer_approved_all_funds
    //
    // Transferring the full approved amount auto-removes the approval entry.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::approve_transfer {
            id: Compact(1u32),
            delegate: lookup(1),
            amount: 50u128,
        })));
        // transfer the full approved amount → approval removed automatically
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer_approved {
            id: Compact(1u32),
            owner: lookup(0),
            destination: lookup(2),
            amount: 50u128,
        })));
        seeds.push(("assets_transfer_approved_all_funds".to_string(), data));
    }

    // =========================================================================
    // cancel_approval_works
    //
    // Various error paths then a successful cancel.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::approve_transfer {
            id: Compact(1u32),
            delegate: lookup(1),
            amount: 50u128,
        })));
        // these should all fail (wrong asset / wrong origin / wrong delegate)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::cancel_approval {
            id: Compact(2u32),  // wrong asset
            delegate: lookup(1),
        })));
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::cancel_approval {
            id: Compact(1u32),  // wrong owner
            delegate: lookup(1),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::cancel_approval {
            id: Compact(1u32),
            delegate: lookup(2),  // wrong delegate
        })));
        // successful cancel
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::cancel_approval {
            id: Compact(1u32),
            delegate: lookup(1),
        })));
        // duplicate cancel should fail
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::cancel_approval {
            id: Compact(1u32),
            delegate: lookup(1),
        })));
        seeds.push(("assets_cancel_approval_works".to_string(), data));
    }

    // =========================================================================
    // force_cancel_approval_works
    //
    // The asset admin (account(0)) can force-cancel any approval.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::approve_transfer {
            id: Compact(1u32),
            delegate: lookup(1),
            amount: 50u128,
        })));
        // non-admin tries → fails NoPermission
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::force_cancel_approval {
            id: Compact(1u32),
            owner: lookup(0),
            delegate: lookup(1),
        })));
        // admin (account(0)) force-cancels
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::force_cancel_approval {
            id: Compact(1u32),
            owner: lookup(0),
            delegate: lookup(1),
        })));
        seeds.push(("assets_force_cancel_approval_works".to_string(), data));
    }

    // =========================================================================
    // transfer_owner_should_work
    //
    // create → transfer_ownership → set_metadata under new owner →
    // transfer_ownership back.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer_ownership {
            id: Compact(1u32),
            owner: lookup(1),
        })));
        // new owner sets metadata
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::set_metadata {
            id: Compact(1u32),
            name: vec![0u8; 10],
            symbol: vec![0u8; 10],
            decimals: 12,
        })));
        // transfer back to account(0); deposit moves with ownership
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer_ownership {
            id: Compact(1u32),
            owner: lookup(0),
        })));
        seeds.push(("assets_transfer_owner_should_work".to_string(), data));
    }

    // =========================================================================
    // repeated_transfer_ownership_and_set_metadata_preserves_reserved_balances
    //
    // Multiple ownership transfers with metadata updates to stress the
    // repatriate_reserved logic.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(12u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer_ownership {
            id: Compact(12u32),
            owner: lookup(1),
        })));
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::set_metadata {
            id: Compact(12u32),
            name: vec![0u8; 10],
            symbol: vec![0u8; 10],
            decimals: 12,
        })));
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer_ownership {
            id: Compact(12u32),
            owner: lookup(0),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer_ownership {
            id: Compact(12u32),
            owner: lookup(1),
        })));
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::set_metadata {
            id: Compact(12u32),
            name: vec![0u8; 10],
            symbol: vec![0u8; 10],
            decimals: 12,
        })));
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer_ownership {
            id: Compact(12u32),
            owner: lookup(0),
        })));
        seeds.push(("assets_repeated_ownership_and_metadata".to_string(), data));
    }

    // =========================================================================
    // set_team_should_work
    //
    // create → set_team (issuer=1, admin=2, freezer=3) → mint via issuer →
    // freeze via freezer → thaw via admin → force_transfer via admin → burn via admin.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        // issuer=1, admin=2, freezer=3
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::set_team {
            id: Compact(1u32),
            issuer: lookup(1),
            admin: lookup(2),
            freezer: lookup(3),
        })));
        // account(1) as issuer mints to itself
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(1),
            amount: 100u128,
        })));
        // account(3) as freezer freezes account(1)
        data.extend(enc(false, 3, RuntimeCall::Assets(pallet_assets::Call::freeze {
            id: Compact(1u32),
            who: lookup(1),
        })));
        // account(2) as admin thaws account(1)
        data.extend(enc(false, 2, RuntimeCall::Assets(pallet_assets::Call::thaw {
            id: Compact(1u32),
            who: lookup(1),
        })));
        // admin force-transfers from account(1) to account(2)
        data.extend(enc(false, 2, RuntimeCall::Assets(pallet_assets::Call::force_transfer {
            id: Compact(1u32),
            source: lookup(1),
            dest: lookup(2),
            amount: 100u128,
        })));
        // admin burns account(2)'s balance
        data.extend(enc(false, 2, RuntimeCall::Assets(pallet_assets::Call::burn {
            id: Compact(1u32),
            who: lookup(2),
            amount: 100u128,
        })));
        seeds.push(("assets_set_team_should_work".to_string(), data));
    }

    // =========================================================================
    // querying_roles_should_work
    //
    // set_team then verify (read-only assertions don't translate, but setting up
    // the state is the valuable part).
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::set_team {
            id: Compact(1u32),
            issuer: lookup(1),
            admin: lookup(2),
            freezer: lookup(3),
        })));
        seeds.push(("assets_querying_roles_should_work".to_string(), data));
    }

    // =========================================================================
    // set_metadata_should_work
    //
    // Various metadata operations: set, update (different lengths), clear.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        // initial set
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::set_metadata {
            id: Compact(1u32),
            name: vec![0u8; 10],
            symbol: vec![0u8; 10],
            decimals: 12,
        })));
        // update – shorter symbol (deposit partially refunded)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::set_metadata {
            id: Compact(1u32),
            name: vec![0u8; 10],
            symbol: vec![0u8; 5],
            decimals: 12,
        })));
        // update – longer symbol (extra deposit taken)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::set_metadata {
            id: Compact(1u32),
            name: vec![0u8; 10],
            symbol: vec![0u8; 15],
            decimals: 12,
        })));
        // clear
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::clear_metadata {
            id: Compact(1u32),
        })));
        seeds.push(("assets_set_metadata_should_work".to_string(), data));
    }

    // =========================================================================
    // set_min_balance_should_work
    //
    // Changing min_balance is restricted when there are holders or when the
    // asset is sufficient.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(42u32),
            admin: lookup(0),
            min_balance: 30u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(42u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        // fails: holders exist and asset is non-sufficient but new val > old val
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::set_min_balance {
            id: Compact(42u32),
            min_balance: 50u128,
        })));
        // burn all holdings so accounts = 0
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::burn {
            id: Compact(42u32),
            who: lookup(0),
            amount: u128::MAX,
        })));
        // now succeeds: no holders
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::set_min_balance {
            id: Compact(42u32),
            min_balance: 50u128,
        })));
        seeds.push(("assets_set_min_balance_should_work".to_string(), data));
    }

    // =========================================================================
    // minting_insufficient_asset_with_deposit_should_work_when_consumers_exhausted
    //
    // For a non-sufficient asset, `touch` reserves a deposit and enables minting.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        // touch creates a deposit-backed account entry
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::touch {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(1),
            amount: 100u128,
        })));
        seeds.push(("assets_minting_with_touch_deposit".to_string(), data));
    }

    // =========================================================================
    // refunding_asset_deposit_with_burn_should_work
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::touch {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(1),
            amount: 100u128,
        })));
        // allow_burn = true: balance is burned and deposit returned
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::refund {
            id: Compact(1u32),
            allow_burn: true,
        })));
        seeds.push(("assets_refunding_deposit_with_burn".to_string(), data));
    }

    // =========================================================================
    // refunding_asset_deposit_without_burn_should_work
    //
    // Transfer all balance away first, then refund deposit without burning.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::touch {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(1),
            amount: 100u128,
        })));
        // transfer all balance out of account(1)
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(2),
            amount: 100u128,
        })));
        // refund without burning (balance is already 0)
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::refund {
            id: Compact(1u32),
            allow_burn: false,
        })));
        seeds.push(("assets_refunding_deposit_without_burn".to_string(), data));
    }

    // =========================================================================
    // refunding_frozen_with_consumer_ref_works
    //
    // A frozen, non-sufficient account whose entry was created by a consumer
    // reference (transfer) can still be refunded.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        // transfer creates a consumer-ref entry for account(1)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 50u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::freeze {
            id: Compact(1u32),
            who: lookup(1),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::freeze_asset {
            id: Compact(1u32),
        })));
        // refund with burn even though account and asset are frozen
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::refund {
            id: Compact(1u32),
            allow_burn: true,
        })));
        seeds.push(("assets_refunding_frozen_consumer_ref".to_string(), data));
    }

    // =========================================================================
    // refunding_frozen_with_deposit_works
    //
    // Deposit-backed frozen account can be refunded with burn.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::touch {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 50u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::freeze {
            id: Compact(1u32),
            who: lookup(1),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::freeze_asset {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::refund {
            id: Compact(1u32),
            allow_burn: true,
        })));
        seeds.push(("assets_refunding_frozen_with_deposit".to_string(), data));
    }

    // =========================================================================
    // account_with_deposit_not_destroyed
    //
    // Touch creates a deposit-backed entry; the account survives zero balance.
    // refund_other cleans up a depositor-created entry.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        // case 1: account(1) holds its own deposit via touch
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::touch {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 50u128,
        })));
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(0),
            amount: 50u128,
        })));
        // account(1) at zero balance but account entry persists (deposit)
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::refund {
            id: Compact(1u32),
            allow_burn: false,
        })));
        // case 2: admin creates entry for account(1) with touch_other
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::touch_other {
            id: Compact(1u32),
            who: lookup(1),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 50u128,
        })));
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(0),
            amount: 50u128,
        })));
        seeds.push(("assets_account_with_deposit_not_destroyed".to_string(), data));
    }

    // =========================================================================
    // refund_other_works
    //
    // Admin and freezer can each create a deposit-backed entry and then refund it.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        // set freezer = account(1), admin = account(2), issuer = account(0)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::set_team {
            id: Compact(1u32),
            issuer: lookup(0),
            admin: lookup(0),
            freezer: lookup(1),
        })));
        // freezer creates deposit-backed entry for account(2)
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::touch_other {
            id: Compact(1u32),
            who: lookup(2),
        })));
        // freezer refunds the deposit
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::refund_other {
            id: Compact(1u32),
            who: lookup(2),
        })));
        // admin creates deposit-backed entry for account(2)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::touch_other {
            id: Compact(1u32),
            who: lookup(2),
        })));
        // admin refunds the deposit
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::refund_other {
            id: Compact(1u32),
            who: lookup(2),
        })));
        seeds.push(("assets_refund_other_works".to_string(), data));
    }

    // =========================================================================
    // touch_other_and_freeze_works
    //
    // touch_other creates an entry for a zero-balance account, which can then
    // be frozen.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        // admin creates account(1) with touch_other
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::touch_other {
            id: Compact(1u32),
            who: lookup(1),
        })));
        // freeze the zero-balance account
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::freeze {
            id: Compact(1u32),
            who: lookup(1),
        })));
        // can transfer TO frozen account
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 50u128,
        })));
        // cannot transfer FROM frozen – will fail
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(0),
            amount: 25u128,
        })));
        seeds.push(("assets_touch_other_and_freeze_works".to_string(), data));
    }

    // =========================================================================
    // touching_and_freezing_account_with_zero_asset_balance_should_work
    //
    // Self-touch on zero-balance account, then freeze, then transfer.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        // account(1) touches to create its own entry (deposit)
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::touch {
            id: Compact(1u32),
        })));
        // admin freezes account(1)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::freeze {
            id: Compact(1u32),
            who: lookup(1),
        })));
        // can transfer TO frozen account(1)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 50u128,
        })));
        // cannot transfer FROM frozen account(1) – will fail
        data.extend(enc(false, 1, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(0),
            amount: 25u128,
        })));
        seeds.push(("assets_touching_and_freezing_zero_balance".to_string(), data));
    }

    // =========================================================================
    // transfer_all_works_1 – transfer_all allow_death
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 200u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(1),
            amount: 100u128,
        })));
        // allow_death=false → transfer all, account(0) reaped
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer_all {
            id: Compact(1u32),
            dest: lookup(1),
            keep_alive: false,
        })));
        seeds.push(("assets_transfer_all_allow_death".to_string(), data));
    }

    // =========================================================================
    // transfer_all_works_2 – transfer_all keep_alive
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 200u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(1),
            amount: 100u128,
        })));
        // keep_alive=true → leaves min_balance in account(0)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer_all {
            id: Compact(1u32),
            dest: lookup(1),
            keep_alive: true,
        })));
        seeds.push(("assets_transfer_all_keep_alive".to_string(), data));
    }

    // =========================================================================
    // transfer_large_asset
    //
    // Mint and transfer values close to u64::MAX (the test used u64 but our
    // Balance is u128; large compact values exercise codec edge cases).
    // =========================================================================
    {
        let large: u128 = (u64::pow(2, 63) + 2) as u128;
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: large,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: large - 1,
        })));
        seeds.push(("assets_transfer_large_asset".to_string(), data));
    }

    // =========================================================================
    // burning_asset_balance_with_positive_balance_should_work
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::burn {
            id: Compact(1u32),
            who: lookup(0),
            amount: u128::MAX,
        })));
        seeds.push(("assets_burning_with_positive_balance".to_string(), data));
    }

    // =========================================================================
    // non_providing_should_work
    //
    // Non-sufficient asset: minting to / transferring to a new account fails
    // unless that account already exists (has Balances).
    // In the fuzzer genesis all accounts 0–4 have balance, so transfers to
    // accounts 0–4 succeed; the "cannot create" failures are exercised with
    // the assert_noop! paths from the original test.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        // mint to account(0) (has balance in genesis)
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        // transfer to account(1) – succeeds because account(1) has native balance
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::transfer {
            id: Compact(1u32),
            target: lookup(1),
            amount: 25u128,
        })));
        // force_transfer to account(2) – succeeds for the same reason
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::force_transfer {
            id: Compact(1u32),
            source: lookup(0),
            dest: lookup(2),
            amount: 25u128,
        })));
        seeds.push(("assets_non_providing_should_work".to_string(), data));
    }

    // =========================================================================
    // approve_transfer_frozen_asset_should_not_work
    //
    // approve_transfer on a frozen asset fails; after thaw it succeeds.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::mint {
            id: Compact(1u32),
            beneficiary: lookup(0),
            amount: 100u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::freeze_asset {
            id: Compact(1u32),
        })));
        // fails: AssetNotLive
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::approve_transfer {
            id: Compact(1u32),
            delegate: lookup(1),
            amount: 50u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::thaw_asset {
            id: Compact(1u32),
        })));
        // now succeeds
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::approve_transfer {
            id: Compact(1u32),
            delegate: lookup(1),
            amount: 50u128,
        })));
        seeds.push(("assets_approve_transfer_frozen_asset".to_string(), data));
    }

    // =========================================================================
    // asset_id_cannot_be_reused (with auto-increment disabled)
    //
    // create → full destroy → recreate same id (works when auto-increment is off).
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::start_destroy {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::finish_destroy {
            id: Compact(1u32),
        })));
        // recreate same id
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(1u32),
            admin: lookup(0),
            min_balance: 1u128,
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::start_destroy {
            id: Compact(1u32),
        })));
        data.extend(enc(false, 0, RuntimeCall::Assets(pallet_assets::Call::finish_destroy {
            id: Compact(1u32),
        })));
        seeds.push(("assets_asset_id_reuse".to_string(), data));
    }

    seeds
}
