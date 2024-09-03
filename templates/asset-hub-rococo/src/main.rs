#![warn(clippy::pedantic)]
use asset_hub_rococo_runtime::{
    AllPalletsWithSystem, Balances, Executive, ParachainSystem, Runtime, RuntimeCall,
    RuntimeOrigin, Timestamp, Assets, AssetConversion,
};
use cumulus_primitives_core::relay_chain::Header;
use codec::{DecodeLimit, Encode};
use frame_support::{
    dispatch::GetDispatchInfo,
    pallet_prelude::Weight,
    traits::{IntegrityTest, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use frame_system::Account;
use pallet_balances::{Holds, TotalIssuance};
use parachains_common::{AccountId, Balance};
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
    collections::BTreeMap,
};
use testnet_parachains_constants::rococo::consensus::SLOT_DURATION;

fn main() {
    let accounts: Vec<AccountId> = (0..5).map(|i| [i; 32].into()).collect();
    let genesis = generate_genesis(&accounts);

    ziggy::fuzz!(|data: &[u8]| {
        process_input(&accounts, &genesis, data);
    });
}

fn generate_genesis(accounts: &[AccountId]) -> Storage {
    use asset_hub_rococo_runtime::{
        AssetsConfig, AuraConfig, AuraExtConfig, BalancesConfig, CollatorSelectionConfig,
        ForeignAssetsConfig, ParachainInfoConfig, ParachainSystemConfig, PolkadotXcmConfig,
        PoolAssetsConfig, RuntimeGenesisConfig, SessionConfig, SystemConfig,
        TransactionPaymentConfig,
    };
    use sp_consensus_aura::sr25519::AuthorityId as AuraId;
    use sp_runtime::{app_crypto::ByteArray, BuildStorage};
    use staging_xcm::v3::{MultiLocation, Junction::{PalletInstance, GeneralIndex}};


    // Configure endowed accounts with initial balance of 1 << 60.
    let balances = accounts.iter().cloned().map(|k| (k, 1 << 60)).collect();
    let authorities = vec![AuraId::from_slice(&[0; 32]).unwrap()];

    let mut storage = RuntimeGenesisConfig {
        system: SystemConfig::default(),
        balances: BalancesConfig { balances },
        aura: AuraConfig { authorities },
        transaction_payment: TransactionPaymentConfig::default(),
        parachain_system: ParachainSystemConfig::default(),
        parachain_info: ParachainInfoConfig::default(),
        collator_selection: CollatorSelectionConfig::default(),
        session: SessionConfig::default(),
        aura_ext: AuraExtConfig::default(),
        assets: AssetsConfig::default(),
        foreign_assets: ForeignAssetsConfig::default(),
        polkadot_xcm: PolkadotXcmConfig::default(),
        pool_assets: PoolAssetsConfig::default(),
    }
    .build_storage()
    .unwrap();

    BasicExternalities::execute_with_storage(&mut storage, || {
        let origin = RuntimeOrigin::signed(accounts[0].clone());
        Assets::create(origin.clone(), 0.into(), accounts[0].clone().into(), 1000).unwrap();
        Assets::mint(origin.clone(), 0.into(), accounts[0].clone().into(), 1_000_000).unwrap();
        AssetConversion::create_pool(
            origin.clone(),
            Box::new(MultiLocation::parent().into()),
            Box::new(MultiLocation::from((PalletInstance(50), GeneralIndex(0))))
        ).unwrap();
        AssetConversion::add_liquidity(
            origin.clone(),
            Box::new(MultiLocation::parent().into()),
            Box::new(MultiLocation::from((PalletInstance(50), GeneralIndex(0)))),
            10_000_000_000,
            10_000,
            10_000_000_000,
            10_000,
            accounts[0].clone()
        ).unwrap();
    });

    storage
}

fn process_input(accounts: &[AccountId], genesis: &Storage, data: &[u8]) {
    let mut data = data;
    // We build the list of extrinsics we will execute
    let extrinsics: Vec<(/* lapse */ u8, /* origin */ u8, RuntimeCall)> =
        iter::from_fn(|| DecodeLimit::decode_with_depth_limit(64, &mut data).ok())
            .filter(|(_, _, x)| !matches!(x, RuntimeCall::System(_)))
            .collect();
    if extrinsics.is_empty() {
        return;
    }

    let mut block: u32 = 1;
    let mut weight: Weight = 0.into();
    let mut elapsed: Duration = Duration::ZERO;

    BasicExternalities::execute_with_storage(&mut genesis.clone(), || {
        let initial_total_issuance = TotalIssuance::<Runtime>::get();

        initialize_block(block, &None);

        for (lapse, origin, extrinsic) in extrinsics {
            if lapse > 0 {
                let prev_header = finalize_block(elapsed);

                block += u32::from(lapse) * 393; // 393 * 256 = 100608 which nearly corresponds to a week
                weight = 0.into();
                elapsed = Duration::ZERO;

                initialize_block(block, &Some(prev_header));
            }

            weight.saturating_accrue(extrinsic.get_dispatch_info().weight);
            if weight.ref_time() >= 2 * WEIGHT_REF_TIME_PER_SECOND {
                #[cfg(not(feature = "fuzzing"))]
                println!("Extrinsic would exhaust block weight, skipping");
                continue;
            }

            let origin = accounts[origin as usize % accounts.len()].clone();

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

    let parent_header = &Header::new(
        block,
        H256::default(),
        H256::default(),
        prev_header.clone().map(|x| x.hash()).unwrap_or_default(),
        Digest {
            logs: vec![DigestItem::PreRuntime(
                AURA_ENGINE_ID,
                Slot::from(u64::from(block)).encode(),
            )],
        },
    );
    Executive::initialize_block(parent_header);

    #[cfg(not(feature = "fuzzing"))]
    println!("  setting timestamp");
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
            current_slot: Slot::from(u64::from(block)),
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
    println!("  finalizing block");
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
            "Sum of all holds ({sum_holds}) should be inferior or equal to reserved balance {}",
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
