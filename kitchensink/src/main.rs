use codec::{DecodeLimit, Encode};
use frame_support::{
    dispatch::GetDispatchInfo,
    pallet_prelude::Weight,
    traits::{IntegrityTest, OriginTrait, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use kitchensink_runtime::{
    constants::{currency::DOLLARS, time::SLOT_DURATION},
    AccountId, AllPalletsWithSystem, Broker, Executive, Runtime, RuntimeCall, RuntimeOrigin,
    Timestamp,
};
use node_primitives::Balance;
use sp_consensus_babe::{
    digests::{PreDigest, SecondaryPlainPreDigest},
    Slot, BABE_ENGINE_ID,
};
use sp_runtime::{
    traits::{Dispatchable, Header},
    Digest, DigestItem, Perbill, Storage,
};
use sp_state_machine::BasicExternalities;
use std::time::{Duration, Instant};

fn genesis(accounts: &[AccountId]) -> Storage {
    use kitchensink_runtime::{
        AssetsConfig, BabeConfig, BalancesConfig, BeefyConfig, CouncilConfig, DemocracyConfig,
        ElectionsConfig, GluttonConfig, GrandpaConfig, ImOnlineConfig, IndicesConfig,
        NominationPoolsConfig, RuntimeGenesisConfig, SessionConfig, SessionKeys, SocietyConfig,
        StakingConfig, SudoConfig, TechnicalCommitteeConfig,
    };
    use pallet_grandpa::AuthorityId as GrandpaId;
    use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
    use pallet_staking::StakerStatus;
    use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
    use sp_consensus_babe::AuthorityId as BabeId;
    use sp_core::{sr25519::Public as MixnetId, Pair};
    use sp_runtime::{app_crypto::ByteArray, BuildStorage};

    let beefy_pair = sp_consensus_beefy::ecdsa_crypto::Pair::generate().0;

    let stakers = vec![(
        [0; 32].into(),
        [0; 32].into(),
        STASH,
        StakerStatus::Validator,
    )];

    let num_endowed_accounts = accounts.len();

    const ENDOWMENT: Balance = 10_000_000 * DOLLARS;
    const STASH: Balance = ENDOWMENT / 1000;

    let storage = RuntimeGenesisConfig {
        system: Default::default(),
        balances: BalancesConfig {
            balances: accounts.iter().cloned().map(|x| (x, ENDOWMENT)).collect(),
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
        },
        beefy: BeefyConfig::default(),
        staking: StakingConfig {
            validator_count: 0u32,
            // validator_count: initial_authorities.len() as u32,
            minimum_validator_count: 0u32,
            // minimum_validator_count: initial_authorities.len() as u32,
            invulnerables: vec![[0; 32].into()],
            slash_reward_fraction: Perbill::from_percent(10),
            stakers,
            ..Default::default()
        },
        democracy: DemocracyConfig::default(),
        elections: ElectionsConfig {
            members: accounts
                .iter()
                .take((num_endowed_accounts + 1) / 2)
                .cloned()
                .map(|member| (member, STASH))
                .collect(),
        },
        council: CouncilConfig::default(),
        technical_committee: TechnicalCommitteeConfig {
            members: accounts
                .iter()
                .take((num_endowed_accounts + 1) / 2)
                .cloned()
                .collect(),
            phantom: Default::default(),
        },
        sudo: SudoConfig { key: None },
        babe: BabeConfig {
            authorities: vec![],
            epoch_config: kitchensink_runtime::BABE_GENESIS_EPOCH_CONFIG,
            ..Default::default()
        },
        im_online: ImOnlineConfig { keys: vec![] },
        authority_discovery: Default::default(),
        grandpa: GrandpaConfig {
            authorities: vec![],
            ..Default::default()
        },
        technical_membership: Default::default(),
        treasury: Default::default(),
        society: SocietyConfig { pot: 0 },
        vesting: Default::default(),
        assets: AssetsConfig {
            // This asset is used by the NIS pallet as counterpart currency.
            assets: vec![(9, [0; 32].into(), true, 1)],
            ..Default::default()
        },
        transaction_storage: Default::default(),
        transaction_payment: Default::default(),
        alliance: Default::default(),
        alliance_motion: Default::default(),
        nomination_pools: NominationPoolsConfig {
            min_create_bond: 10 * DOLLARS,
            min_join_bond: DOLLARS,
            ..Default::default()
        },
        glutton: GluttonConfig {
            compute: Default::default(),
            storage: Default::default(),
            trash_data_count: Default::default(),
            ..Default::default()
        },
        pool_assets: Default::default(),
        safe_mode: Default::default(),
        tx_pause: Default::default(),
        mixnet: Default::default(),
    }
    .build_storage()
    .unwrap();
    let mut chain = BasicExternalities::new(storage);
    chain.execute_with(|| {
        // We set the configuration for the broker pallet
        Broker::configure(
            RuntimeOrigin::root(),
            pallet_broker::ConfigRecord {
                advance_notice: 2,
                interlude_length: 1,
                leadin_length: 1,
                ideal_bulk_proportion: Default::default(),
                limit_cores_offered: None,
                region_length: 3,
                renewal_bump: Perbill::from_percent(10),
                contribution_timeout: 5,
            },
        )
        .unwrap();

        /*
        // WIP: found the society before each input
        externalities.execute_with(|| {
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
        });
        */
    });
    chain.into_storages()
}

fn recursively_find_call(call: RuntimeCall, matches_on: fn(RuntimeCall) -> bool) -> bool {
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
    } else if let RuntimeCall::Lottery(pallet_lottery::Call::buy_ticket { call })
    | RuntimeCall::Multisig(pallet_multisig::Call::as_multi_threshold_1 {
        call, ..
    })
    | RuntimeCall::Utility(pallet_utility::Call::as_derivative { call, .. })
    | RuntimeCall::Proxy(pallet_proxy::Call::proxy { call, .. })
    | RuntimeCall::Council(pallet_collective::Call::propose {
        proposal: call, ..
    }) = call
    {
        return recursively_find_call(*call.clone(), matches_on);
    } else if matches_on(call) {
        return true;
    }
    false
}

fn start_block(block: u32) {
    #[cfg(not(fuzzing))]
    println!("\ninitializing block {block}");

    let pre_digest = Digest {
        logs: vec![DigestItem::PreRuntime(
            BABE_ENGINE_ID,
            PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
                slot: Slot::from(block as u64),
                authority_index: 42,
            })
            .encode(),
        )],
    };

    Executive::initialize_block(&Header::new(
        block,
        Default::default(),
        Default::default(),
        Default::default(),
        pre_digest,
    ));

    #[cfg(not(fuzzing))]
    println!("  setting timestamp");
    Timestamp::set(RuntimeOrigin::none(), block as u64 * SLOT_DURATION).unwrap();
}

fn end_block(elapsed: Duration) {
    #[cfg(not(fuzzing))]
    println!("\n  time spent: {elapsed:?}");
    assert!(elapsed.as_secs() <= 2, "block execution took too much time");

    #[cfg(not(fuzzing))]
    println!("\n  finalizing block");
    Executive::finalize_block();
}

fn main() {
    let endowed_accounts: Vec<AccountId> = (0..5).map(|i| [i; 32].into()).collect();
    let genesis_storage = genesis(&endowed_accounts);

    ziggy::fuzz!(|data: &[u8]| {
        // We build the list of extrinsics we will execute
        let mut extrinsic_data = data;
        // Vec<(lapse, origin, extrinsic)>
        let extrinsics: Vec<(u8, u8, RuntimeCall)> = std::iter::from_fn(|| {
            DecodeLimit::decode_with_depth_limit(64, &mut extrinsic_data).ok()
        })
        .filter(|(_, _, x): &(_, _, RuntimeCall)| {
            !recursively_find_call(x.clone(), |call| {
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
                        RuntimeCall::Contracts(pallet_contracts::Call::instantiate_with_code {
                            gas_limit: _limit,
                            ..
                        })
                        // }) if limit.ref_time() > 10_000_000_000)
                    )

                // We filter out contracts call that will take too long because of fuzzer instrumentation
                || matches!(
                        &call,
                        RuntimeCall::Contracts(pallet_contracts::Call::upload_code {
                            ..
                        })
                    )

                // We filter out contracts call that will take too long because of fuzzer instrumentation
                || matches!(
                        &call,
                        RuntimeCall::Contracts(
                            pallet_contracts::Call::instantiate_with_code_old_weight { .. }
                        )
                    )
                // We filter out Contracts::migrate calls that can debug_assert
                || matches!(
                        &call,
                        RuntimeCall::Contracts(
                            pallet_contracts::Call::migrate { .. }
                        )
                    )
                // We filter out a Society::bid call that will cause an overflow
                // See https://github.com/paritytech/srlabs_findings/issues/292
                || matches!(
                        &call,
                        RuntimeCall::Society(pallet_society::Call::bid { .. } |
pallet_society::Call::vouch { .. })
                    )
                // We filter out safe_mode calls, as they block timestamps from being set.
                || matches!(&call, RuntimeCall::SafeMode(..))
                // We filter out store extrinsics because BasicExternalities does not support them.
                || matches!(
                        &call,
                        RuntimeCall::TransactionStorage(pallet_transaction_storage::Call::store { .. })
                            | RuntimeCall::Remark(pallet_remark::Call::store { .. })
                    )
                // We filter out deprecated extrinsics that lead to failing TryState
                || matches!(
                        &call,
                        RuntimeCall::Treasury(pallet_treasury::Call::approve_proposal { .. })
                            | RuntimeCall::Treasury(pallet_treasury::Call::reject_proposal{ .. })
                            | RuntimeCall::Treasury(pallet_treasury::Call::propose_spend{ .. })
                    )
                || matches!(
                        &call,
                        RuntimeCall::NominationPools(..)
                )
            })
        })
        .collect();
        if extrinsics.is_empty() {
            return;
        }

        // `chain` represents the state of our mock chain.
        let mut chain = BasicExternalities::new(genesis_storage.clone());

        let mut current_block: u32 = 1;
        let mut current_weight: Weight = Weight::zero();
        let mut elapsed: Duration = Duration::ZERO;

        let mut initial_total_issuance = 0;

        chain.execute_with(|| {
            initial_total_issuance = pallet_balances::TotalIssuance::<Runtime>::get();

            start_block(current_block);

            for (lapse, origin, extrinsic) in extrinsics {
                    if lapse > 0 {
                    // We end the current block
                    end_block(elapsed);

                    // 393 * 256 = 100608 which nearly corresponds to a week
                    let actual_lapse = u32::from(lapse) * 393;
                    // We update our state variables
                    current_block += actual_lapse;
                    current_weight = Weight::zero();
                    elapsed = Duration::ZERO;

                    // We start the next block
                    start_block(current_block);
                }

                // We compute the weight to avoid overweight blocks.
                current_weight = current_weight.saturating_add(extrinsic.get_dispatch_info().weight);

                if current_weight.ref_time() >= 2 * WEIGHT_REF_TIME_PER_SECOND {
                    #[cfg(not(fuzzing))]
                    println!("Extrinsic would exhaust block weight, skipping");
                    continue;
                }

                let now = Instant::now(); // We get the current time for timing purposes.

                let origin_account = endowed_accounts[origin as usize % endowed_accounts.len()].clone();

                // We do not continue if the origin account does not have a free balance
                let acc = frame_system::Account::<Runtime>::get(&origin_account);
                if acc.data.free == 0 {
                    #[cfg(not(fuzzing))]
                    println!(
                        "\n    origin {origin_account:?} does not have free balance, skipping"
                    );
                    return;
                }

                #[cfg(not(fuzzing))]
                {
                    println!("\n    origin:     {origin_account:?}");
                    println!("    call:       {extrinsic:?}");
                }
                let _res = extrinsic
                    .clone()
                    .dispatch(RuntimeOrigin::signed(origin_account));
                #[cfg(not(fuzzing))]
                println!("    result:     {_res:?}");

                elapsed += now.elapsed();
            }

            end_block(elapsed);

            // After execution of all blocks, we run invariants
            let mut total_free: Balance = 0;
            let mut total_reserved: Balance = 0;
            for acc in frame_system::Account::<Runtime>::iter() {
                // Check that the consumer/provider state is valid.
                let acc_consumers = acc.1.consumers;
                let acc_providers = acc.1.providers;
                assert!(!(acc_consumers > 0 && acc_providers == 0), "Invalid state");
                #[cfg(not(fuzzing))]
                {
                    println!("   account: {acc:?}");
                    println!("      data: {:?}", acc.1.data);
                }
                // Increment our balance counts
                total_free += acc.1.data.free;
                total_reserved += acc.1.data.reserved;
                // Check that locks and holds are valid.
                let max_lock: Balance = kitchensink_runtime::Balances::locks(&acc.0).iter().map(|l| l.amount).max().unwrap_or_default();
                assert_eq!(max_lock, acc.1.data.frozen, "Max lock should be equal to frozen balance");
                let sum_holds: Balance = pallet_balances::Holds::<Runtime>::get(&acc.0).iter().map(|l| l.amount).sum();
                assert!(
                    sum_holds <= acc.1.data.reserved,
                    "Sum of all holds ({sum_holds}) should be less than or equal to reserved balance {}",
                    acc.1.data.reserved
                );
            }
            let total_issuance = pallet_balances::TotalIssuance::<Runtime>::get();
            let total_counted = total_free + total_reserved;

            assert!(total_issuance == total_counted, "Inconsistent total issuance: {total_issuance} but counted {total_counted}");
            assert!(total_issuance <= initial_total_issuance, "Total issuance too high: {total_issuance} but initial was {initial_total_issuance}");

            #[cfg(not(fuzzing))]
            println!("\nrunning integrity tests\n");
            // We run all developer-defined integrity tests
            AllPalletsWithSystem::integrity_test();
            #[cfg(not(fuzzing))]
            println!("running try_state for block {current_block}\n");
            AllPalletsWithSystem::try_state(current_block, TryStateSelect::All).unwrap();
        });
    });
}
