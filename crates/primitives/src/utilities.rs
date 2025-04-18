use crate::{
    b256, TokenBalances, B256, BASE_TOKEN_ID, BLOB_GASPRICE_UPDATE_FRACTION, MIN_BLOB_GASPRICE,
    TARGET_BLOB_GAS_PER_BLOCK,
};
pub use alloy_primitives::keccak256;
use alloy_primitives::{Address, U256};

/// The Keccak-256 hash of the empty string `""`.
pub const KECCAK_EMPTY: B256 =
    b256!("c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470");

/// Calculates the `excess_blob_gas` from the parent header's `blob_gas_used` and `excess_blob_gas`.
///
/// See also [the EIP-4844 helpers]<https://eips.ethereum.org/EIPS/eip-4844#helpers>
/// (`calc_excess_blob_gas`).
#[inline]
pub fn calc_excess_blob_gas(parent_excess_blob_gas: u64, parent_blob_gas_used: u64) -> u64 {
    (parent_excess_blob_gas + parent_blob_gas_used).saturating_sub(TARGET_BLOB_GAS_PER_BLOCK)
}

/// Calculates the blob gas price from the header's excess blob gas field.
///
/// See also [the EIP-4844 helpers](https://eips.ethereum.org/EIPS/eip-4844#helpers)
/// (`get_blob_gasprice`).
#[inline]
pub fn calc_blob_gasprice(excess_blob_gas: u64) -> u128 {
    fake_exponential(
        MIN_BLOB_GASPRICE,
        excess_blob_gas,
        BLOB_GASPRICE_UPDATE_FRACTION,
    )
}

/// Creates a simple balances map with the given balance for the base token.
pub fn init_balances(base_balance: U256) -> TokenBalances {
    let mut balances = TokenBalances::new();
    balances.insert(BASE_TOKEN_ID, base_balance);
    balances
}

/// Approximates `factor * e ** (numerator / denominator)` using Taylor expansion.
///
/// This is used to calculate the blob price.
///
/// See also [the EIP-4844 helpers](https://eips.ethereum.org/EIPS/eip-4844#helpers)
/// (`fake_exponential`).
///
/// # Panics
///
/// This function panics if `denominator` is zero.
#[inline]
pub fn fake_exponential(factor: u64, numerator: u64, denominator: u64) -> u128 {
    assert_ne!(denominator, 0, "attempt to divide by zero");
    let factor = factor as u128;
    let numerator = numerator as u128;
    let denominator = denominator as u128;

    let mut i = 1;
    let mut output = 0;
    let mut numerator_accum = factor * denominator;
    while numerator_accum > 0 {
        output += numerator_accum;

        // Denominator is asserted as not zero at the start of the function.
        numerator_accum = (numerator_accum * numerator) / (denominator * i);
        i += 1;
    }
    output / denominator
}

/// Returns the token ID by hashing the address and sub ID.
pub fn token_id_address(address: Address, sub_id: U256) -> U256 {
    let first = &address[..];
    let second_bytes = B256::from(sub_id);
    let second = &second_bytes[..];
    keccak256([first, second].concat()).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GAS_PER_BLOB;

    // https://github.com/ethereum/go-ethereum/blob/28857080d732857030eda80c69b9ba2c8926f221/consensus/misc/eip4844/eip4844_test.go#L27
    #[test]
    fn test_calc_excess_blob_gas() {
        for t @ &(excess, blobs, expected) in &[
            // The excess blob gas should not increase from zero if the used blob
            // slots are below - or equal - to the target.
            (0, 0, 0),
            (0, 1, 0),
            (0, TARGET_BLOB_GAS_PER_BLOCK / GAS_PER_BLOB, 0),
            // If the target blob gas is exceeded, the excessBlobGas should increase
            // by however much it was overshot
            (
                0,
                (TARGET_BLOB_GAS_PER_BLOCK / GAS_PER_BLOB) + 1,
                GAS_PER_BLOB,
            ),
            (
                1,
                (TARGET_BLOB_GAS_PER_BLOCK / GAS_PER_BLOB) + 1,
                GAS_PER_BLOB + 1,
            ),
            (
                1,
                (TARGET_BLOB_GAS_PER_BLOCK / GAS_PER_BLOB) + 2,
                2 * GAS_PER_BLOB + 1,
            ),
            // The excess blob gas should decrease by however much the target was
            // under-shot, capped at zero.
            (
                TARGET_BLOB_GAS_PER_BLOCK,
                TARGET_BLOB_GAS_PER_BLOCK / GAS_PER_BLOB,
                TARGET_BLOB_GAS_PER_BLOCK,
            ),
            (
                TARGET_BLOB_GAS_PER_BLOCK,
                (TARGET_BLOB_GAS_PER_BLOCK / GAS_PER_BLOB) - 1,
                TARGET_BLOB_GAS_PER_BLOCK - GAS_PER_BLOB,
            ),
            (
                TARGET_BLOB_GAS_PER_BLOCK,
                (TARGET_BLOB_GAS_PER_BLOCK / GAS_PER_BLOB) - 2,
                TARGET_BLOB_GAS_PER_BLOCK - (2 * GAS_PER_BLOB),
            ),
            (
                GAS_PER_BLOB - 1,
                (TARGET_BLOB_GAS_PER_BLOCK / GAS_PER_BLOB) - 1,
                0,
            ),
        ] {
            let actual = calc_excess_blob_gas(excess, blobs * GAS_PER_BLOB);
            assert_eq!(actual, expected, "test: {t:?}");
        }
    }

    // https://github.com/ethereum/go-ethereum/blob/28857080d732857030eda80c69b9ba2c8926f221/consensus/misc/eip4844/eip4844_test.go#L60
    #[test]
    fn test_calc_blob_fee() {
        let blob_fee_vectors = &[
            (0, 1),
            (2314057, 1),
            (2314058, 2),
            (10 * 1024 * 1024, 23),
            // calc_blob_gasprice approximates `e ** (excess_blob_gas / BLOB_GASPRICE_UPDATE_FRACTION)` using Taylor expansion
            //
            // to roughly find where boundaries will be hit:
            // 2 ** bits = e ** (excess_blob_gas / BLOB_GASPRICE_UPDATE_FRACTION)
            // excess_blob_gas = ln(2 ** bits) * BLOB_GASPRICE_UPDATE_FRACTION
            (148099578, 18446739238971471609), // output is just below the overflow
            (148099579, 18446744762204311910), // output is just after the overflow
            (161087488, 902580055246494526580),
        ];

        for &(excess, expected) in blob_fee_vectors {
            let actual = calc_blob_gasprice(excess);
            assert_eq!(actual, expected, "test: {excess}");
        }
    }

    // https://github.com/ethereum/go-ethereum/blob/28857080d732857030eda80c69b9ba2c8926f221/consensus/misc/eip4844/eip4844_test.go#L78
    #[test]
    fn fake_exp() {
        for t @ &(factor, numerator, denominator, expected) in &[
            (1u64, 0u64, 1u64, 1u128),
            (38493, 0, 1000, 38493),
            (0, 1234, 2345, 0),
            (1, 2, 1, 6), // approximate 7.389
            (1, 4, 2, 6),
            (1, 3, 1, 16), // approximate 20.09
            (1, 6, 2, 18),
            (1, 4, 1, 49), // approximate 54.60
            (1, 8, 2, 50),
            (10, 8, 2, 542), // approximate 540.598
            (11, 8, 2, 596), // approximate 600.58
            (1, 5, 1, 136),  // approximate 148.4
            (1, 5, 2, 11),   // approximate 12.18
            (2, 5, 2, 23),   // approximate 24.36
            (1, 50000000, 2225652, 5709098764),
            (1, 380928, BLOB_GASPRICE_UPDATE_FRACTION, 1),
        ] {
            let actual = fake_exponential(factor, numerator, denominator);
            assert_eq!(actual, expected, "test: {t:?}");
        }
    }
}

#[cfg(feature = "std")]
pub mod bytes_parsing {
    use crate::{Address, U256};

    use alloy_primitives::{Bytes, FixedBytes};
    use std::{mem::size_of, vec::Vec};

    #[derive(Debug)]
    pub enum BytesParsingError {
        InvalidInput,
    }

    impl std::error::Error for BytesParsingError {}

    impl std::fmt::Display for BytesParsingError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                BytesParsingError::InvalidInput => write!(f, "Invalid input"),
            }
        }
    }

    pub fn consume_bytes_from(
        input: &mut Bytes,
        no_bytes: usize,
    ) -> Result<Vec<u8>, BytesParsingError> {
        if input.len() < no_bytes {
            return Err(BytesParsingError::InvalidInput);
        }
        Ok(input.split_to(no_bytes).to_vec())
    }

    pub fn consume_u8_from(input: &mut Bytes) -> Result<u8, BytesParsingError> {
        const U8_LEN: usize = size_of::<u8>();
        let bytes = consume_bytes_from(input, U8_LEN)?;
        Ok(u8::from_be_bytes(bytes.try_into().unwrap()))
    }

    pub fn consume_u16_from(input: &mut Bytes) -> Result<u16, BytesParsingError> {
        const U16_LEN: usize = size_of::<u16>();
        let bytes = consume_bytes_from(input, U16_LEN)?;
        Ok(u16::from_be_bytes(bytes.try_into().unwrap()))
    }

    pub fn consume_u32_from(input: &mut Bytes) -> Result<u32, BytesParsingError> {
        const U32_LEN: usize = size_of::<u32>();
        let bytes = consume_bytes_from(input, U32_LEN)?;
        Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
    }

    pub fn consume_u256_from(input: &mut Bytes) -> Result<U256, BytesParsingError> {
        const U256_LEN: usize = U256::BYTES;
        let bytes = consume_bytes_from(input, U256_LEN)?;
        Ok(U256::from_be_bytes::<U256_LEN>(bytes.try_into().unwrap()))
    }

    pub fn consume_usize_from(input: &mut Bytes) -> Result<usize, BytesParsingError> {
        const USIZE_LEN: usize = size_of::<usize>();
        let bytes = consume_bytes_from(input, USIZE_LEN)?;
        Ok(usize::from_be_bytes(bytes.try_into().unwrap()))
    }

    pub fn consume_address_from(input: &mut Bytes) -> Result<Address, BytesParsingError> {
        let word = consume_word_from(input)?;
        Ok(Address::from_word(word))
    }

    pub fn consume_word_from(input: &mut Bytes) -> Result<FixedBytes<32>, BytesParsingError> {
        const WORD_LEN: usize = U256::BYTES;
        let bytes = consume_bytes_from(input, WORD_LEN)?;
        Ok(FixedBytes::from_slice(bytes.as_slice()))
    }
}
