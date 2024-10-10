#![warn(clippy::pedantic)]
use codec::{DecodeLimit, Encode};
use coretime_kusama_runtime::{
    AllPalletsWithSystem, Balances, Broker, Executive, ParachainSystem, Runtime, RuntimeCall,
    RuntimeOrigin, Timestamp,
};
use cumulus_primitives_core::relay_chain::Header;
use frame_support::{
    dispatch::GetDispatchInfo,
    pallet_prelude::Weight,
    traits::{IntegrityTest, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use frame_system::Account;
use pallet_balances::{Holds, TotalIssuance};
use pallet_broker::{ConfigRecord, ConfigRecordOf, CoreIndex, CoreMask, Timeslice};
use parachains_common::{AccountId, Balance, SLOT_DURATION};
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use sp_runtime::{
    testing::H256,
    traits::{Dispatchable, Header as _},
    Digest, DigestItem, Perbill, Storage,
};
use sp_state_machine::BasicExternalities;
use std::{
    collections::{BTreeMap, HashMap},
    iter,
    time::{Duration, Instant},
};
use system_parachains_constants::kusama::currency::UNITS;

fn main() {
    let accounts: Vec<AccountId> = (0..5).map(|i| [i; 32].into()).collect();
    let genesis = generate_genesis(&accounts);

    ziggy::fuzz!(|data: &[u8]| {
        process_input(&accounts, &genesis, data);
    });
}

fn generate_genesis(accounts: &[AccountId]) -> Storage {
    use coretime_kusama_runtime::BuildStorage;
    use coretime_kusama_runtime::{
        AuraConfig, AuraExtConfig, BalancesConfig, BrokerConfig, CollatorSelectionConfig,
        ParachainInfoConfig, ParachainSystemConfig, PolkadotXcmConfig, RuntimeGenesisConfig,
        SessionConfig, SessionKeys, SystemConfig, TransactionPaymentConfig,
    };
    use sp_application_crypto::ByteArray;
    use sp_consensus_aura::sr25519::AuthorityId as AuraId;

    let initial_authorities: Vec<(AccountId, AuraId)> =
        vec![([0; 32].into(), AuraId::from_slice(&[0; 32]).unwrap())];

    let mut storage = RuntimeGenesisConfig {
        system: SystemConfig::default(),
        balances: BalancesConfig {
            // Configure endowed accounts with initial balance of 1 << 60.
            balances: accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
        },
        aura: AuraConfig::default(),
        broker: BrokerConfig {
            _config: Default::default(),
        },
        session: SessionConfig {
            keys: initial_authorities
                .iter()
                .map(|x| (x.0.clone(), x.0.clone(), SessionKeys { aura: x.1.clone() }))
                .collect::<Vec<_>>(),
        },
        collator_selection: CollatorSelectionConfig {
            invulnerables: initial_authorities.iter().map(|x| (x.0.clone())).collect(),
            candidacy_bond: 1 << 57,
            desired_candidates: 1,
        },
        aura_ext: AuraExtConfig::default(),
        parachain_info: ParachainInfoConfig::default(),
        parachain_system: ParachainSystemConfig::default(),
        polkadot_xcm: PolkadotXcmConfig::default(),
        transaction_payment: TransactionPaymentConfig::default(),
    }
    .build_storage()
    .unwrap();

    BasicExternalities::execute_with_storage(&mut storage, || {
        initialize_block(1, &None);
        Broker::configure(RuntimeOrigin::root(), new_config()).unwrap();
        Broker::start_sales(RuntimeOrigin::root(), 10 * UNITS, 1).unwrap();
        // We have to send 1 DOT to the coretime burn address because of a defensive assertion that cannot be
        // reached in a real-world environment.
        use sp_runtime::traits::AccountIdConversion;
        let coretime_burn_account: AccountId =
            frame_support::PalletId(*b"py/ctbrn").into_account_truncating();
        let coretime_burn_address = coretime_burn_account.into();
        Balances::transfer_keep_alive(
            RuntimeOrigin::signed(accounts[0].clone()),
            coretime_burn_address,
            1 * UNITS,
        )
        .unwrap();
        initialize_block(2, &Some(finalize_block(Duration::ZERO)));
    });

    storage
}

fn new_config() -> ConfigRecordOf<Runtime> {
    ConfigRecord {
        advance_notice: 1,
        interlude_length: 1,
        leadin_length: 2,
        ideal_bulk_proportion: Perbill::from_percent(100),
        limit_cores_offered: None,
        region_length: 3,
        renewal_bump: Perbill::from_percent(3),
        contribution_timeout: 1,
    }
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
    } else if let RuntimeCall::Multisig(pallet_multisig::Call::as_multi_threshold_1 {
        call, ..
    })
    | RuntimeCall::Utility(pallet_utility::Call::as_derivative { call, .. })
    | RuntimeCall::Proxy(pallet_proxy::Call::proxy { call, .. }) = call
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
    let extrinsics: Vec<(/* lapse */ u8, /* origin */ u8, RuntimeCall)> =
        iter::from_fn(|| DecodeLimit::decode_with_depth_limit(64, &mut extrinsic_data).ok())
            .filter(|(_, _, x): &(_, _, RuntimeCall)| {
                !recursively_find_call(x.clone(), |call| {
                    matches!(call.clone(), RuntimeCall::System(_))
                        || matches!(
                            call.clone(),
                            RuntimeCall::PolkadotXcm(pallet_xcm::Call::execute { .. })
                        )
                })
            })
            .collect();
    if extrinsics.is_empty() {
        return;
    }

    let mut block: u32 = 2;
    let mut weight: Weight = Weight::zero();
    let mut elapsed: Duration = Duration::ZERO;

    BasicExternalities::execute_with_storage(&mut genesis.clone(), || {
        let initial_total_issuance = pallet_balances::TotalIssuance::<Runtime>::get();

        for (lapse, origin, extrinsic) in extrinsics {
            if lapse > 0 {
                let prev_header = finalize_block(elapsed);

                // We update our state variables
                block += u32::from(lapse) * 393;
                weight = Weight::zero();
                elapsed = Duration::ZERO;

                initialize_block(block, &Some(prev_header));
            }

            weight.saturating_accrue(extrinsic.get_dispatch_info().weight);
            if weight.ref_time() >= 2 * WEIGHT_REF_TIME_PER_SECOND {
                #[cfg(not(feature = "fuzzing"))]
                println!("Extrinsic would exhaust block weight, skipping");
                continue;
            }

            let origin = accounts[usize::from(origin) % accounts.len()].clone();

            #[cfg(not(feature = "fuzzing"))]
            println!("\n    origin:     {origin:?}");
            #[cfg(not(feature = "fuzzing"))]
            println!("    call:       {extrinsic:?}");

            let now = Instant::now(); // We get the current time for timing purposes.
            #[allow(unused_variables)]
            let res = extrinsic.dispatch(RuntimeOrigin::signed(origin));
            elapsed += now.elapsed();

            #[cfg(not(feature = "fuzzing"))]
            println!("    result:     {res:?}");
        }

        finalize_block(elapsed);

        check_invariants(block, initial_total_issuance);
    });
}

fn initialize_block(block: u32, prev_header: &Option<Header>) {
    #[cfg(not(feature = "fuzzing"))]
    println!("\ninitializing block {block}");

    let pre_digest = Digest {
        logs: vec![DigestItem::PreRuntime(
            AURA_ENGINE_ID,
            Slot::from(u64::from(block)).encode(),
        )],
    };
    let parent_header = Header::new(
        block,
        H256::default(),
        H256::default(),
        prev_header.clone().map(|h| h.hash()).unwrap_or_default(),
        pre_digest,
    );

    Executive::initialize_block(&parent_header.clone());
    Timestamp::set(RuntimeOrigin::none(), u64::from(block) * SLOT_DURATION).unwrap();

    #[cfg(not(feature = "fuzzing"))]
    println!("  setting parachain validation data");
    let parachain_validation_data = {
        use cumulus_primitives_core::{relay_chain::HeadData, PersistedValidationData};
        use cumulus_primitives_parachain_inherent::ParachainInherentData;
        use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;

        let parent_head = HeadData(
            prev_header
                .clone()
                .unwrap_or(parent_header.clone())
                .encode(),
        );
        let sproof_builder = RelayStateSproofBuilder {
            para_id: 100.into(),
            current_slot: Slot::from(2 * u64::from(block)),
            included_para_head: Some(parent_head.clone()),
            ..Default::default()
        };

        let (relay_parent_storage_root, relay_chain_state) =
            sproof_builder.into_state_root_and_proof();
        ParachainInherentData {
            validation_data: PersistedValidationData {
                parent_head,
                relay_parent_number: block,
                relay_parent_storage_root,
                max_pov_size: 1000,
            },
            relay_chain_state,
            downward_messages: Vec::default(),
            horizontal_messages: BTreeMap::default(),
        }
    };
    ParachainSystem::set_validation_data(RuntimeOrigin::none(), parachain_validation_data).unwrap();
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
    assert!(
        total_issuance <= counted_issuance,
        "Inconsistent total issuance: {total_issuance} but counted {counted_issuance}"
    );
    assert!(
        total_issuance <= initial_total_issuance,
        "Total issuance {total_issuance} greater than initial issuance {initial_total_issuance}"
    );
    // Broker invariants
    let status =
        pallet_broker::Status::<Runtime>::get().expect("Broker pallet should always have a status");
    let sale_info = pallet_broker::SaleInfo::<Runtime>::get()
        .expect("Broker pallet should always have a sale info");
    assert!(
        sale_info.first_core <= status.core_count,
        "Sale info first_core too large"
    );
    assert!(
        sale_info.cores_sold <= sale_info.cores_offered,
        "Sale info cores mismatch"
    );
    let regions: Vec<pallet_broker::RegionId> = pallet_broker::Regions::<Runtime>::iter()
        .map(|n| n.0)
        .collect();
    let mut masks: std::collections::HashMap<(Timeslice, CoreIndex), CoreMask> = HashMap::default();
    for region in regions {
        let region_record = pallet_broker::Regions::<Runtime>::get(region)
            .expect("Region id should have a region record");
        for region_timeslice in region.begin..region_record.end {
            let mut existing_mask = *masks
                .get(&(region_timeslice, region.core))
                .unwrap_or(&CoreMask::void());
            assert_eq!(existing_mask ^ region.mask, existing_mask | region.mask);
            existing_mask |= region.mask;
            masks.insert((region_timeslice, region.core), existing_mask);
        }
    }
    // We run all developer-defined integrity tests
    AllPalletsWithSystem::integrity_test();
    AllPalletsWithSystem::try_state(block, TryStateSelect::All).unwrap();
}
