use codec::{DecodeLimit, Encode};
use frame_support::{
    dispatch::GetDispatchInfo,
    pallet_prelude::Weight,
    traits::{IntegrityTest, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use kusama_runtime_constants::{currency::UNITS, time::SLOT_DURATION};
use pallet_grandpa::AuthorityId as GrandpaId;
use pallet_staking::StakerStatus;
use polkadot_primitives::{AccountId, Balance, BlockNumber};
use polkadot_primitives::{AssignmentId, ValidatorId};
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_consensus_babe::{
    digests::{PreDigest, SecondaryPlainPreDigest},
    Slot, BABE_ENGINE_ID,
};
use sp_runtime::{app_crypto::ByteArray, BuildStorage, Perbill};
use sp_runtime::{
    traits::{Dispatchable, Header},
    Digest, DigestItem, Storage,
};
use sp_state_machine::BasicExternalities;
use staging_kusama_runtime::{
    AllPalletsWithSystem, Executive, Identity, ParaInherent, Runtime, RuntimeCall, RuntimeOrigin,
    Timestamp,
};
use std::time::{Duration, Instant};
type BeefyId = sp_consensus_beefy::ecdsa_crypto::AuthorityId;

struct Authority {
    account: AccountId,
    grandpa: GrandpaId,
    babe: BabeId,
    beefy: BeefyId,
    validator: ValidatorId,
    assignment: AssignmentId,
    authority_discovery: AuthorityDiscoveryId,
}

fn genesis(accounts: &[AccountId]) -> Storage {
    use staging_kusama_runtime as kusama;

    let initial_authorities: Vec<Authority> = vec![Authority {
        account: [0; 32].into(),
        grandpa: GrandpaId::from_slice(&[0; 32]).unwrap(),
        babe: BabeId::from_slice(&[0; 32]).unwrap(),
        beefy: sp_application_crypto::ecdsa::Public::from_raw([0u8; 33]).into(),
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

    let _num_endowed_accounts = accounts.len();

    const ENDOWMENT: Balance = 10_000_000 * UNITS;
    const STASH: Balance = ENDOWMENT / 1000;

    let storage = kusama::RuntimeGenesisConfig {
        system: Default::default(),
        balances: kusama::BalancesConfig {
            // Configure endowed accounts with initial balance of 1 << 60.
            balances: accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
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
                            beefy: x.beefy.clone(),
                            para_validator: x.validator.clone(),
                            para_assignment: x.assignment.clone(),
                            authority_discovery: x.authority_discovery.clone(),
                        },
                    )
                })
                .collect::<Vec<_>>(),
        },
        beefy: Default::default(),
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
            ..Default::default()
        },
        grandpa: Default::default(),
        authority_discovery: Default::default(),
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
        nomination_pools: kusama::NominationPoolsConfig {
            min_create_bond: 1 << 43,
            min_join_bond: 1 << 42,
            ..Default::default()
        },
        nis_counterpart_balances: Default::default(),
        registrar: Default::default(),
        society: Default::default(),
        transaction_payment: Default::default(),
    }
    .build_storage()
    .unwrap();
    let mut chain = BasicExternalities::new(storage);
    chain.execute_with(|| {
        Identity::add_registrar(RuntimeOrigin::root(), accounts[0].clone().into()).unwrap();
    });
    chain.into_storages()
}

fn recursively_find_call(call: RuntimeCall, matches_on: fn(RuntimeCall) -> bool) -> bool {
    if let RuntimeCall::Utility(
        pallet_utility::Call::batch { calls }
        | pallet_utility::Call::force_batch { calls }
        | pallet_utility::Call::batch_all { calls },
    ) = call
    {
        for call in calls {
            if recursively_find_call(call.clone(), matches_on) {
                return true;
            }
        }
    } else if let RuntimeCall::Multisig(pallet_multisig::Call::as_multi_threshold_1 {
        call, ..
    })
    | RuntimeCall::Utility(pallet_utility::Call::as_derivative { call, .. })
    | RuntimeCall::Proxy(pallet_proxy::Call::proxy { call, .. }) = call
    {
        return recursively_find_call(*call.clone(), matches_on);
    } else if matches_on(call) {
        return true;
    }
    false
}

fn start_block(block: u32) {
    #[cfg(not(fuzzing))]
    println!("\ninitializing block {block}");

    let pre_digest = Digest {
        logs: vec![DigestItem::PreRuntime(
            BABE_ENGINE_ID,
            PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
                slot: Slot::from(block as u64),
                authority_index: 0,
            })
            .encode(),
        )],
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
    Timestamp::set(RuntimeOrigin::none(), block as u64 * SLOT_DURATION).unwrap();

    #[cfg(not(fuzzing))]
    println!("  setting bitfields");
    ParaInherent::enter(
        RuntimeOrigin::none(),
        polkadot_primitives::InherentData {
            parent_header: grandparent_header,
            bitfields: Default::default(),
            backed_candidates: Default::default(),
            disputes: Default::default(),
        },
    )
    .unwrap();
}

fn end_block(elapsed: Duration) {
    #[cfg(not(fuzzing))]
    println!("\n  time spent: {elapsed:?}");
    assert!(elapsed.as_secs() <= 2, "block execution took too much time");

    #[cfg(not(fuzzing))]
    println!("  finalizing block");
    Executive::finalize_block();
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
        .filter(|(_, _, x): &(_, _, RuntimeCall)| {
            !recursively_find_call(x.clone(), |call| {
                // We filter out calls with Fungible(0) as they cause a debug crash
                matches!(call.clone(), RuntimeCall::XcmPallet(pallet_xcm::Call::execute { message, .. })
                    if matches!(message.as_ref(), staging_xcm::VersionedXcm::V2(staging_xcm::v2::Xcm(msg))
                        if msg.iter().any(|m| matches!(m, staging_xcm::opaque::v2::prelude::BuyExecution { fees: staging_xcm::v2::MultiAsset { fun, .. }, .. }
                            if fun == &staging_xcm::v2::Fungibility::Fungible(0)
                        ))
                    )
                )
                || matches!(call.clone(), RuntimeCall::System(_))
            })
        })
        .collect();
        if extrinsics.is_empty() {
            return;
        }

        // `chain` represents the state of our mock chain.
        let mut chain = BasicExternalities::new(genesis_storage.clone());

        let mut current_block: u32 = 1;
        let mut current_weight: Weight = Weight::zero();
        let mut elapsed: Duration = Duration::ZERO;

        let mut initial_total_issuance = 0;

        chain.execute_with(|| {
            initial_total_issuance = pallet_balances::TotalIssuance::<Runtime>::get();

            start_block(current_block);

            for (lapse, origin, extrinsic) in extrinsics {
                if lapse > 0 {
                    // We end the current block
                    end_block(elapsed);

                    // 393 * 256 = 100608 which nearly corresponds to a week
                    let actual_lapse = u32::from(lapse) * 393;
                    // We update our state variables
                    current_block += actual_lapse;
                    // current_timestamp += u64::from(lapse) * SLOT_DURATION;
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

                let origin = if matches!(
                    extrinsic,
                    RuntimeCall::Bounties(pallet_bounties::Call::approve_bounty { .. })
                        | RuntimeCall::Bounties(pallet_bounties::Call::propose_curator { .. })
                        | RuntimeCall::Bounties(pallet_bounties::Call::close_bounty { .. })
                ) {
                    RuntimeOrigin::root()
                } else {
                    RuntimeOrigin::signed(endowed_accounts[origin as usize % endowed_accounts.len()].clone())
                };

                #[cfg(not(fuzzing))]
                {
                    println!("\n    origin:     {origin:?}");
                    println!("    call:       {extrinsic:?}");
                }

                let _res = extrinsic.clone().dispatch(origin);
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
                let max_lock: Balance = staging_kusama_runtime::Balances::locks(&acc.0).iter().map(|l| l.amount).max().unwrap_or_default();
                assert!(max_lock <= acc.1.data.frozen, "Max lock ({max_lock}) should be less than or equal to frozen balance ({})", acc.1.data.frozen);
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
            assert!(
                total_issuance <= initial_total_issuance,
                "Inconsistent total issuance: {total_issuance} but initial {initial_total_issuance}"
            );
            #[cfg(not(fuzzing))]
            println!("\nrunning integrity tests\n");
            // We run all developer-defined integrity tests
            AllPalletsWithSystem::integrity_test();
            #[cfg(not(fuzzing))]
            println!("running try_state for block {current_block}\n");
            AllPalletsWithSystem::try_state(current_block, TryStateSelect::All).unwrap();
        });
    });
}
