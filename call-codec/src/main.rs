use codec::{Decode, Encode, Error};
use sp_state_machine::BasicExternalities as Externalities;
// use kitchensink_runtime::RuntimeCall;
// use polkadot_runtime::RuntimeCall;
// use staging_kusama_runtime::RuntimeCall;
use asset_hub_polkadot_runtime::RuntimeCall;

// for kitchensink
// const BLOCKLIST: [[u8; 2]; 1] = [[0x0b, 0x04]];
// for polkadot
// const BLOCKLIST: [[u8; 2]; 2] = [[0x07, 0x04], [0x36, 0x00]];
// for kusama
// const BLOCKLIST: [[u8; 2]; 1] = [[0x06, 0x04]];
// for polkadot asset hub
const BLOCKLIST: [[u8; 2]; 3] = [[0x01, 0x00], [0xff, 0x07], [0x59, 0x04]];

fn main() {
    ziggy::fuzz!(|data: &[u8]| {
        if data.len() < 2 {
            return;
        }
        if BLOCKLIST.iter().any(|b| data.windows(2).any(|w| w == b)) {
            return;
        }
        let mut extrinsic_data = data;
        Externalities::execute_with(&mut Default::default(), || {
            let maybe_extrinsic: Result<RuntimeCall, Error> = Decode::decode(&mut extrinsic_data);
            if let Ok(extrinsic) = maybe_extrinsic {
                #[cfg(not(feature = "fuzzing"))]
                println!("Found extrinsic: {extrinsic:?}");
                let original_extrinsic_data = &data[..data.len() - extrinsic_data.len()];
                let re_encoding = Encode::encode(&extrinsic);
                if original_extrinsic_data != re_encoding {
                    #[cfg(not(feature = "fuzzing"))]
                    println!("Original  : 0x{}", hex::encode(original_extrinsic_data));
                    #[cfg(not(feature = "fuzzing"))]
                    println!("Reencoding: 0x{}", hex::encode(re_encoding));
                    panic!("Original encoding does not match re-encoding:");
                }
                #[cfg(not(feature = "fuzzing"))]
                println!("Original encoding matches re-encoding");
            }
        })
    });
}
