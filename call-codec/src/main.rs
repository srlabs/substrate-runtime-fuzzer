use codec::{DecodeLimit, Encode};
use kitchensink_runtime::RuntimeCall;

const BLOCKLIST: [[u8; 2]; 1] = [[0x0b, 0x04]];

fn main() {
    ziggy::fuzz!(|data: &[u8]| {
        if data.len() < 2 {
            return;
        }
        if BLOCKLIST.iter().any(|b| data.windows(2).any(|w| w == b)) {
            return;
        }
        let mut extrinsic_data = data;
        // TODO Try to remove the depth limit
        let maybe_extrinsic: Result<RuntimeCall, codec::Error> =
            DecodeLimit::decode_with_depth_limit(64, &mut extrinsic_data);

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
    });
}
