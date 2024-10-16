#![warn(clippy::pedantic)]
use codec::{DecodeLimit, Encode};
use frame_support::{
    dispatch::GetDispatchInfo,
    pallet_prelude::Weight,
    traits::{IntegrityTest, OriginTrait, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use frame_system::Account;
use kusama_runtime_constants::{currency::UNITS, time::SLOT_DURATION};
use pallet_balances::{Holds, TotalIssuance};
use pallet_grandpa::AuthorityId as GrandpaId;
use pallet_staking::StakerStatus;
use polkadot_primitives::{AccountId, AssignmentId, Balance, Header, ValidatorId};
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_consensus_babe::{
    digests::{PreDigest, SecondaryPlainPreDigest},
    Slot, BABE_ENGINE_ID,
};
use sp_runtime::{app_crypto::ByteArray, BuildStorage, Perbill};
use sp_runtime::{
    testing::H256,
    traits::{Dispatchable, Header as _},
    Digest, DigestItem, Storage,
};
use sp_state_machine::BasicExternalities;
use staging_kusama_runtime::{
    AllPalletsWithSystem, Balances, Executive, ParaInherent, Runtime, RuntimeCall, RuntimeOrigin,
    Timestamp,
};
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
    use staging_kusama_runtime as kusama;

    const ENDOWMENT: Balance = 10_000_000 * UNITS;
    const STASH: Balance = ENDOWMENT / 1000;

    let initial_authority = kusama::SessionKeys {
        grandpa: GrandpaId::from_slice(&[0; 32]).unwrap(),
        babe: BabeId::from_slice(&[0; 32]).unwrap(),
        beefy: sp_application_crypto::ecdsa::Public::from_raw([0u8; 33]).into(),
        para_validator: ValidatorId::from_slice(&[0; 32]).unwrap(),
        para_assignment: AssignmentId::from_slice(&[0; 32]).unwrap(),
        authority_discovery: AuthorityDiscoveryId::from_slice(&[0; 32]).unwrap(),
    };

    let stakers = vec![(
        [0; 32].into(),
        [0; 32].into(),
        STASH,
        StakerStatus::Validator,
    )];

    let mut storage = kusama::RuntimeGenesisConfig {
        system: kusama::SystemConfig::default(),
        balances: kusama::BalancesConfig {
            // Configure endowed accounts with initial balance of 1 << 60.
            balances: accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
        },
        indices: kusama::IndicesConfig { indices: vec![] },
        session: kusama::SessionConfig {
            keys: vec![([0; 32].into(), [0; 32].into(), initial_authority)],
        },
        beefy: kusama::BeefyConfig::default(),
        staking: kusama::StakingConfig {
            validator_count: 1,
            minimum_validator_count: 1,
            invulnerables: vec![[0; 32].into()],
            slash_reward_fraction: Perbill::from_percent(10),
            stakers,
            ..Default::default()
        },
        babe: kusama::BabeConfig {
            epoch_config: kusama::BABE_GENESIS_EPOCH_CONFIG,
            ..Default::default()
        },
        grandpa: kusama::GrandpaConfig::default(),
        authority_discovery: kusama::AuthorityDiscoveryConfig::default(),
        claims: kusama::ClaimsConfig {
            claims: vec![],
            vesting: vec![],
        },
        vesting: kusama::VestingConfig { vesting: vec![] },
        treasury: kusama::TreasuryConfig::default(),
        hrmp: kusama::HrmpConfig::default(),
        configuration: kusama::ConfigurationConfig::default(),
        paras: kusama::ParasConfig::default(),
        xcm_pallet: kusama::XcmPalletConfig::default(),
        nomination_pools: kusama::NominationPoolsConfig {
            min_create_bond: 1 << 43,
            min_join_bond: 1 << 42,
            ..Default::default()
        },
        nis_counterpart_balances: kusama::NisCounterpartBalancesConfig::default(),
        registrar: kusama::RegistrarConfig::default(),
        society: kusama::SocietyConfig::default(),
        transaction_payment: kusama::TransactionPaymentConfig::default(),
    }
    .build_storage()
    .unwrap();
    BasicExternalities::execute_with_storage(&mut storage, || {
        // Identity::add_registrar(RuntimeOrigin::root(), accounts[0].clone().into()).unwrap();
    });
    storage
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

fn process_input(accounts: &[AccountId], genesis: &Storage, data: &[u8]) {
    let mut extrinsic_data = data;
    // We build the list of extrinsics we will execute
    #[allow(deprecated)]
    let extrinsics: Vec<(/* lapse */ u8, /* origin */ u8, RuntimeCall)> = iter::from_fn(|| {
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
                || matches!(call.clone(), RuntimeCall::XcmPallet(pallet_xcm::Call::transfer_assets_using_type_and_then { assets, ..})
                    if staging_xcm::v2::MultiAssets::try_from(*assets.clone())
                        .map(|assets| assets.inner().iter().any(|a| matches!(a, staging_xcm::v2::MultiAsset { fun, .. }
                            if fun == &staging_xcm::v2::Fungibility::Fungible(0)
                        ))).unwrap_or(false)
                )
                || matches!(call.clone(), RuntimeCall::System(_))
                || matches!(
                    &call,
                    RuntimeCall::Referenda(pallet_referenda::Call::submit {
                        proposal_origin: matching_origin,
                        ..
                    }) if RuntimeOrigin::from(*matching_origin.clone()).caller() == RuntimeOrigin::root().caller()
                )
            })
        })
        .collect();
    if extrinsics.is_empty() {
        return;
    }

    let mut block: u32 = 1;
    let mut weight: Weight = Weight::zero();
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

            weight.saturating_accrue(extrinsic.get_dispatch_info().weight);
            if weight.ref_time() >= 2 * WEIGHT_REF_TIME_PER_SECOND {
                #[cfg(not(feature = "fuzzing"))]
                println!("Extrinsic would exhaust block weight, skipping");
                continue;
            }

            let origin = if matches!(
                extrinsic,
                RuntimeCall::Bounties(
                    pallet_bounties::Call::approve_bounty { .. }
                        | pallet_bounties::Call::propose_curator { .. }
                        | pallet_bounties::Call::close_bounty { .. }
                )
            ) {
                RuntimeOrigin::root()
            } else {
                RuntimeOrigin::signed(accounts[origin as usize % accounts.len()].clone())
            };

            #[cfg(not(feature = "fuzzing"))]
            println!("\n    origin:     {origin:?}");
            #[cfg(not(feature = "fuzzing"))]
            println!("    call:       {extrinsic:?}");

            let now = Instant::now(); // We get the current time for timing purposes.
            #[allow(unused_variables)]
            let res = extrinsic.clone().dispatch(origin);
            elapsed += now.elapsed();

            #[cfg(not(feature = "fuzzing"))]
            println!("    result:     {res:?}");
        }

        finalize_block(elapsed);

        check_invariants(block, initial_total_issuance);
    });
}

fn initialize_block(block: u32) {
    #[cfg(not(feature = "fuzzing"))]
    println!("\ninitializing block {block}");

    let pre_digest = Digest {
        logs: vec![DigestItem::PreRuntime(
            BABE_ENGINE_ID,
            PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
                slot: Slot::from(u64::from(block)),
                authority_index: 0,
            })
            .encode(),
        )],
    };

    let grandparent_header = Header::new(
        block,
        H256::default(),
        H256::default(),
        <frame_system::Pallet<Runtime>>::parent_hash(),
        pre_digest.clone(),
    );

    let parent_header = Header::new(
        block,
        H256::default(),
        H256::default(),
        grandparent_header.hash(),
        pre_digest,
    );

    Executive::initialize_block(&parent_header);

    #[cfg(not(feature = "fuzzing"))]
    println!("  setting timestamp");
    Timestamp::set(RuntimeOrigin::none(), u64::from(block) * SLOT_DURATION).unwrap();

    #[cfg(not(feature = "fuzzing"))]
    println!("  setting bitfields");
    ParaInherent::enter(
        RuntimeOrigin::none(),
        polkadot_primitives::InherentData {
            parent_header: grandparent_header,
            backed_candidates: Vec::default(),
            bitfields: Vec::default(),
            disputes: Vec::default(),
        },
    )
    .unwrap();
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
    // After execution of all blocks, we run invariants
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
        assert!(
            max_lock <= info.data.frozen,
            "Max lock ({max_lock}) should be less than or equal to frozen balance ({})",
            info.data.frozen
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
    assert!(
        total_issuance == counted_issuance,
        "Inconsistent total issuance: {total_issuance} but counted {counted_issuance}"
    );
    assert!(
        total_issuance <= initial_total_issuance,
        "Total issuance {total_issuance} greater than initial issuance {initial_total_issuance}"
    );
    // We run all developer-defined integrity tests
    AllPalletsWithSystem::integrity_test();
    AllPalletsWithSystem::try_state(block, TryStateSelect::All).unwrap();
}
