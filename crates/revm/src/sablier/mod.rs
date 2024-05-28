use crate::primitives::Address;

pub mod native_tokens;

/// Similar to `crate::u64_to_address`, but adds the number 706 as a prefix. 706 is the sum of the ASCII value
/// of the characters in the string "Sablier".
///
/// Example: 0x7060000000000000000000000000000000000001
#[inline]
pub const fn u64_to_prefixed_address(x: u64) -> Address {
    let x = x.to_be_bytes();
    Address::new([
        70, 60, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, x[0], x[1], x[2], x[3], x[4], x[5], x[6], x[7],
    ])
}
