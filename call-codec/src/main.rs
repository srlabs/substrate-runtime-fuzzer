use codec::{DecodeLimit, Encode};
use kitchensink_runtime::RuntimeCall;

fn main() {
    ziggy::fuzz!(|data: &[u8]| {
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
                println!("Original  : {}", hex::encode((original_extrinsic_data));
                #[cfg(not(feature = "fuzzing"))]
                println!("Reencoding: {}", hex::encode((re_encoding));
                panic!("Original encoding does not match re-encoding:")
            }
            #[cfg(not(feature = "fuzzing"))]
            println!("Original encoding matches re-encoding");
        }
    });
}
