#![warn(clippy::pedantic)]
use codec::{DecodeLimit, Encode};
use collectives_polkadot_runtime::{
    AllPalletsWithSystem, Balances, Executive, ParachainSystem, Runtime, RuntimeCall,
    RuntimeOrigin, Timestamp,
};
use cumulus_primitives_core::relay_chain::Header;
use frame_support::{
    dispatch::GetDispatchInfo,
    pallet_prelude::Weight,
    traits::{IntegrityTest, TryState, TryStateSelect},
    weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use frame_system::Account;
use pallet_balances::{Freezes, Holds, TotalIssuance};
use parachains_common::{AccountId, AuraId, Balance, SLOT_DURATION};
use sp_runtime::{
    testing::H256,
    traits::{Dispatchable, Header as _},
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
    use collectives_polkadot_runtime::{
        BalancesConfig, CollatorSelectionConfig, ParachainInfoConfig, PolkadotXcmConfig,
        RuntimeGenesisConfig, SessionConfig, SessionKeys, SystemConfig,
    };
    use sp_runtime::{app_crypto::ByteArray, BuildStorage};

    let initial_authorities: Vec<(AccountId, AuraId)> =
        vec![([0; 32].into(), AuraId::from_slice(&[0; 32]).unwrap())];

    RuntimeGenesisConfig {
        system: SystemConfig::default(),
        balances: BalancesConfig {
            balances: accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
            dev_accounts: None,
        },
        parachain_info: ParachainInfoConfig::default(),
        collator_selection: CollatorSelectionConfig {
            invulnerables: initial_authorities.iter().map(|x| x.0.clone()).collect(),
            candidacy_bond: 1 << 57,
            desired_candidates: 1,
        },
        session: SessionConfig {
            keys: initial_authorities
                .iter()
                .map(|x| (x.0.clone(), x.0.clone(), SessionKeys { aura: x.1.clone() }))
                .collect::<Vec<_>>(),
            non_authority_keys: vec![],
        },
        polkadot_xcm: PolkadotXcmConfig::default(),
        ..Default::default()
    }
    .build_storage()
    .unwrap()
}

fn recursively_find_call(call: &RuntimeCall, matches_on: fn(&RuntimeCall) -> bool) -> bool {
    if let RuntimeCall::Utility(
        pallet_utility::Call::batch { calls }
        | pallet_utility::Call::force_batch { calls }
        | pallet_utility::Call::batch_all { calls },
    ) = call
    {
        for nested in calls {
            if recursively_find_call(nested, matches_on) {
                return true;
            }
        }
    } else if let RuntimeCall::Utility(pallet_utility::Call::if_else { main, fallback }) = call {
        return recursively_find_call(main, matches_on)
            || recursively_find_call(fallback, matches_on);
    } else if let RuntimeCall::Multisig(pallet_multisig::Call::as_multi_threshold_1 {
        call, ..
    })
    | RuntimeCall::Utility(
        pallet_utility::Call::as_derivative { call, .. }
        | pallet_utility::Call::with_weight { call, .. }
        | pallet_utility::Call::dispatch_as { call, .. }
        | pallet_utility::Call::dispatch_as_fallible { call, .. },
    )
    | RuntimeCall::Proxy(
        pallet_proxy::Call::proxy { call, .. } | pallet_proxy::Call::proxy_announced { call, .. },
    ) = call
    {
        return recursively_find_call(call, matches_on);
    } else if matches_on(call) {
        return true;
    }
    false
}

fn call_filter(_: &RuntimeCall) -> bool {
    false
}

fn process_input(accounts: &[AccountId], genesis: &Storage, data: &[u8]) {
    let mut extrinsic_data = data;

    let extrinsics: Vec<(u8, u8, RuntimeCall)> =
        iter::from_fn(|| DecodeLimit::decode_with_depth_limit(64, &mut extrinsic_data).ok())
            .filter(|(_, _, x): &(_, _, RuntimeCall)| !recursively_find_call(x, call_filter))
            .collect();

    if extrinsics.is_empty() {
        return;
    }

    BasicExternalities::execute_with_storage(&mut genesis.clone(), || {
        let mut block: u32 = 1;
        let mut weight: Weight = Weight::zero();
        let mut elapsed: Duration = Duration::ZERO;
        let initial_total_issuance = TotalIssuance::<Runtime>::get();

        initialize_block(block, None);

        for (lapse, origin, extrinsic) in extrinsics {
            if lapse > 0 {
                let prev_header = finalize_block(elapsed);

                block += u32::from(lapse);
                weight = Weight::zero();
                elapsed = Duration::ZERO;

                initialize_block(block, Some(&prev_header));
            }

            let origin = accounts[origin as usize % accounts.len()].clone();

            #[cfg(not(feature = "fuzzing"))]
            println!("\n    origin:     {origin:?}");
            #[cfg(not(feature = "fuzzing"))]
            println!("    call:       {extrinsic:?}");

            let pre_weight = extrinsic.get_dispatch_info().call_weight;
            let cumulative_weight = weight.saturating_add(pre_weight);
            if cumulative_weight.ref_time() >= 2 * WEIGHT_REF_TIME_PER_SECOND {
                #[cfg(not(feature = "fuzzing"))]
                println!("Extrinsic would exhaust block weight, skipping");
                continue;
            }
            weight = cumulative_weight;

            let now = Instant::now();
            let res = extrinsic.dispatch(RuntimeOrigin::signed(origin));
            elapsed += now.elapsed();

            #[cfg(not(feature = "fuzzing"))]
            println!("    result:     {res:?}");

            let actual_weight = res.unwrap_or_else(|e| e.post_info).actual_weight;
            let post_weight = actual_weight.unwrap_or_default();
            assert!(pre_weight.ref_time().saturating_mul(2) >= post_weight.ref_time(), "Pre-dispatch weight ref time ({}) is smaller than post-dispatch weight ref time ({})", pre_weight.ref_time(), post_weight.ref_time());
            assert!(pre_weight.proof_size().saturating_mul(2) >= post_weight.proof_size(), "Pre-dispatch weight proof size ({}) is smaller than post-dispatch weight proof size ({})", pre_weight.proof_size(), post_weight.proof_size());
        }

        finalize_block(elapsed);

        check_invariants(block, initial_total_issuance);
    });
}

fn initialize_block(block: u32, prev_header: Option<&Header>) {
    #[cfg(not(feature = "fuzzing"))]
    println!("\ninitializing block {block}");

    let pre_digest = Digest {
        logs: vec![DigestItem::PreRuntime(
            sp_consensus_aura::AURA_ENGINE_ID,
            sp_consensus_aura::Slot::from(u64::from(block)).encode(),
        )],
    };
    let parent_header = &Header::new(
        block,
        H256::default(),
        H256::default(),
        prev_header.map(Header::hash).unwrap_or_default(),
        pre_digest,
    );
    Executive::initialize_block(parent_header);

    #[cfg(not(feature = "fuzzing"))]
    println!("  setting timestamp");
    Timestamp::set(RuntimeOrigin::none(), u64::from(block) * SLOT_DURATION).unwrap();

    #[cfg(not(feature = "fuzzing"))]
    println!("  setting parachain validation data");
    let parachain_validation_data = {
        use cumulus_pallet_parachain_system::parachain_inherent::BasicParachainInherentData;
        use cumulus_primitives_core::relay_chain::HeadData;
        use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;

        let parent_head = HeadData(prev_header.unwrap_or(parent_header).encode());
        let sproof_builder = RelayStateSproofBuilder {
            para_id: 100.into(),
            current_slot: cumulus_primitives_core::relay_chain::Slot::from(2 * u64::from(block)),
            included_para_head: Some(parent_head.clone()),
            ..Default::default()
        };

        let relay_parent_offset = 1;
        let (relay_parent_storage_root, relay_chain_state, relay_parent_descendants) =
            sproof_builder.into_state_root_proof_and_descendants(relay_parent_offset);
        BasicParachainInherentData {
            validation_data: polkadot_primitives::PersistedValidationData {
                parent_head,
                relay_parent_number: block,
                relay_parent_storage_root,
                max_pov_size: 1000,
            },
            relay_chain_state,
            collator_peer_id: None,
            relay_parent_descendants,
        }
    };
    let inbound_message_data = {
        use cumulus_pallet_parachain_system::parachain_inherent::{
            AbridgedInboundMessagesCollection, InboundMessagesData,
        };
        InboundMessagesData::new(
            AbridgedInboundMessagesCollection::default(),
            AbridgedInboundMessagesCollection::default(),
        )
    };
    ParachainSystem::set_validation_data(
        RuntimeOrigin::none(),
        parachain_validation_data,
        inbound_message_data,
    )
    .unwrap();
}

fn finalize_block(elapsed: Duration) -> Header {
    #[cfg(not(feature = "fuzzing"))]
    println!("\n  time spent: {elapsed:?}");
    assert!(elapsed.as_secs() <= 1, "block execution took too much time");

    #[cfg(not(feature = "fuzzing"))]
    println!("finalizing block");
    Executive::finalize_block()
}

fn check_invariants(block: u32, initial_total_issuance: Balance) {
    let ed = <Runtime as pallet_balances::Config>::ExistentialDeposit::get();
    let mut counted_free = 0;
    let mut counted_reserved = 0;
    for (account, info) in Account::<Runtime>::iter() {
        let consumers = info.consumers;
        let providers = info.providers;
        assert!(!(consumers > 0 && providers == 0), "Invalid c/p state");
        counted_free += info.data.free;
        counted_reserved += info.data.reserved;

        let total_balance = info.data.free + info.data.reserved;
        assert!(
            total_balance == 0 || total_balance >= ed,
            "Account {account:?} has total balance ({total_balance}) below existential deposit ({ed}) but non-zero"
        );

        assert!(
            info.data.free + info.data.reserved >= info.data.frozen,
            "Account {account:?} has frozen balance ({}) exceeding total balance ({})",
            info.data.frozen,
            info.data.free + info.data.reserved
        );

        let max_lock: Balance = Balances::locks(&account)
            .iter()
            .map(|l| l.amount)
            .max()
            .unwrap_or_default();
        let freezes = Freezes::<Runtime>::get(&account);
        let max_freeze = freezes
            .iter()
            .map(|freeze| freeze.amount)
            .max()
            .unwrap_or(0);
        assert_eq!(
            info.data.frozen,
            max_lock.max(max_freeze),
            "Frozen balance should be the max of the max lock and max freeze"
        );

        for (i, freeze) in freezes.iter().enumerate() {
            assert!(
                freeze.amount > 0,
                "Account {account:?} has a freeze with zero amount"
            );
            for other in freezes.iter().skip(i + 1) {
                assert!(
                    freeze.id != other.id,
                    "Account {account:?} has duplicate freeze IDs"
                );
            }
        }

        let holds = Holds::<Runtime>::get(&account);
        let sum_holds: Balance = holds.iter().map(|l| l.amount).sum();
        assert!(
            sum_holds <= info.data.reserved,
            "Sum of all holds ({sum_holds}) should be less than or equal to reserved balance {}",
            info.data.reserved
        );

        for (i, hold) in holds.iter().enumerate() {
            assert!(
                hold.amount > 0,
                "Account {account:?} has a hold with zero amount"
            );
            for other in holds.iter().skip(i + 1) {
                assert!(
                    hold.id != other.id,
                    "Account {account:?} has duplicate hold IDs"
                );
            }
        }
    }
    let total_issuance = TotalIssuance::<Runtime>::get();
    let counted_issuance = counted_free + counted_reserved;
    assert_eq!(total_issuance, counted_issuance);
    assert!(total_issuance <= initial_total_issuance);

    AllPalletsWithSystem::integrity_test();
    AllPalletsWithSystem::try_state(block, TryStateSelect::All).unwrap();
}
