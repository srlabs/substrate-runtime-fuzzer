use codec::{DecodeLimit, Encode};
use coretime_kusama_runtime::{
    AllPalletsWithSystem, Balances, Broker, Executive, Runtime, RuntimeCall, RuntimeOrigin,
    Timestamp, UncheckedExtrinsic,
};
use frame_support::{
    dispatch::GetDispatchInfo,
    pallet_prelude::Weight,
    traits::{IntegrityTest, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use pallet_broker::{ConfigRecord, ConfigRecordOf};
use parachains_common::{AccountId, Balance, BlockNumber, SLOT_DURATION};
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use sp_runtime::{
    traits::{Dispatchable, Header},
    Digest, DigestItem, Perbill, Storage,
};
use std::time::{Duration, Instant};
use substrate_runtime_fuzzer::*;
use system_parachains_constants::kusama::currency::UNITS;

// We use a simple Map-based Externalities implementation
pub type Externalities = sp_state_machine::BasicExternalities;

pub fn new_config() -> ConfigRecordOf<Runtime> {
    ConfigRecord {
        advance_notice: 2,
        interlude_length: 1,
        leadin_length: 1,
        ideal_bulk_proportion: Default::default(),
        limit_cores_offered: None,
        region_length: 3,
        renewal_bump: Perbill::from_percent(10),
        contribution_timeout: 5,
    }
}

fn main() {
    let endowed_accounts: Vec<AccountId> = (0..5).map(|i| [i; 32].into()).collect();

    let genesis_storage: Storage = {
        use coretime_kusama_runtime::{
            BalancesConfig, CollatorSelectionConfig, RuntimeGenesisConfig, SessionConfig,
            SessionKeys,
        };
        use sp_consensus_aura::sr25519::AuthorityId as AuraId;
        use sp_runtime::app_crypto::ByteArray;
        use sp_runtime::BuildStorage;

        let initial_authorities: Vec<(AccountId, AuraId)> =
            vec![([0; 32].into(), AuraId::from_slice(&[0; 32]).unwrap())];

        let storage = RuntimeGenesisConfig {
            system: Default::default(),
            balances: BalancesConfig {
                // Configure endowed accounts with initial balance of 1 << 60.
                balances: endowed_accounts
                    .iter()
                    .cloned()
                    .map(|k| (k, 1 << 60))
                    .collect(),
            },
            aura: Default::default(),
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
            aura_ext: Default::default(),
            parachain_info: Default::default(),
            parachain_system: Default::default(),
            polkadot_xcm: Default::default(),
            transaction_payment: Default::default(),
        }
        .build_storage()
        .unwrap();
        let mut chain = Externalities::new(storage.clone());

        chain.execute_with(|| {
            // Broker setup
            Executive::initialize_block(&Header::new(
                1,
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            ));
            Timestamp::set(RuntimeOrigin::none(), 0).unwrap();

            let parachain_validation_data = {
                use cumulus_primitives_core::relay_chain::HeadData;
                use cumulus_primitives_core::PersistedValidationData;
                use cumulus_primitives_parachain_inherent::ParachainInherentData;
                use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;

                let parent_head: HeadData = Default::default();
                let sproof_builder = RelayStateSproofBuilder {
                    para_id: 100.into(),
                    current_slot: Slot::from(1),
                    included_para_head: Some(parent_head.clone()),
                    ..Default::default()
                };

                let (relay_parent_storage_root, relay_chain_state) =
                    sproof_builder.into_state_root_and_proof();
                let data = ParachainInherentData {
                    validation_data: PersistedValidationData {
                        parent_head,
                        relay_parent_number: 1,
                        relay_parent_storage_root,
                        max_pov_size: 1000,
                    },
                    relay_chain_state,
                    downward_messages: Default::default(),
                    horizontal_messages: Default::default(),
                };
                cumulus_pallet_parachain_system::Call::set_validation_data { data }
            };

            Executive::apply_extrinsic(UncheckedExtrinsic::new_unsigned(
                RuntimeCall::ParachainSystem(parachain_validation_data),
            ))
            .unwrap()
            .unwrap();

            Broker::configure(RuntimeOrigin::root(), new_config()).unwrap();
            Broker::start_sales(RuntimeOrigin::root(), 10 * UNITS, 1).unwrap();

            let pre_digest = Digest {
                logs: vec![DigestItem::PreRuntime(
                    AURA_ENGINE_ID,
                    Slot::from(2).encode(),
                )],
            };

            let prev_header = Executive::finalize_block();

            let parent_header = &Header::new(
                2,
                Default::default(),
                Default::default(),
                prev_header.hash(),
                pre_digest,
            );

            Executive::initialize_block(parent_header);
            Timestamp::set(RuntimeOrigin::none(), SLOT_DURATION * 2).unwrap();

            let parachain_validation_data = {
                use cumulus_primitives_core::relay_chain::HeadData;
                use cumulus_primitives_core::PersistedValidationData;
                use cumulus_primitives_parachain_inherent::ParachainInherentData;
                use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;

                let parent_head = HeadData(prev_header.clone().encode());
                let sproof_builder = RelayStateSproofBuilder {
                    para_id: 100.into(),
                    current_slot: Slot::from(4),
                    included_para_head: Some(parent_head.clone()),
                    ..Default::default()
                };

                let (relay_parent_storage_root, relay_chain_state) =
                    sproof_builder.into_state_root_and_proof();
                let data = ParachainInherentData {
                    validation_data: PersistedValidationData {
                        parent_head,
                        relay_parent_number: 2,
                        relay_parent_storage_root,
                        max_pov_size: 1000,
                    },
                    relay_chain_state,
                    downward_messages: Default::default(),
                    horizontal_messages: Default::default(),
                };
                cumulus_pallet_parachain_system::Call::set_validation_data { data }
            };

            Executive::apply_extrinsic(UncheckedExtrinsic::new_unsigned(
                RuntimeCall::ParachainSystem(parachain_validation_data),
            ))
            .unwrap()
            .unwrap();
        });
        chain.into_storages()
    };

    ziggy::fuzz!(|data: &[u8]| {
        let mut extrinsic_data = data;

        // Max weight for a block.
        let max_weight: Weight = Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND * 2, 0);

        let mut extrinsics: Vec<(u8, u8, RuntimeCall)> = vec![];
        while let Ok(decoded) = DecodeLimit::decode_with_depth_limit(64, &mut extrinsic_data) {
            extrinsics.push(decoded);
        }
        extrinsics = extrinsics
            .into_iter()
            .filter(|(_, _, call)| !matches!(call, RuntimeCall::System(..)))
            .collect();
        if extrinsics.is_empty() {
            return;
        }

        // `externalities` represents the state of our mock chain.
        let mut externalities = Externalities::new(genesis_storage.clone());

        let mut current_block: u32 = 2;
        let mut current_weight: Weight = Weight::zero();
        let mut elapsed: Duration = Duration::ZERO;

        let start_block = |block: u32, lapse: u32| {
            #[cfg(not(fuzzing))]
            println!("\ninitializing block {}", block + lapse);

            let next_block = block + lapse;
            let current_timestamp = u64::from(next_block) * SLOT_DURATION;
            let pre_digest = Digest {
                logs: vec![DigestItem::PreRuntime(
                    AURA_ENGINE_ID,
                    Slot::from(current_timestamp / SLOT_DURATION).encode(),
                )],
            };

            let prev_header = Executive::finalize_block();

            let parent_header = &Header::new(
                next_block + 1,
                Default::default(),
                Default::default(),
                prev_header.hash(),
                pre_digest,
            );
            Executive::initialize_block(parent_header);

            // We apply the timestamp extrinsic for the current block.
            Executive::apply_extrinsic(UncheckedExtrinsic::new_unsigned(RuntimeCall::Timestamp(
                pallet_timestamp::Call::set {
                    now: current_timestamp,
                },
            )))
            .unwrap()
            .unwrap();

            let parachain_validation_data = {
                use cumulus_primitives_core::relay_chain::HeadData;
                use cumulus_primitives_core::PersistedValidationData;
                use cumulus_primitives_parachain_inherent::ParachainInherentData;
                use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;

                let parent_head = HeadData(prev_header.clone().encode());
                let sproof_builder = RelayStateSproofBuilder {
                    para_id: 100.into(),
                    current_slot: Slot::from(2 * current_timestamp / SLOT_DURATION),
                    included_para_head: Some(parent_head.clone()),
                    ..Default::default()
                };

                let (relay_parent_storage_root, relay_chain_state) =
                    sproof_builder.into_state_root_and_proof();
                let data = ParachainInherentData {
                    validation_data: PersistedValidationData {
                        parent_head,
                        relay_parent_number: next_block,
                        relay_parent_storage_root,
                        max_pov_size: 1000,
                    },
                    relay_chain_state,
                    downward_messages: Default::default(),
                    horizontal_messages: Default::default(),
                };
                cumulus_pallet_parachain_system::Call::set_validation_data { data }
            };

            Executive::apply_extrinsic(UncheckedExtrinsic::new_unsigned(
                RuntimeCall::ParachainSystem(parachain_validation_data),
            ))
            .unwrap()
            .unwrap();

            // Calls that need to be called before each block starts (init_calls) go here
        };

        for (lapse, origin, extrinsic) in extrinsics {
            if lapse > 0 {
                // We update our state variables
                current_weight = Weight::zero();
                elapsed = Duration::ZERO;

                // We end the previous block and start the next block
                externalities.execute_with(|| start_block(current_block, u32::from(lapse) * 393));
                current_block += u32::from(lapse) * 393;
            }

            // We get the current time for timing purposes.
            let now = Instant::now();

            let mut call_weight = Weight::zero();
            // We compute the weight to avoid overweight blocks.
            externalities.execute_with(|| {
                call_weight = extrinsic.get_dispatch_info().weight;
            });

            current_weight = current_weight.saturating_add(call_weight);
            if current_weight.ref_time() >= max_weight.ref_time() {
                #[cfg(not(fuzzing))]
                println!("Skipping because of max weight {max_weight}");
                continue;
            }

            externalities.execute_with(|| {
                let origin_account =
                    endowed_accounts[usize::from(origin) % endowed_accounts.len()].clone();
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
            });

            elapsed += now.elapsed();
        }

        #[cfg(not(fuzzing))]
        println!("\n  time spent: {elapsed:?}");
        assert!(
            elapsed.as_secs() <= MAX_TIME_FOR_BLOCK,
            "block execution took too much time"
        );

        // We end the final block
        externalities.execute_with(|| {
            // Finilization
            Executive::finalize_block();
            // Invariants
            #[cfg(not(fuzzing))]
            println!("\ntesting invariants for block {current_block}");
            <AllPalletsWithSystem as TryState<BlockNumber>>::try_state(
                current_block,
                TryStateSelect::All,
            )
            .unwrap();
        });

        // After execution of all blocks.
        externalities.execute_with(|| {
            // We keep track of the sum of balance of accounts
            let mut counted_free = 0;
            let mut counted_reserved = 0;

            for acc in frame_system::Account::<Runtime>::iter() {
                // Check that the consumer/provider state is valid.
                let acc_consumers = acc.1.consumers;
                let acc_providers = acc.1.providers;
                assert!(!(acc_consumers > 0 && acc_providers == 0), "Invalid state");

                // Increment our balance counts
                counted_free += acc.1.data.free;
                counted_reserved += acc.1.data.reserved;
                // Check that locks and holds are valid.
                let max_lock: Balance = Balances::locks(&acc.0).iter().map(|l| l.amount).max().unwrap_or_default();
                assert_eq!(max_lock, acc.1.data.frozen, "Max lock should be equal to frozen balance");
                let sum_holds: Balance = pallet_balances::Holds::<Runtime>::get(&acc.0).iter().map(|l| l.amount).sum();
                assert!(
                    sum_holds <= acc.1.data.reserved,
                    "Sum of all holds ({sum_holds}) should be less than or equal to reserved balance {}",
                    acc.1.data.reserved
                );
            }

            let total_issuance = pallet_balances::TotalIssuance::<Runtime>::get();
            let counted_issuance = counted_free + counted_reserved;
            // The reason we do not simply use `!=` here is that some balance might be transfered to another chain via XCM.
            // If we find some kind of workaround for this, we could replace `<` by `!=` here and make the check stronger.
            assert!(
                total_issuance <= counted_issuance,
                "Inconsistent total issuance: {total_issuance} but counted {counted_issuance}"
            );

            // Broker invariants
            let status = pallet_broker::Status::<Runtime>::get().expect("Broker pallet should always have a status");
            let sale_info = pallet_broker::SaleInfo::<Runtime>::get().expect("Broker pallet should always have a sale info");
            assert!(sale_info.first_core <= status.core_count, "Sale info first_core too large");
            assert!(sale_info.cores_sold <= sale_info.cores_offered, "Sale info cores mismatch");

            let regions: Vec<pallet_broker::RegionId> = pallet_broker::Regions::<Runtime>::iter().map(|n| n.0).collect();
            let mut masks: std::collections::HashMap::<(Timeslice, CoreIndex), CoreMask> = Default::default();
            for region in regions {
                let mut existing_mask = *masks.get(&(region.begin, region.core)).unwrap_or(&CoreMask::void());
                assert_eq!(existing_mask ^ region.mask, existing_mask | region.mask);
                existing_mask |= region.mask;
                masks.insert((region.begin, region.core), existing_mask);
            }
            for mask in masks {
                assert_eq!(mask.1, CoreMask::complete());
            }

            #[cfg(not(fuzzing))]
            println!("running integrity tests");
            // We run all developer-defined integrity tests
            <AllPalletsWithSystem as IntegrityTest>::integrity_test();
        });
    });
}
