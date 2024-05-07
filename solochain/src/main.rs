use codec::{DecodeLimit, Encode};
use frame_support::{
    dispatch::GetDispatchInfo,
    pallet_prelude::Weight,
    traits::{IntegrityTest, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use frame_system::Account;
use pallet_balances::{Holds, TotalIssuance};
use solochain_template_runtime::{
    AccountId, AllPalletsWithSystem, Balance, Balances, Executive, Runtime, RuntimeCall,
    RuntimeOrigin, Timestamp, SLOT_DURATION,
};
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use sp_runtime::{
    testing::H256,
    traits::{Dispatchable, Header},
    Digest, DigestItem, Storage,
};
use sp_state_machine::BasicExternalities;
use std::{
    iter,
    time::{Duration, Instant},
};

fn genesis(accounts: &[AccountId]) -> Storage {
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
        balances: BalancesConfig { balances },
        aura: AuraConfig { authorities },
        grandpa: GrandpaConfig::default(),
        sudo: SudoConfig { key: None }, // Assign no network admin rights.
        transaction_payment: TransactionPaymentConfig::default(),
    }
    .build_storage()
    .unwrap()
}

fn start_block(block: u32) {
    #[cfg(not(fuzzing))]
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

    #[cfg(not(fuzzing))]
    println!("  setting timestamp");
    Timestamp::set(RuntimeOrigin::none(), u64::from(block) * SLOT_DURATION).unwrap();
}

fn end_block(elapsed: Duration) {
    #[cfg(not(fuzzing))]
    println!("\n  time spent: {elapsed:?}");
    assert!(elapsed.as_secs() <= 2, "block execution took too much time");

    #[cfg(not(fuzzing))]
    println!("  finalizing block");
    Executive::finalize_block();
}

fn run_input(accounts: &[AccountId], genesis: &Storage, data: &[u8]) {
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

        start_block(block);

        for (lapse, origin, extrinsic) in extrinsics {
            if lapse > 0 {
                end_block(elapsed);

                block += u32::from(lapse) * 393; // 393 * 256 = 100608 which nearly corresponds to a week
                weight = 0.into();
                elapsed = Duration::ZERO;

                start_block(block);
            }

            // We compute the weight to avoid overweight blocks.
            weight = weight.saturating_add(extrinsic.get_dispatch_info().weight);
            if weight.ref_time() >= 2 * WEIGHT_REF_TIME_PER_SECOND {
                #[cfg(not(fuzzing))]
                println!("Extrinsic would exhaust block weight, skipping");
                continue;
            }

            let origin = accounts[origin as usize % accounts.len()].clone();

            #[cfg(not(fuzzing))]
            println!("\n    origin:     {origin:?}");
            #[cfg(not(fuzzing))]
            println!("    call:       {extrinsic:?}");

            let now = Instant::now(); // We get the current time for timing purposes.
            #[allow(unused_variables)]
            let res = extrinsic.dispatch(RuntimeOrigin::signed(origin));
            elapsed += now.elapsed();

            #[cfg(not(fuzzing))]
            println!("    result:     {res:?}");
        }

        end_block(elapsed);

        // After execution of all blocks, we run invariants
        let mut counted_free = 0;
        let mut counted_reserved = 0;
        for acc in Account::<Runtime>::iter() {
            // Check that the consumer/provider state is valid.
            let acc_consumers = acc.1.consumers;
            let acc_providers = acc.1.providers;
            assert!(!(acc_consumers > 0 && acc_providers == 0), "Invalid state");

            // Increment our balance counts
            counted_free += acc.1.data.free;
            counted_reserved += acc.1.data.reserved;
            // Check that locks and holds are valid.
            let max_lock: Balance = Balances::locks(&acc.0)
                .iter()
                .map(|l| l.amount)
                .max()
                .unwrap_or_default();
            assert_eq!(
                max_lock, acc.1.data.frozen,
                "Max lock should be equal to frozen balance"
            );
            let sum_holds: Balance = Holds::<Runtime>::get(&acc.0).iter().map(|l| l.amount).sum();
            assert!(
                sum_holds <= acc.1.data.reserved,
                "Sum of all holds ({sum_holds}) should be less than or equal to reserved balance {}",
                acc.1.data.reserved
            );
        }
        let total_issuance = TotalIssuance::<Runtime>::get();
        let counted_issuance = counted_free + counted_reserved;
        assert_eq!(total_issuance, counted_issuance);
        assert!(total_issuance <= initial_total_issuance);
        // We run all developer-defined integrity tests
        AllPalletsWithSystem::integrity_test();
        AllPalletsWithSystem::try_state(block, TryStateSelect::All).unwrap();
    });
}

fn main() {
    let accounts: Vec<AccountId> = (0..5).map(|i| [i; 32].into()).collect();
    let genesis = genesis(&accounts);

    ziggy::fuzz!(|data: &[u8]| {
        run_input(&accounts, &genesis, data);
    });
}
