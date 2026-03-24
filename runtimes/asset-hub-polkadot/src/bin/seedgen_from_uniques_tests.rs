#![warn(clippy::pedantic)]
#![allow(clippy::too_many_lines)]
use asset_hub_polkadot_runtime::RuntimeCall;
use codec::Encode;
use sp_runtime::MultiAddress;
use std::{fs, path::Path};

fn main() {
    let out_dir = Path::new("seedgen");
    fs::create_dir_all(out_dir).expect("create seedgen dir");

    let seeds = build_seeds();
    for (name, seed) in &seeds {
        let path = out_dir.join(format!("{name}.seed"));
        fs::write(&path, seed).expect("write seed file");
    }

    println!("wrote {} seeds to {}", seeds.len(), out_dir.display());
}

type AccountId = sp_runtime::AccountId32;

fn account(index: u8) -> AccountId {
    [index; 32].into()
}

fn lookup(index: u8) -> MultiAddress<AccountId, ()> {
    MultiAddress::Id(account(index))
}

fn encode_extrinsic(advance_block: bool, origin: u8, call: RuntimeCall) -> Vec<u8> {
    (advance_block, origin, call).encode()
}

fn build_seeds() -> Vec<(String, Vec<u8>)> {
    let mut seeds = Vec::new();

    // --- test: basic_minting_should_work ---
    // create collection
    {
        let mut data = Vec::new();
        // create(signed(0), collection=0, admin=account(0))
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        // mint(signed(0), collection=0, item=42, owner=account(0))
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 42,
                owner: lookup(0),
            }),
        ));
        // create(signed(1), collection=1, admin=account(1))
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 1,
                admin: lookup(1),
            }),
        ));
        // mint(signed(1), collection=1, item=69, owner=account(0))
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 1,
                item: 69,
                owner: lookup(0),
            }),
        ));
        seeds.push(("test_basic_minting_should_work".to_string(), data));
    }

    // --- test: lifecycle_should_work ---
    {
        let mut data = Vec::new();
        // create(signed(0), collection=0, admin=account(0))
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        // set_collection_metadata(signed(0), 0, [0,0], false)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_collection_metadata {
                collection: 0,
                data: vec![0, 0].try_into().unwrap(),
                is_frozen: false,
            }),
        ));
        // mint(signed(0), 0, 42, account(2))
        // Test uses account 10 -> map to account(2)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 42,
                owner: lookup(2),
            }),
        ));
        // mint(signed(0), 0, 69, account(3))
        // Test uses account 20 -> map to account(3)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 69,
                owner: lookup(3),
            }),
        ));
        // set_metadata(signed(0), 0, 42, [42,42], false)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_metadata {
                collection: 0,
                item: 42,
                data: vec![42, 42].try_into().unwrap(),
                is_frozen: false,
            }),
        ));
        // set_metadata(signed(0), 0, 69, [69,69], false)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_metadata {
                collection: 0,
                item: 69,
                data: vec![69, 69].try_into().unwrap(),
                is_frozen: false,
            }),
        ));
        // destroy(signed(0), 0, witness{items:2, item_metadatas:2, attributes:0})
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::destroy {
                collection: 0,
                witness: pallet_uniques::DestroyWitness {
                    items: 2,
                    item_metadatas: 2,
                    attributes: 0,
                },
            }),
        ));
        seeds.push(("test_lifecycle_should_work".to_string(), data));
    }

    // --- test: destroy_with_bad_witness_should_not_work ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 42,
                owner: lookup(0),
            }),
        ));
        // destroy with witness that doesn't account for the minted item
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::destroy {
                collection: 0,
                witness: pallet_uniques::DestroyWitness {
                    items: 0,
                    item_metadatas: 0,
                    attributes: 0,
                },
            }),
        ));
        seeds.push((
            "test_destroy_with_bad_witness_should_not_work".to_string(),
            data,
        ));
    }

    // --- test: transfer_should_work ---
    {
        let mut data = Vec::new();
        // create(signed(0), 0, admin=account(0))
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        // mint(signed(0), 0, 42, account(1))
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 42,
                owner: lookup(1),
            }),
        ));
        // transfer(signed(1), 0, 42, account(2))
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::transfer {
                collection: 0,
                item: 42,
                dest: lookup(2),
            }),
        ));
        // approve_transfer(signed(2), 0, 42, account(1))
        data.extend(encode_extrinsic(
            false,
            2,
            RuntimeCall::Uniques(pallet_uniques::Call::approve_transfer {
                collection: 0,
                item: 42,
                delegate: lookup(1),
            }),
        ));
        // transfer(signed(1), 0, 42, account(3))
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::transfer {
                collection: 0,
                item: 42,
                dest: lookup(3),
            }),
        ));
        seeds.push(("test_transfer_should_work".to_string(), data));
    }

    // --- test: freezing_should_work ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 42,
                owner: lookup(0),
            }),
        ));
        // freeze item
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::freeze {
                collection: 0,
                item: 42,
            }),
        ));
        // thaw item
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::thaw {
                collection: 0,
                item: 42,
            }),
        ));
        // freeze collection
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::freeze_collection { collection: 0 }),
        ));
        // thaw collection
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::thaw_collection { collection: 0 }),
        ));
        // transfer after thaw
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::transfer {
                collection: 0,
                item: 42,
                dest: lookup(1),
            }),
        ));
        seeds.push(("test_freezing_should_work".to_string(), data));
    }

    // --- test: origin_guards_should_work ---
    {
        let mut data = Vec::new();
        // create collection
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 42,
                owner: lookup(0),
            }),
        ));
        // set_accept_ownership(signed(1), Some(0))
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::set_accept_ownership {
                maybe_collection: Some(0),
            }),
        ));
        // transfer_ownership attempt (will fail - no permission)
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::transfer_ownership {
                collection: 0,
                new_owner: lookup(1),
            }),
        ));
        // set_team attempt (will fail - no permission)
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::set_team {
                collection: 0,
                issuer: lookup(1),
                admin: lookup(1),
                freezer: lookup(1),
            }),
        ));
        seeds.push(("test_origin_guards_should_work".to_string(), data));
    }

    // --- test: transfer_owner_should_work ---
    {
        let mut data = Vec::new();
        // create(signed(0), 0, account(0))
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        // set_accept_ownership(signed(1), Some(0))
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::set_accept_ownership {
                maybe_collection: Some(0),
            }),
        ));
        // transfer_ownership(signed(0), 0, account(1))
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::transfer_ownership {
                collection: 0,
                new_owner: lookup(1),
            }),
        ));
        // set_collection_metadata(signed(1), 0, [0;20], false)
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::set_collection_metadata {
                collection: 0,
                data: vec![0u8; 20].try_into().unwrap(),
                is_frozen: false,
            }),
        ));
        // mint(signed(0), 0, 42, account(0))
        // After ownership transfer, issuer is still account(0) since we didn't change it
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 42,
                owner: lookup(0),
            }),
        ));
        // set_metadata(signed(1), 0, 42, [0;20], false)
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::set_metadata {
                collection: 0,
                item: 42,
                data: vec![0u8; 20].try_into().unwrap(),
                is_frozen: false,
            }),
        ));
        // set_accept_ownership(signed(2), Some(0))
        data.extend(encode_extrinsic(
            false,
            2,
            RuntimeCall::Uniques(pallet_uniques::Call::set_accept_ownership {
                maybe_collection: Some(0),
            }),
        ));
        // transfer_ownership(signed(1), 0, account(2))
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::transfer_ownership {
                collection: 0,
                new_owner: lookup(2),
            }),
        ));
        seeds.push(("test_transfer_owner_should_work".to_string(), data));
    }

    // --- test: set_team_should_work ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        // set_team(signed(0), 0, issuer=1, admin=2, freezer=3)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_team {
                collection: 0,
                issuer: lookup(1),
                admin: lookup(2),
                freezer: lookup(3),
            }),
        ));
        // mint(signed(1), 0, 42, account(1)) — issuer mints
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 42,
                owner: lookup(1),
            }),
        ));
        // freeze(signed(3), 0, 42) — freezer freezes
        data.extend(encode_extrinsic(
            false,
            3,
            RuntimeCall::Uniques(pallet_uniques::Call::freeze {
                collection: 0,
                item: 42,
            }),
        ));
        // thaw(signed(2), 0, 42) — admin thaws
        data.extend(encode_extrinsic(
            false,
            2,
            RuntimeCall::Uniques(pallet_uniques::Call::thaw {
                collection: 0,
                item: 42,
            }),
        ));
        // transfer(signed(2), 0, 42, account(2)) — admin transfers
        data.extend(encode_extrinsic(
            false,
            2,
            RuntimeCall::Uniques(pallet_uniques::Call::transfer {
                collection: 0,
                item: 42,
                dest: lookup(2),
            }),
        ));
        // burn(signed(2), 0, 42, None) — admin burns
        data.extend(encode_extrinsic(
            false,
            2,
            RuntimeCall::Uniques(pallet_uniques::Call::burn {
                collection: 0,
                item: 42,
                check_owner: None,
            }),
        ));
        seeds.push(("test_set_team_should_work".to_string(), data));
    }

    // --- test: set_collection_metadata_should_work ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        // set_collection_metadata(signed(0), 0, [0;20], false)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_collection_metadata {
                collection: 0,
                data: vec![0u8; 20].try_into().unwrap(),
                is_frozen: false,
            }),
        ));
        // update metadata shorter
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_collection_metadata {
                collection: 0,
                data: vec![0u8; 15].try_into().unwrap(),
                is_frozen: false,
            }),
        ));
        // update metadata longer
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_collection_metadata {
                collection: 0,
                data: vec![0u8; 25].try_into().unwrap(),
                is_frozen: false,
            }),
        ));
        // freeze metadata
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_collection_metadata {
                collection: 0,
                data: vec![0u8; 15].try_into().unwrap(),
                is_frozen: true,
            }),
        ));
        // clear_collection_metadata (will fail - frozen)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::clear_collection_metadata { collection: 0 }),
        ));
        seeds.push((
            "test_set_collection_metadata_should_work".to_string(),
            data,
        ));
    }

    // --- test: set_item_metadata_should_work ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 42,
                owner: lookup(0),
            }),
        ));
        // set_metadata(signed(0), 0, 42, [0;20], false)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_metadata {
                collection: 0,
                item: 42,
                data: vec![0u8; 20].try_into().unwrap(),
                is_frozen: false,
            }),
        ));
        // update shorter
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_metadata {
                collection: 0,
                item: 42,
                data: vec![0u8; 15].try_into().unwrap(),
                is_frozen: false,
            }),
        ));
        // freeze
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_metadata {
                collection: 0,
                item: 42,
                data: vec![0u8; 15].try_into().unwrap(),
                is_frozen: true,
            }),
        ));
        // clear_metadata(signed(0), 0, 42) - will fail because frozen
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::clear_metadata {
                collection: 0,
                item: 42,
            }),
        ));
        seeds.push(("test_set_item_metadata_should_work".to_string(), data));
    }

    // --- test: set_attribute_should_work ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        // set_attribute(signed(0), 0, None, [0], [0])
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_attribute {
                collection: 0,
                maybe_item: None,
                key: vec![0].try_into().unwrap(),
                value: vec![0].try_into().unwrap(),
            }),
        ));
        // set_attribute(signed(0), 0, Some(0), [0], [0])
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_attribute {
                collection: 0,
                maybe_item: Some(0),
                key: vec![0].try_into().unwrap(),
                value: vec![0].try_into().unwrap(),
            }),
        ));
        // set_attribute(signed(0), 0, Some(0), [1], [0])
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_attribute {
                collection: 0,
                maybe_item: Some(0),
                key: vec![1].try_into().unwrap(),
                value: vec![0].try_into().unwrap(),
            }),
        ));
        // update attribute with longer value
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_attribute {
                collection: 0,
                maybe_item: None,
                key: vec![0].try_into().unwrap(),
                value: vec![0; 10].try_into().unwrap(),
            }),
        ));
        // clear_attribute(signed(0), 0, Some(0), [1])
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::clear_attribute {
                collection: 0,
                maybe_item: Some(0),
                key: vec![1].try_into().unwrap(),
            }),
        ));
        // destroy
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::destroy {
                collection: 0,
                witness: pallet_uniques::DestroyWitness {
                    items: 0,
                    item_metadatas: 0,
                    attributes: 2,
                },
            }),
        ));
        seeds.push(("test_set_attribute_should_work".to_string(), data));
    }

    // --- test: set_attribute_should_respect_freeze ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_attribute {
                collection: 0,
                maybe_item: None,
                key: vec![0].try_into().unwrap(),
                value: vec![0].try_into().unwrap(),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_attribute {
                collection: 0,
                maybe_item: Some(0),
                key: vec![0].try_into().unwrap(),
                value: vec![0].try_into().unwrap(),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_attribute {
                collection: 0,
                maybe_item: Some(1),
                key: vec![0].try_into().unwrap(),
                value: vec![0].try_into().unwrap(),
            }),
        ));
        // freeze collection metadata
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_collection_metadata {
                collection: 0,
                data: vec![].try_into().unwrap(),
                is_frozen: true,
            }),
        ));
        // set attribute on item (should still work since only collection-level is frozen)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_attribute {
                collection: 0,
                maybe_item: Some(0),
                key: vec![0].try_into().unwrap(),
                value: vec![1].try_into().unwrap(),
            }),
        ));
        // freeze item 0 metadata
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_metadata {
                collection: 0,
                item: 0,
                data: vec![].try_into().unwrap(),
                is_frozen: true,
            }),
        ));
        // set attribute on item 1 (should still work)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_attribute {
                collection: 0,
                maybe_item: Some(1),
                key: vec![0].try_into().unwrap(),
                value: vec![1].try_into().unwrap(),
            }),
        ));
        seeds.push((
            "test_set_attribute_should_respect_freeze".to_string(),
            data,
        ));
    }

    // --- test: burn_works ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        // set_team(signed(0), 0, issuer=1, admin=2, freezer=3)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_team {
                collection: 0,
                issuer: lookup(1),
                admin: lookup(2),
                freezer: lookup(3),
            }),
        ));
        // mint(signed(1), 0, 42, account(4)) — issuer mints to account 4
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 42,
                owner: lookup(4),
            }),
        ));
        // mint(signed(1), 0, 69, account(4))
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 69,
                owner: lookup(4),
            }),
        ));
        // burn(signed(4), 0, 42, Some(account(4))) — owner burns
        data.extend(encode_extrinsic(
            false,
            4,
            RuntimeCall::Uniques(pallet_uniques::Call::burn {
                collection: 0,
                item: 42,
                check_owner: Some(lookup(4)),
            }),
        ));
        // burn(signed(2), 0, 69, Some(account(4))) — admin burns
        data.extend(encode_extrinsic(
            false,
            2,
            RuntimeCall::Uniques(pallet_uniques::Call::burn {
                collection: 0,
                item: 69,
                check_owner: Some(lookup(4)),
            }),
        ));
        seeds.push(("test_burn_works".to_string(), data));
    }

    // --- test: approval_lifecycle_works ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 42,
                owner: lookup(1),
            }),
        ));
        // approve_transfer(signed(1), 0, 42, account(2))
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::approve_transfer {
                collection: 0,
                item: 42,
                delegate: lookup(2),
            }),
        ));
        // transfer(signed(2), 0, 42, account(3))
        data.extend(encode_extrinsic(
            false,
            2,
            RuntimeCall::Uniques(pallet_uniques::Call::transfer {
                collection: 0,
                item: 42,
                dest: lookup(3),
            }),
        ));
        // approve_transfer(signed(3), 0, 42, account(1))
        data.extend(encode_extrinsic(
            false,
            3,
            RuntimeCall::Uniques(pallet_uniques::Call::approve_transfer {
                collection: 0,
                item: 42,
                delegate: lookup(1),
            }),
        ));
        // transfer(signed(1), 0, 42, account(1))
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::transfer {
                collection: 0,
                item: 42,
                dest: lookup(1),
            }),
        ));
        seeds.push(("test_approval_lifecycle_works".to_string(), data));
    }

    // --- test: approved_account_gets_reset_after_transfer ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 42,
                owner: lookup(1),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::approve_transfer {
                collection: 0,
                item: 42,
                delegate: lookup(2),
            }),
        ));
        // owner transfers (resets approval)
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::transfer {
                collection: 0,
                item: 42,
                dest: lookup(4),
            }),
        ));
        // old delegate tries to transfer (should fail)
        data.extend(encode_extrinsic(
            false,
            2,
            RuntimeCall::Uniques(pallet_uniques::Call::transfer {
                collection: 0,
                item: 42,
                dest: lookup(3),
            }),
        ));
        // new owner transfers
        data.extend(encode_extrinsic(
            false,
            4,
            RuntimeCall::Uniques(pallet_uniques::Call::transfer {
                collection: 0,
                item: 42,
                dest: lookup(0),
            }),
        ));
        seeds.push((
            "test_approved_account_gets_reset_after_transfer".to_string(),
            data,
        ));
    }

    // --- test: approved_account_gets_reset_after_buy_item ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 1,
                owner: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::approve_transfer {
                collection: 0,
                item: 1,
                delegate: lookup(4),
            }),
        ));
        // set_price(signed(0), 0, 1, Some(15), None)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_price {
                collection: 0,
                item: 1,
                price: Some(15),
                whitelisted_buyer: None,
            }),
        ));
        // buy_item(signed(1), 0, 1, 15)
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::buy_item {
                collection: 0,
                item: 1,
                bid_price: 15,
            }),
        ));
        seeds.push((
            "test_approved_account_gets_reset_after_buy_item".to_string(),
            data,
        ));
    }

    // --- test: cancel_approval_works ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 42,
                owner: lookup(1),
            }),
        ));
        // approve_transfer(signed(1), 0, 42, account(2))
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::approve_transfer {
                collection: 0,
                item: 42,
                delegate: lookup(2),
            }),
        ));
        // cancel_approval(signed(1), 0, 42, Some(account(2)))
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::cancel_approval {
                collection: 0,
                item: 42,
                maybe_check_delegate: Some(lookup(2)),
            }),
        ));
        seeds.push(("test_cancel_approval_works".to_string(), data));
    }

    // --- test: cancel_approval_works_with_admin ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 42,
                owner: lookup(1),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::approve_transfer {
                collection: 0,
                item: 42,
                delegate: lookup(2),
            }),
        ));
        // admin cancels approval
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::cancel_approval {
                collection: 0,
                item: 42,
                maybe_check_delegate: Some(lookup(2)),
            }),
        ));
        seeds.push(("test_cancel_approval_works_with_admin".to_string(), data));
    }

    // --- test: max_supply_should_work ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        // set_collection_max_supply(signed(0), 0, 2)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_collection_max_supply {
                collection: 0,
                max_supply: 2,
            }),
        ));
        // mint two items
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 0,
                owner: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 1,
                owner: lookup(0),
            }),
        ));
        // try to mint a third (should fail - max supply reached)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 2,
                owner: lookup(0),
            }),
        ));
        // destroy
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::destroy {
                collection: 0,
                witness: pallet_uniques::DestroyWitness {
                    items: 2,
                    item_metadatas: 0,
                    attributes: 0,
                },
            }),
        ));
        seeds.push(("test_max_supply_should_work".to_string(), data));
    }

    // --- test: set_price_should_work ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 1,
                owner: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 2,
                owner: lookup(0),
            }),
        ));
        // set_price(signed(0), 0, 1, Some(1), None)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_price {
                collection: 0,
                item: 1,
                price: Some(1),
                whitelisted_buyer: None,
            }),
        ));
        // set_price(signed(0), 0, 2, Some(2), Some(account(2)))
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_price {
                collection: 0,
                item: 2,
                price: Some(2),
                whitelisted_buyer: Some(lookup(2)),
            }),
        ));
        // unset price
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_price {
                collection: 0,
                item: 2,
                price: None,
                whitelisted_buyer: None,
            }),
        ));
        seeds.push(("test_set_price_should_work".to_string(), data));
    }

    // --- test: buy_item_should_work ---
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        // mint 3 items to account(0)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 1,
                owner: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 2,
                owner: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 3,
                owner: lookup(0),
            }),
        ));
        // set_price item 1 = 20, no whitelisted buyer
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_price {
                collection: 0,
                item: 1,
                price: Some(20),
                whitelisted_buyer: None,
            }),
        ));
        // set_price item 2 = 30, whitelisted buyer = account(2)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_price {
                collection: 0,
                item: 2,
                price: Some(30),
                whitelisted_buyer: Some(lookup(2)),
            }),
        ));
        // buy item 1 with higher bid (account(1))
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::buy_item {
                collection: 0,
                item: 1,
                bid_price: 21,
            }),
        ));
        // whitelisted buyer buys item 2 (account(2))
        data.extend(encode_extrinsic(
            false,
            2,
            RuntimeCall::Uniques(pallet_uniques::Call::buy_item {
                collection: 0,
                item: 2,
                bid_price: 30,
            }),
        ));
        // set price on item 3 for freeze test
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_price {
                collection: 0,
                item: 3,
                price: Some(20),
                whitelisted_buyer: None,
            }),
        ));
        // freeze collection
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::freeze_collection { collection: 0 }),
        ));
        // buy_item should fail while frozen
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::buy_item {
                collection: 0,
                item: 3,
                bid_price: 20,
            }),
        ));
        // thaw collection
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::thaw_collection { collection: 0 }),
        ));
        // freeze specific item
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::freeze {
                collection: 0,
                item: 3,
            }),
        ));
        // buy_item should fail with frozen item
        data.extend(encode_extrinsic(
            false,
            1,
            RuntimeCall::Uniques(pallet_uniques::Call::buy_item {
                collection: 0,
                item: 3,
                bid_price: 20,
            }),
        ));
        seeds.push(("test_buy_item_should_work".to_string(), data));
    }

    // --- test: clear_collection_metadata_works ---
    {
        let mut data = Vec::new();
        // create(signed(0), 0, account(0))
        // Test uses account 123 as admin, map to account(2)
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(2),
            }),
        ));
        // set_collection_metadata
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::set_collection_metadata {
                collection: 0,
                data: vec![0, 0, 0, 0, 0, 0, 0, 0, 0].try_into().unwrap(),
                is_frozen: false,
            }),
        ));
        // clear_collection_metadata
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::clear_collection_metadata { collection: 0 }),
        ));
        // destroy
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::destroy {
                collection: 0,
                witness: pallet_uniques::DestroyWitness {
                    items: 0,
                    item_metadatas: 0,
                    attributes: 0,
                },
            }),
        ));
        seeds.push(("test_clear_collection_metadata_works".to_string(), data));
    }

    // --- test: redeposit ---
    // From force_item_status test, the redeposit call
    {
        let mut data = Vec::new();
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::create {
                collection: 0,
                admin: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 42,
                owner: lookup(0),
            }),
        ));
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::mint {
                collection: 0,
                item: 69,
                owner: lookup(1),
            }),
        ));
        // redeposit(signed(0), 0, [42, 69])
        data.extend(encode_extrinsic(
            false,
            0,
            RuntimeCall::Uniques(pallet_uniques::Call::redeposit {
                collection: 0,
                items: vec![42, 69],
            }),
        ));
        seeds.push(("test_redeposit".to_string(), data));
    }

    seeds
}
