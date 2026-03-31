#![warn(clippy::pedantic)]
#![allow(clippy::too_many_lines)]

//! Seed generator derived from `substrate/frame/multisig/src/tests.rs`.
//!
//! Each seed encodes one test scenario as a sequence of SCALE-encoded
//! `(bool, u8, RuntimeCall)` tuples for the kitchensink fuzzer.
//!
//! ## Key translation notes
//!
//! ### Account mapping
//! The test genesis gives u64 accounts small balances (`[1..5 → 10,10,10,5,2]`).
//! Fuzzer accounts all have `ENDOWMENT = 10_000_000 DOLLARS`, so any deposit fits.
//!
//! | Test account | Fuzzer origin | Fuzzer `AccountId32` |
//! |---|---|---|
//! | 1 | 0 | [0;32] |
//! | 2 | 1 | [1;32] |
//! | 3 | 2 | [2;32] |
//! | 4 | 3 | [3;32] |
//! | 6 (recipient) | 3 | [3;32] |
//! | 7 (recipient) | 4 | [4;32] |
//!
//! ### Timepoint
//! The fuzzer dispatches calls via `extrinsic.dispatch(...)` directly, bypassing
//! `Executive::apply_extrinsic`.  Therefore `System::extrinsic_index()` always
//! returns `None → 0`, meaning **every call in the same block stores / reads the
//! same timepoint `{ height: 1, index: 0 }`**.  All follow-up approval calls must
//! supply `Some(Timepoint { height: 1, index: 0 })`.
//!
//! ### Multisig account
//! Computed from the sorted signatory list using the same formula as the pallet:
//! `blake2_256(("modlpy/utilisuba", sorted_signatories, threshold).encode())`.
//! This is a pure computation – no externalities required.
//!
//! ### `max_weight`
//! The final `as_multi` that actually *executes* the inner call needs a non-zero
//! `max_weight`.  We use `Weight::from_parts(1_000_000_000, 100_000)` (~1 ms),
//! which is generous for a balance transfer yet far below the 2-second block limit.
//! Intermediate approvals use `Weight::zero()`.
//!
//! ### Inner call
//! `Balances::transfer_allow_death(dest = account(3), value = 1 DOLLAR)`
//! dispatched from the multisig account.  The multisig account is funded beforehand
//! by transferring 5 DOLLARS from account(0).

use codec::Encode;
use frame_support::weights::Weight;
use kitchensink_runtime::RuntimeCall;
use node_primitives::AccountIndex;
use pallet_multisig::Timepoint;
use sp_core::hashing::blake2_256;
use sp_runtime::MultiAddress;
use std::{fs, path::Path};

// ─── constants ───────────────────────────────────────────────────────────────

/// DOLLARS = 100 * CENTS = 100 * `1_000` * MILLICENTS = 100 * `1_000` * `1_000_000_000`
const DOLLARS: u128 = 100 * 1_000 * 1_000_000_000;

// ─── helpers ─────────────────────────────────────────────────────────────────

type AccountId = sp_runtime::AccountId32;

fn account(idx: u8) -> AccountId {
    [idx; 32].into()
}

fn lookup(idx: u8) -> MultiAddress<AccountId, AccountIndex> {
    MultiAddress::Id(account(idx))
}

/// Encode one extrinsic tuple `(advance_block, origin, call)`.
fn enc(advance_block: bool, origin: u8, call: RuntimeCall) -> Vec<u8> {
    (advance_block, origin, call).encode()
}

/// Compute the multisig composite account ID for a sorted list of signatories
/// at a given threshold.  Replicates `pallet_multisig::Pallet::multi_account_id`
/// using `sp_core::hashing::blake2_256` (pure Rust, no externalities needed).
fn multi_account_id(signatories: &[AccountId], threshold: u16) -> AccountId {
    let entropy = (b"modlpy/utilisuba", signatories, threshold).using_encoded(blake2_256);
    // AccountId32 is exactly 32 bytes; blake2_256 returns exactly 32 bytes.
    sp_runtime::AccountId32::new(entropy)
}

/// The three fuzzer accounts that act as signatories, in ascending byte order.
/// They sort as [0;32] < [1;32] < [2;32], satisfying the `ensure_sorted` check.
fn signatories_3() -> [AccountId; 3] {
    [account(0), account(1), account(2)]
}

/// Standard inner call: transfer 1 DOLLAR from the multisig account to account(3).
fn inner_transfer_call() -> Box<RuntimeCall> {
    Box::new(RuntimeCall::Balances(
        pallet_balances::Call::transfer_allow_death {
            dest: lookup(3),
            value: DOLLARS,
        },
    ))
}

/// SCALE-encode the inner call and return its `blake2_256` hash.
fn inner_transfer_hash() -> [u8; 32] {
    blake2_256(&inner_transfer_call().encode())
}

/// Timepoint recorded when the first approval lands in the fuzzer (block 1, index 0).
/// Since the fuzzer bypasses `Executive::apply_extrinsic`, `extrinsic_index()` is
/// always `None → 0` for every call in the block.
fn tp1() -> Timepoint<u32> {
    Timepoint {
        height: 1,
        index: 0,
    }
}

/// Large-enough `max_weight` for the executing `as_multi` call.
fn exec_weight() -> Weight {
    Weight::from_parts(1_000_000_000, 100_000)
}

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

#[allow(clippy::cast_lossless)]
fn build_seeds() -> Vec<(String, Vec<u8>)> {
    let mut seeds: Vec<(String, Vec<u8>)> = Vec::new();

    // Pre-compute addresses and hashes used in multiple seeds.
    let sigs3 = signatories_3();
    let multi_2of3 = multi_account_id(&sigs3, 2);
    let multi_3of3 = multi_account_id(&sigs3, 3);
    let multi_1of3 = multi_account_id(&sigs3, 1);
    let call_hash = inner_transfer_hash();

    // Helper: fund the multisig account from account(0).
    let fund_multi = |multi: &AccountId| -> Vec<u8> {
        enc(
            false,
            0,
            RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
                dest: MultiAddress::Id(multi.clone()),
                value: 5 * DOLLARS,
            }),
        )
    };

    // =========================================================================
    // multisig_deposit_is_taken_and_returned
    //
    // 2-of-3 using as_multi for both steps: sig0 proposes (no tp) → sig1 executes
    // (with tp + weight).  Deposit is taken from sig0 on the first call and
    // returned once execution completes.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(fund_multi(&multi_2of3));

        // sig0: first approval (no timepoint), stores the call
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call: inner_transfer_call(),
                max_weight: Weight::zero(),
            }),
        ));

        // sig1: second approval, threshold met → executes
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: Some(tp1()),
                call: inner_transfer_call(),
                max_weight: exec_weight(),
            }),
        ));

        seeds.push(("multisig_deposit_taken_and_returned".to_string(), data));
    }

    // =========================================================================
    // multisig_2_of_3_works  (approve×2 then as_multi execute)
    //
    // sig0 and sig1 submit hash-only approvals; sig2 provides the full call
    // and triggers execution.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(fund_multi(&multi_2of3));

        // sig0: first approval (hash-only)
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // sig1: second approval (hash-only, timepoint required)
        // Even though threshold is now met, approve_as_multi never executes.
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: Some(tp1()),
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // sig2: supplies call data and executes
        data.extend(enc(
            false,
            2,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(1)],
                maybe_timepoint: Some(tp1()),
                call: inner_transfer_call(),
                max_weight: exec_weight(),
            }),
        ));

        seeds.push(("multisig_2_of_3_works".to_string(), data));
    }

    // =========================================================================
    // multisig_2_of_3_as_multi_works
    //
    // Both participants use as_multi (full call), no hash-only step.
    // sig0 first → sig1 executes.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(fund_multi(&multi_2of3));

        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call: inner_transfer_call(),
                max_weight: Weight::zero(),
            }),
        ));

        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: Some(tp1()),
                call: inner_transfer_call(),
                max_weight: exec_weight(),
            }),
        ));

        seeds.push(("multisig_2_of_3_as_multi_works".to_string(), data));
    }

    // =========================================================================
    // multisig_3_of_3_works
    //
    // All three signatories must approve before execution:
    // sig0 approve → sig1 approve → sig2 as_multi execute.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(fund_multi(&multi_3of3));

        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 3,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 3,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: Some(tp1()),
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        data.extend(enc(
            false,
            2,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 3,
                other_signatories: vec![account(0), account(1)],
                maybe_timepoint: Some(tp1()),
                call: inner_transfer_call(),
                max_weight: exec_weight(),
            }),
        ));

        seeds.push(("multisig_3_of_3_works".to_string(), data));
    }

    // =========================================================================
    // multisig_handles_no_preimage_after_all_approve
    //
    // All three signatories use hash-only approvals; then any one of them
    // re-submits with the full call data to trigger execution.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(fund_multi(&multi_3of3));

        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 3,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 3,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: Some(tp1()),
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        data.extend(enc(
            false,
            2,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 3,
                other_signatories: vec![account(0), account(1)],
                maybe_timepoint: Some(tp1()),
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // sig2 re-submits with full call to execute
        data.extend(enc(
            false,
            2,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 3,
                other_signatories: vec![account(0), account(1)],
                maybe_timepoint: Some(tp1()),
                call: inner_transfer_call(),
                max_weight: exec_weight(),
            }),
        ));

        seeds.push(("multisig_handles_no_preimage".to_string(), data));
    }

    // =========================================================================
    // cancel_multisig_returns_deposit  (3-of-3, two approvals, then cancel)
    //
    // Only the original depositor (sig0) can cancel.
    // =========================================================================
    {
        let mut data = Vec::new();

        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 3,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 3,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: Some(tp1()),
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // sig1 tries to cancel → fails (NotOwner)
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::cancel_as_multi {
                threshold: 3,
                other_signatories: vec![account(0), account(2)],
                timepoint: tp1(),
                call_hash,
            }),
        ));

        // sig0 (depositor) cancels → succeeds, deposit returned
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::cancel_as_multi {
                threshold: 3,
                other_signatories: vec![account(1), account(2)],
                timepoint: tp1(),
                call_hash,
            }),
        ));

        seeds.push(("multisig_cancel_returns_deposit".to_string(), data));
    }

    // =========================================================================
    // cancel_multisig_works  (2-of-3)
    // =========================================================================
    {
        let mut data = Vec::new();

        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: Some(tp1()),
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::cancel_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                timepoint: tp1(),
                call_hash,
            }),
        ));

        seeds.push(("multisig_cancel_works".to_string(), data));
    }

    // =========================================================================
    // multisig_1_of_3_works  (as_multi_threshold_1)
    //
    // Threshold-1 multisig executes immediately; no deposit, no prior approvals.
    // The multisig account must have balance for the inner transfer.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(fund_multi(&multi_1of3));

        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi_threshold_1 {
                other_signatories: vec![account(1), account(2)],
                call: inner_transfer_call(),
            }),
        ));

        seeds.push(("multisig_1_of_3_threshold_1".to_string(), data));
    }

    // =========================================================================
    // multisig_2_of_3_as_multi_with_many_calls_works
    //
    // Two independent 2-of-3 multisig operations run concurrently:
    //   call_a → sig0 proposes; call_b → sig1 proposes; sig2 executes both.
    //
    // Since extrinsic_index is always 0 in the fuzzer, both operations record
    // timepoint {1, 0}; they are distinguished by their different call hashes.
    // =========================================================================
    {
        // Two different inner calls
        let call_a: Box<RuntimeCall> = Box::new(RuntimeCall::Balances(
            pallet_balances::Call::transfer_allow_death {
                dest: lookup(3),
                value: DOLLARS,
            },
        ));
        let call_b: Box<RuntimeCall> = Box::new(RuntimeCall::Balances(
            pallet_balances::Call::transfer_allow_death {
                dest: lookup(4),
                value: 2 * DOLLARS,
            },
        ));
        let hash_a = blake2_256(&call_a.encode());
        let hash_b = blake2_256(&call_b.encode());

        let mut data = Vec::new();
        // Fund the 2-of-3 multisig for both transfers
        data.extend(enc(
            false,
            0,
            RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
                dest: MultiAddress::Id(multi_2of3.clone()),
                value: 10 * DOLLARS,
            }),
        ));

        // sig0 proposes call_a (no timepoint)
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call: call_a.clone(),
                max_weight: Weight::zero(),
            }),
        ));

        // sig1 proposes call_b (no timepoint – independent multisig operation)
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: None,
                call: call_b.clone(),
                max_weight: Weight::zero(),
            }),
        ));

        // sig2 executes call_a (stored under hash_a, timepoint {1,0})
        data.extend(enc(
            false,
            2,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(1)],
                maybe_timepoint: Some(tp1()),
                call: call_a,
                max_weight: exec_weight(),
            }),
        ));

        // sig2 executes call_b (stored under hash_b, same timepoint {1,0})
        data.extend(enc(
            false,
            2,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(1)],
                maybe_timepoint: Some(tp1()),
                call: call_b,
                max_weight: exec_weight(),
            }),
        ));

        // suppress unused variable warnings (hash_a/hash_b used implicitly above)
        let _ = (hash_a, hash_b);

        seeds.push(("multisig_many_calls_works".to_string(), data));
    }

    // =========================================================================
    // multisig_2_of_3_cannot_reissue_same_call
    //
    // After sig0→sig1 execute, the multisig account is drained.  A second
    // identical multisig completes but the inner transfer fails (funds unavailable).
    // =========================================================================
    {
        let call_small: Box<RuntimeCall> = Box::new(RuntimeCall::Balances(
            pallet_balances::Call::transfer_allow_death {
                dest: lookup(3),
                value: DOLLARS / 2,
            },
        ));
        let hash_small = blake2_256(&call_small.encode());

        let mut data = Vec::new();
        // Fund with exactly enough for one execution
        data.extend(enc(
            false,
            0,
            RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
                dest: MultiAddress::Id(multi_2of3.clone()),
                value: DOLLARS / 2,
            }),
        ));

        // First execution: sig0 proposes
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call: call_small.clone(),
                max_weight: Weight::zero(),
            }),
        ));

        // sig1 executes – succeeds, funds consumed
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: Some(tp1()),
                call: call_small.clone(),
                max_weight: exec_weight(),
            }),
        ));

        // Second attempt: sig0 proposes again (no timepoint → new multisig entry)
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call: call_small.clone(),
                max_weight: Weight::zero(),
            }),
        ));

        // sig2 executes – inner transfer fails (FundsUnavailable) but outer Ok
        data.extend(enc(
            false,
            2,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(1)],
                maybe_timepoint: Some(tp1()),
                call: call_small,
                max_weight: exec_weight(),
            }),
        ));

        let _ = hash_small;
        seeds.push(("multisig_cannot_reissue_same_call".to_string(), data));
    }

    // =========================================================================
    // duplicate_approvals_are_ignored
    //
    // Approving the same multisig twice from the same signatory returns
    // AlreadyApproved; as_multi from the same signatory also returns
    // AlreadyApproved.  A fresh signatory's approval succeeds.
    // =========================================================================
    {
        let mut data = Vec::new();

        // sig0: first approval
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // sig0: duplicate approve → AlreadyApproved (error, still interesting seed)
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: Some(tp1()),
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // sig0: duplicate via as_multi → AlreadyApproved
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: Some(tp1()),
                call: inner_transfer_call(),
                max_weight: Weight::zero(),
            }),
        ));

        // sig1: new approval → succeeds (threshold met; does not execute via approve_as_multi)
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: Some(tp1()),
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // sig2: also tries to approve → AlreadyApproved (threshold already met)
        data.extend(enc(
            false,
            2,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(1)],
                maybe_timepoint: Some(tp1()),
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        seeds.push(("multisig_duplicate_approvals_ignored".to_string(), data));
    }

    // =========================================================================
    // timepoint_checking_works
    //
    // Various timepoint violations (UnexpectedTimepoint, NoTimepoint,
    // WrongTimepoint) followed by the correct sequence.
    // =========================================================================
    {
        let wrong_tp = Timepoint {
            height: 99u32,
            index: 0u32,
        };

        let mut data = Vec::new();
        data.extend(fund_multi(&multi_2of3));

        // sig1: second-approval without a matching first → UnexpectedTimepoint
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: Some(tp1()),
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // sig0: first approval (no timepoint) → OK
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // sig1: as_multi with None timepoint → NoTimepoint
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: None,
                call: inner_transfer_call(),
                max_weight: Weight::zero(),
            }),
        ));

        // sig1: as_multi with wrong timepoint → WrongTimepoint
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: Some(wrong_tp),
                call: inner_transfer_call(),
                max_weight: Weight::zero(),
            }),
        ));

        // sig1: correct sequence → executes
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: Some(tp1()),
                call: inner_transfer_call(),
                max_weight: exec_weight(),
            }),
        ));

        seeds.push(("multisig_timepoint_checking_works".to_string(), data));
    }

    // =========================================================================
    // weight_check_works
    //
    // When max_weight is too low (Weight::zero()) on the *final* as_multi call,
    // the call returns MaxWeightTooLow.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(fund_multi(&multi_2of3));

        // sig0: first approval
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call: inner_transfer_call(),
                max_weight: Weight::zero(),
            }),
        ));

        // sig1: second approval with max_weight=0 → MaxWeightTooLow (error path)
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: Some(tp1()),
                call: inner_transfer_call(),
                max_weight: Weight::zero(),
            }),
        ));

        // sig1: retry with adequate weight → executes
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: Some(tp1()),
                call: inner_transfer_call(),
                max_weight: exec_weight(),
            }),
        ));

        seeds.push(("multisig_weight_check_works".to_string(), data));
    }

    // =========================================================================
    // cancel_as_multi_not_found_with_wrong_signatories / wrong_call_hash
    //
    // Two error paths for cancel: wrong signatories → NotFound; wrong hash →
    // NotFound.  Then the correct cancel succeeds.
    // =========================================================================
    {
        let wrong_hash = [0u8; 32];
        let mut data = Vec::new();

        // Create a 2-of-3 multisig
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // cancel with wrong signatories → NotFound
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::cancel_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(3)], // wrong
                timepoint: tp1(),
                call_hash,
            }),
        ));

        // cancel with wrong hash → NotFound
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::cancel_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                timepoint: tp1(),
                call_hash: wrong_hash,
            }),
        ));

        // correct cancel → succeeds
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::cancel_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                timepoint: tp1(),
                call_hash,
            }),
        ));

        seeds.push(("multisig_cancel_error_paths".to_string(), data));
    }

    // =========================================================================
    // approve_as_multi_requires_timepoint_after_first_approval
    //
    // Second approval with None timepoint → NoTimepoint; with correct timepoint
    // → succeeds.
    // =========================================================================
    {
        let mut data = Vec::new();

        // sig0: first approval (no timepoint)
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // sig1: second approval without timepoint → NoTimepoint
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // sig1: with correct timepoint → succeeds
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                maybe_timepoint: Some(tp1()),
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        seeds.push(("multisig_timepoint_required_after_first".to_string(), data));
    }

    // =========================================================================
    // as_multi_with_signatories_out_of_order_fails
    // approve_as_multi_with_signatories_out_of_order_fails
    // cancel_as_multi_with_signatories_out_of_order_fails
    //
    // Out-of-order signatories → SignatoriesOutOfOrder, then correct order.
    // =========================================================================
    {
        let mut data = Vec::new();

        // as_multi out-of-order → fails
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(2), account(1)], // reversed
                maybe_timepoint: None,
                call: inner_transfer_call(),
                max_weight: Weight::zero(),
            }),
        ));

        // approve_as_multi out-of-order → fails
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(2), account(1)], // reversed
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // correct order → succeeds (first approval)
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // cancel with out-of-order → SignatoriesOutOfOrder
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::cancel_as_multi {
                threshold: 2,
                other_signatories: vec![account(2), account(1)], // reversed
                timepoint: tp1(),
                call_hash,
            }),
        ));

        // correct cancel → succeeds
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::cancel_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                timepoint: tp1(),
                call_hash,
            }),
        ));

        seeds.push(("multisig_signatories_out_of_order".to_string(), data));
    }

    // =========================================================================
    // as_multi_with_sender_in_signatories_fails
    // approve_as_multi_with_sender_in_signatories_fails
    // cancel_as_multi_with_sender_in_signatories_fails
    //
    // Including the sender in the other_signatories list → SenderInSignatories.
    // =========================================================================
    {
        let mut data = Vec::new();

        // as_multi: sender (account(0)) in signatories → fails
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(1)], // sender account(0) included
                maybe_timepoint: None,
                call: inner_transfer_call(),
                max_weight: Weight::zero(),
            }),
        ));

        // correct first approval by sig0
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // sig1 includes themselves → SenderInSignatories
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(1)], // sender account(1) included
                maybe_timepoint: Some(tp1()),
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // sig0 tries to cancel including themselves → SenderInSignatories
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::cancel_as_multi {
                threshold: 2,
                other_signatories: vec![account(0), account(2)], // sender included
                timepoint: tp1(),
                call_hash,
            }),
        ));

        // correct cancel
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::cancel_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                timepoint: tp1(),
                call_hash,
            }),
        ));

        seeds.push(("multisig_sender_in_signatories_fails".to_string(), data));
    }

    // =========================================================================
    // minimum_threshold_check_works  (threshold 0 and 1 both fail)
    // too_many_signatories_fails
    // =========================================================================
    {
        let mut data = Vec::new();

        // threshold = 0 → MinimumThreshold
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 0,
                other_signatories: vec![account(1)],
                maybe_timepoint: None,
                call: inner_transfer_call(),
                max_weight: Weight::zero(),
            }),
        ));

        // threshold = 1 with others → MinimumThreshold
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 1,
                other_signatories: vec![account(1)],
                maybe_timepoint: None,
                call: inner_transfer_call(),
                max_weight: Weight::zero(),
            }),
        ));

        // too many signatories (MaxSignatories = 100, use 3 which is fine; this
        // is a fuzz-valuable call even though it won't hit the limit with 3 accts)
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call: inner_transfer_call(),
                max_weight: Weight::zero(),
            }),
        ));

        seeds.push(("multisig_threshold_and_signatory_checks".to_string(), data));
    }

    // =========================================================================
    // poke_deposit_fails_for_non_existent_multisig / poke_deposit_works
    //
    // poke_deposit on a non-existent multisig → NotFound.
    // poke_deposit on an existing multisig → Pays::Yes (deposit unchanged).
    // =========================================================================
    {
        let mut data = Vec::new();

        // poke non-existent → NotFound
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::poke_deposit {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                call_hash: [0u8; 32],
            }),
        ));

        // create a multisig
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // poke existing multisig (deposit unchanged → Pays::Yes)
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::poke_deposit {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                call_hash,
            }),
        ));

        seeds.push(("multisig_poke_deposit".to_string(), data));
    }

    // =========================================================================
    // poke_deposit_fails_for_non_owner
    //
    // Only the original depositor can poke.
    // =========================================================================
    {
        let mut data = Vec::new();

        // sig0 creates the multisig
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // sig1 tries to poke → NotOwner (note: sig1's other_signatories computed from sig1's POV)
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::poke_deposit {
                threshold: 2,
                other_signatories: vec![account(0), account(2)],
                call_hash,
            }),
        ));

        // sig0 pokes → succeeds
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::poke_deposit {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                call_hash,
            }),
        ));

        seeds.push(("multisig_poke_deposit_non_owner".to_string(), data));
    }

    // =========================================================================
    // Bonus: 2-of-2 multisig (sig0 + sig1 only)
    //
    // Exercises a different signatory set and threshold combination.
    // =========================================================================
    {
        let sigs2 = [account(0), account(1)];
        let multi_2of2 = multi_account_id(&sigs2, 2);

        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
                dest: MultiAddress::Id(multi_2of2),
                value: 5 * DOLLARS,
            }),
        ));

        // sig0: first approval
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(1)],
                maybe_timepoint: None,
                call: inner_transfer_call(),
                max_weight: Weight::zero(),
            }),
        ));

        // sig1: second approval → executes
        data.extend(enc(
            false,
            1,
            RuntimeCall::Multisig(pallet_multisig::Call::as_multi {
                threshold: 2,
                other_signatories: vec![account(0)],
                maybe_timepoint: Some(tp1()),
                call: inner_transfer_call(),
                max_weight: exec_weight(),
            }),
        ));

        seeds.push(("multisig_2_of_2_works".to_string(), data));
    }

    // =========================================================================
    // Bonus: poke_deposit_fails_for_signatories_out_of_order
    // =========================================================================
    {
        let mut data = Vec::new();

        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::approve_as_multi {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                maybe_timepoint: None,
                call_hash,
                max_weight: Weight::zero(),
            }),
        ));

        // poke with out-of-order signatories → SignatoriesOutOfOrder
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::poke_deposit {
                threshold: 2,
                other_signatories: vec![account(2), account(1)], // wrong order
                call_hash,
            }),
        ));

        // correct order → succeeds
        data.extend(enc(
            false,
            0,
            RuntimeCall::Multisig(pallet_multisig::Call::poke_deposit {
                threshold: 2,
                other_signatories: vec![account(1), account(2)],
                call_hash,
            }),
        ));

        seeds.push(("multisig_poke_deposit_out_of_order".to_string(), data));
    }

    seeds
}
