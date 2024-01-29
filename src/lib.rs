use codec::{Decode, DecodeLimit};

// The initial timestamp at the start of an input run.
pub const INITIAL_TIMESTAMP: u64 = 0;

/// The maximum number of blocks per fuzzer input.
/// If set to 0, then there is no limit at all.
/// Feel free to set this to a low number (e.g. 4) when you begin your fuzzing campaign and then set it back to 32 once you have good coverage.
pub const MAX_BLOCKS_PER_INPUT: usize = 4;

/// The maximum number of extrinsics per block.
/// If set to 0, then there is no limit at all.
/// Feel free to set this to a low number (e.g. 4) when you begin your fuzzing campaign and then set it back to 0 once you have good coverage.
pub const MAX_EXTRINSICS_PER_BLOCK: usize = 4;

/// Max number of seconds a block should run for.
pub const MAX_TIME_FOR_BLOCK: u64 = 6;

/// The max number of blocks we want the fuzzer to run to between extrinsics.
/// Considering 1 block every 6 seconds, 100_800 blocks correspond to 1 week.
pub const MAX_BLOCK_LAPSE: u32 = 100_800;

// Extrinsic delimiter: `********`
const DELIMITER: [u8; 8] = [42; 8];

// The data structure we use for iterating over extrinsics
pub struct Data<'a> {
    data: &'a [u8],
    pointer: usize,
    size: usize,
}

impl<'a> Data<'a> {
    pub fn from_data(data: &'a [u8]) -> Self {
        Data {
            data,
            pointer: 0,
            size: 0,
        }
    }

    pub fn size_limit_reached(&self) -> bool {
        !(MAX_BLOCKS_PER_INPUT == 0 || MAX_EXTRINSICS_PER_BLOCK == 0)
            && self.size >= MAX_BLOCKS_PER_INPUT * MAX_EXTRINSICS_PER_BLOCK
    }

    pub fn extract_extrinsics<T: Decode>(&mut self) -> Vec<(Option<u32>, usize, T)> {
        let mut block_count = 0;
        let mut extrinsics_in_block = 0;

        self.filter_map(|data| {
            // We have reached the limit of block we want to decode
            if MAX_BLOCKS_PER_INPUT != 0 && block_count >= MAX_BLOCKS_PER_INPUT {
                return None;
            }
            // lapse is u32 (4 bytes), origin is u16 (2 bytes) -> 6 bytes minimum
            let min_data_len = 4 + 2;
            if data.len() <= min_data_len {
                return None;
            }
            let lapse: u32 = u32::from_ne_bytes(data[0..4].try_into().unwrap());
            let origin: usize = u16::from_ne_bytes(data[4..6].try_into().unwrap()) as usize;
            let mut encoded_extrinsic: &[u8] = &data[6..];

            // If the lapse is in the range [1, MAX_BLOCK_LAPSE] it is valid.
            let maybe_lapse = match lapse {
                1..=MAX_BLOCK_LAPSE => Some(lapse),
                _ => None,
            };
            // We have reached the limit of extrinsics for this block
            if maybe_lapse.is_none()
                && MAX_EXTRINSICS_PER_BLOCK != 0
                && extrinsics_in_block >= MAX_EXTRINSICS_PER_BLOCK
            {
                return None;
            }

            match DecodeLimit::decode_with_depth_limit(64, &mut encoded_extrinsic) {
                Ok(decoded_extrinsic) => {
                    if maybe_lapse.is_some() {
                        block_count += 1;
                        extrinsics_in_block = 1;
                    } else {
                        extrinsics_in_block += 1;
                    }
                    // We have reached the limit of block we want to decode
                    if MAX_BLOCKS_PER_INPUT != 0 && block_count >= MAX_BLOCKS_PER_INPUT {
                        return None;
                    }
                    Some((maybe_lapse, origin, decoded_extrinsic))
                }
                Err(_) => None,
            }
        })
        .collect()
    }
}

impl<'a> Iterator for Data<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.data.len() <= self.pointer || self.size_limit_reached() {
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
