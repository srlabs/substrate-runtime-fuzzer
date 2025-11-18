#![warn(clippy::pedantic)]
use asset_hub_polkadot_runtime::{
    AllPalletsWithSystem, Balances, Executive, ParachainSystem, Runtime, RuntimeCall,
    RuntimeOrigin, Timestamp,
};
use codec::{DecodeLimit, Encode};
use cumulus_primitives_core::relay_chain::Header;
use frame_support::{
    dispatch::GetDispatchInfo,
    pallet_prelude::Weight,
    traits::{IntegrityTest, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use frame_system::Account;
use pallet_balances::{Holds, TotalIssuance};
use parachains_common::{AccountId, Balance, SLOT_DURATION};
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use sp_runtime::{
    testing::H256,
    traits::{Dispatchable, Header as _},
    Digest, DigestItem, Storage,
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

fn generate_genesis(accounts: &[AccountId]) -> Storage {
    use asset_hub_polkadot_runtime::{
        AssetsConfig, AuraConfig, AuraExtConfig, BalancesConfig, ClaimsConfig,
        CollatorSelectionConfig, ForeignAssetsConfig, IndicesConfig,
        MultiBlockElectionVerifierConfig, NominationPoolsConfig, ParachainInfoConfig,
        ParachainSystemConfig, PolkadotXcmConfig, PoolAssetsConfig, RuntimeGenesisConfig,
        SessionConfig, SessionKeys, StakingConfig, SystemConfig, TransactionPaymentConfig,
        TreasuryConfig, VestingConfig,
    };
    use sp_consensus_aura::ed25519::AuthorityId as AuraId;
    use sp_runtime::app_crypto::ByteArray;
    use sp_runtime::BuildStorage;

    let initial_authorities: Vec<(AccountId, AuraId)> =
        vec![([0; 32].into(), AuraId::from_slice(&[0; 32]).unwrap())];

    RuntimeGenesisConfig {
        system: SystemConfig::default(),
        balances: BalancesConfig {
            // Configure endowed accounts with initial balance of 1 << 60.
            balances: accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
            dev_accounts: None,
        },
        aura: AuraConfig::default(),
        session: SessionConfig {
            keys: initial_authorities
                .iter()
                .map(|x| (x.0.clone(), x.0.clone(), SessionKeys { aura: x.1.clone() }))
                .collect::<Vec<_>>(),
            non_authority_keys: vec![],
        },
        collator_selection: CollatorSelectionConfig {
            invulnerables: initial_authorities.iter().map(|x| x.0.clone()).collect(),
            candidacy_bond: 1 << 57,
            desired_candidates: 1,
        },
        aura_ext: AuraExtConfig::default(),
        parachain_info: ParachainInfoConfig::default(),
        parachain_system: ParachainSystemConfig::default(),
        polkadot_xcm: PolkadotXcmConfig::default(),
        assets: AssetsConfig::default(),
        foreign_assets: ForeignAssetsConfig::default(),
        pool_assets: PoolAssetsConfig::default(),
        transaction_payment: TransactionPaymentConfig::default(),
        vesting: VestingConfig::default(),
        claims: ClaimsConfig::default(),
        indices: IndicesConfig::default(),
        multi_block_election_verifier: MultiBlockElectionVerifierConfig::default(),
        nomination_pools: NominationPoolsConfig::default(),
        staking: StakingConfig::default(),
        treasury: TreasuryConfig::default(),
    }
    .build_storage()
    .unwrap()
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
    } else if let RuntimeCall::Utility(pallet_utility::Call::if_else {
        main: main_call,
        fallback,
    }) = call
    {
        return recursively_find_call(*main_call, matches_on)
            || recursively_find_call(*fallback, matches_on);
    } else if let RuntimeCall::Multisig(pallet_multisig::Call::as_multi_threshold_1 {
        call, ..
    })
    | RuntimeCall::Utility(
        pallet_utility::Call::as_derivative { call, .. }
        | pallet_utility::Call::with_weight { call, .. }
        | pallet_utility::Call::dispatch_as { call, .. }
        | pallet_utility::Call::dispatch_as_fallible { call, .. },
    )
    | RuntimeCall::Whitelist(
        pallet_whitelist::Call::dispatch_whitelisted_call_with_preimage { call, .. },
    )
    | RuntimeCall::Proxy(
        pallet_proxy::Call::proxy { call, .. } | pallet_proxy::Call::proxy_announced { call, .. },
    ) = call
    {
        return recursively_find_call(*call.clone(), matches_on);
    } else if matches_on(call) {
        return true;
    }
    false
}

fn process_input(accounts: &[AccountId], genesis: &Storage, data: &[u8]) {
    // We build the list of extrinsics we will execute
    let mut extrinsic_data = data;
    // Vec<(lapse, origin, extrinsic)>

    BasicExternalities::execute_with_storage(&mut genesis.clone(), || {
        #[allow(deprecated)]
    let extrinsics: Vec<(u8, u8, RuntimeCall)> =
        iter::from_fn(|| DecodeLimit::decode_with_depth_limit(64, &mut extrinsic_data).ok())
            .filter(|(_, _, x): &(_, _, RuntimeCall)| {
            !recursively_find_call(x.clone(), |call| {
                // We filter out calls with Fungible(0) as they cause a debug crash
                matches!(call.clone(), RuntimeCall::PolkadotXcm(pallet_xcm::Call::execute { message, .. })
                    if matches!(message.as_ref(), staging_xcm::VersionedXcm::V3(staging_xcm::v3::Xcm(msg))
                        if msg.iter().any(|m| matches!(m, staging_xcm::opaque::v3::prelude::BuyExecution { fees: staging_xcm::v3::MultiAsset { fun, .. }, .. }
                            if *fun == staging_xcm::v3::Fungibility::Fungible(0)
                        ))
                    )
                ) || matches!(call.clone(), RuntimeCall::System(_))
                || matches!(call.clone(), RuntimeCall::AhMigrator(_))
                || matches!(call.clone(), RuntimeCall::Vesting(pallet_vesting::Call::vested_transfer { .. }))
            })
        }).collect();

        if extrinsics.is_empty() {
            return;
        }

        let mut block: u32 = 1;
        let mut weight: Weight = Weight::zero();
        let mut elapsed: Duration = Duration::ZERO;
        let initial_total_issuance = pallet_balances::TotalIssuance::<Runtime>::get();

        initialize_block(block, None);

        for (lapse, origin, extrinsic) in extrinsics {
            if lapse > 0 {
                println!("YO");
                let prev_header = finalize_block(elapsed);

                // We update our state variables
                block += u32::from(lapse);
                weight = Weight::zero();
                elapsed = Duration::ZERO;

                // We start the next block
                initialize_block(block, Some(&prev_header));
            }

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

            let origin = accounts[origin as usize % accounts.len()].clone();

            #[cfg(not(feature = "fuzzing"))]
            println!("\n    origin:     {origin:?}");

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

fn initialize_block(block: u32, prev_header: Option<&Header>) {
    #[cfg(not(feature = "fuzzing"))]
    println!("\ninitializing block {block}");

    let pre_digest = Digest {
        logs: vec![DigestItem::PreRuntime(
            AURA_ENGINE_ID,
            Slot::from(u64::from(block)).encode(),
        )],
    };
    let parent_header = &Header::new(
        block,
        H256::default(),
        H256::default(),
        prev_header.map(Header::hash).unwrap_or_default(),
        pre_digest,
    );
    Executive::initialize_block(parent_header);

    #[cfg(not(feature = "fuzzing"))]
    println!("  setting timestamp");
    Timestamp::set(RuntimeOrigin::none(), u64::from(block) * SLOT_DURATION).unwrap();

    #[cfg(not(feature = "fuzzing"))]
    println!("  setting parachain validation data");
    let parachain_validation_data = {
        use cumulus_primitives_core::relay_chain::HeadData;
        // use cumulus_primitives_parachain_inherent::ParachainInherentData;
        use cumulus_pallet_parachain_system::parachain_inherent::BasicParachainInherentData;
        use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;

        let parent_head = HeadData(prev_header.unwrap_or(parent_header).encode());
        let sproof_builder = RelayStateSproofBuilder {
            para_id: 100.into(),
            current_slot: cumulus_primitives_core::relay_chain::Slot::from(2 * u64::from(block)),
            included_para_head: Some(parent_head.clone()),
            ..Default::default()
        };

        let (relay_parent_storage_root, relay_chain_state) =
            sproof_builder.into_state_root_and_proof();
        BasicParachainInherentData {
            validation_data: polkadot_primitives::PersistedValidationData {
                parent_head,
                relay_parent_number: block,
                relay_parent_storage_root,
                max_pov_size: 1000,
            },
            relay_chain_state,
            collator_peer_id: None,
            relay_parent_descendants: vec![],
        }
    };
    let inbound_message_data = {
        use cumulus_pallet_parachain_system::parachain_inherent::{
            AbridgedInboundMessagesCollection, InboundMessagesData,
        };
        InboundMessagesData::new(
            AbridgedInboundMessagesCollection::default(),
            AbridgedInboundMessagesCollection::default(),
        )
    };
    ParachainSystem::set_validation_data(
        RuntimeOrigin::none(),
        parachain_validation_data,
        inbound_message_data,
    )
    .unwrap();
}

fn finalize_block(elapsed: Duration) -> Header {
    #[cfg(not(feature = "fuzzing"))]
    println!("\n  time spent: {elapsed:?}");
    assert!(elapsed.as_secs() <= 2, "block execution took too much time");

    #[cfg(not(feature = "fuzzing"))]
    println!("finalizing block");
    Executive::finalize_block()
}

fn check_invariants(block: u32, initial_total_issuance: Balance) {
    let mut counted_free = 0;
    let mut counted_reserved = 0;
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
    // The reason we do not simply use `!=` here is that some balance might be transfered to another chain via XCM.
    // If we find some kind of workaround for this, we could replace `<` by `!=` here and make the check stronger.
    assert!(total_issuance <= counted_issuance,);
    assert!(total_issuance <= initial_total_issuance,);
    // We run all developer-defined integrity tests
    AllPalletsWithSystem::integrity_test();
    AllPalletsWithSystem::try_state(block, TryStateSelect::All).unwrap();
}
