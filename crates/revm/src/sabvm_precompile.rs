use crate::{
    precompile::{u64_to_address, Error, PrecompileResult},
    primitives::{Address, Bytes, U256},
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

        // Create a local mutable  copy of the input bytes
        let mut input = input.clone();

        // Parse the input bytes, to figure out what opcode to execute
        let opcode_id = match consume_bytes_from(&mut input, 4) {
            Ok(bytes) => u32::from_be_bytes(bytes.try_into().unwrap()),
            Err(err) => return Err(err),
        };

        // Handle the different opcodes
        match opcode_id {
            // MNABALANCE
            0x2E => {
                // Extract the address from the input
                let address = match consume_bytes_from(&mut input, 20) {
                    Ok(bytes) => Address::from_word(bytes.as_slice().try_into().unwrap()),
                    Err(err) => return Err(err),
                };

                const ASSET_ID_LEN: usize = 32;

                // Extract the asset_id from the input
                let asset_id = match consume_bytes_from(&mut input, ASSET_ID_LEN) {
                    Ok(bytes) => U256::from_be_bytes::<ASSET_ID_LEN>(bytes.try_into().unwrap()),
                    Err(err) => return Err(err),
                };

                match evmctx.balance(address, asset_id) {
                    Ok(balance) => Ok((gas_used, balance.0.to_be_bytes::<ASSET_ID_LEN>().into())),
                    Err(_) => Err(Error::SabVMInvalidInput),
                }
            }

            _ => Err(Error::SabVMInvalidInput),
        }
    }
}
