#![warn(clippy::pedantic)]
use codec::{DecodeLimit, Encode};
use frame_support::{
    dispatch::{DispatchInfo, GetDispatchInfo},
    pallet_prelude::Weight,
    traits::{IntegrityTest, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use frame_system::Account;
use pallet_balances::{Holds, TotalIssuance};
use solochain_template_runtime::{
    AccountId, AllPalletsWithSystem, Balance, Balances, Executive, Runtime, RuntimeCall,
    RuntimeOrigin, Timestamp, TxExtension, SLOT_DURATION,
};
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use sp_runtime::{
    testing::H256,
    traits::{Dispatchable, Header, TransactionExtension, TxBaseImplication},
    transaction_validity::TransactionSource,
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
    use solochain_template_runtime::{
        AuraConfig, BalancesConfig, GrandpaConfig, RuntimeGenesisConfig, SudoConfig, SystemConfig,
        TransactionPaymentConfig,
    };
    use sp_consensus_aura::sr25519::AuthorityId as AuraId;
    use sp_runtime::{app_crypto::ByteArray, BuildStorage};

    // Configure endowed accounts with initial balance of 1 << 60.
    let balances = accounts.iter().cloned().map(|k| (k, 1 << 60)).collect();
    let authorities = vec![AuraId::from_slice(&[0; 32]).unwrap()];

    RuntimeGenesisConfig {
        system: SystemConfig::default(),
        balances: BalancesConfig {
            balances,
            dev_accounts: None,
        },
        aura: AuraConfig { authorities },
        grandpa: GrandpaConfig::default(),
        sudo: SudoConfig { key: None }, // Assign no network admin rights.
        transaction_payment: TransactionPaymentConfig::default(),
    }
    .build_storage()
    .unwrap()
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

        initialize_block(block);

        for (lapse, origin, extrinsic) in extrinsics {
            if lapse > 0 {
                finalize_block(elapsed);

                block += u32::from(lapse) * 393; // 393 * 256 = 100608 which nearly corresponds to a week
                weight = 0.into();
                elapsed = Duration::ZERO;

                initialize_block(block);
            }

            weight.saturating_accrue(extrinsic.get_dispatch_info().call_weight);
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

            let ext: TxExtension = (
                frame_system::AuthorizeCall::<Runtime>::new(),
                frame_system::CheckNonZeroSender::<Runtime>::new(),
                frame_system::CheckSpecVersion::<Runtime>::new(),
                frame_system::CheckTxVersion::<Runtime>::new(),
                frame_system::CheckGenesis::<Runtime>::new(),
                frame_system::CheckEra::<Runtime>::from(sp_runtime::generic::Era::immortal()), // TODO MORTAL
                frame_system::CheckNonce::<Runtime>::from(0),
                frame_system::CheckWeight::<Runtime>::new(),
                pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(1000),
                frame_metadata_hash_extension::CheckMetadataHash::<Runtime>::new(true),
                frame_system::WeightReclaim::<Runtime>::new(),
            );

            let maybe_validation = ext.validate(
                RuntimeOrigin::signed(origin.clone()),
                &extrinsic,
                &DispatchInfo::default(), // TODO Check if we can do better than default
                100,                      // TODO Put actual length of extrinsic
                Default::default(),       // TODO Check if we can do better than default
                &TxBaseImplication(0),    // TODO Check if we can do better
                TransactionSource::Local,
            );

            if let Ok((_, validation, origin)) = maybe_validation {
                let preparation = ext
                    .prepare(
                        validation,
                        &origin.clone(),
                        &extrinsic,
                        &DispatchInfo::default(),
                        100,
                    )
                    .expect("Transaction validated, should also prepare correctly");

                let now = Instant::now(); // We get the current time for timing purposes.
                let res = extrinsic.dispatch(origin);
                elapsed += now.elapsed();

                #[cfg(not(feature = "fuzzing"))]
                println!("    result:     {res:?}");

                let mut post_info = match res {
                    Ok(p) => p,
                    Err(p) => p.post_info,
                };

                let result = res.map(|_| ()).map_err(|e| e.error);

                TxExtension::post_dispatch(
                    preparation,
                    &DispatchInfo::default(), // TODO Check if we can do better than default
                    &mut post_info,
                    100, // TODO put actual extrisic length
                    &result,
                )
                .expect("Post-dispatch should never fail");
            }
        }

        finalize_block(elapsed);

        check_invariants(block, initial_total_issuance);
    });
}

fn initialize_block(block: u32) {
    #[cfg(not(feature = "fuzzing"))]
    println!("\ninitializing block {block}");

    Executive::initialize_block(&Header::new(
        block,
        H256::default(),
        H256::default(),
        H256::default(),
        Digest {
            logs: vec![DigestItem::PreRuntime(
                AURA_ENGINE_ID,
                Slot::from(u64::from(block)).encode(),
            )],
        },
    ));

    #[cfg(not(feature = "fuzzing"))]
    println!("  setting timestamp");
    Timestamp::set(RuntimeOrigin::none(), u64::from(block) * SLOT_DURATION).unwrap();
}

fn finalize_block(elapsed: Duration) {
    #[cfg(not(feature = "fuzzing"))]
    println!("\n  time spent: {elapsed:?}");
    assert!(elapsed.as_secs() <= 2, "block execution took too much time");

    #[cfg(not(feature = "fuzzing"))]
    println!("  finalizing block");
    Executive::finalize_block();
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
    assert_eq!(total_issuance, counted_issuance);
    assert!(total_issuance <= initial_total_issuance);
    // We run all developer-defined integrity tests
    AllPalletsWithSystem::integrity_test();
    AllPalletsWithSystem::try_state(block, TryStateSelect::All).unwrap();
}
