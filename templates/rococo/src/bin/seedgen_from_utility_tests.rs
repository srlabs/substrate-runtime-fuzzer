#![warn(clippy::pedantic)]
#![allow(clippy::too_many_lines)]

//! Seed generator derived from `substrate/frame/utility/src/tests.rs`.
//!
//! Each seed encodes one test scenario as a sequence of SCALE-encoded
//! `(bool, u8, RuntimeCall)` tuples for the rococo fuzzer.
//!
//! ## Account mapping
//! Test accounts are u64 (1, 2, 3, ...).  Fuzzer accounts are AccountId32 with
//! 5 fixed addresses [0;32]..[4;32].  Mapping used here:
//!
//! | Test account | Fuzzer origin | Fuzzer `AccountId32` |
//! |---|---|---|
//! | 1 | 0 | [0;32] |
//! | 2 | 1 | [1;32] |
//! | 3 | 2 | [2;32] |
//! | 4 | 3 | [3;32] |
//! | 5 / 6 / 666 / 777 | 4 | [4;32] |
//!
//! Fuzzer accounts each start with `1 << 60`; test balances (10) are scaled to
//! `UNITS` so transfer arithmetic stays comparable while leaving headroom.
//!
//! ## Skipped tests
//! - Anything dispatched from `RuntimeOrigin::root()`: `batch_with_root_works`,
//!   `with_weight_works`, `dispatch_as_works`, `dispatch_as_fallible_works`,
//!   `if_else_with_root_works`, `force_batch_doesnt_work_with_inherents`.
//!   The fuzzer dispatches every extrinsic with a signed origin, so these
//!   would always fail with `BadOrigin`.
//! - `*_with_council_origin`: Rococo has no `Collective`/`Council` pallet.
//! - `*_doesnt_work_with_inherents`: inner `Timestamp::set` is filtered out by
//!   the fuzzer's pallet allowlist (Timestamp not in `is_disallowed`).
//! - `as_derivative_handles_weight_refund` / `batch*_handles_weight_refund` /
//!   `batch_weight_calculation_doesnt_overflow`: rely on test-only
//!   `Example::foobar` / `RootTesting::fill_block` calls.
//! - `batch_limit`: 40_000 calls would blow past block weight; the fuzzer
//!   skips over-weight calls anyway.
//! - `none_origin_does_not_work`: fuzzer never dispatches with `RawOrigin::None`.
//! - `as_derivative_filters` / `batch_with_signed_filters`: filters on
//!   `transfer_keep_alive` are mock-only; rococo's `BaseCallFilter` allows
//!   it, so the call would succeed instead of demonstrating the filter.
//!
//! ## Translation notes
//! - Test calls referencing `pallet_root_testing` / `Example` / `Democracy` are
//!   replaced with semantically similar `Balances` calls when the original test
//!   was about the wrapping pallet's behaviour, not the inner call's identity.
//! - `derivative_account_id` is exposed by `pallet_utility` and computes
//!   `b2b256("modlpy/utilisuba" ++ who ++ index)` so we can pre-fund derivative
//!   accounts.

use codec::Encode;
use rococo_runtime::RuntimeCall;
use sp_runtime::{AccountId32, MultiAddress};
use std::{fs, path::Path};

// ─── constants ───────────────────────────────────────────────────────────────

const UNITS: u128 = 1_000_000_000_000;
const ENDOWMENT: u128 = 1 << 60;
/// A value larger than any account's balance — guarantees the inner transfer fails.
const TOO_MUCH: u128 = ENDOWMENT * 2;

// ─── helpers ─────────────────────────────────────────────────────────────────

type AccountId = AccountId32;

fn account(idx: u8) -> AccountId {
    [idx; 32].into()
}

fn lookup(idx: u8) -> MultiAddress<AccountId, ()> {
    MultiAddress::Id(account(idx))
}

fn enc(advance_block: bool, origin: u8, call: RuntimeCall) -> Vec<u8> {
    (advance_block, origin, call).encode()
}

fn transfer_call(dest: u8, value: u128) -> RuntimeCall {
    RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
        dest: lookup(dest),
        value,
    })
}

fn transfer_keep_alive_call(dest: u8, value: u128) -> RuntimeCall {
    RuntimeCall::Balances(pallet_balances::Call::transfer_keep_alive {
        dest: lookup(dest),
        value,
    })
}

fn transfer_to_acc_call(dest: AccountId, value: u128) -> RuntimeCall {
    RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
        dest: MultiAddress::Id(dest),
        value,
    })
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
    // as_derivative_works
    //
    // Pre-fund the derivative account of (account(0), index=0), then call
    // `as_derivative(0, transfer(...))` so the inner transfer is dispatched
    // from the derivative account.
    // =========================================================================
    {
        let sub_0_0: AccountId = pallet_utility::derivative_account_id(account(0), 0);

        let mut data = Vec::new();
        // Fund derivative account so it has balance to spend.
        data.extend(enc(false, 0, transfer_to_acc_call(sub_0_0, 5 * UNITS)));

        // as_derivative(0, transfer(account(1), 3 UNITS)) → spends from sub_0_0.
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::as_derivative {
                index: 0,
                call: Box::new(transfer_call(1, 3 * UNITS)),
            }),
        ));

        // as_derivative(1, transfer(...)) → unfunded sub-account → FundsUnavailable.
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::as_derivative {
                index: 1,
                call: Box::new(transfer_call(1, 3 * UNITS)),
            }),
        ));

        seeds.push(("utility_as_derivative_works".into(), data));
    }

    // =========================================================================
    // as_derivative — chain (sub of sub)
    //
    // Wrap as_derivative inside as_derivative.  The inner dispatch lands at
    // derivative_account_id(derivative_account_id(account(0), 0), 1).
    // =========================================================================
    {
        let sub_0_0: AccountId = pallet_utility::derivative_account_id(account(0), 0);
        let sub_sub: AccountId = pallet_utility::derivative_account_id(sub_0_0.clone(), 1);

        let mut data = Vec::new();
        data.extend(enc(false, 0, transfer_to_acc_call(sub_0_0.clone(), 10 * UNITS)));
        data.extend(enc(false, 0, transfer_to_acc_call(sub_sub, 5 * UNITS)));

        let inner = transfer_call(2, UNITS);
        let nested = RuntimeCall::Utility(pallet_utility::Call::as_derivative {
            index: 1,
            call: Box::new(inner),
        });
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::as_derivative {
                index: 0,
                call: Box::new(nested),
            }),
        ));

        seeds.push(("utility_as_derivative_nested".into(), data));
    }

    // =========================================================================
    // batch_with_signed_works
    //
    // signed batch of two transfers from account(0) to account(1).
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::batch {
                calls: vec![transfer_call(1, 5 * UNITS), transfer_call(1, 5 * UNITS)],
            }),
        ));
        seeds.push(("utility_batch_signed_works".into(), data));
    }

    // =========================================================================
    // batch_early_exit_works
    //
    // batch of [transfer(ok), transfer(too_much), transfer(ok)] — the second
    // call fails, batch is interrupted, third call never executes.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::batch {
                calls: vec![
                    transfer_call(1, 5 * UNITS),
                    transfer_call(1, TOO_MUCH),
                    transfer_call(1, 5 * UNITS),
                ],
            }),
        ));
        seeds.push(("utility_batch_early_exit".into(), data));
    }

    // =========================================================================
    // batch_all_works
    //
    // signed batch_all of two transfers — atomic success.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::batch_all {
                calls: vec![transfer_call(1, 5 * UNITS), transfer_call(1, 5 * UNITS)],
            }),
        ));
        seeds.push(("utility_batch_all_works".into(), data));
    }

    // =========================================================================
    // batch_all_revert
    //
    // batch_all where the middle call fails — the entire batch is rolled back,
    // so even the first (otherwise successful) call has no observable effect.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::batch_all {
                calls: vec![
                    transfer_call(1, 5 * UNITS),
                    transfer_call(1, TOO_MUCH),
                    transfer_call(1, 5 * UNITS),
                ],
            }),
        ));
        seeds.push(("utility_batch_all_revert".into(), data));
    }

    // =========================================================================
    // batch_all_does_not_nest
    //
    // batch_all containing a batch_all: the inner batch_all is filtered (nested
    // batch_all is blocked by the dispatch filter installed by the outer call).
    // Also the `batch_all(batch(batch_all(..)))` form which surfaces the filter
    // through the intermediate `batch`.
    // =========================================================================
    {
        let inner_calls = vec![
            transfer_call(1, UNITS),
            transfer_call(1, UNITS),
            transfer_call(1, UNITS),
        ];
        let inner_batch_all = RuntimeCall::Utility(pallet_utility::Call::batch_all {
            calls: inner_calls,
        });

        let mut data = Vec::new();
        // Direct nested batch_all.
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::batch_all {
                calls: vec![inner_batch_all.clone()],
            }),
        ));
        // batch_all(batch(batch_all(..))) — filter persists through the batch.
        let nested = RuntimeCall::Utility(pallet_utility::Call::batch {
            calls: vec![inner_batch_all],
        });
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::batch_all {
                calls: vec![nested],
            }),
        ));

        seeds.push(("utility_batch_all_does_not_nest".into(), data));
    }

    // =========================================================================
    // force_batch_works
    //
    // force_batch with a mix of valid and invalid transfers — completes all
    // calls, emitting `ItemFailed` for failures and `BatchCompletedWithErrors`
    // at the end (at least one failed).
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::force_batch {
                calls: vec![
                    transfer_call(1, 5 * UNITS),
                    transfer_call(1, TOO_MUCH),
                    transfer_call(1, 5 * UNITS),
                    transfer_call(1, 5 * UNITS),
                ],
            }),
        ));
        // A force_batch that succeeds entirely.
        data.extend(enc(
            false,
            1,
            RuntimeCall::Utility(pallet_utility::Call::force_batch {
                calls: vec![transfer_call(0, 5 * UNITS), transfer_call(0, 5 * UNITS)],
            }),
        ));
        // A force_batch with only one (failing) call.
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::force_batch {
                calls: vec![transfer_call(1, TOO_MUCH)],
            }),
        ));
        seeds.push(("utility_force_batch_works".into(), data));
    }

    // =========================================================================
    // if_else_with_signed_works
    //
    // if_else where the main call fails — the fallback executes and the event
    // `IfElseFallbackCalled { main_error }` is emitted.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::if_else {
                main: Box::new(transfer_call(1, TOO_MUCH)),
                fallback: Box::new(transfer_call(1, 5 * UNITS)),
            }),
        ));
        seeds.push(("utility_if_else_main_fails".into(), data));
    }

    // =========================================================================
    // if_else_successful_main_call
    //
    // if_else where the main call succeeds — fallback is skipped entirely.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::if_else {
                main: Box::new(transfer_call(1, 9 * UNITS)),
                fallback: Box::new(transfer_call(1, UNITS)),
            }),
        ));
        seeds.push(("utility_if_else_main_succeeds".into(), data));
    }

    // =========================================================================
    // if_else_failing_fallback_call
    //
    // Both main and fallback fail — the whole `if_else` call returns Err.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::if_else {
                main: Box::new(transfer_call(1, TOO_MUCH)),
                fallback: Box::new(transfer_call(1, TOO_MUCH)),
            }),
        ));
        seeds.push(("utility_if_else_both_fail".into(), data));
    }

    // =========================================================================
    // if_else_with_nested_if_else_works
    //
    // Outer main is an `if_else(transfer too much, transfer 5)`: inner main
    // fails, inner fallback succeeds, so outer main succeeds and outer fallback
    // is never executed.
    // =========================================================================
    {
        let inner_if_else = RuntimeCall::Utility(pallet_utility::Call::if_else {
            main: Box::new(transfer_call(1, TOO_MUCH)),
            fallback: Box::new(transfer_call(1, 5 * UNITS)),
        });
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::if_else {
                main: Box::new(inner_if_else),
                fallback: Box::new(transfer_call(1, 7 * UNITS)),
            }),
        ));
        seeds.push(("utility_if_else_nested".into(), data));
    }

    // =========================================================================
    // batch_can_nest_inside_batch
    //
    // Plain `batch` (unlike `batch_all`) does not install a no-nest filter, so
    // a batch inside a batch dispatches normally.  Useful for exercising the
    // recursive call decoder and the fuzzer's call-filter walker.
    // =========================================================================
    {
        let inner_batch = RuntimeCall::Utility(pallet_utility::Call::batch {
            calls: vec![transfer_call(1, UNITS), transfer_call(1, UNITS)],
        });
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::batch {
                calls: vec![inner_batch, transfer_call(2, UNITS)],
            }),
        ));
        seeds.push(("utility_batch_nested".into(), data));
    }

    // =========================================================================
    // batch_mixing_pallets
    //
    // batch combining Balances + Multisig + Proxy inner calls — exercises the
    // recursive filter walker and several pallets in a single dispatch.
    // =========================================================================
    {
        let multisig_call = RuntimeCall::Multisig(pallet_multisig::Call::as_multi_threshold_1 {
            other_signatories: vec![account(1), account(2)],
            call: Box::new(transfer_call(3, UNITS)),
        });
        let proxy_add = RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
            delegate: lookup(1),
            proxy_type: rococo_runtime::ProxyType::Any,
            delay: 0,
        });
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::batch_all {
                calls: vec![transfer_call(1, UNITS), proxy_add, multisig_call],
            }),
        ));
        seeds.push(("utility_batch_mixed_pallets".into(), data));
    }

    // =========================================================================
    // as_derivative_with_keep_alive_inner
    //
    // Wraps `transfer_keep_alive` (the mock test rejected this via filter, but
    // rococo allows it).  Pre-funds the sub-account with enough that the
    // inner keep-alive transfer leaves >= ED behind.
    // =========================================================================
    {
        let sub_1_3: AccountId = pallet_utility::derivative_account_id(account(1), 3);
        let mut data = Vec::new();
        data.extend(enc(false, 1, transfer_to_acc_call(sub_1_3, 10 * UNITS)));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Utility(pallet_utility::Call::as_derivative {
                index: 3,
                call: Box::new(transfer_keep_alive_call(2, UNITS)),
            }),
        ));
        seeds.push(("utility_as_derivative_keep_alive_inner".into(), data));
    }

    // =========================================================================
    // batch_then_proxy_uses_state
    //
    // batch_all sets up a proxy; the next extrinsic exercises the resulting
    // proxy.  Two extrinsics produce coverage that isn't reachable from a
    // single utility-only batch.
    // =========================================================================
    {
        let setup = RuntimeCall::Utility(pallet_utility::Call::batch_all {
            calls: vec![
                transfer_call(1, 2 * UNITS),
                RuntimeCall::Proxy(pallet_proxy::Call::add_proxy {
                    delegate: lookup(1),
                    proxy_type: rococo_runtime::ProxyType::Any,
                    delay: 0,
                }),
            ],
        });

        let use_proxy = RuntimeCall::Proxy(pallet_proxy::Call::proxy {
            real: lookup(0),
            force_proxy_type: None,
            call: Box::new(transfer_call(2, UNITS)),
        });

        let mut data = Vec::new();
        data.extend(enc(false, 0, setup));
        data.extend(enc(false, 1, use_proxy));
        seeds.push(("utility_batch_setup_then_proxy".into(), data));
    }

    // =========================================================================
    // batch_cross_block
    //
    // batch in block 1, batch_all in block 2 — exercises the per-block
    // weight/elapsed reset together with the utility wrappers.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Utility(pallet_utility::Call::batch {
                calls: vec![transfer_call(1, UNITS), transfer_call(2, UNITS)],
            }),
        ));
        data.extend(enc(
            true,
            1,
            RuntimeCall::Utility(pallet_utility::Call::batch_all {
                calls: vec![transfer_call(3, UNITS), transfer_call(4, UNITS)],
            }),
        ));
        seeds.push(("utility_batch_cross_block".into(), data));
    }

    // =========================================================================
    // many_signers
    //
    // Each fuzzer account in turn signs a small batch — exposes the fuzzer to
    // five different signed origins so subsequent mutations exercise more
    // origin/inner-call combinations.
    // =========================================================================
    {
        let mut data = Vec::new();
        for origin in 0u8..5 {
            let dest = (origin + 1) % 5;
            data.extend(enc(
                false,
                origin,
                RuntimeCall::Utility(pallet_utility::Call::batch {
                    calls: vec![transfer_call(dest, UNITS)],
                }),
            ));
        }
        seeds.push(("utility_batch_all_signers".into(), data));
    }

    seeds
}
