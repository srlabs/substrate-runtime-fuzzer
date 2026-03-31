#![warn(clippy::pedantic)]
#![allow(clippy::too_many_lines)]

//! Seed generator derived from `substrate/frame/staking/src/tests.rs`.
//!
//! ## Genesis state (kitchensink)
//! - `account(0)` = stash+controller, bonded as Validator, `STASH = ENDOWMENT/1000 = 10_000 DOLLARS`
//! - `account(0)` is the sole **invulnerable** validator
//! - `accounts(1..=4)` – unbonded, each has `ENDOWMENT = 10_000_000 DOLLARS`
//! - `SessionsPerEra = 6`, `BondingDuration = 672 eras`, `MaxUnlockingChunks = 32`
//!
//! ## Signed calls (no root required)
//! `bond`, `bond_extra`, `unbond`, `withdraw_unbonded`, `validate`, `nominate`,
//! `chill`, `set_payee`, `set_controller`, `rebond`, `payout_stakers`,
//! `payout_stakers_by_page`, `reap_stash`, `kick`, `chill_other`,
//! `force_apply_min_commission`, `update_payee`, `migrate_currency`
//!
//! ## Root/Admin-only calls – skipped
//! `set_validator_count`, `increase_validator_count`, `scale_validator_count`,
//! `force_no_eras`, `force_new_era`, `set_invulnerables`, `force_unstake`,
//! `force_new_era_always`, `cancel_deferred_slash`, `set_staking_configs`,
//! `set_min_commission`, `deprecate_controller_batch`, `restore_ledger`
//!
//! ## Note on `payout_stakers`
//! Era 0 is active at block 1; it hasn't ended yet.  `payout_stakers(account(0), 0)`
//! will return `InvalidEraToReward`.  The seed is still included because the fuzzer
//! can mutate `advance_block` flags to drive the runtime through era boundaries.
//!
//! ## Note on `NominationPools`
//! NominationPools calls are filtered by the kitchensink fuzzer – excluded here.

use codec::Encode;
use kitchensink_runtime::RuntimeCall;
use node_primitives::AccountIndex;
use pallet_staking::{RewardDestination, ValidatorPrefs};
use sp_runtime::{MultiAddress, Perbill};
use std::{fs, path::Path};

// ─── constants ───────────────────────────────────────────────────────────────

const MILLICENTS: u128 = 1_000_000_000;
const CENTS: u128 = 1_000 * MILLICENTS;
const DOLLARS: u128 = 100 * CENTS;

/// Initial free balance of each fuzzer account.
const ENDOWMENT: u128 = 10_000_000 * DOLLARS;
/// Amount account(0) has already bonded (= ENDOWMENT / 1000).
const GENESIS_STASH: u128 = ENDOWMENT / 1000;
/// Bond amount used in most seeds (above ED, well below ENDOWMENT).
const BOND: u128 = 1_000 * DOLLARS;

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
    // bond → validate
    //
    // account(1) bonds and declares itself a validator with default prefs.
    // This is the simplest path to having two validators on chain.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND,
                payee: RewardDestination::Staked,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::validate {
                prefs: ValidatorPrefs::default(),
            }),
        ));
        seeds.push(("staking_bond_and_validate".into(), data));
    }

    // =========================================================================
    // bond → validate (with commission)
    //
    // Same as above but with 10% commission and blocked=false.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND,
                payee: RewardDestination::Stash,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::validate {
                prefs: ValidatorPrefs {
                    commission: Perbill::from_percent(10),
                    blocked: false,
                },
            }),
        ));
        seeds.push(("staking_bond_and_validate_with_commission".into(), data));
    }

    // =========================================================================
    // bond → nominate
    //
    // account(1) bonds and nominates account(0) (the genesis invulnerable validator).
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND,
                payee: RewardDestination::Staked,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::nominate {
                targets: vec![lookup(0)],
            }),
        ));
        seeds.push(("staking_bond_and_nominate".into(), data));
    }

    // =========================================================================
    // bond → nominate (multiple accounts, single target)
    //
    // accounts(1..=4) all bond and nominate account(0).
    // =========================================================================
    {
        let mut data = Vec::new();
        for origin in 1u8..=4 {
            data.extend(enc(
                false,
                origin,
                RuntimeCall::Staking(pallet_staking::Call::bond {
                    value: BOND,
                    payee: RewardDestination::Staked,
                }),
            ));
            data.extend(enc(
                false,
                origin,
                RuntimeCall::Staking(pallet_staking::Call::nominate {
                    targets: vec![lookup(0)],
                }),
            ));
        }
        seeds.push(("staking_multi_nominator".into(), data));
    }

    // =========================================================================
    // bond → chill
    //
    // Bonding followed immediately by chill (idle role). The account
    // remains bonded but is neither validating nor nominating.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND,
                payee: RewardDestination::Staked,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::chill {}),
        ));
        seeds.push(("staking_bond_and_chill".into(), data));
    }

    // =========================================================================
    // bond → validate → chill
    //
    // Bond, start validating, then chill. Exercises the validator→idle transition.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND,
                payee: RewardDestination::Staked,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::validate {
                prefs: ValidatorPrefs::default(),
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::chill {}),
        ));
        seeds.push(("staking_bond_validate_chill".into(), data));
    }

    // =========================================================================
    // bond → nominate → chill
    //
    // Bond, nominate, then chill. Exercises the nominator→idle transition.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND,
                payee: RewardDestination::Staked,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::nominate {
                targets: vec![lookup(0)],
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::chill {}),
        ));
        seeds.push(("staking_bond_nominate_chill".into(), data));
    }

    // =========================================================================
    // switching_roles
    //
    // account(1) bonds → nominates → switches to validating → switches back.
    // Mirrors the `switching_roles` test.
    // =========================================================================
    {
        let mut data = Vec::new();
        // Bond and nominate
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND * 2,
                payee: RewardDestination::Account(account(1)),
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::nominate {
                targets: vec![lookup(0)],
            }),
        ));
        // Switch to validator
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::validate {
                prefs: ValidatorPrefs::default(),
            }),
        ));
        // Switch back to nominator
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::nominate {
                targets: vec![lookup(0)],
            }),
        ));
        // Chill
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::chill {}),
        ));
        seeds.push(("staking_switching_roles".into(), data));
    }

    // =========================================================================
    // bond_extra_works
    //
    // account(0) is already bonded (GENESIS_STASH). bond_extra adds more.
    // Uses `u128::MAX` for max_additional to bond everything available.
    // =========================================================================
    {
        let mut data = Vec::new();
        // Small bond_extra
        data.extend(enc(
            false,
            0,
            RuntimeCall::Staking(pallet_staking::Call::bond_extra {
                max_additional: BOND,
            }),
        ));
        // Bond everything remaining
        data.extend(enc(
            false,
            0,
            RuntimeCall::Staking(pallet_staking::Call::bond_extra {
                max_additional: u128::MAX,
            }),
        ));
        seeds.push(("staking_bond_extra".into(), data));
    }

    // =========================================================================
    // bond_extra on a freshly bonded account
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND,
                payee: RewardDestination::Staked,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond_extra {
                max_additional: BOND,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::validate {
                prefs: ValidatorPrefs::default(),
            }),
        ));
        seeds.push(("staking_bond_then_bond_extra".into(), data));
    }

    // =========================================================================
    // bond → unbond
    //
    // bond + immediate unbond. The unbond creates an unlocking chunk but the
    // BondingDuration (672 eras) means it won't mature for a very long time.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND * 2,
                payee: RewardDestination::Staked,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::unbond { value: BOND }),
        ));
        seeds.push(("staking_bond_unbond".into(), data));
    }

    // =========================================================================
    // bond → unbond → withdraw_unbonded
    //
    // After unbonding, withdraw_unbonded is a no-op (bonding period not elapsed)
    // but the call itself must succeed.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND * 2,
                payee: RewardDestination::Staked,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::unbond { value: BOND }),
        ));
        // withdraw immediately – unlocking chunk too young, ledger unchanged
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::withdraw_unbonded {
                num_slashing_spans: 0,
            }),
        ));
        seeds.push(("staking_bond_unbond_withdraw".into(), data));
    }

    // =========================================================================
    // bond → unbond → rebond
    //
    // Rebond re-activates an unlocking chunk before it matures.
    // Mirrors `rebond_works`.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND * 3,
                payee: RewardDestination::Staked,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::unbond { value: BOND * 2 }),
        ));
        // Rebond half of what was unbonded
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::rebond { value: BOND }),
        ));
        seeds.push(("staking_bond_unbond_rebond".into(), data));
    }

    // =========================================================================
    // bond → unbond multiple chunks → rebond all
    //
    // Creates several unlocking chunks across different blocks, then rebonds them all.
    // Uses `advance_block = true` to place each unbond in a new block so they
    // each create a distinct chunk.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND * 6,
                payee: RewardDestination::Staked,
            }),
        ));
        // Three unbond chunks in three consecutive blocks
        for i in 0u8..3 {
            data.extend(enc(
                i > 0,
                1,
                RuntimeCall::Staking(pallet_staking::Call::unbond { value: BOND }),
            ));
        }
        // Rebond everything
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::rebond { value: BOND * 3 }),
        ));
        seeds.push(("staking_multiple_unbond_chunks_then_rebond".into(), data));
    }

    // =========================================================================
    // set_payee – all variants
    //
    // account(0) is already bonded; cycle through all payee types.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Staking(pallet_staking::Call::set_payee {
                payee: RewardDestination::Staked,
            }),
        ));
        data.extend(enc(
            false,
            0,
            RuntimeCall::Staking(pallet_staking::Call::set_payee {
                payee: RewardDestination::Stash,
            }),
        ));
        data.extend(enc(
            false,
            0,
            RuntimeCall::Staking(pallet_staking::Call::set_payee {
                payee: RewardDestination::Account(account(1)),
            }),
        ));
        data.extend(enc(
            false,
            0,
            RuntimeCall::Staking(pallet_staking::Call::set_payee {
                payee: RewardDestination::None,
            }),
        ));
        // Reset back to Staked
        data.extend(enc(
            false,
            0,
            RuntimeCall::Staking(pallet_staking::Call::set_payee {
                payee: RewardDestination::Staked,
            }),
        ));
        seeds.push(("staking_set_payee_variants".into(), data));
    }

    // =========================================================================
    // set_payee after bond + nominate
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND,
                payee: RewardDestination::Staked,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::nominate {
                targets: vec![lookup(0)],
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::set_payee {
                payee: RewardDestination::Account(account(2)),
            }),
        ));
        seeds.push(("staking_bond_nominate_set_payee".into(), data));
    }

    // =========================================================================
    // set_controller (deprecated but still callable)
    //
    // No-op in practice (controller already == stash), but exercises the code path.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Staking(pallet_staking::Call::set_controller {}),
        ));
        seeds.push(("staking_set_controller".into(), data));
    }

    // =========================================================================
    // validate → blocking_and_kicking_works
    //
    // account(1) bonds + validates with blocked=true, account(2) bonds + nominates
    // account(1), then account(1) kicks account(2).
    // =========================================================================
    {
        let mut data = Vec::new();
        // account(1): bond + validate (blocked)
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND,
                payee: RewardDestination::Staked,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::validate {
                prefs: ValidatorPrefs {
                    commission: Perbill::zero(),
                    blocked: true,
                },
            }),
        ));
        // account(2): bond + nominate account(1)
        data.extend(enc(
            false,
            2,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND,
                payee: RewardDestination::Staked,
            }),
        ));
        data.extend(enc(
            false,
            2,
            RuntimeCall::Staking(pallet_staking::Call::nominate {
                targets: vec![lookup(1)],
            }),
        ));
        // account(1): kick account(2) as nominator
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::kick {
                who: vec![lookup(2)],
            }),
        ));
        seeds.push(("staking_blocking_and_kicking".into(), data));
    }

    // =========================================================================
    // payout_stakers
    //
    // Era 0 is active but not completed → InvalidEraToReward.
    // Still a valuable seed: the fuzzer can add block-advancement to drive era
    // completion and then this seed would succeed.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::payout_stakers {
                validator_stash: account(0),
                era: 0,
            }),
        ));
        seeds.push(("staking_payout_stakers_era0".into(), data));
    }

    // =========================================================================
    // payout_stakers_by_page
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::payout_stakers_by_page {
                validator_stash: account(0),
                era: 0,
                page: 0,
            }),
        ));
        seeds.push(("staking_payout_stakers_by_page".into(), data));
    }

    // =========================================================================
    // reap_stash
    //
    // account(0) is invulnerable and fully funded → FundedTarget (error).
    // Still useful: the fuzzer may mutate the stash or the conditions to hit
    // the success path (e.g. after slashing or balance drain).
    // =========================================================================
    {
        let mut data = Vec::new();
        // Try to reap account(0) – funded, should fail
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::reap_stash {
                stash: account(0),
                num_slashing_spans: 0,
            }),
        ));
        // Try to reap account(1) – not stashed, should fail (NotStash)
        data.extend(enc(
            false,
            0,
            RuntimeCall::Staking(pallet_staking::Call::reap_stash {
                stash: account(1),
                num_slashing_spans: 0,
            }),
        ));
        seeds.push(("staking_reap_stash".into(), data));
    }

    // =========================================================================
    // chill_other
    //
    // Without `MinNominatorBond` / `ChillThreshold` set, chill_other returns
    // CannotChillOther.  The seed exercises the code path; the fuzzer can add
    // set_staking_configs (via other seeds) or mutate this one.
    // =========================================================================
    {
        let mut data = Vec::new();
        // account(1) bonds + nominates
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND,
                payee: RewardDestination::Staked,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::nominate {
                targets: vec![lookup(0)],
            }),
        ));
        // account(2) tries to chill account(1) – CannotChillOther without thresholds
        data.extend(enc(
            false,
            2,
            RuntimeCall::Staking(pallet_staking::Call::chill_other { stash: account(1) }),
        ));
        seeds.push(("staking_chill_other".into(), data));
    }

    // =========================================================================
    // force_apply_min_commission
    //
    // Anyone can call this to push a validator's commission up to the on-chain
    // minimum.  Since MinCommission defaults to 0, this is a no-op but the call
    // path is exercised.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::force_apply_min_commission {
                validator_stash: account(0),
            }),
        ));
        seeds.push(("staking_force_apply_min_commission".into(), data));
    }

    // =========================================================================
    // update_payee
    //
    // Updates the stored payee for a ledger where the controller == stash.
    // Only meaningful when account has a staking ledger.
    // =========================================================================
    {
        let mut data = Vec::new();
        // account(0) is already bonded; update its payee entry
        data.extend(enc(
            false,
            0,
            RuntimeCall::Staking(pallet_staking::Call::update_payee {
                controller: account(0),
            }),
        ));
        seeds.push(("staking_update_payee".into(), data));
    }

    // =========================================================================
    // migrate_currency
    //
    // Migrates a staker's ledger from the old Currency to the new fungible
    // system.  The genesis uses the new system so this will return an error,
    // but the call path is valuable for mutation.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::migrate_currency { stash: account(0) }),
        ));
        seeds.push(("staking_migrate_currency".into(), data));
    }

    // =========================================================================
    // Full lifecycle: bond → validate → bond_extra → unbond → withdraw_unbonded
    //
    // Mirrors `bond_extra_and_withdraw_unbonded_works`. The withdraw won't
    // release funds (BondingDuration not met) but exercises all call paths.
    // =========================================================================
    {
        let mut data = Vec::new();
        // Bond
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND * 2,
                payee: RewardDestination::Staked,
            }),
        ));
        // Validate
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::validate {
                prefs: ValidatorPrefs::default(),
            }),
        ));
        // Bond extra
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond_extra {
                max_additional: BOND,
            }),
        ));
        // Unbond part
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::unbond { value: BOND * 2 }),
        ));
        // Try to withdraw (no-op, chunk not mature)
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::withdraw_unbonded {
                num_slashing_spans: 0,
            }),
        ));
        seeds.push(("staking_full_lifecycle".into(), data));
    }

    // =========================================================================
    // Full lifecycle with chill: bond → nominate → chill → unbond → rebond
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            2,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND * 4,
                payee: RewardDestination::Stash,
            }),
        ));
        data.extend(enc(
            false,
            2,
            RuntimeCall::Staking(pallet_staking::Call::nominate {
                targets: vec![lookup(0)],
            }),
        ));
        data.extend(enc(
            false,
            2,
            RuntimeCall::Staking(pallet_staking::Call::chill {}),
        ));
        data.extend(enc(
            false,
            2,
            RuntimeCall::Staking(pallet_staking::Call::unbond { value: BOND * 2 }),
        ));
        data.extend(enc(
            false,
            2,
            RuntimeCall::Staking(pallet_staking::Call::rebond { value: BOND }),
        ));
        data.extend(enc(
            false,
            2,
            RuntimeCall::Staking(pallet_staking::Call::withdraw_unbonded {
                num_slashing_spans: 0,
            }),
        ));
        seeds.push(("staking_nominate_chill_unbond_rebond".into(), data));
    }

    // =========================================================================
    // Cross-block lifecycle
    //
    // Exercises staking calls across multiple blocks (advance_block = true
    // finalizes the current block and starts a new one before each extrinsic).
    // =========================================================================
    {
        let mut data = Vec::new();
        // Block 1: bond
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND * 3,
                payee: RewardDestination::Staked,
            }),
        ));
        // Block 2: validate
        data.extend(enc(
            true,
            1,
            RuntimeCall::Staking(pallet_staking::Call::validate {
                prefs: ValidatorPrefs::default(),
            }),
        ));
        // Block 3: bond_extra
        data.extend(enc(
            true,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond_extra {
                max_additional: BOND,
            }),
        ));
        // Block 4: unbond
        data.extend(enc(
            true,
            1,
            RuntimeCall::Staking(pallet_staking::Call::unbond { value: BOND }),
        ));
        // Block 5: rebond
        data.extend(enc(
            true,
            1,
            RuntimeCall::Staking(pallet_staking::Call::rebond { value: BOND / 2 }),
        ));
        // Block 6: chill
        data.extend(enc(
            true,
            1,
            RuntimeCall::Staking(pallet_staking::Call::chill {}),
        ));
        seeds.push(("staking_cross_block_lifecycle".into(), data));
    }

    // =========================================================================
    // Double-bond fails
    //
    // Attempting to bond an already-bonded stash → AlreadyBonded.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND,
                payee: RewardDestination::Staked,
            }),
        ));
        // Second bond → AlreadyBonded
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND,
                payee: RewardDestination::Staked,
            }),
        ));
        seeds.push(("staking_double_bond_fails".into(), data));
    }

    // =========================================================================
    // validate without bond → NotStash error
    // =========================================================================
    {
        let mut data = Vec::new();
        // account(1) not bonded → NotController
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::validate {
                prefs: ValidatorPrefs::default(),
            }),
        ));
        seeds.push(("staking_validate_without_bond".into(), data));
    }

    // =========================================================================
    // nominate without bond → NotController error
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::nominate {
                targets: vec![lookup(0)],
            }),
        ));
        seeds.push(("staking_nominate_without_bond".into(), data));
    }

    // =========================================================================
    // unbond without bond → NotController error
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::unbond { value: BOND }),
        ));
        seeds.push(("staking_unbond_without_bond".into(), data));
    }

    // =========================================================================
    // bond + validate with 100% commission
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: BOND,
                payee: RewardDestination::Staked,
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::validate {
                prefs: ValidatorPrefs {
                    commission: Perbill::one(),
                    blocked: false,
                },
            }),
        ));
        seeds.push(("staking_validate_max_commission".into(), data));
    }

    // =========================================================================
    // bond with small value (≥ ED but below MinValidatorBond / MinNominatorBond
    // if those are set; exercises InsufficientBond paths on subsequent calls)
    // =========================================================================
    {
        let small_bond = DOLLARS; // 1 DOLLAR, above ED (also DOLLARS)
        let mut data = Vec::new();
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::bond {
                value: small_bond,
                payee: RewardDestination::Staked,
            }),
        ));
        // validate may succeed (MinValidatorBond defaults to 0)
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::validate {
                prefs: ValidatorPrefs::default(),
            }),
        ));
        data.extend(enc(
            false,
            1,
            RuntimeCall::Staking(pallet_staking::Call::chill {}),
        ));
        seeds.push(("staking_bond_small_value".into(), data));
    }

    // =========================================================================
    // Mixed validators + nominators: two validators, two nominators
    //
    // Exercises the voter-list and election paths when multiple stakers exist.
    // =========================================================================
    {
        let mut data = Vec::new();
        // account(1) and account(2) become validators
        for origin in [1u8, 2] {
            data.extend(enc(
                false,
                origin,
                RuntimeCall::Staking(pallet_staking::Call::bond {
                    value: BOND * 2,
                    payee: RewardDestination::Staked,
                }),
            ));
            data.extend(enc(
                false,
                origin,
                RuntimeCall::Staking(pallet_staking::Call::validate {
                    prefs: ValidatorPrefs::default(),
                }),
            ));
        }
        // account(3) and account(4) nominate both new validators
        for origin in [3u8, 4] {
            data.extend(enc(
                false,
                origin,
                RuntimeCall::Staking(pallet_staking::Call::bond {
                    value: BOND,
                    payee: RewardDestination::Staked,
                }),
            ));
            data.extend(enc(
                false,
                origin,
                RuntimeCall::Staking(pallet_staking::Call::nominate {
                    targets: vec![lookup(1), lookup(2)],
                }),
            ));
        }
        seeds.push(("staking_two_validators_two_nominators".into(), data));
    }

    // =========================================================================
    // account(0) chill (already validator, invulnerable)
    //
    // Chilling account(0) is possible but the invulnerability may keep it
    // in the active set anyway.
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Staking(pallet_staking::Call::chill {}),
        ));
        // Re-validate with different commission
        data.extend(enc(
            false,
            0,
            RuntimeCall::Staking(pallet_staking::Call::validate {
                prefs: ValidatorPrefs {
                    commission: Perbill::from_percent(5),
                    blocked: false,
                },
            }),
        ));
        seeds.push((
            "staking_existing_validator_chill_and_revalidate".into(),
            data,
        ));
    }

    // =========================================================================
    // account(0): unbond + withdraw_unbonded + bond_extra cycle
    //
    // Uses account(0)'s existing bond. Exercises rebond/unbond on account
    // with slashing spans = 0 (no slashes in genesis).
    // =========================================================================
    {
        let mut data = Vec::new();
        data.extend(enc(
            false,
            0,
            RuntimeCall::Staking(pallet_staking::Call::unbond {
                value: GENESIS_STASH / 2,
            }),
        ));
        data.extend(enc(
            false,
            0,
            RuntimeCall::Staking(pallet_staking::Call::withdraw_unbonded {
                num_slashing_spans: 0,
            }),
        ));
        data.extend(enc(
            false,
            0,
            RuntimeCall::Staking(pallet_staking::Call::bond_extra {
                max_additional: GENESIS_STASH / 2,
            }),
        ));
        seeds.push(("staking_existing_validator_unbond_rebond".into(), data));
    }

    seeds
}
