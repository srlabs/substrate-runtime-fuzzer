use codec::{DecodeLimit, Encode};
use frame_support::{
    dispatch::GetDispatchInfo,
    pallet_prelude::Weight,
    traits::{IntegrityTest, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use kusama_runtime::{
    AllPalletsWithSystem, Executive, Runtime, RuntimeCall, RuntimeOrigin, UncheckedExtrinsic,
};
use kusama_runtime_constants::{currency::UNITS, time::SLOT_DURATION};
use polkadot_primitives::{AccountId, Balance, BlockNumber};
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
type Externalities = sp_state_machine::TestExternalities<sp_core::Blake2Hasher>;

// The initial timestamp at the start of an input run.
const INITIAL_TIMESTAMP: u64 = 0;

/// The maximum number of extrinsics per fuzzer input.
const MAX_EXTRINSIC_COUNT: usize = 32;

/// Max number of seconds a block should run for.
const MAX_TIME_FOR_BLOCK: u64 = 6;

// We do not skip more than DEFAULT_STORAGE_PERIOD to avoid pallet_transaction_storage from
// panicking on finalize.
const MAX_BLOCK_LAPSE: u32 = sp_transaction_storage_proof::DEFAULT_STORAGE_PERIOD;

// Decode depth limit
const MAX_DECODE_LIMIT: u32 = 52;

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

use cumulus_primitives_core::relay_chain::{AssignmentId, ValidatorId};
use pallet_grandpa::AuthorityId as GrandpaId;
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use pallet_staking::StakerStatus;
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_runtime::{app_crypto::ByteArray, BuildStorage, Perbill};

struct Authority {
    account: AccountId,
    _nominator: AccountId,
    grandpa: GrandpaId,
    babe: BabeId,
    im_online: ImOnlineId,
    validator: ValidatorId,
    assignment: AssignmentId,
    authority_discovery: AuthorityDiscoveryId,
}

fn main() {
    let endowed_accounts: Vec<AccountId> = (0..5).map(|i| [i; 32].into()).collect();

    let genesis_storage: Storage = {
        use kusama_runtime as kusama;

        let initial_authorities: Vec<Authority> = vec![Authority {
            account: [0; 32].into(),
            _nominator: [0; 32].into(),
            grandpa: GrandpaId::from_slice(&[0; 32]).unwrap(),
            babe: BabeId::from_slice(&[0; 32]).unwrap(),
            im_online: ImOnlineId::from_slice(&[0; 32]).unwrap(),
            validator: ValidatorId::from_slice(&[0; 32]).unwrap(),
            assignment: AssignmentId::from_slice(&[0; 32]).unwrap(),
            authority_discovery: AuthorityDiscoveryId::from_slice(&[0; 32]).unwrap(),
        }];

        let stakers = vec![(
            [0; 32].into(),
            [0; 32].into(),
            STASH,
            StakerStatus::Validator,
        )];

        let _num_endowed_accounts = endowed_accounts.len();

        const ENDOWMENT: Balance = 10_000_000 * UNITS;
        const STASH: Balance = ENDOWMENT / 1000;

        kusama::GenesisConfig {
            system: Default::default(),
            balances: kusama::BalancesConfig {
                // Configure endowed accounts with initial balance of 1 << 60.
                balances: endowed_accounts
                    .iter()
                    .cloned()
                    .map(|k| (k, 1 << 60))
                    .collect(),
            },
            indices: kusama::IndicesConfig { indices: vec![] },
            session: kusama::SessionConfig {
                keys: initial_authorities
                    .iter()
                    .map(|x| {
                        (
                            x.account.clone(),
                            x.account.clone(),
                            kusama::SessionKeys {
                                grandpa: x.grandpa.clone(),
                                babe: x.babe.clone(),
                                im_online: x.im_online.clone(),
                                para_validator: x.validator.clone(),
                                para_assignment: x.assignment.clone(),
                                authority_discovery: x.authority_discovery.clone(),
                            },
                        )
                    })
                    .collect::<Vec<_>>(),
            },
            staking: kusama::StakingConfig {
                validator_count: initial_authorities.len() as u32,
                minimum_validator_count: initial_authorities.len() as u32,
                invulnerables: vec![[0; 32].into()],
                slash_reward_fraction: Perbill::from_percent(10),
                stakers,
                ..Default::default()
            },
            babe: kusama::BabeConfig {
                authorities: Default::default(),
                epoch_config: Some(kusama::BABE_GENESIS_EPOCH_CONFIG),
            },
            grandpa: Default::default(),
            im_online: Default::default(),
            authority_discovery: kusama::AuthorityDiscoveryConfig { keys: vec![] },
            claims: kusama::ClaimsConfig {
                claims: vec![],
                vesting: vec![],
            },
            vesting: kusama::VestingConfig { vesting: vec![] },
            treasury: Default::default(),
            hrmp: Default::default(),
            configuration: kusama::ConfigurationConfig {
                config: Default::default(),
            },
            paras: Default::default(),
            xcm_pallet: Default::default(),
            nomination_pools: Default::default(),
            nis_counterpart_balances: Default::default(),
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

                match DecodeLimit::decode_all_with_depth_limit(
                    MAX_DECODE_LIMIT,
                    &mut encoded_extrinsic,
                ) {
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
                            authority_index: 0,
                        })
                        .encode(),
                    )],
                },
            };

            use sp_runtime::{generic, traits::BlakeTwo256};
            let grandparent_header: generic::Header<BlockNumber, BlakeTwo256> = Header::new(
                block,
                Default::default(),
                Default::default(),
                <frame_system::Pallet<Runtime>>::parent_hash(),
                pre_digest.clone(),
            );

            let parent_header = Header::new(
                block,
                Default::default(),
                Default::default(),
                grandparent_header.hash(),
                pre_digest,
            );

            Executive::initialize_block(&parent_header);

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

            #[cfg(not(fuzzing))]
            println!("  setting bitfields");
            // We apply the timestamp extrinsic for the current block.
            Executive::apply_extrinsic(UncheckedExtrinsic::new_unsigned(
                RuntimeCall::ParaInherent(
                    polkadot_runtime_parachains::paras_inherent::Call::enter {
                        data: polkadot_primitives::InherentData {
                            parent_header: grandparent_header,
                            bitfields: Default::default(),
                            backed_candidates: Default::default(),
                            disputes: Default::default(),
                        },
                    },
                ),
            ))
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

        'extrinsics_loop: for (lapse, origin, extrinsic) in extrinsics {
            // We filter out a Society::bid call that will cause an overflow
            // See https://github.com/paritytech/srlabs_findings/issues/292
            if matches!(
                extrinsic.clone(),
                RuntimeCall::Society(pallet_society::Call::bid { .. })
            ) {
                continue;
            }

            // We filter out calls with Fungible(0) as they cause a debug crash
            if let RuntimeCall::XcmPallet(pallet_xcm::Call::execute { message, .. }) =
                extrinsic.clone()
            {
                if let xcm::VersionedXcm::V2(xcm::v2::Xcm(msg)) = *message {
                    for m in msg {
                        if matches!(m, xcm::opaque::v2::prelude::BuyExecution { fees: xcm::v2::MultiAsset { fun, .. }, .. } if fun == xcm::v2::Fungibility::Fungible(0))
                        {
                            continue 'extrinsics_loop;
                        }
                    }
                }
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
            let mut counted_free = 0;
            let mut counted_reserved = 0;
            let mut _counted_frozen = 0;

            for acc in frame_system::Account::<Runtime>::iter() {
                // Check that the consumer/provider state is valid.
                let acc_consumers = acc.1.consumers;
                let acc_providers = acc.1.providers;
                if acc_consumers > 0 && acc_providers == 0 {
                    panic!("Invalid state");
                }

                // Increment our balance counts
                counted_free += acc.1.data.free;
                counted_reserved += acc.1.data.reserved;
                _counted_frozen += acc.1.data.frozen;
            }
            let total_issuance = pallet_balances::TotalIssuance::<Runtime>::get();
            let counted_issuance = counted_free + counted_reserved;
            if total_issuance != counted_issuance {
                panic!(
                    "Inconsistent total issuance: {total_issuance} but counted {counted_issuance}"
                );
            }

            #[cfg(not(fuzzing))]
            println!("\nrunning integrity tests\n");
            // We run all developer-defined integrity tests
            <AllPalletsWithSystem as IntegrityTest>::integrity_test();
        });
    });
}
