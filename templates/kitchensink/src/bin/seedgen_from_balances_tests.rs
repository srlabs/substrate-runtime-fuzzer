#![warn(clippy::pedantic)]
#![allow(clippy::too_many_lines)]

//! Seed generator derived from `substrate/frame/balances/src/tests/dispatchable_tests.rs`
//! and related test files.
//!
//! ## Signed calls available
//! - `transfer_allow_death` – transfer, reap source if it falls below ED
//! - `transfer_keep_alive`  – transfer, error if source would fall below ED
//! - `transfer_all`         – transfer the entire reducible balance
//! - `burn`                 – destroy own free balance (reduces total issuance)
//! - `upgrade_accounts`     – upgrade account storage format
//!
//! Root-only calls (`force_transfer`, `force_set_balance`, `force_unreserve`,
//! `force_adjust_total_issuance`) are skipped – the fuzzer dispatches with signed
//! origins only.
//!
//! ## Genesis context
//! - All five accounts `[0;32]`…`[4;32]` start with `ENDOWMENT = 10_000_000 DOLLARS`.
//! - `ExistentialDeposit = 1 DOLLAR = 10^14`.
//! - Account(0) is a staking validator with `STASH = ENDOWMENT / 1000` bonded, so
//!   a small portion of its balance is frozen.  Seeds that intend to fully drain an
//!   account use account(1) which has no special roles.
//! - `burn` decreases `TotalIssuance`; the invariant check `total_issuance <=
//!   initial_total_issuance` is therefore always satisfied.

use codec::Encode;
use kitchensink_runtime::RuntimeCall;
use node_primitives::AccountIndex;
use sp_runtime::MultiAddress;
use std::{fs, path::Path};

// ─── constants ───────────────────────────────────────────────────────────────

const MILLICENTS: u128 = 1_000_000_000;
const CENTS: u128 = 1_000 * MILLICENTS;
const DOLLARS: u128 = 100 * CENTS; // 10^14
const ENDOWMENT: u128 = 10_000_000 * DOLLARS; // 10^21
const ED: u128 = DOLLARS; // ExistentialDeposit = 1 DOLLAR

// ─── helpers ─────────────────────────────────────────────────────────────────

type AccountId = sp_runtime::AccountId32;

fn account(idx: u8) -> AccountId {
    [idx; 32].into()
}

fn lookup(idx: u8) -> MultiAddress<AccountId, AccountIndex> {
    MultiAddress::Id(account(idx))
}

fn enc(advance_block: bool, origin: u8, call: RuntimeCall) -> Vec<u8> {
    (advance_block, origin, call).encode()
}

fn transfer(origin: u8, dest: u8, value: u128) -> Vec<u8> {
    enc(
        false,
        origin,
        RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
            dest: lookup(dest),
            value,
        }),
    )
}

fn transfer_keep_alive(origin: u8, dest: u8, value: u128) -> Vec<u8> {
    enc(
        false,
        origin,
        RuntimeCall::Balances(pallet_balances::Call::transfer_keep_alive {
            dest: lookup(dest),
            value,
        }),
    )
}

fn transfer_all(origin: u8, dest: u8, keep_alive: bool) -> Vec<u8> {
    enc(
        false,
        origin,
        RuntimeCall::Balances(pallet_balances::Call::transfer_all {
            dest: lookup(dest),
            keep_alive,
        }),
    )
}

fn burn(origin: u8, value: u128, keep_alive: bool) -> Vec<u8> {
    enc(
        false,
        origin,
        RuntimeCall::Balances(pallet_balances::Call::burn { value, keep_alive }),
    )
}

// ─── entry point ─────────────────────────────────────────────────────────────

fn main() {
    let out_dir = Path::new("seedgen");
    fs::create_dir_all(out_dir).expect("create seedgen dir");

    let seeds = build_seeds();
    for (name, data) in &seeds {
        fs::write(out_dir.join(format!("{name}.seed")), data).expect("write seed");
    }
    println!("wrote {} seeds to {}", seeds.len(), out_dir.display());
}

// ─── seeds ───────────────────────────────────────────────────────────────────

fn build_seeds() -> Vec<(String, Vec<u8>)> {
    let mut seeds: Vec<(String, Vec<u8>)> = Vec::new();

    // =========================================================================
    // balance_transfer_works
    //
    // Basic transfer: account(0) sends 1 DOLLAR to account(1).
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(transfer(0, 1, DOLLARS));
        seeds.push(("balances_transfer_allow_death_basic".into(), data));
    }

    // =========================================================================
    // transfer_allow_death – source reaped
    //
    // account(1) transfers all but a dust amount; since the remainder is below
    // the ED the source is reaped (transfer_allow_death sends everything).
    // Uses account(1) which has no staking/election locks.
    // =========================================================================
    {
        let mut data = Vec::new();
        // Leave 1 token behind – below ED (10^14) → source is reaped.
        data.extend(transfer(1, 0, ENDOWMENT - 1));
        seeds.push(("balances_transfer_allow_death_reaped".into(), data));
    }

    // =========================================================================
    // dust_account_removal_should_work  (from dispatchable_tests)
    //
    // Transfer enough so that the source drops below ED, then the account is
    // removed.  We also verify that a subsequent send from account(0) to the
    // already-rich account(1) works fine.
    // =========================================================================
    {
        let mut data = Vec::new();
        // account(2) → account(3): leave 1 token below ED → account(2) reaped
        data.extend(transfer(2, 3, ENDOWMENT - 1));
        // account(0) → account(1): normal transfer after (unrelated accounts)
        data.extend(transfer(0, 1, 10 * DOLLARS));
        seeds.push(("balances_dust_account_removal".into(), data));
    }

    // =========================================================================
    // transfer_allow_death – chain of transfers
    //
    // 0→1→2→3→4→0: exercises five distinct signer/destination pairs so the
    // fuzzer learns to route balances through all accounts.
    // =========================================================================
    {
        let amount = DOLLARS;
        let mut data = Vec::new();
        data.extend(transfer(0, 1, amount));
        data.extend(transfer(1, 2, amount));
        data.extend(transfer(2, 3, amount));
        data.extend(transfer(3, 4, amount));
        data.extend(transfer(4, 0, amount));
        seeds.push(("balances_transfer_chain".into(), data));
    }

    // =========================================================================
    // transfer_allow_death – bidirectional ping-pong
    //
    // Exercises the same pair repeatedly and verifies that total issuance
    // remains constant after each round-trip.
    // =========================================================================
    {
        let amount = 5 * DOLLARS;
        let mut data = Vec::new();
        for _ in 0..3 {
            data.extend(transfer(0, 1, amount));
            data.extend(transfer(1, 0, amount));
        }
        seeds.push(("balances_transfer_bidirectional".into(), data));
    }

    // =========================================================================
    // transfer_allow_death – large value (near ENDOWMENT)
    //
    // Exercises compact encoding for large u128 values and verifies that the
    // pallet correctly handles near-maximum balance transfers.
    // =========================================================================
    {
        let mut data = Vec::new();
        // Send 99% of ENDOWMENT; remainder (1%) is well above ED.
        let large = ENDOWMENT / 100 * 99;
        data.extend(transfer(0, 1, large));
        // Send it back so we exercise both directions with big amounts.
        data.extend(transfer(1, 0, large));
        seeds.push(("balances_transfer_large_value".into(), data));
    }

    // =========================================================================
    // transfer_allow_death – many small transfers (accumulation)
    //
    // Multiple small amounts flowing to a single recipient mirrors realistic
    // fee-collection or reward-distribution patterns.
    // =========================================================================
    {
        let small = CENTS; // 10^12 per transfer, well above 0
        let mut data = Vec::new();
        for src in 1u8..=4 {
            data.extend(transfer(src, 0, small));
        }
        seeds.push(("balances_transfer_many_to_one".into(), data));
    }

    // =========================================================================
    // transfer_allow_death – fan-out (one to many)
    // =========================================================================
    {
        let amount = 2 * DOLLARS;
        let mut data = Vec::new();
        for dest in 1u8..=4 {
            data.extend(transfer(0, dest, amount));
        }
        seeds.push(("balances_transfer_fan_out".into(), data));
    }

    // =========================================================================
    // transfer_keep_alive_works
    //
    // Attempting to transfer the entire balance with keep_alive fails
    // (NotExpendable); a transfer leaving at least ED succeeds.
    // =========================================================================
    {
        let mut data = Vec::new();
        // Trying to send everything → error (NotExpendable)
        data.extend(transfer_keep_alive(1, 0, ENDOWMENT));
        // Sending all but ED → leaves exactly ED, succeeds
        data.extend(transfer_keep_alive(1, 0, ENDOWMENT - ED));
        seeds.push(("balances_transfer_keep_alive_works".into(), data));
    }

    // =========================================================================
    // transfer_keep_alive – just-above-ED threshold
    //
    // Probe the boundary: ENDOWMENT - ED + 1 would leave ED - 1 behind (below
    // ED → should fail), while ENDOWMENT - ED is exactly the ED remainder.
    // =========================================================================
    {
        let mut data = Vec::new();
        // Would leave ED - 1 → NotExpendable
        data.extend(transfer_keep_alive(1, 0, ENDOWMENT - ED + 1));
        // Leaves exactly ED → OK
        data.extend(transfer_keep_alive(1, 0, ENDOWMENT - ED));
        seeds.push(("balances_transfer_keep_alive_boundary".into(), data));
    }

    // =========================================================================
    // transfer_keep_alive – multiple hops staying alive
    // =========================================================================
    {
        let amount = DOLLARS;
        let mut data = Vec::new();
        data.extend(transfer_keep_alive(0, 1, amount));
        data.extend(transfer_keep_alive(1, 2, amount));
        data.extend(transfer_keep_alive(2, 3, amount));
        data.extend(transfer_keep_alive(3, 4, amount));
        seeds.push(("balances_transfer_keep_alive_chain".into(), data));
    }

    // =========================================================================
    // transfer_all_works_1  (allow_death)
    //
    // account(1) transfers its entire reducible balance to account(0).
    // Because keep_alive=false, if the remaining balance < ED the account is
    // reaped.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(transfer_all(1, 0, false));
        seeds.push(("balances_transfer_all_allow_death".into(), data));
    }

    // =========================================================================
    // transfer_all_works_2  (keep_alive)
    //
    // account(1) transfers everything except ED to account(0).
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(transfer_all(1, 0, true));
        seeds.push(("balances_transfer_all_keep_alive".into(), data));
    }

    // =========================================================================
    // transfer_all – all origins, same destination
    //
    // Each account(1..4) drains itself to account(0), exercising the call with
    // five different signers.
    // =========================================================================
    {
        let mut data = Vec::new();
        for src in 1u8..=4 {
            data.extend(transfer_all(src, 0, false));
        }
        seeds.push(("balances_transfer_all_multi_source".into(), data));
    }

    // =========================================================================
    // transfer_all – allow_death then keep_alive (different keep_alive flag)
    // =========================================================================
    {
        let mut data = Vec::new();
        // First drain account(1) → account(2) (allow death)
        data.extend(transfer_all(1, 2, false));
        // Then account(2) keeps itself alive, sends rest to account(3)
        data.extend(transfer_all(2, 3, true));
        seeds.push(("balances_transfer_all_mixed".into(), data));
    }

    // =========================================================================
    // burn_works – partial burn then reaping burn
    //
    // 1. Burn a small amount (no reap).
    // 2. Try to burn too much with keep_alive → error.
    // 3. Burn the remainder allowing death → account reaped, total issuance drops.
    // =========================================================================
    {
        let init = ENDOWMENT;
        let burn1 = DOLLARS; // leaves init - burn1, still alive
        let burn2 = init - burn1 - ED + 1; // would leave ED-1 < ED with keep_alive → error
        let burn3 = init - burn1; // burns everything remaining, allow_death → reaped

        let mut data = Vec::new();
        // 1. Burn some (succeeds, account survives)
        data.extend(burn(1, burn1, false));
        // 2. Burn with keep_alive that would reap → error
        data.extend(burn(1, burn2, true));
        // 3. Burn all remaining, allow death → account reaped
        data.extend(burn(1, burn3, false));
        seeds.push(("balances_burn_partial_then_reap".into(), data));
    }

    // =========================================================================
    // burn – keep_alive prevents reaping
    // =========================================================================
    {
        let mut data = Vec::new();
        // Burn everything except ED → leaves exactly ED (keep_alive=true)
        data.extend(burn(1, ENDOWMENT - ED, true));
        // Now account(1) has exactly ED; burn 1 more with keep_alive → error
        data.extend(burn(1, 1, true));
        // But allow_death = false would also fail because balance < ED after burn.
        // Burn with allow_death and a tiny amount leaves 0 → reaped.
        data.extend(burn(1, ED, false));
        seeds.push(("balances_burn_keep_alive".into(), data));
    }

    // =========================================================================
    // burn – error: more than available
    // =========================================================================
    {
        let mut data = Vec::new();
        // Try to burn more than balance → FundsUnavailable
        data.extend(burn(0, ENDOWMENT + 1, false));
        // Correct amount → succeeds
        data.extend(burn(0, DOLLARS, false));
        seeds.push(("balances_burn_error_too_much".into(), data));
    }

    // =========================================================================
    // burn – multiple sequential burns reducing total issuance step by step
    // =========================================================================
    {
        let step = DOLLARS;
        let mut data = Vec::new();
        for _ in 0..5 {
            data.extend(burn(2, step, true));
        }
        seeds.push(("balances_burn_sequential".into(), data));
    }

    // =========================================================================
    // burn – each account burns once (broad coverage of signers)
    // =========================================================================
    {
        let mut data = Vec::new();
        for origin in 0u8..5 {
            data.extend(burn(origin, DOLLARS, false));
        }
        seeds.push(("balances_burn_all_signers".into(), data));
    }

    // =========================================================================
    // upgrade_accounts_should_work
    //
    // upgrade_accounts with a list of all five fuzzer accounts.  In the fuzzer
    // genesis accounts are already in new-logic format, so this exercises the
    // "no-op upgrade" path (Pays::Yes).  The fuzzer may mutate this to cover
    // accounts that haven't been upgraded yet.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Balances(pallet_balances::Call::upgrade_accounts {
                who: vec![account(0), account(1), account(2), account(3), account(4)],
            }),
        ));
        seeds.push(("balances_upgrade_accounts_all".into(), data));
    }

    // =========================================================================
    // upgrade_accounts – single account
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Balances(pallet_balances::Call::upgrade_accounts {
                who: vec![account(2)],
            }),
        ));
        seeds.push(("balances_upgrade_accounts_single".into(), data));
    }

    // =========================================================================
    // upgrade_accounts – empty list (returns Pays::Yes immediately)
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Balances(pallet_balances::Call::upgrade_accounts { who: vec![] }),
        ));
        seeds.push(("balances_upgrade_accounts_empty".into(), data));
    }

    // =========================================================================
    // Combined: transfer then burn (exercises both branches in the same block)
    // =========================================================================
    {
        let mut data = Vec::new();
        // account(0) sends some to account(1)
        data.extend(transfer(0, 1, 10 * DOLLARS));
        // account(1) burns a portion of what it received
        data.extend(burn(1, 5 * DOLLARS, false));
        // account(1) sends the rest back
        data.extend(transfer(1, 0, 5 * DOLLARS));
        seeds.push(("balances_transfer_then_burn".into(), data));
    }

    // =========================================================================
    // Combined: transfer_all → upgrade_accounts → transfer_keep_alive
    //
    // Exercises three different signed calls in one seed, giving the fuzzer a
    // richer starting point for call-sequence mutation.
    // =========================================================================
    {
        let mut data = Vec::new();
        // account(3) drains to account(4) (allow death)
        data.extend(transfer_all(3, 4, false));
        // Upgrade a set of accounts
        data.extend(enc(
            false,
            0,
            RuntimeCall::Balances(pallet_balances::Call::upgrade_accounts {
                who: vec![account(1), account(2)],
            }),
        ));
        // account(0) keeps itself alive while sending to account(2)
        data.extend(transfer_keep_alive(0, 2, 3 * DOLLARS));
        seeds.push(("balances_multi_call_sequence".into(), data));
    }

    // =========================================================================
    // Cross-block: transfer in block 1, then transfer in block 2
    //
    // The `advance_block = true` flag tells the fuzzer to finalize block 1 and
    // start block 2 before dispatching the second extrinsic.  This exercises
    // the invariant check across multiple blocks.
    // =========================================================================
    {
        let mut data = Vec::new();
        // Block 1: account(0) → account(1)
        data.extend(enc(
            false,
            0,
            RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
                dest: lookup(1),
                value: DOLLARS,
            }),
        ));
        // Block 2: account(1) → account(2)
        data.extend(enc(
            true,
            1,
            RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
                dest: lookup(2),
                value: DOLLARS,
            }),
        ));
        seeds.push(("balances_transfer_cross_block".into(), data));
    }

    // =========================================================================
    // Cross-block: burn in block 1, transfer_all in block 2
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Balances(pallet_balances::Call::burn {
                value: 10 * DOLLARS,
                keep_alive: false,
            }),
        ));
        data.extend(enc(
            true,
            1,
            RuntimeCall::Balances(pallet_balances::Call::transfer_all {
                dest: lookup(2),
                keep_alive: true,
            }),
        ));
        seeds.push(("balances_burn_then_transfer_all_cross_block".into(), data));
    }

    // =========================================================================
    // Edge: transfer value = 0
    //
    // Transferring 0 is a valid extrinsic (no-op transfer in FRAME fungible).
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(transfer(0, 1, 0));
        seeds.push(("balances_transfer_zero".into(), data));
    }

    // =========================================================================
    // Edge: transfer value = 1 (minimum non-zero amount)
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(transfer(0, 1, 1));
        seeds.push(("balances_transfer_one".into(), data));
    }

    // =========================================================================
    // Edge: burn value = 1
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(burn(0, 1, false));
        seeds.push(("balances_burn_one".into(), data));
    }

    seeds
}
