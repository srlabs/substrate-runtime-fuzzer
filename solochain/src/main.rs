use codec::{DecodeLimit, Encode};
use frame_support::{
    dispatch::GetDispatchInfo,
    pallet_prelude::Weight,
    traits::{IntegrityTest, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use solochain_template_runtime::{
    AccountId, AllPalletsWithSystem, Balance, Balances, Executive, Runtime, RuntimeCall,
    RuntimeOrigin, Timestamp, SLOT_DURATION,
};
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use sp_runtime::{
    traits::{Dispatchable, Header},
    Digest, DigestItem, Storage,
};
use sp_state_machine::BasicExternalities;
use std::time::{Duration, Instant};

fn genesis(accounts: &[AccountId]) -> Storage {
    use solochain_template_runtime::{
        AuraConfig, BalancesConfig, RuntimeGenesisConfig, SudoConfig,
    };
    use sp_consensus_aura::sr25519::AuthorityId as AuraId;
    use sp_runtime::app_crypto::ByteArray;
    use sp_runtime::BuildStorage;

    // Configure endowed accounts with initial balance of 1 << 60.
    let balances = accounts.iter().cloned().map(|k| (k, 1 << 60)).collect();
    let authorities = vec![AuraId::from_slice(&[0; 32]).unwrap()];

    RuntimeGenesisConfig {
        system: Default::default(),
        balances: BalancesConfig { balances },
        aura: AuraConfig { authorities },
        grandpa: Default::default(),
        sudo: SudoConfig { key: None }, // Assign no network admin rights.
        transaction_payment: Default::default(),
    }
    .build_storage()
    .unwrap()
}

fn main() {
    let endowed_accounts: Vec<AccountId> = (0..5).map(|i| [i; 32].into()).collect();

    let genesis_storage = genesis(&endowed_accounts);

    ziggy::fuzz!(|data: &[u8]| {
        // We build the list of extrinsics we will execute
        let mut extrinsic_data = data;
        let mut extrinsics: Vec<(u8, u8, RuntimeCall)> = vec![];
        while let Ok(decoded) = DecodeLimit::decode_with_depth_limit(64, &mut extrinsic_data) {
            match decoded {
                (_, _, RuntimeCall::System(_)) => continue,
                _ => {}
            }
            extrinsics.push(decoded);
        }
        if extrinsics.is_empty() {
            return;
        }

        // `chain` corresponds to the chain's state
        let mut chain = BasicExternalities::new(genesis_storage.clone());

        let mut current_block: u32 = 1;
        let mut current_weight: Weight = Weight::zero();
        let mut elapsed: Duration = Duration::ZERO;

        let start_block = |block: u32| {
            #[cfg(not(fuzzing))]
            println!("\ninitializing block {block}");

            let pre_digest = Digest {
                logs: vec![DigestItem::PreRuntime(
                    AURA_ENGINE_ID,
                    Slot::from(block as u64).encode(),
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
        };

        let end_block = |current_block: u32, elapsed: Duration| {
            #[cfg(not(fuzzing))]
            println!("\n  time spent: {elapsed:?}");
            assert!(elapsed.as_secs() <= 2, "block execution took too much time");

            #[cfg(not(fuzzing))]
            println!("  finalizing block {current_block}");
            Executive::finalize_block();
        };

        chain.execute_with(|| {
            start_block(current_block);

            for (lapse, origin, extrinsic) in extrinsics {
                if lapse > 0 {
                    // We end the current block
                    end_block(current_block, elapsed);

                    // 393 * 256 = 100608 which nearly corresponds to a week
                    let actual_lapse = u32::from(lapse) * 393;
                    // We update our state variables
                    current_block += actual_lapse;
                    // current_timestamp += u64::from(actual_lapse) * SLOT_DURATION;
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

                let origin_account =
                    endowed_accounts[origin as usize % endowed_accounts.len()].clone();

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

            // We end the final block
            end_block(current_block, elapsed);

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
            assert_eq!(total_issuance, counted_issuance, "Inconsistent total issuance vs counted issuance");

            #[cfg(not(fuzzing))]
            println!("\nrunning integrity tests");
            // We run all developer-defined integrity tests
            AllPalletsWithSystem::integrity_test();
            #[cfg(not(fuzzing))]
            println!("running try_state for block {current_block}\n");
            AllPalletsWithSystem::try_state(current_block, TryStateSelect::All).unwrap();
        });
    });
}
