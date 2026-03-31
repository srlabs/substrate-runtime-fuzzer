#![warn(clippy::pedantic)]
#![allow(clippy::too_many_lines)]

//! Seed generator derived from `substrate/frame/asset-conversion/src/tests.rs`.
//!
//! Each seed encodes one full test scenario as a sequence of SCALE-encoded
//! `(bool, u8, RuntimeCall)` tuples. The fuzzer uses these as a starting
//! corpus and mutates them to explore deeper code paths in the `AssetConversion`
//! pallet.
//!
//! Translation notes
//! -----------------
//! * `Assets::force_create(root, id, owner, ...)` → replaced by
//!   `Assets::create(signed(0), id, lookup(0), min_balance)`. The kitchensink
//!   genesis gives every account `10_000_000` DOLLARS, so the `AssetDeposit` is
//!   always covered.
//! * `Balances::force_set_balance(root, ...)` → dropped entirely. Genesis
//!   endowment (`10_000_000` DOLLARS per account) far exceeds any amount used
//!   in the tests.
//! * Test account integer `N` (1-indexed u128) → fuzzer origin `N-1`, account
//!   `[N-1; 32]`. Exception: test account 2 used in two-user tests → origin 1.
//! * `mint_to`, `withdraw_to`, `send_to` take `T::AccountId = AccountId32`,
//!   so use `account(idx)` directly (not `MultiAddress`).
//! * The path arg in swap calls is `Vec<Box<NativeOrWithId<u32>>>`.
//! * All native (asset1) amounts are scaled by DOLLARS
//!   (= `100_000_000_000_000` planks = 10^14) because the kitchensink
//!   `ExistentialDeposit` is 1 DOLLAR, and `add_liquidity` rejects
//!   amounts below the native minimum balance.
//! * Non-native asset amounts are kept small (`min_balance` = 1 for created
//!   assets), matching test values directly.
//! * Trait-based tests (`SwapCredit`, `Swap` trait, genesis-pool tests) are
//!   skipped – they do not map to dispatchable `RuntimeCall` variants.

use codec::{Compact, Encode};
use frame_support::traits::fungible::NativeOrWithId;
use kitchensink_runtime::RuntimeCall;
use node_primitives::AccountIndex;
use sp_runtime::MultiAddress;
use std::{fs, path::Path};

// ── constants ─────────────────────────────────────────────────────────────────

/// 1 DOT in planks: MILLICENTS=1e9, CENTS=1e12, DOLLARS=1e14.
/// Matches `kitchensink_runtime::constants::currency::DOLLARS`.
const DOLLARS: u128 = 100_000_000_000_000;

// ── type aliases ─────────────────────────────────────────────────────────────

type AccountId = sp_runtime::AccountId32;

/// Account `[idx; 32]` – mirrors the fuzzer's account list (0..4).
fn account(idx: u8) -> AccountId {
    [idx; 32].into()
}

/// `MultiAddress::Id([idx; 32])` – used wherever a call takes
/// `AccountIdLookupOf<T>` (i.e. pallet-assets calls).
fn lookup(idx: u8) -> MultiAddress<AccountId, AccountIndex> {
    MultiAddress::Id(account(idx))
}

/// Encode one extrinsic in fuzzer wire format: `(advance_block, origin, call)`.
fn enc(advance_block: bool, origin: u8, call: RuntimeCall) -> Vec<u8> {
    (advance_block, origin, call).encode()
}

/// Shorthand: `NativeOrWithId::Native`.
fn native() -> NativeOrWithId<u32> {
    NativeOrWithId::Native
}

/// Shorthand: `NativeOrWithId::WithId(id)`.
fn with_id(id: u32) -> NativeOrWithId<u32> {
    NativeOrWithId::WithId(id)
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

// ── helpers ───────────────────────────────────────────────────────────────────

/// Emit the two calls needed to create (and optionally mint) a test asset.
/// Replaces `create_tokens(owner, vec![WithId(id)])` + optional
/// `Assets::mint(...)` from the tests.
///
/// * `creator_origin` – account index that calls `Assets::create`.
/// * `id`             – asset id.
/// * `mint_to`        – account that receives the mint (only if `mint_amount > 0`).
/// * `mint_amount`    – amount to mint; set to 0 to skip the mint call.
fn asset_create_and_mint(
    data: &mut Vec<u8>,
    creator_origin: u8,
    id: u32,
    mint_to: u8,
    mint_amount: u128,
) {
    data.extend(enc(
        false,
        creator_origin,
        RuntimeCall::Assets(pallet_assets::Call::create {
            id: Compact(id),
            admin: lookup(creator_origin),
            min_balance: 1u128,
        }),
    ));
    if mint_amount > 0 {
        data.extend(enc(
            false,
            creator_origin,
            RuntimeCall::Assets(pallet_assets::Call::mint {
                id: Compact(id),
                beneficiary: lookup(mint_to),
                amount: mint_amount,
            }),
        ));
    }
}

// ── seed scenarios ────────────────────────────────────────────────────────────

#[allow(clippy::cast_lossless)]
fn build_seeds() -> Vec<(String, Vec<u8>)> {
    let mut seeds: Vec<(String, Vec<u8>)> = Vec::new();

    // =========================================================================
    // can_create_pool
    //
    // Create fungible assets (WithId(2) and WithId(1)) and open two pools:
    // Native/WithId(2) and WithId(1)/WithId(2) (non-native pair).
    // Mirrors: can_create_pool test.
    // =========================================================================
    {
        let mut data = Vec::new();

        // Create asset id=2 (owned/issued by account(0))
        asset_create_and_mint(&mut data, 0, 2, 0, 0);

        // create_pool(WithId(2), Native)
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(with_id(2)),
                asset2: Box::new(native()),
            }),
        ));

        // Create asset id=1 then open a WithId(1)/WithId(2) non-native pool
        asset_create_and_mint(&mut data, 0, 1, 0, 0);
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(with_id(1)),
                asset2: Box::new(with_id(2)),
            }),
        ));

        seeds.push(("asset_conversion_can_create_pool".to_string(), data));
    }

    // =========================================================================
    // can_add_liquidity
    //
    // Two pools (Native/WithId(2) and Native/WithId(3)).  Mint tokens to
    // account(0) then add liquidity to both.
    //
    // Native amounts scaled by DOLLARS so they exceed the ExistentialDeposit.
    // Ratio: 10_000 * DOLLARS native : 10 asset  (≈ 1000:1).
    // Mirrors: can_add_liquidity test.
    // =========================================================================
    {
        let mut data = Vec::new();

        // Create assets 2 and 3, mint 1000 of each to account(0)
        asset_create_and_mint(&mut data, 0, 2, 0, 1000);
        asset_create_and_mint(&mut data, 0, 3, 0, 1000);

        // Open both pools
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
            }),
        ));
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(3)),
            }),
        ));

        // add_liquidity pool 1: 10_000 * DOLLARS native : 10 asset2
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
                amount1_desired: 10_000 * DOLLARS,
                amount2_desired: 10u128,
                amount1_min: 10_000 * DOLLARS,
                amount2_min: 10u128,
                mint_to: account(0),
            }),
        ));

        // add_liquidity pool 2 (reversed order): 10 asset3 : 10_000 * DOLLARS native
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(with_id(3)),
                asset2: Box::new(native()),
                amount1_desired: 10u128,
                amount2_desired: 10_000 * DOLLARS,
                amount1_min: 10u128,
                amount2_min: 10_000 * DOLLARS,
                mint_to: account(0),
            }),
        ));

        seeds.push(("asset_conversion_can_add_liquidity".to_string(), data));
    }

    // =========================================================================
    // can_remove_liquidity
    //
    // Full lifecycle: create asset → create pool → add liquidity → remove all
    // LP tokens.
    //
    // Pool ratio: 1_000 * DOLLARS native : 100_000 asset2.
    // Initial LP ≈ sqrt(1_000 * DOLLARS * 100_000) - 100
    //            = sqrt(10^22) - 100 ≈ 3_162_277_660 - 100 = 3_162_277_560.
    // We burn 3_162_000_000 (slightly less) to leave some LP in the pool.
    // Mirrors: can_remove_liquidity test.
    // =========================================================================
    {
        let mut data = Vec::new();

        // mint 100_001 (= 100_000 liquidity + 1 min_balance remainder) so the
        // account is not killed when transferring exactly 100_000 to the pool.
        asset_create_and_mint(&mut data, 0, 2, 0, 100_001);

        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
            }),
        ));

        // add_liquidity: 1_000 * DOLLARS native : 100_000 asset2
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
                amount1_desired: 1_000 * DOLLARS,
                amount2_desired: 100_000u128,
                amount1_min: 1u128,
                amount2_min: 1u128,
                mint_to: account(0),
            }),
        ));

        // remove_liquidity: burn most LP tokens; min_receive = 0 to avoid
        // slippage errors (fuzzer will tighten).
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::remove_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
                lp_token_burn: 3_162_000_000u128,
                amount1_min_receive: 0u128,
                amount2_min_receive: 0u128,
                withdraw_to: account(0),
            }),
        ));

        seeds.push(("asset_conversion_can_remove_liquidity".to_string(), data));
    }

    // =========================================================================
    // can_swap_with_native
    //
    // Create pool (10_000 * DOLLARS native : 200 asset2), add liquidity,
    // then swap_exact_tokens_for_tokens (100 asset2 → native).
    // Mirrors: can_swap_with_native test.
    // =========================================================================
    {
        let mut data = Vec::new();

        // Create asset 2, mint 1000 to account(0)
        asset_create_and_mint(&mut data, 0, 2, 0, 1000);

        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
            }),
        ));

        // add_liquidity: 10_000 * DOLLARS native : 200 asset2
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
                amount1_desired: 10_000 * DOLLARS,
                amount2_desired: 200u128,
                amount1_min: 1u128,
                amount2_min: 1u128,
                mint_to: account(0),
            }),
        ));

        // swap_exact: 100 asset2 → native; keep_alive=false
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(
                pallet_asset_conversion::Call::swap_exact_tokens_for_tokens {
                    path: vec![Box::new(with_id(2)), Box::new(native())],
                    amount_in: 100u128,
                    amount_out_min: 1u128,
                    send_to: account(0),
                    keep_alive: false,
                },
            ),
        ));

        seeds.push(("asset_conversion_swap_with_native".to_string(), data));
    }

    // =========================================================================
    // can_swap_with_realistic_values
    //
    // Large-scale pool: 200_000 * UNIT native : 1_000_000 * UNIT asset2
    // (ratio ~$5/DOT).  Swap 10 * UNIT of asset2 (USD) for native (DOT).
    // UNIT = 1_000_000_000 planks (sub-dollar precision for large-value test).
    // Mirrors: can_swap_with_realistic_values test.
    // =========================================================================
    {
        const UNIT: u128 = 1_000_000_000u128;
        let mut data = Vec::new();

        // Mint 1_100_000 * UNIT of asset2 to account(0)
        asset_create_and_mint(&mut data, 0, 2, 0, 1_100_000 * UNIT);

        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
            }),
        ));

        // add_liquidity: 200_000 * UNIT native : 1_000_000 * UNIT asset2
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
                amount1_desired: 200_000 * UNIT,
                amount2_desired: 1_000_000 * UNIT,
                amount1_min: 1u128,
                amount2_min: 1u128,
                mint_to: account(0),
            }),
        ));

        // swap 10 * UNIT asset2 (USD) → native (DOT); expect ~1.99 DOT out
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(
                pallet_asset_conversion::Call::swap_exact_tokens_for_tokens {
                    path: vec![Box::new(with_id(2)), Box::new(native())],
                    amount_in: 10 * UNIT,
                    amount_out_min: 1u128,
                    send_to: account(0),
                    keep_alive: false,
                },
            ),
        ));

        seeds.push(("asset_conversion_swap_realistic_values".to_string(), data));
    }

    // =========================================================================
    // can_swap_tokens_for_exact_tokens
    //
    // swap_tokens_for_exact_tokens: pay native, receive exact 50 asset2.
    // Pool ratio: 10_000 * DOLLARS native : 200 asset2.
    // expect_in ≈ 50 * 10_000 * DOLLARS * 1000 / (150 * 997) + 1
    //           ≈ 3_344 * DOLLARS.
    // amount_in_max = 3_500 * DOLLARS.
    // keep_alive = true exercises the extra keep-alive balance check.
    // Mirrors: can_swap_tokens_for_exact_tokens test.
    // =========================================================================
    {
        let mut data = Vec::new();

        asset_create_and_mint(&mut data, 0, 2, 0, 1000);

        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
            }),
        ));

        // add_liquidity: 10_000 * DOLLARS native : 200 asset2
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
                amount1_desired: 10_000 * DOLLARS,
                amount2_desired: 200u128,
                amount1_min: 1u128,
                amount2_min: 1u128,
                mint_to: account(0),
            }),
        ));

        // swap_tokens_for_exact: receive 50 asset2, pay up to 3_500 * DOLLARS
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(
                pallet_asset_conversion::Call::swap_tokens_for_exact_tokens {
                    path: vec![Box::new(native()), Box::new(with_id(2))],
                    amount_out: 50u128,
                    amount_in_max: 3_500 * DOLLARS,
                    send_to: account(0),
                    keep_alive: true,
                },
            ),
        ));

        seeds.push(("asset_conversion_swap_tokens_for_exact".to_string(), data));
    }

    // =========================================================================
    // can_swap_tokens_for_exact_tokens_when_not_liquidity_provider
    //
    // account(1) creates asset 2, opens the pool and provides liquidity.
    // account(0) (a non-LP) performs a swap.
    // account(1) removes their LP tokens afterwards.
    // Mirrors: can_swap_tokens_for_exact_tokens_when_not_liquidity_provider.
    // =========================================================================
    {
        let mut data = Vec::new();

        // account(1) creates asset 2 and mints to itself
        data.extend(enc(
            false,
            1,
            RuntimeCall::Assets(pallet_assets::Call::create {
                id: Compact(2u32),
                admin: lookup(1),
                min_balance: 1u128,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Assets(pallet_assets::Call::mint {
                id: Compact(2u32),
                beneficiary: lookup(1),
                amount: 1000u128,
            }),
        ));

        // account(1) opens the pool and adds liquidity
        data.extend(enc(
            false,
            1,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
                amount1_desired: 10_000 * DOLLARS,
                amount2_desired: 200u128,
                amount1_min: 1u128,
                amount2_min: 1u128,
                mint_to: account(1),
            }),
        ));

        // account(0) swaps: pay native, receive 50 asset2 (keep_alive=true)
        // expect_in ≈ 3_344 * DOLLARS; allow up to 3_500 * DOLLARS
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(
                pallet_asset_conversion::Call::swap_tokens_for_exact_tokens {
                    path: vec![Box::new(native()), Box::new(with_id(2))],
                    amount_out: 50u128,
                    amount_in_max: 3_500 * DOLLARS,
                    send_to: account(0),
                    keep_alive: true,
                },
            ),
        ));

        // account(1) removes LP tokens
        // LP minted ≈ sqrt(10_000 * DOLLARS * 200) - 100
        //           = sqrt(2 * 10^18) - 100 ≈ 1_414_213_562 - 100 = 1_414_213_462
        data.extend(enc(
            false,
            1,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::remove_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
                lp_token_burn: 1_414_213_462u128,
                amount1_min_receive: 0u128,
                amount2_min_receive: 0u128,
                withdraw_to: account(1),
            }),
        ));

        seeds.push(("asset_conversion_swap_non_lp".to_string(), data));
    }

    // =========================================================================
    // swap_exact_tokens_for_tokens_in_multi_hops
    //
    // Two pools: Native/WithId(2) (10_000*DOLLARS : 200) and
    //            WithId(2)/WithId(3) (200 : 2000).
    // Multi-hop: 500 * DOLLARS native → asset2 → asset3, min_out=80.
    //
    // Expected intermediate:
    //   get_amount_out(500*DOLLARS, 10_000*DOLLARS, 200)
    //   = 500*DOLLARS * 997 * 200 / (10_000*DOLLARS * 1000 + 500*DOLLARS * 997)
    //   ≈ 9 asset2
    //   get_amount_out(9, 200, 2000) ≈ 85 asset3  (> 80, so min_out satisfied)
    // Mirrors: swap_exact_tokens_for_tokens_in_multi_hops test.
    // =========================================================================
    {
        let mut data = Vec::new();

        // Create assets 2 and 3, mint to account(0)
        asset_create_and_mint(&mut data, 0, 2, 0, 10_000);
        asset_create_and_mint(&mut data, 0, 3, 0, 10_000);

        // Open both pools
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
            }),
        ));
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(with_id(2)),
                asset2: Box::new(with_id(3)),
            }),
        ));

        // add_liquidity pool 1: 10_000 * DOLLARS native : 200 asset2
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
                amount1_desired: 10_000 * DOLLARS,
                amount2_desired: 200u128,
                amount1_min: 1u128,
                amount2_min: 1u128,
                mint_to: account(0),
            }),
        ));

        // add_liquidity pool 2: 200 asset2 : 2000 asset3
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(with_id(2)),
                asset2: Box::new(with_id(3)),
                amount1_desired: 200u128,
                amount2_desired: 2_000u128,
                amount1_min: 1u128,
                amount2_min: 1u128,
                mint_to: account(0),
            }),
        ));

        // multi-hop swap_exact: 500 * DOLLARS native → asset2 → asset3
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(
                pallet_asset_conversion::Call::swap_exact_tokens_for_tokens {
                    path: vec![Box::new(native()), Box::new(with_id(2)), Box::new(with_id(3))],
                    amount_in: 500 * DOLLARS,
                    amount_out_min: 80u128,
                    send_to: account(0),
                    keep_alive: true,
                },
            ),
        ));

        seeds.push(("asset_conversion_multi_hop_swap_exact".to_string(), data));
    }

    // =========================================================================
    // swap_tokens_for_exact_tokens_in_multi_hops
    //
    // Same two-pool setup; use swap_tokens_for_exact_tokens to receive
    // exactly 100 asset3, paying native.
    //
    // Expected amounts (working backwards):
    //   get_amount_in(100, 200, 2000)  = 100*200*1000 / (1900*997) + 1 ≈ 11 asset2
    //   get_amount_in(11, 10_000*D, 200) ≈ 11*10_000*D*1000/(189*997) + 1
    //                                    ≈ 584 * DOLLARS
    // amount_in_max = 1_000 * DOLLARS (comfortable margin).
    // Mirrors: swap_tokens_for_exact_tokens_in_multi_hops test.
    // =========================================================================
    {
        let mut data = Vec::new();

        asset_create_and_mint(&mut data, 0, 2, 0, 10_000);
        asset_create_and_mint(&mut data, 0, 3, 0, 10_000);

        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
            }),
        ));
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(with_id(2)),
                asset2: Box::new(with_id(3)),
            }),
        ));

        // pool 1: 10_000 * DOLLARS native : 200 asset2
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
                amount1_desired: 10_000 * DOLLARS,
                amount2_desired: 200u128,
                amount1_min: 1u128,
                amount2_min: 1u128,
                mint_to: account(0),
            }),
        ));

        // pool 2: 200 asset2 : 2000 asset3
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(with_id(2)),
                asset2: Box::new(with_id(3)),
                amount1_desired: 200u128,
                amount2_desired: 2_000u128,
                amount1_min: 1u128,
                amount2_min: 1u128,
                mint_to: account(0),
            }),
        ));

        // multi-hop swap_exact_out: receive 100 asset3, pay at most 1_000 * DOLLARS
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(
                pallet_asset_conversion::Call::swap_tokens_for_exact_tokens {
                    path: vec![Box::new(native()), Box::new(with_id(2)), Box::new(with_id(3))],
                    amount_out: 100u128,
                    amount_in_max: 1_000 * DOLLARS,
                    send_to: account(0),
                    keep_alive: true,
                },
            ),
        ));

        seeds.push((
            "asset_conversion_multi_hop_swap_exact_out".to_string(),
            data,
        ));
    }

    // =========================================================================
    // check_no_panic_when_try_swap_close_to_empty_pool
    //
    // Add liquidity (10_000 * DOLLARS native : 200 asset2), remove most of it,
    // then attempt a swap on the near-empty pool.
    //
    // Initial LP ≈ sqrt(10_000 * DOLLARS * 200) - 100
    //            = sqrt(2 * 10^18) - 100 ≈ 1_414_213_462.
    // We burn 1_414_213_362 LP tokens (leaving ~100 in pool).
    // Mirrors: check_no_panic_when_try_swap_close_to_empty_pool test.
    // =========================================================================
    {
        let mut data = Vec::new();

        asset_create_and_mint(&mut data, 0, 2, 0, 1000);

        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
            }),
        ));

        // add_liquidity: 10_000 * DOLLARS native : 200 asset2
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
                amount1_desired: 10_000 * DOLLARS,
                amount2_desired: 200u128,
                amount1_min: 1u128,
                amount2_min: 1u128,
                mint_to: account(0),
            }),
        ));

        // remove most liquidity; leave only the minimum 100 LP tokens
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::remove_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
                lp_token_burn: 1_414_213_362u128,
                amount1_min_receive: 1u128,
                amount2_min_receive: 1u128,
                withdraw_to: account(0),
            }),
        ));

        // swap on near-empty pool: try to receive some native, pay asset2
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(
                pallet_asset_conversion::Call::swap_tokens_for_exact_tokens {
                    path: vec![Box::new(with_id(2)), Box::new(native())],
                    amount_out: DOLLARS,
                    amount_in_max: 500u128,
                    send_to: account(0),
                    keep_alive: false,
                },
            ),
        ));

        seeds.push((
            "asset_conversion_swap_close_to_empty_pool".to_string(),
            data,
        ));
    }

    // =========================================================================
    // quote_price_exact_tokens_for_tokens_matches_execution
    //
    // account(0) provides liquidity (10_000 * DOLLARS : 200 asset2).
    // account(1) gets 1 asset2 and swaps it for native (exact-in).
    // Mirrors: quote_price_exact_tokens_for_tokens_matches_execution test.
    // =========================================================================
    {
        let mut data = Vec::new();

        // account(0) creates asset 2 and mints to itself
        asset_create_and_mint(&mut data, 0, 2, 0, 1000);
        // mint 1 asset2 to account(1) (the swapper)
        data.extend(enc(
            false,
            0,
            RuntimeCall::Assets(pallet_assets::Call::mint {
                id: Compact(2u32),
                beneficiary: lookup(1),
                amount: 1u128,
            }),
        ));

        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
            }),
        ));

        // add_liquidity: 10_000 * DOLLARS native : 200 asset2
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
                amount1_desired: 10_000 * DOLLARS,
                amount2_desired: 200u128,
                amount1_min: 1u128,
                amount2_min: 1u128,
                mint_to: account(0),
            }),
        ));

        // account(1) swaps 1 asset2 → native (exact-in, keep_alive=false)
        data.extend(enc(
            false,
            1,
            RuntimeCall::AssetConversion(
                pallet_asset_conversion::Call::swap_exact_tokens_for_tokens {
                    path: vec![Box::new(with_id(2)), Box::new(native())],
                    amount_in: 1u128,
                    amount_out_min: 1u128,
                    send_to: account(1),
                    keep_alive: false,
                },
            ),
        ));

        seeds.push((
            "asset_conversion_quote_exact_tokens_matches_execution".to_string(),
            data,
        ));
    }

    // =========================================================================
    // cannot_block_pool_creation
    //
    // account(1) (attacker) creates extra tokens (ids 10–24) to fill up the
    // future pool account's consumer-references.
    // account(0) (user) creates asset 2 and still successfully opens the pool
    // and adds liquidity, proving the attacker cannot block creation.
    // Mirrors: cannot_block_pool_creation test.
    // =========================================================================
    {
        let mut data = Vec::new();

        // account(0) creates asset 2 and mints to itself
        asset_create_and_mint(&mut data, 0, 2, 0, 10_000);

        // account(1) creates tokens 10..24 (attack vectors)
        for i in 10u32..25 {
            data.extend(enc(
                false,
                1,
                RuntimeCall::Assets(pallet_assets::Call::create {
                    id: Compact(i),
                    admin: lookup(1),
                    min_balance: 1u128,
                }),
            ));
            data.extend(enc(
                false,
                1,
                RuntimeCall::Assets(pallet_assets::Call::mint {
                    id: Compact(i),
                    beneficiary: lookup(1),
                    amount: 1000u128,
                }),
            ));
        }

        // account(0) still opens the target pool
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
            }),
        ));

        // account(0) adds liquidity successfully despite the attacker
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
                amount1_desired: 10_000 * DOLLARS,
                amount2_desired: 100u128,
                amount1_min: 1u128,
                amount2_min: 10u128,
                mint_to: account(0),
            }),
        ));

        seeds.push((
            "asset_conversion_cannot_block_pool_creation".to_string(),
            data,
        ));
    }

    // =========================================================================
    // add_tiny_liquidity_directly_to_pool_address
    //
    // Two pools: Native/WithId(2) and Native/WithId(3).
    // Add liquidity to both even when the pool address already holds some
    // native (simulated by genesis / prior txs).
    // Mirrors: add_tiny_liquidity_directly_to_pool_address test.
    // =========================================================================
    {
        let mut data = Vec::new();

        asset_create_and_mint(&mut data, 0, 2, 0, 1000);
        asset_create_and_mint(&mut data, 0, 3, 0, 1000);

        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
            }),
        ));
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::create_pool {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(3)),
            }),
        ));

        // add_liquidity to pool 1: 10_000 * DOLLARS native : 10 asset2
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(2)),
                amount1_desired: 10_000 * DOLLARS,
                amount2_desired: 10u128,
                amount1_min: 10_000 * DOLLARS,
                amount2_min: 10u128,
                mint_to: account(0),
            }),
        ));

        // add_liquidity to pool 2: 10_000 * DOLLARS native : 10 asset3
        data.extend(enc(
            false,
            0,
            RuntimeCall::AssetConversion(pallet_asset_conversion::Call::add_liquidity {
                asset1: Box::new(native()),
                asset2: Box::new(with_id(3)),
                amount1_desired: 10_000 * DOLLARS,
                amount2_desired: 10u128,
                amount1_min: 10_000 * DOLLARS,
                amount2_min: 10u128,
                mint_to: account(0),
            }),
        ));

        seeds.push((
            "asset_conversion_add_tiny_liquidity_to_pool_address".to_string(),
            data,
        ));
    }

    seeds
}
