use codec::{DecodeLimit, Encode};
use frame_support::{
    dispatch::GetDispatchInfo,
    pallet_prelude::Weight,
    traits::{IntegrityTest, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use kitchensink_runtime::{
    constants::{currency::DOLLARS, time::SLOT_DURATION},
    AccountId, AllPalletsWithSystem, Executive, Runtime, RuntimeCall, RuntimeOrigin,
    UncheckedExtrinsic,
};
use node_primitives::BlockNumber;
use sp_consensus_babe::{
    digests::{PreDigest, SecondaryPlainPreDigest},
    Slot, BABE_ENGINE_ID,
};
use sp_runtime::{
    traits::{Dispatchable, Header},
    Digest, DigestItem, Storage,
};
use std::time::{Duration, Instant};

/// Types from the fuzzed runtime.
type Balance = <Runtime as pallet_balances::Config>::Balance;
// We use a simple Map-based Externalities implementation
type Externalities = sp_state_machine::BasicExternalities;

// The initial timestamp at the start of an input run.
const INITIAL_TIMESTAMP: u64 = 0;

/// The maximum number of extrinsics per fuzzer input.
const MAX_EXTRINSIC_COUNT: usize = 16;

/// Max number of seconds a block should run for.
const MAX_TIME_FOR_BLOCK: u64 = 6;

// We do not skip more than DEFAULT_STORAGE_PERIOD to avoid pallet_transaction_storage from
// panicking on finalize.
const MAX_BLOCK_LAPSE: u32 = sp_transaction_storage_proof::DEFAULT_STORAGE_PERIOD;

// Extrinsic delimiter: `********`
const DELIMITER: [u8; 8] = [42; 8];

struct Data<'a> {
    data: &'a [u8],
    pointer: usize,
    size: usize,
}

impl<'a> Iterator for Data<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.data.len() <= self.pointer || self.size >= MAX_EXTRINSIC_COUNT {
            return None;
        }
        let next_delimiter = self.data[self.pointer..]
            .windows(DELIMITER.len())
            .position(|window| window == DELIMITER);
        let next_pointer = match next_delimiter {
            Some(delimiter) => self.pointer + delimiter,
            None => self.data.len(),
        };
        let res = Some(&self.data[self.pointer..next_pointer]);
        self.pointer = next_pointer + DELIMITER.len();
        self.size += 1;
        res
    }
}

fn main() {
    let endowed_accounts: Vec<AccountId> = (0..5).map(|i| [i; 32].into()).collect();

    let genesis_storage: Storage = {
        use kitchensink_runtime::{
            AssetsConfig, BabeConfig, BalancesConfig, CouncilConfig, DemocracyConfig,
            ElectionsConfig, GluttonConfig, GrandpaConfig, ImOnlineConfig, IndicesConfig,
            NominationPoolsConfig, RuntimeGenesisConfig, SessionConfig, SessionKeys, SocietyConfig,
            StakingConfig, SudoConfig, TechnicalCommitteeConfig,
        };
        use pallet_grandpa::AuthorityId as GrandpaId;
        use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
        use pallet_staking::StakerStatus;
        use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
        use sp_consensus_babe::AuthorityId as BabeId;
        use sp_runtime::{app_crypto::ByteArray, BuildStorage, Perbill};

        let initial_authorities: Vec<(
            AccountId,
            AccountId,
            GrandpaId,
            BabeId,
            ImOnlineId,
            AuthorityDiscoveryId,
        )> = vec![(
            [0; 32].into(),
            [0; 32].into(),
            GrandpaId::from_slice(&[0; 32]).unwrap(),
            BabeId::from_slice(&[0; 32]).unwrap(),
            ImOnlineId::from_slice(&[0; 32]).unwrap(),
            AuthorityDiscoveryId::from_slice(&[0; 32]).unwrap(),
        )];

        let stakers = vec![(
            [0; 32].into(),
            [0; 32].into(),
            STASH,
            StakerStatus::Validator,
        )];

        let num_endowed_accounts = endowed_accounts.len();

        const ENDOWMENT: Balance = 10_000_000 * DOLLARS;
        const STASH: Balance = ENDOWMENT / 1000;

        RuntimeGenesisConfig {
            system: Default::default(),
            balances: BalancesConfig {
                balances: endowed_accounts
                    .iter()
                    .cloned()
                    .map(|x| (x, ENDOWMENT))
                    .collect(),
            },
            indices: IndicesConfig { indices: vec![] },
            session: SessionConfig {
                keys: initial_authorities
                    .iter()
                    .map(|x| {
                        (
                            x.0.clone(),
                            x.0.clone(),
                            SessionKeys {
                                grandpa: x.2.clone(),
                                babe: x.3.clone(),
                                im_online: x.4.clone(),
                                authority_discovery: x.5.clone(),
                            },
                        )
                    })
                    .collect::<Vec<_>>(),
            },
            staking: StakingConfig {
                validator_count: initial_authorities.len() as u32,
                minimum_validator_count: initial_authorities.len() as u32,
                invulnerables: vec![[0; 32].into()],
                slash_reward_fraction: Perbill::from_percent(10),
                stakers,
                ..Default::default()
            },
            democracy: DemocracyConfig::default(),
            elections: ElectionsConfig {
                members: endowed_accounts
                    .iter()
                    .take((num_endowed_accounts + 1) / 2)
                    .cloned()
                    .map(|member| (member, STASH))
                    .collect(),
            },
            council: CouncilConfig::default(),
            technical_committee: TechnicalCommitteeConfig {
                members: endowed_accounts
                    .iter()
                    .take((num_endowed_accounts + 1) / 2)
                    .cloned()
                    .collect(),
                phantom: Default::default(),
            },
            sudo: SudoConfig { key: None },
            babe: BabeConfig {
                authorities: vec![],
                epoch_config: Some(kitchensink_runtime::BABE_GENESIS_EPOCH_CONFIG),
                ..Default::default()
            },
            im_online: ImOnlineConfig { keys: vec![] },
            authority_discovery: Default::default(),
            grandpa: GrandpaConfig {
                authorities: vec![],
                ..Default::default()
            },
            technical_membership: Default::default(),
            treasury: Default::default(),
            society: SocietyConfig { pot: 0 },
            vesting: Default::default(),
            assets: AssetsConfig {
                // This asset is used by the NIS pallet as counterpart currency.
                assets: vec![(9, [0; 32].into(), true, 1)],
                ..Default::default()
            },
            transaction_storage: Default::default(),
            transaction_payment: Default::default(),
            alliance: Default::default(),
            alliance_motion: Default::default(),
            nomination_pools: NominationPoolsConfig {
                min_create_bond: 10 * DOLLARS,
                min_join_bond: DOLLARS,
                ..Default::default()
            },
            glutton: GluttonConfig {
                compute: Default::default(),
                storage: Default::default(),
                trash_data_count: Default::default(),
                ..Default::default()
            },
            pool_assets: Default::default(),
            safe_mode: Default::default(),
            tx_pause: Default::default(),
        }
        .build_storage()
        .unwrap()
    };

    ziggy::fuzz!(|data: &[u8]| {
        let iteratable = Data {
            data,
            pointer: 0,
            size: 0,
        };

        // Max weight for a block.
        let max_weight: Weight = Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND * 2, 0);

        let extrinsics: Vec<(u32, usize, RuntimeCall)> = iteratable
            .filter_map(|data| {
                // lapse is u32 (4 bytes), origin is u16 (2 bytes) -> 6 bytes minimum
                let min_data_len = 4 + 2;
                if data.len() <= min_data_len {
                    return None;
                }
                let lapse: u32 = u32::from_ne_bytes(data[0..4].try_into().unwrap());
                let origin: usize = u16::from_ne_bytes(data[4..6].try_into().unwrap()) as usize;
                let mut encoded_extrinsic: &[u8] = &data[6..];

                match DecodeLimit::decode_all_with_depth_limit(64, &mut encoded_extrinsic) {
                    Ok(decoded_extrinsic) => Some((lapse, origin, decoded_extrinsic)),
                    Err(_) => None,
                }
            })
            .collect();

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
                        BABE_ENGINE_ID,
                        PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
                            slot: Slot::from(current_timestamp / SLOT_DURATION),
                            authority_index: 42,
                        })
                        .encode(),
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
            println!("\n  finalizing block {current_block}");
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

        for (lapse, origin, extrinsic) in extrinsics {
            fn recursively_find_call(
                call: RuntimeCall,
                matches_on: fn(RuntimeCall) -> bool,
            ) -> bool {
                if let RuntimeCall::Utility(pallet_utility::Call::batch { calls })
                | RuntimeCall::Utility(pallet_utility::Call::force_batch { calls })
                | RuntimeCall::Utility(pallet_utility::Call::batch_all { calls }) = call
                {
                    for call in calls {
                        if recursively_find_call(call.clone(), matches_on) {
                            return true;
                        }
                    }
                } else if let RuntimeCall::Lottery(pallet_lottery::Call::buy_ticket { call })
                | RuntimeCall::Multisig(pallet_multisig::Call::as_multi_threshold_1 {
                    call,
                    ..
                })
                | RuntimeCall::Utility(pallet_utility::Call::as_derivative {
                    call,
                    ..
                }) = call
                {
                    return recursively_find_call(*call.clone(), matches_on);
                } else if matches_on(call) {
                    return true;
                }
                false
            }

            // We disallow referenda calls with root origin
            use frame_support::traits::OriginTrait;
            if recursively_find_call(extrinsic.clone(), |call| {
                matches!(
                    call,
                    RuntimeCall::Referenda(pallet_referenda::Call::submit {
                proposal_origin: matching_origin,
                ..
            }) | RuntimeCall::RankedPolls(pallet_referenda::Call::submit {
                proposal_origin: matching_origin,
                ..
            }) if RuntimeOrigin::from(*matching_origin.clone()).caller() == RuntimeOrigin::root().caller())
            }) {
                continue;
            }

            // We disallow batches of referenda
            // See https://github.com/paritytech/srlabs_findings/issues/296
            if recursively_find_call(extrinsic.clone(), |call| {
                matches!(
                    call,
                    RuntimeCall::Referenda(pallet_referenda::Call::submit { .. })
                )
            }) {
                continue;
            }

            // We filter out contracts call that will take too long because of fuzzer instrumentation
            if recursively_find_call(extrinsic.clone(), |call| {
                matches!(
                    call,
                    RuntimeCall::Contracts(pallet_contracts::Call::instantiate_with_code {
                        gas_limit: _limit,
                        ..
                    })
                )
                // }) if limit.ref_time() > 10_000_000_000)
            }) {
                continue;
            }

            // We filter out contracts call that will take too long because of fuzzer instrumentation
            if recursively_find_call(extrinsic.clone(), |call| {
                matches!(
                    call,
                    RuntimeCall::Contracts(
                        pallet_contracts::Call::instantiate_with_code_old_weight { .. }
                    )
                )
            }) {
                continue;
            }

            // We filter out a Society::bid call that will cause an overflow
            // See https://github.com/paritytech/srlabs_findings/issues/292
            if recursively_find_call(extrinsic.clone(), |call| {
                matches!(
                    call,
                    RuntimeCall::Society(pallet_society::Call::bid { .. })
                        | RuntimeCall::Society(pallet_society::Call::vouch { .. })
                )
            }) {
                continue;
            }

            // We filter out safe_mode calls, as they block timestamps from being set.
            if recursively_find_call(extrinsic.clone(), |call| {
                matches!(call, RuntimeCall::SafeMode(..))
            }) {
                continue;
            }

            // If the lapse is in the range [0, MAX_BLOCK_LAPSE] we finalize the block and initialize
            // a new one.
            if lapse > 0 && lapse < MAX_BLOCK_LAPSE {
                // We end the current block
                externalities.execute_with(|| end_block(current_block, current_timestamp));

                // We update our state variables
                current_block += lapse;
                current_timestamp += lapse as u64 * SLOT_DURATION;
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
                println!("Skipping because of max weight {}", max_weight);
                continue;
            }

            externalities.execute_with(|| {
                let origin_account = endowed_accounts[origin % endowed_accounts.len()].clone();
                #[cfg(not(fuzzing))]
                {
                    println!("\n    origin:     {:?}", origin_account);
                    println!("    call:       {:?}", extrinsic);
                }
                let _res = extrinsic
                    .clone()
                    .dispatch(RuntimeOrigin::signed(origin_account));
                #[cfg(not(fuzzing))]
                println!("    result:     {:?}", _res);

                // Uncomment to print events for debugging purposes
                /*
                #[cfg(not(fuzzing))]
                {
                    let all_events = kitchensink_runtime::System::events();
                    let events: Vec<_> = all_events.clone().into_iter().skip(already_seen).collect();
                    already_seen = all_events.len();
                    println!("  events:     {:?}\n", events);
                }
                */
            });

            elapsed += now.elapsed();
        }

        #[cfg(not(fuzzing))]
        println!("\n  time spent: {:?}", elapsed);
        if elapsed.as_secs() > MAX_TIME_FOR_BLOCK {
            panic!("block execution took too much time")
        }

        // We end the final block
        externalities.execute_with(|| end_block(current_block, current_timestamp));

        // After execution of all blocks.
        externalities.execute_with(|| {
            // We keep track of the total free balance of accounts
            let mut total_free: Balance = 0;
            let mut total_reserved: Balance = 0;
            // let mut _total_frozen: Balance = 0;
            for acc in frame_system::Account::<Runtime>::iter() {
                // Check that the consumer/provider state is valid.
                let acc_consumers = acc.1.consumers;
                let acc_providers = acc.1.providers;
                if acc_consumers > 0 && acc_providers == 0 {
                    panic!("Invalid state");
                }
                #[cfg(not(fuzzing))]
                {
                    println!("   account: {:?}", acc);
                    println!("      data: {:?}", acc.1.data);
                }
                // Increment our balance counts
                total_free += acc.1.data.free;
                total_reserved += acc.1.data.reserved;
                // _total_frozen += acc.1.data.frozen;
            }
            let total_issuance = pallet_balances::TotalIssuance::<Runtime>::get();
            let total_counted = total_free + total_reserved;

            if total_issuance != total_counted {
                panic!("Inconsistent total issuance: {total_issuance} but counted {total_counted}");
            }

            #[cfg(not(fuzzing))]
            println!("\nrunning integrity tests\n");
            // We run all developer-defined integrity tests
            <AllPalletsWithSystem as IntegrityTest>::integrity_test();
        });
    });
}
