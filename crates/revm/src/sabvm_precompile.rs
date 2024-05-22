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
        const OPCODE_ID_LEN: usize = std::mem::size_of::<u8>();
        let opcode_id = match consume_bytes_from(&mut input, OPCODE_ID_LEN) {
            Ok(bytes) => u8::from_be_bytes(bytes.try_into().unwrap()),
            Err(err) => return Err(err),
        };

        // Handle the different opcodes
        match opcode_id {
            // BALANCEOF
            0x2E => {
                // Extract the address from the input
                const ADDRESS_LEN: usize = U160::BYTES;
                let address: Address = match consume_bytes_from(&mut input, ADDRESS_LEN) {
                    Ok(bytes) => {
                        U160::from_be_bytes::<ADDRESS_LEN>(bytes.try_into().unwrap()).into()
                    }
                    Err(err) => return Err(err),
                };

                const ASSET_ID_LEN: usize = U256::BYTES;

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

            // MINT
            0xC0 => {
                // Extract the sub_id from the input
                const SUB_ID_LEN: usize = U256::BYTES;
                let sub_id = match consume_bytes_from(&mut input, SUB_ID_LEN) {
                    Ok(bytes) => U256::from_be_bytes::<SUB_ID_LEN>(bytes.try_into().unwrap()),
                    Err(err) => return Err(err),
                };

                // Extract the amount from the input
                const AMOUNT_LEN: usize = U256::BYTES;
                let amount = match consume_bytes_from(&mut input, AMOUNT_LEN) {
                    Ok(bytes) => U256::from_be_bytes::<AMOUNT_LEN>(bytes.try_into().unwrap()),
                    Err(err) => return Err(err),
                };

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
