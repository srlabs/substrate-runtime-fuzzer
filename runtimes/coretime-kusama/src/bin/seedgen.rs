#![warn(clippy::pedantic)]
use coretime_kusama_runtime::{Runtime, RuntimeCall};
use codec::{Decode, Encode};
use frame_metadata::{RuntimeMetadata, RuntimeMetadataPrefixed};
use frame_support::traits::GetCallMetadata;
use scale_info::TypeDef;
use sp_state_machine::BasicExternalities;
use std::{collections::BTreeMap, fs, path::Path};

fn main() {
    let seeds = build_seeds();

    let out_dir = Path::new("seedgen");
    fs::create_dir_all(out_dir).expect("create seedgen dir");
    for (name, seed) in &seeds {
        let path = out_dir.join(format!("{name}.seed"));
        fs::write(&path, seed).expect("write seed file");
    }

    println!("wrote {} seeds to {}", seeds.len(), out_dir.display());
}

fn build_seeds() -> Vec<(String, Vec<u8>)> {
    let mut seeds: Vec<(String, Vec<u8>)> = Vec::new();

    let metadata = load_metadata_v14();
    let call_map = build_call_map(&metadata);
    for (pallet, calls) in call_map {
        let expected =
            pallet_call_variant_count(&metadata, pallet.as_str()).expect("pallet call count");
        let actual = calls.len();
        println!("created {actual}/{expected} seeds for {pallet}");
        if actual != expected {
            println!(
                "  skipped {} calls in {pallet} (unencodable with only zeroes)",
                expected - actual
            );
        }

        for call in calls {
            let meta = call.get_call_metadata();
            let name = format!(
                "{}_{}",
                meta.pallet_name.to_ascii_lowercase(),
                meta.function_name
            );
            seeds.push((name, (0u8, 0u8, call).encode()));
        }
    }

    seeds
}

fn build_call_map(
    metadata: &frame_metadata::v14::RuntimeMetadataV14,
) -> BTreeMap<String, Vec<RuntimeCall>> {
    let mut map: BTreeMap<String, Vec<RuntimeCall>> = BTreeMap::new();
    for pallet in &metadata.pallets {
        let calls = pallet.calls.as_ref();
        if calls.is_none() {
            continue;
        }
        let calls = calls.expect("checked");
        let mut runtime_calls = Vec::new();
        let call_def = metadata
            .types
            .resolve(calls.ty.id)
            .expect("resolve pallet call type");
        let TypeDef::Variant(variants) = &call_def.type_def else {
            continue;
        };
        for variant in &variants.variants {
            if variant.name == "__Ignore" {
                continue;
            }
            if let Some(call) = runtime_call_from_metadata(metadata, pallet, variant) {
                runtime_calls.push(call);
            }
        }
        map.insert(pallet.name.clone(), runtime_calls);
    }
    map
}

fn load_metadata_v14() -> frame_metadata::v14::RuntimeMetadataV14 {
    let bytes: Vec<u8> = BasicExternalities::default().execute_with(|| Runtime::metadata().into());
    let metadata = RuntimeMetadataPrefixed::decode(&mut &bytes[..]).expect("decode metadata");
    match metadata.1 {
        RuntimeMetadata::V14(m) => m,
        _ => panic!("unsupported metadata version"),
    }
}

fn runtime_call_from_metadata(
    metadata: &frame_metadata::v14::RuntimeMetadataV14,
    pallet: &frame_metadata::v14::PalletMetadata<scale_info::form::PortableForm>,
    variant: &scale_info::Variant<scale_info::form::PortableForm>,
) -> Option<RuntimeCall> {
    let calls = pallet.calls.as_ref()?;
    let call_def = metadata.types.resolve(calls.ty.id)?;
    let TypeDef::Variant(_) = &call_def.type_def else {
        return None;
    };
    let mut bytes = pallet.index.encode();
    bytes.extend_from_slice(&variant.index.encode());
    bytes.extend_from_slice(&[0u8; 1024]);
    RuntimeCall::decode(&mut &bytes[..]).ok()
}

fn pallet_call_variant_count(
    metadata: &frame_metadata::v14::RuntimeMetadataV14,
    pallet_name: &str,
) -> Option<usize> {
    let pallet = metadata.pallets.iter().find(|p| p.name == pallet_name)?;
    let calls = pallet.calls.as_ref()?;
    let ty = metadata.types.resolve(calls.ty.id)?;
    let TypeDef::Variant(variants) = &ty.type_def else {
        return None;
    };
    Some(
        variants
            .variants
            .iter()
            .filter(|variant| variant.name != "__Ignore")
            .count(),
    )
}
