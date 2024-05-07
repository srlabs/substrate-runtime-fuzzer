use asset_hub_kusama_runtime::{
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
    collections::BTreeMap,
    iter,
    time::{Duration, Instant},
};

fn genesis(accounts: &[AccountId]) -> Storage {
    use asset_hub_kusama_runtime::{
        AssetsConfig, AuraConfig, AuraExtConfig, BalancesConfig, CollatorSelectionConfig,
        ForeignAssetsConfig, ParachainInfoConfig, ParachainSystemConfig, PolkadotXcmConfig,
        PoolAssetsConfig, RuntimeGenesisConfig, SessionConfig, SessionKeys, SystemConfig,
        TransactionPaymentConfig,
    };
    use sp_consensus_aura::sr25519::AuthorityId as AuraId;
    use sp_runtime::app_crypto::ByteArray;
    use sp_runtime::BuildStorage;

    let initial_authorities: Vec<(AccountId, AuraId)> =
        vec![([0; 32].into(), AuraId::from_slice(&[0; 32]).unwrap())];

    RuntimeGenesisConfig {
        system: SystemConfig::default(),
        balances: BalancesConfig {
            // Configure endowed accounts with initial balance of 1 << 60.
            balances: accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
        },
        aura: AuraConfig::default(),
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
        assets: AssetsConfig::default(),
        foreign_assets: ForeignAssetsConfig::default(),
        pool_assets: PoolAssetsConfig::default(),
        transaction_payment: TransactionPaymentConfig::default(),
    }
    .build_storage()
    .unwrap()
}

fn start_block(block: u32, prev_header: &Option<Header>) {
    #[cfg(not(fuzzing))]
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
        prev_header.clone().map(|x| x.hash()).unwrap_or_default(),
        pre_digest,
    );
    Executive::initialize_block(parent_header);

    #[cfg(not(fuzzing))]
    println!("  setting timestamp");
    Timestamp::set(RuntimeOrigin::none(), u64::from(block) * SLOT_DURATION).unwrap();

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

fn end_block(elapsed: Duration) -> Header {
    #[cfg(not(fuzzing))]
    println!("\n  time spent: {elapsed:?}");
    assert!(elapsed.as_secs() <= 2, "block execution took too much time");

    #[cfg(not(fuzzing))]
    println!("finalizing block");
    Executive::finalize_block()
}

fn run_input(accounts: &[AccountId], genesis: &Storage, data: &[u8]) {
    // We build the list of extrinsics we will execute
    let mut extrinsic_data = data;
    // Vec<(lapse, origin, extrinsic)>
    let extrinsics: Vec<(u8, u8, RuntimeCall)> =
        iter::from_fn(|| DecodeLimit::decode_with_depth_limit(64, &mut extrinsic_data).ok())
            .filter(|(_, _, x)| !matches!(x, RuntimeCall::System(_)))
            .collect();
    if extrinsics.is_empty() {
        return;
    }

    let mut current_block: u32 = 1;
    let mut current_weight: Weight = Weight::zero();
    let mut elapsed: Duration = Duration::ZERO;

    BasicExternalities::execute_with_storage(&mut genesis.clone(), || {
        let initial_total_issuance = pallet_balances::TotalIssuance::<Runtime>::get();

        start_block(current_block, &None);

        for (lapse, origin, extrinsic) in extrinsics {
            if lapse > 0 {
                let prev_header = end_block(elapsed);

                // We update our state variables
                current_block += u32::from(lapse);
                current_weight = Weight::zero();
                elapsed = Duration::ZERO;

                // We start the next block
                start_block(current_block, &Some(prev_header));
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

            let origin_account = accounts[origin as usize % accounts.len()].clone();
            #[cfg(not(fuzzing))]
            {
                println!("\n    origin:     {origin_account:?}");
                println!("    call:       {extrinsic:?}");
            }
            #[allow(unused_variables)]
            let res = extrinsic.dispatch(RuntimeOrigin::signed(origin_account));
            #[cfg(not(fuzzing))]
            println!("    result:     {res:?}");

            elapsed += now.elapsed();
        }

        end_block(elapsed);

        // After execution of all blocks, we run invariants
        let mut counted_free = 0;
        let mut counted_reserved = 0;
        for (account, info) in frame_system::Account::<Runtime>::iter() {
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
        AllPalletsWithSystem::try_state(current_block, TryStateSelect::All).unwrap();
    });
}

fn main() {
    let accounts: Vec<AccountId> = (0..5).map(|i| [i; 32].into()).collect();
    let genesis = genesis(&accounts);

    ziggy::fuzz!(|data: &[u8]| {
        run_input(&accounts, &genesis, data);
    });
}
