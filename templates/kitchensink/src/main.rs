#![warn(clippy::pedantic)]
use codec::{DecodeLimit, Encode};
use frame_support::{
    dispatch::GetDispatchInfo,
    pallet_prelude::Weight,
    traits::{IntegrityTest, OriginTrait, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use frame_system::Account;
use kitchensink_runtime::{
    constants::{currency::DOLLARS, time::SLOT_DURATION},
    AccountId, AllPalletsWithSystem, Balances, Broker, Executive, Runtime, RuntimeCall,
    RuntimeOrigin, Timestamp,
};
use node_primitives::Balance;
use pallet_balances::{Holds, TotalIssuance};
use sp_consensus_babe::{
    digests::{PreDigest, SecondaryPlainPreDigest},
    Slot, BABE_ENGINE_ID,
};
use sp_runtime::{
    testing::H256,
    traits::{Dispatchable, Header},
    Digest, DigestItem, FixedU64, Perbill, Storage,
};
use sp_state_machine::BasicExternalities;
use std::{
    iter,
    time::{Duration, Instant},
};

fn main() {
    let accounts: Vec<AccountId> = (0..5).map(|i| [i; 32].into()).collect();
    let genesis = generate_genesis(&accounts);

    ziggy::fuzz!(|data: &[u8]| {
        process_input(&accounts, &genesis, data);
    });
}
#[allow(clippy::too_many_lines)]
fn generate_genesis(accounts: &[AccountId]) -> Storage {
    use kitchensink_runtime::{
        AllianceConfig, AllianceMotionConfig, AssetsConfig, AuthorityDiscoveryConfig, BabeConfig,
        BalancesConfig, BeefyConfig, BrokerConfig, CouncilConfig, DemocracyConfig, ElectionsConfig,
        GluttonConfig, GrandpaConfig, ImOnlineConfig, IndicesConfig, MixnetConfig,
        NominationPoolsConfig, PoolAssetsConfig, ReviveConfig, RuntimeGenesisConfig,
        SafeModeConfig, SessionConfig, SessionKeys, SocietyConfig, StakingConfig, SudoConfig,
        SystemConfig, TechnicalCommitteeConfig, TechnicalMembershipConfig,
        TransactionPaymentConfig, TransactionStorageConfig, TreasuryConfig, TxPauseConfig,
        VestingConfig,
    };
    use pallet_grandpa::AuthorityId as GrandpaId;
    use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
    use pallet_staking::StakerStatus;
    use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
    use sp_consensus_babe::AuthorityId as BabeId;
    use sp_core::{sr25519::Public as MixnetId, Pair};
    use sp_runtime::{app_crypto::ByteArray, BuildStorage};

    const ENDOWMENT: Balance = 10_000_000 * DOLLARS;
    const STASH: Balance = ENDOWMENT / 1000;

    let beefy_pair = sp_consensus_beefy::ecdsa_crypto::Pair::generate().0;

    let stakers = vec![(
        [0; 32].into(),
        [0; 32].into(),
        STASH,
        StakerStatus::Validator,
    )];

    let num_endowed_accounts = accounts.len();

    let mut storage = RuntimeGenesisConfig {
        system: SystemConfig::default(),
        balances: BalancesConfig {
            balances: accounts.iter().cloned().map(|x| (x, ENDOWMENT)).collect(),
            dev_accounts: None,
        },
        indices: IndicesConfig { indices: vec![] },
        session: SessionConfig {
            keys: vec![(
                [0; 32].into(),
                [0; 32].into(),
                SessionKeys {
                    grandpa: GrandpaId::from_slice(&[0; 32]).unwrap(),
                    babe: BabeId::from_slice(&[0; 32]).unwrap(),
                    beefy: beefy_pair.public(),
                    im_online: ImOnlineId::from_slice(&[0; 32]).unwrap(),
                    authority_discovery: AuthorityDiscoveryId::from_slice(&[0; 32]).unwrap(),
                    mixnet: MixnetId::from_slice(&[0; 32]).unwrap().into(),
                },
            )],
            non_authority_keys: vec![],
        },
        beefy: BeefyConfig::default(),
        staking: StakingConfig {
            validator_count: 0u32,
            minimum_validator_count: 0u32,
            invulnerables: vec![[0; 32].into()],
            slash_reward_fraction: Perbill::from_percent(10),
            stakers,
            ..Default::default()
        },
        democracy: DemocracyConfig::default(),
        elections: ElectionsConfig {
            members: accounts
                .iter()
                .take(num_endowed_accounts.div_ceil(2))
                .cloned()
                .map(|member| (member, STASH))
                .collect(),
        },
        council: CouncilConfig::default(),
        technical_committee: TechnicalCommitteeConfig {
            members: accounts
                .iter()
                .take(num_endowed_accounts.div_ceil(2))
                .cloned()
                .collect(),
            ..Default::default()
        },
        sudo: SudoConfig { key: None },
        babe: BabeConfig {
            authorities: vec![],
            epoch_config: kitchensink_runtime::BABE_GENESIS_EPOCH_CONFIG,
            ..Default::default()
        },
        im_online: ImOnlineConfig { keys: vec![] },
        authority_discovery: AuthorityDiscoveryConfig::default(),
        grandpa: GrandpaConfig::default(),
        technical_membership: TechnicalMembershipConfig::default(),
        treasury: TreasuryConfig::default(),
        society: SocietyConfig { pot: 0 },
        vesting: VestingConfig::default(),
        assets: AssetsConfig {
            // This asset is used by the NIS pallet as counterpart currency.
            assets: vec![(9, [0; 32].into(), true, 1)],
            ..Default::default()
        },
        transaction_storage: TransactionStorageConfig::default(),
        transaction_payment: TransactionPaymentConfig::default(),
        alliance: AllianceConfig::default(),
        alliance_motion: AllianceMotionConfig::default(),
        nomination_pools: NominationPoolsConfig {
            min_create_bond: 10 * DOLLARS,
            min_join_bond: DOLLARS,
            ..Default::default()
        },
        glutton: GluttonConfig {
            compute: FixedU64::default(),
            storage: FixedU64::default(),
            trash_data_count: Default::default(),
            ..Default::default()
        },
        pool_assets: PoolAssetsConfig::default(),
        safe_mode: SafeModeConfig::default(),
        tx_pause: TxPauseConfig::default(),
        mixnet: MixnetConfig::default(),
        broker: BrokerConfig::default(),
        revive: ReviveConfig::default(),
    }
    .build_storage()
    .unwrap();
    BasicExternalities::execute_with_storage(&mut storage, || {
        // We set the configuration for the broker pallet
        Broker::configure(
            RuntimeOrigin::root(),
            pallet_broker::ConfigRecord {
                advance_notice: 2,
                interlude_length: 1,
                leadin_length: 1,
                ideal_bulk_proportion: Perbill::default(),
                limit_cores_offered: None,
                region_length: 3,
                renewal_bump: Perbill::from_percent(10),
                contribution_timeout: 5,
            },
        )
        .unwrap();
        /*
        // WIP: found the society before each input
        RuntimeCall::Sudo(pallet_sudo::Call::sudo {
            call: RuntimeCall::Society(pallet_society::Call::found_society {
                founder: AccountId::from([0; 32]).into(),
                max_members: 2,
                max_intake: 2,
                max_strikes: 2,
                candidate_deposit: 1_000,
                rules: vec![0],
            })
            .into(),
        })
        .dispatch(RuntimeOrigin::root())
        .unwrap();
        */
    });
    storage
}

fn recursively_find_call(call: RuntimeCall, matches_on: fn(&RuntimeCall) -> bool) -> bool {
    if let RuntimeCall::Utility(
        pallet_utility::Call::batch { calls }
        | pallet_utility::Call::force_batch { calls }
        | pallet_utility::Call::batch_all { calls },
    ) = call
    {
        for call in calls {
            if recursively_find_call(call.clone(), matches_on) {
                return true;
            }
        }
    } else if let RuntimeCall::Utility(pallet_utility::Call::if_else { main, fallback }) = call {
        return recursively_find_call(*main.clone(), matches_on)
            || recursively_find_call(*fallback.clone(), matches_on);
    } else if let RuntimeCall::Lottery(pallet_lottery::Call::buy_ticket { call })
    | RuntimeCall::Multisig(pallet_multisig::Call::as_multi_threshold_1 {
        call, ..
    })
    | RuntimeCall::Utility(
        pallet_utility::Call::as_derivative { call, .. }
        | pallet_utility::Call::with_weight { call, .. }
        | pallet_utility::Call::dispatch_as_fallible { call, .. }
        | pallet_utility::Call::dispatch_as { call, .. },
    )
    | RuntimeCall::Sudo(
        pallet_sudo::Call::sudo { call, .. }
        | pallet_sudo::Call::sudo_unchecked_weight { call, .. },
    )
    | RuntimeCall::Whitelist(
        pallet_whitelist::Call::dispatch_whitelisted_call_with_preimage { call, .. },
    )
    | RuntimeCall::Proxy(
        pallet_proxy::Call::proxy { call, .. } | pallet_proxy::Call::proxy_announced { call, .. },
    )
    | RuntimeCall::Revive(pallet_revive::Call::dispatch_as_fallback_account { call })
    | RuntimeCall::Recovery(pallet_recovery::Call::as_recovered { call, .. })
    | RuntimeCall::Council(
        pallet_collective::Call::propose { proposal: call, .. }
        | pallet_collective::Call::execute { proposal: call, .. },
    )
    | RuntimeCall::AllianceMotion(
        pallet_collective::Call::propose { proposal: call, .. }
        | pallet_collective::Call::execute { proposal: call, .. },
    )
    | RuntimeCall::TechnicalCommittee(
        pallet_collective::Call::propose { proposal: call, .. }
        | pallet_collective::Call::execute { proposal: call, .. },
    ) = call
    {
        return recursively_find_call(*call, matches_on);
    } else if matches_on(&call) {
        return true;
    }
    false
}

fn call_filter(call: &RuntimeCall) -> bool {
    // We disallow referenda calls with root origin
    matches!(
        &call,
        RuntimeCall::Referenda(pallet_referenda::Call::submit {
            proposal_origin: matching_origin,
            ..
        }) | RuntimeCall::RankedPolls(pallet_referenda::Call::submit {
            proposal_origin: matching_origin,
            ..
        }) if RuntimeOrigin::from(*matching_origin.clone()).caller() == RuntimeOrigin::root().caller()
    )
    // We disallow batches of referenda
    // See https://github.com/paritytech/srlabs_findings/issues/296
    || matches!(
            &call,
            RuntimeCall::Referenda(pallet_referenda::Call::submit { .. })
        )
    // We filter out contracts call that will take too long because of fuzzer instrumentation
    || matches!(
            &call,
            RuntimeCall::Contracts(
                pallet_contracts::Call::instantiate_with_code { .. } |
                pallet_contracts::Call::upload_code { .. } |
                pallet_contracts::Call::instantiate_with_code_old_weight { .. } |
                pallet_contracts::Call::migrate { .. }
            )
        )
    || matches!(
            &call,
            RuntimeCall::Revive(
                pallet_revive::Call::instantiate_with_code { .. } |
                pallet_revive::Call::upload_code { .. }
            )
        )
    // We filter out safe_mode calls, as they block timestamps from being set.
    || matches!(&call, RuntimeCall::SafeMode(..))
    // We filter out store extrinsics because BasicExternalities does not support them.
    || matches!(
            &call,
            RuntimeCall::TransactionStorage(pallet_transaction_storage::Call::store { .. })
                | RuntimeCall::Remark(pallet_remark::Call::store { .. })
        )
    || matches!(
            &call,
            RuntimeCall::NominationPools(..)
    )
    || matches!(
            &call,
            RuntimeCall::MetaTx(pallet_meta_tx::Call::dispatch { .. })
    )
    || matches!(
            &call,
            RuntimeCall::AssetRewards(pallet_asset_rewards::Call::create_pool { .. })
    )
    || matches!(
            &call,
            RuntimeCall::VoterList(pallet_bags_list::Call::rebag {  .. })
    )
    || matches!(
            &call,
            RuntimeCall::Assets(pallet_assets::Call::set_reserves {  .. })
            | RuntimeCall::PoolAssets(pallet_assets::Call::set_reserves {  .. })
    )
}

fn process_input(accounts: &[AccountId], genesis: &Storage, data: &[u8]) {
    // We build the list of extrinsics we will execute
    let mut extrinsic_data = data;
    // Vec<(next_block, origin, extrinsic)>
    let extrinsics: Vec<(bool, u8, RuntimeCall)> =
        iter::from_fn(|| DecodeLimit::decode_with_depth_limit(64, &mut extrinsic_data).ok())
            .filter(|(_, _, x): &(_, _, RuntimeCall)| {
                !recursively_find_call(x.clone(), call_filter)
            })
            .collect();
    if extrinsics.is_empty() {
        return;
    }

    let mut block: u32 = 1;
    let mut weight: Weight = Weight::zero();
    let mut elapsed: Duration = Duration::ZERO;

    BasicExternalities::execute_with_storage(&mut genesis.clone(), || {
        let initial_total_issuance = TotalIssuance::<Runtime>::get();

        initialize_block(block);

        for (next_block, origin, extrinsic) in extrinsics {
            if next_block {
                // We end the current block
                finalize_block(elapsed);

                block += 1;
                weight = Weight::zero();
                elapsed = Duration::ZERO;

                // We start the next block
                initialize_block(block);
            }

            let origin = accounts[origin as usize % accounts.len()].clone();

            // We do not continue if the origin account does not have a free balance
            let account = Account::<Runtime>::get(&origin);
            if account.data.free == 0 {
                #[cfg(not(feature = "fuzzing"))]
                println!("\n    origin {origin:?} does not have free balance, skipping");
                continue;
            }

            #[cfg(not(feature = "fuzzing"))]
            println!("\n    origin:     {origin:?}");
            #[cfg(not(feature = "fuzzing"))]
            println!("    call:       {extrinsic:?}");

            let pre_weight = extrinsic.get_dispatch_info().call_weight;
            let cumulative_weight = weight.saturating_add(pre_weight);
            if cumulative_weight.ref_time() >= 2 * WEIGHT_REF_TIME_PER_SECOND {
                #[cfg(not(feature = "fuzzing"))]
                println!("Extrinsic would exhaust block weight, skipping");
                continue;
            }
            weight = cumulative_weight;

            let now = Instant::now(); // We get the current time for timing purposes.
            let res = extrinsic.dispatch(RuntimeOrigin::signed(origin));
            elapsed += now.elapsed();

            #[cfg(not(feature = "fuzzing"))]
            println!("    result:     {res:?}");

            let actual_weight = res.unwrap_or_else(|e| e.post_info).actual_weight;
            let post_weight = actual_weight.unwrap_or_default();
            assert!(pre_weight.ref_time().saturating_mul(2) >= post_weight.ref_time(), "Pre-dispatch weight ref time ({}) is smaller than post-dispatch weight ref time ({})", pre_weight.ref_time(), post_weight.ref_time());
            assert!(pre_weight.proof_size().saturating_mul(2) >= post_weight.proof_size(), "Pre-dispatch weight proof size ({}) is smaller than post-dispatch weight proof size ({})", pre_weight.proof_size(), post_weight.proof_size());
        }

        finalize_block(elapsed);

        check_invariants(block, initial_total_issuance);
    });
}
fn initialize_block(block: u32) {
    #[cfg(not(feature = "fuzzing"))]
    println!("\ninitializing block {block}");

    let pre_digest = Digest {
        logs: vec![DigestItem::PreRuntime(
            BABE_ENGINE_ID,
            PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
                slot: Slot::from(u64::from(block)),
                authority_index: 42,
            })
            .encode(),
        )],
    };

    Executive::initialize_block(&Header::new(
        block,
        H256::default(),
        H256::default(),
        H256::default(),
        pre_digest,
    ));

    #[cfg(not(feature = "fuzzing"))]
    println!("  setting timestamp");
    Timestamp::set(RuntimeOrigin::none(), u64::from(block) * SLOT_DURATION).unwrap();
}

fn finalize_block(elapsed: Duration) {
    #[cfg(not(feature = "fuzzing"))]
    println!("\n  time spent: {elapsed:?}");
    assert!(elapsed.as_secs() <= 1, "block execution took too much time");

    #[cfg(not(feature = "fuzzing"))]
    println!("\n  finalizing block");
    Executive::finalize_block();
}

fn check_invariants(block: u32, initial_total_issuance: Balance) {
    // After execution of all blocks, we run invariants
    let mut counted_free: Balance = 0;
    let mut counted_reserved: Balance = 0;
    for (account, info) in Account::<Runtime>::iter() {
        let consumers = info.consumers;
        let providers = info.providers;
        assert!(!(consumers > 0 && providers == 0), "Invalid c/p state");
        counted_free += info.data.free;
        counted_reserved += info.data.reserved;
        let max_lock: Balance = Balances::locks(&account)
            .iter()
            .map(|l| l.amount)
            .max()
            .unwrap_or_default();
        assert_eq!(
            max_lock, info.data.frozen,
            "Max lock should be equal to frozen balance"
        );
        let sum_holds: Balance = Holds::<Runtime>::get(&account)
            .iter()
            .map(|l| l.amount)
            .sum();
        assert!(
            sum_holds <= info.data.reserved,
            "Sum of all holds ({sum_holds}) should be less than or equal to reserved balance {}",
            info.data.reserved
        );
    }
    let total_issuance = TotalIssuance::<Runtime>::get();
    let counted_issuance = counted_free + counted_reserved;
    assert_eq!(total_issuance, counted_issuance);
    assert!(total_issuance <= initial_total_issuance);
    // We run all developer-defined integrity tests
    AllPalletsWithSystem::integrity_test();
    AllPalletsWithSystem::try_state(block, TryStateSelect::All).unwrap();
}
