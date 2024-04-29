use asset_hub_kusama_runtime::{
    AllPalletsWithSystem, Executive, ParachainSystem, Runtime, RuntimeCall, RuntimeOrigin,
    Timestamp,
};
use codec::{DecodeLimit, Encode};
use cumulus_primitives_core::relay_chain::Header;
use frame_support::{
    dispatch::GetDispatchInfo,
    pallet_prelude::Weight,
    traits::{IntegrityTest, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use parachains_common::{AccountId, Balance, SLOT_DURATION};
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use sp_runtime::{
    traits::{Dispatchable, Header as _},
    Digest, DigestItem, Storage,
};
use sp_state_machine::BasicExternalities;
use std::time::{Duration, Instant};

fn genesis(accounts: &[AccountId]) -> Storage {
    use asset_hub_kusama_runtime::{
        BalancesConfig, CollatorSelectionConfig, RuntimeGenesisConfig, SessionConfig, SessionKeys,
    };
    use sp_consensus_aura::sr25519::AuthorityId as AuraId;
    use sp_runtime::app_crypto::ByteArray;
    use sp_runtime::BuildStorage;

    let initial_authorities: Vec<(AccountId, AuraId)> =
        vec![([0; 32].into(), AuraId::from_slice(&[0; 32]).unwrap())];

    RuntimeGenesisConfig {
        system: Default::default(),
        balances: BalancesConfig {
            // Configure endowed accounts with initial balance of 1 << 60.
            balances: accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
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
        assets: Default::default(),
        foreign_assets: Default::default(),
        pool_assets: Default::default(),
        transaction_payment: Default::default(),
    }
    .build_storage()
    .unwrap()
}

fn start_block(block: u32, prev_header: Option<Header>) {
    #[cfg(not(fuzzing))]
    println!("\ninitializing block {}", block);

    let current_timestamp = u64::from(block) * SLOT_DURATION;
    let pre_digest = Digest {
        logs: vec![DigestItem::PreRuntime(
            AURA_ENGINE_ID,
            Slot::from(current_timestamp / SLOT_DURATION).encode(),
        )],
    };

    let parent_header = &Header::new(
        block,
        Default::default(),
        Default::default(),
        prev_header.clone().map(|x| x.hash()).unwrap_or_default(),
        pre_digest,
    );
    Executive::initialize_block(parent_header);

    #[cfg(not(fuzzing))]
    println!("  setting timestamp");
    Timestamp::set(RuntimeOrigin::none(), current_timestamp).unwrap();

    #[cfg(not(fuzzing))]
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
            current_slot: Slot::from(2 * current_timestamp / SLOT_DURATION),
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
            downward_messages: Default::default(),
            horizontal_messages: Default::default(),
        }
    };
    ParachainSystem::set_validation_data(RuntimeOrigin::none(), parachain_validation_data).unwrap();
}

fn end_block(elapsed: Duration) -> Header {
    #[cfg(not(fuzzing))]
    println!("\n  time spent: {elapsed:?}");
    assert!(elapsed.as_secs() <= 2, "block execution took too much time");

    #[cfg(not(fuzzing))]
    println!("finalizing block");
    Executive::finalize_block()
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
        .filter(|(_, _, x)| !matches!(x, RuntimeCall::System(_)))
        .collect();
        if extrinsics.is_empty() {
            return;
        }

        // `chain` represents the state of our mock chain.
        let mut chain = BasicExternalities::new(genesis_storage.clone());

        let mut current_block: u32 = 1;
        let mut current_weight: Weight = Weight::zero();
        let mut elapsed: Duration = Duration::ZERO;

        chain.execute_with(|| {
            start_block(current_block, None);

            for (lapse, origin, extrinsic) in extrinsics {
                if lapse > 0 {
                    let prev_header = end_block(elapsed);

                    // We update our state variables
                    current_block += u32::from(lapse);
                    current_weight = Weight::zero();
                    elapsed = Duration::ZERO;

                    // We start the next block
                    start_block(current_block, Some(prev_header));
                }

                // We compute the weight to avoid overweight blocks.
                current_weight = current_weight.saturating_add(extrinsic.get_dispatch_info().weight);

                if current_weight.ref_time() >= 2 * WEIGHT_REF_TIME_PER_SECOND {
                    #[cfg(not(fuzzing))]
                    println!("Skipping because of max weight {current_weight}");
                    continue;
                }

                // We get the current time for timing purposes.
                let now = Instant::now();

                let origin_account = endowed_accounts[origin as usize % endowed_accounts.len()].clone();
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
                let max_lock: Balance = asset_hub_kusama_runtime::Balances::locks(&acc.0).iter().map(|l| l.amount).max().unwrap_or_default();
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

            #[cfg(not(fuzzing))]
            println!("running integrity tests");
            // We run all developer-defined integrity tests
            AllPalletsWithSystem::integrity_test();
            #[cfg(not(fuzzing))]
            println!("running try_state for block {current_block}\n");
            AllPalletsWithSystem::try_state(current_block, TryStateSelect::All).unwrap();
        });
    });
}
