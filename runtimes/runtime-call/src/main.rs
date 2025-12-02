use codec::{Decode, Encode, Error};
use sp_state_machine::BasicExternalities as Externalities;

fn fuzz_runtime<RuntimeCall>(name: &str, blocklist: &[[u8; 2]], data: &[u8])
where
    RuntimeCall: Decode + Encode + core::fmt::Debug,
{
    if blocklist.iter().any(|b| data.windows(2).any(|w| w == b)) {
        return;
    }

    // Clone data so each runtime gets an untouched view.
    let buffer = data.to_vec();
    let mut extrinsic_data: &[u8] = &buffer;

    let maybe_extrinsic: Result<RuntimeCall, Error> = Decode::decode(&mut extrinsic_data);
    if let Ok(extrinsic) = maybe_extrinsic {
        #[cfg(not(feature = "fuzzing"))]
        println!("Fuzzing {name}");
        #[cfg(not(feature = "fuzzing"))]
        println!("Found extrinsic: {extrinsic:?}");

        let consumed = &buffer[..buffer.len() - extrinsic_data.len()];
        let re_encoding = Encode::encode(&extrinsic);
        if consumed != re_encoding {
            #[cfg(not(feature = "fuzzing"))]
            println!("Original  : 0x{}", hex::encode(consumed));
            #[cfg(not(feature = "fuzzing"))]
            println!("Reencoding: 0x{}", hex::encode(re_encoding));
            panic!("Original encoding does not match re-encoding:");
        }
        #[cfg(not(feature = "fuzzing"))]
        println!("Original encoding matches re-encoding");
    }
}

macro_rules! fuzz_runtimes {
    ($data:expr, [$(($name:expr, $call:path, $blocklist:expr)),+ $(,)?]) => {
        $(
            fuzz_runtime::<$call>($name, $blocklist, $data);
        )+
    };
}

fn main() {
    ziggy::fuzz!(|data: &[u8]| {
        if data.len() < 2 {
            return;
        }
        Externalities::execute_with(&mut Externalities::default(), || {
            fuzz_runtimes!(
                data,
                [
                    (
                        "polkadot",
                        polkadot_runtime::RuntimeCall,
                        &[[0x07, 0x04], [0x36, 0x00]]
                    ),
                    (
                        "kusama",
                        staging_kusama_runtime::RuntimeCall,
                        &[[0x06, 0x04], [0x36, 0x00]]
                    ),
                    (
                        "asset hub polkadot",
                        asset_hub_polkadot_runtime::RuntimeCall,
                        &[[0x01, 0x00], [0x59, 0x04], [0xff, 0x07]]
                    ),
                    (
                        "asset hub kusama",
                        asset_hub_kusama_runtime::RuntimeCall,
                        &[[0x01, 0x00], [0x59, 0x04], [0xff, 0x07]]
                    ),
                    (
                        "collectives polkadot",
                        collectives_polkadot_runtime::RuntimeCall,
                        &[[0x01, 0x00]]
                    ),
                    (
                        "coretime polkadot",
                        coretime_polkadot_runtime::RuntimeCall,
                        &[[0x01, 0x00]]
                    ),
                    (
                        "people polkadot",
                        people_polkadot_runtime::RuntimeCall,
                        &[[0x01, 0x00]]
                    ),
                    (
                        "coretime kusama",
                        coretime_kusama_runtime::RuntimeCall,
                        &[[0x01, 0x00]]
                    ),
                    (
                        "glutton kusama",
                        glutton_kusama_runtime::RuntimeCall,
                        &[[0x01, 0x00]]
                    ),
                    (
                        "people kusama",
                        people_kusama_runtime::RuntimeCall,
                        &[[0x01, 0x00]]
                    ),
                ]
            );
        });
    });
}
