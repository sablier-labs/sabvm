use crate::{
    precompile::{u64_to_address, Error, PrecompileResult},
    primitives::{Address, Bytes, U160, U256},
    ContextStatefulPrecompileMut, Database, InnerEvmContext,
};

pub const ADDRESS: Address = u64_to_address(50); // TODO: find a meaningful address for the precompile

/// The base gas cost of the precompile operation.
pub const SABVM_BASE_GAS_COST: u64 = 15;

pub struct SabVMContextPrecompile;

impl Clone for SabVMContextPrecompile {
    fn clone(&self) -> Self {
        SabVMContextPrecompile
    }
}

fn consume_bytes_from(input: &mut Bytes, no_bytes: usize) -> Result<Vec<u8>, Error> {
    if input.len() < no_bytes {
        return Err(Error::SabVMInvalidInput);
    }
    Ok(input.split_to(no_bytes).to_vec())
}

fn consume_u8(input: &mut Bytes) -> Result<u8, Error> {
    const U8_LEN: usize = std::mem::size_of::<u8>();
    let bytes = consume_bytes_from(input, U8_LEN)?;
    Ok(u8::from_be_bytes(bytes.try_into().unwrap()))
}

fn consume_u256_from(input: &mut Bytes) -> Result<U256, Error> {
    const U256_LEN: usize = U256::BYTES;
    let bytes = consume_bytes_from(input, U256_LEN)?;
    Ok(U256::from_be_bytes::<U256_LEN>(bytes.try_into().unwrap()))
}

fn consume_address_from(input: &mut Bytes) -> Result<Address, Error> {
    const ADDRESS_LEN: usize = U160::BYTES;
    let bytes = consume_bytes_from(input, ADDRESS_LEN)?;
    Ok(U160::from_be_bytes::<ADDRESS_LEN>(bytes.try_into().unwrap()).into())
}

impl<DB: Database> ContextStatefulPrecompileMut<DB> for SabVMContextPrecompile {
    fn call_mut(
        &mut self,
        input: &Bytes,
        gas_limit: u64,
        evmctx: &mut InnerEvmContext<DB>,
    ) -> PrecompileResult {
        let gas_used = SABVM_BASE_GAS_COST;
        if gas_used > gas_limit {
            return Err(Error::OutOfGas);
        }

        // Create a local mutable copy of the input bytes
        let mut input = input.clone();

        // Parse the input bytes, to figure out what opcode to execute
        let opcode_id = consume_u8(&mut input)?;

        // TODO: instead of opcode ids, operate based on function selectors from the INativeTokens interface

        // Handle the different opcodes
        match opcode_id {
            // BALANCEOF
            0x2E => {
                // Extract the address from the input
                let address = consume_address_from(&mut input)?;

                // Extract the asset_id from the input
                let asset_id = consume_u256_from(&mut input)?;

                match evmctx.balance(address, asset_id) {
                    Ok(balance) => {
                        Ok((gas_used, balance.0.to_be_bytes::<{ U256::BYTES }>().into()))
                    }
                    Err(_) => Err(Error::SabVMInvalidInput),
                }
            }

            // MINT
            0xC0 => {
                // Extract the sub_id from the input
                let sub_id = consume_u256_from(&mut input)?;

                // Extract the amount from the input
                let amount = consume_u256_from(&mut input)?;

                let minter = evmctx.env().tx.caller;
                if evmctx
                    .journaled_state
                    .mint(minter, sub_id, amount, &mut evmctx.db)
                {
                    Ok((gas_used, Bytes::new()))
                } else {
                    Err(Error::Other(String::from("Mint failed")))
                }
            }

            _ => Err(Error::SabVMInvalidInput),
        }
    }
}
