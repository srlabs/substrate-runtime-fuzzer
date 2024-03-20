use codec::Encode;
use frame_support::{
    dispatch::GetDispatchInfo,
    pallet_prelude::Weight,
    traits::{IntegrityTest, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use solochain_template_runtime::{
    AccountId, AllPalletsWithSystem, BlockNumber, Executive, Runtime, RuntimeCall, RuntimeOrigin,
    UncheckedExtrinsic, SLOT_DURATION,
};
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use sp_runtime::{
    traits::{Dispatchable, Header},
    Digest, DigestItem, Storage,
};
use std::time::{Duration, Instant};
use substrate_runtime_fuzzer::*;

// We use a simple Map-based Externalities implementation
pub type Externalities = sp_state_machine::BasicExternalities;

/// Types from the fuzzed runtime.
type Balance = <Runtime as pallet_balances::Config>::Balance;

fn main() {
    let endowed_accounts: Vec<AccountId> = (0..5).map(|i| [i; 32].into()).collect();

    let genesis_storage: Storage = {
        use pallet_grandpa::AuthorityId as GrandpaId;
        use solochain_template_runtime::{
            AuraConfig, BalancesConfig, GrandpaConfig, RuntimeGenesisConfig, SudoConfig,
        };
        use sp_consensus_aura::sr25519::AuthorityId as AuraId;
        use sp_runtime::app_crypto::ByteArray;
        use sp_runtime::BuildStorage;

        let initial_authorities: Vec<(AuraId, GrandpaId)> = vec![(
            AuraId::from_slice(&[0; 32]).unwrap(),
            GrandpaId::from_slice(&[0; 32]).unwrap(),
        )];

        RuntimeGenesisConfig {
            system: Default::default(),
            balances: BalancesConfig {
                // Configure endowed accounts with initial balance of 1 << 60.
                balances: endowed_accounts
                    .iter()
                    .cloned()
                    .map(|k| (k, 1 << 60))
                    .collect(),
            },
            aura: AuraConfig {
                authorities: initial_authorities.iter().map(|x| (x.0.clone())).collect(),
            },
            grandpa: GrandpaConfig {
                authorities: initial_authorities
                    .iter()
                    .map(|x| (x.1.clone(), 1))
                    .collect(),
                ..Default::default()
            },
            sudo: SudoConfig {
                // Assign no network admin rights.
                key: None,
            },
            transaction_payment: Default::default(),
        }
        .build_storage()
        .unwrap()
    };

    ziggy::fuzz!(|data: &[u8]| {
        let mut iteratable = Data::from_data(data);

        // Max weight for a block.
        let max_weight: Weight = Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND * 2, 0);

        let extrinsics: Vec<(Option<u32>, usize, RuntimeCall)> =
            iteratable.extract_extrinsics::<RuntimeCall>();

        if extrinsics.is_empty() {
            return;
        }

        // `externalities` represents the state of our mock chain.
        let mut externalities = Externalities::new(genesis_storage.clone());

        let mut current_block: u32 = 1;
        let mut current_timestamp: u64 = INITIAL_TIMESTAMP;
        let mut current_weight: Weight = Weight::zero();
        // let mut already_seen = 0; // This must be uncommented if you want to print events
        let mut elapsed: Duration = Duration::ZERO;

        let start_block = |block: u32, current_timestamp: u64| {
            #[cfg(not(fuzzing))]
            println!("\ninitializing block {block}");

            let pre_digest = match current_timestamp {
                INITIAL_TIMESTAMP => Default::default(),
                _ => Digest {
                    logs: vec![DigestItem::PreRuntime(
                        AURA_ENGINE_ID,
                        Slot::from(current_timestamp / SLOT_DURATION).encode(),
                    )],
                },
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
            // We apply the timestamp extrinsic for the current block.
            Executive::apply_extrinsic(UncheckedExtrinsic::new_unsigned(RuntimeCall::Timestamp(
                pallet_timestamp::Call::set {
                    now: current_timestamp,
                },
            )))
            .unwrap()
            .unwrap();

            // Calls that need to be called before each block starts (init_calls) go here
        };

        let end_block = |current_block: u32, _current_timestamp: u64| {
            #[cfg(not(fuzzing))]
            println!("  finalizing block {current_block}");
            Executive::finalize_block();

            #[cfg(not(fuzzing))]
            println!("  testing invariants for block {current_block}");
            <AllPalletsWithSystem as TryState<BlockNumber>>::try_state(
                current_block,
                TryStateSelect::All,
            )
            .unwrap();
        };

        externalities.execute_with(|| start_block(current_block, current_timestamp));

        for (maybe_lapse, origin, extrinsic) in extrinsics {
            // If the lapse is in the range [0, MAX_BLOCK_LAPSE] we finalize the block and initialize
            // a new one.
            if let Some(lapse) = maybe_lapse {
                // We end the current block
                externalities.execute_with(|| end_block(current_block, current_timestamp));

                // We update our state variables
                current_block += lapse;
                current_timestamp += u64::from(lapse) * SLOT_DURATION;
                current_weight = Weight::zero();
                elapsed = Duration::ZERO;

                // We start the next block
                externalities.execute_with(|| start_block(current_block, current_timestamp));
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
                let origin_account = endowed_accounts[origin % endowed_accounts.len()].clone();

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

                // Uncomment to print events for debugging purposes
                /*
                #[cfg(not(fuzzing))]
                {
                    let all_events = solochain_template_runtime::System::events();
                    let events: Vec<_> = all_events.clone().into_iter().skip(already_seen).collect();
                    already_seen = all_events.len();
                    println!("  events:     {:?}\n", events);
                }
                */
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
        externalities.execute_with(|| end_block(current_block, current_timestamp));

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
                let max_lock: Balance = solochain_template_runtime::Balances::locks(&acc.0).iter().map(|l| l.amount).max().unwrap_or_default();
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
            assert!(
                total_issuance == counted_issuance,
                "Inconsistent total issuance: {total_issuance} but counted {counted_issuance}"
            );

            #[cfg(not(fuzzing))]
            println!("\nrunning integrity tests\n");
            // We run all developer-defined integrity tests
            <AllPalletsWithSystem as IntegrityTest>::integrity_test();
        });
    });
}
