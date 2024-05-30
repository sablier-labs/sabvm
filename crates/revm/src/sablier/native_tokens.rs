//! Stateful precompile to implement Native Tokens.
use crate::{
    precompile::{Error, PrecompileResult},
    primitives::{utilities::bytes_parsing::*, Address, Bytes, U256},
    ContextStatefulPrecompileMut, Database, InnerEvmContext,
};
use std::{string::String, vec::Vec};

pub const ADDRESS: Address = crate::sablier::u64_to_prefixed_address(1);

/// The base gas cost for the NativeTokens precompile operations.
pub const BASE_GAS_COST: u64 = 15;

pub struct NativeTokensContextPrecompile;

impl Clone for NativeTokensContextPrecompile {
    fn clone(&self) -> Self {
        NativeTokensContextPrecompile
    }
}

// TODO: uncomment the verification below when smart contracts are allowed to be deployed on the Mainnet
// fn is_caller_eoa<DB: Database>(
//     evmctx: &mut InnerEvmContext<DB>,
// ) -> Result<bool, EVMError<DB::Error>> {
//     let caller = evmctx.env.tx.caller;
//     evmctx.code(caller).map(|(bytecode, _)| bytecode.is_empty())
// }

impl<DB: Database> ContextStatefulPrecompileMut<DB> for NativeTokensContextPrecompile {
    fn call_mut(
        &mut self,
        input: &Bytes,
        gas_limit: u64,
        evmctx: &mut InnerEvmContext<DB>,
    ) -> PrecompileResult {
        let gas_used = BASE_GAS_COST;
        if gas_used > gas_limit {
            return Err(Error::OutOfGas);
        }

        // TODO: uncomment the verification below when smart contracts are allowed to be deployed on the Mainnet
        // match is_caller_eoa(evmctx) {
        //     Ok(is_eoa) => {
        //         if is_eoa {
        //             return Err(Error::SabVMUnauthorizedCaller);
        //         }
        //     }
        //     Err(_) => {
        //         return Err(Error::SabVMUnauthorizedCaller);
        //     }
        // }

        // Create a local mutable copy of the input bytes
        let mut input = input.clone();

        // Parse the input bytes, to figure out what opcode to execute
        let opcode_id = consume_u8(&mut input).map_err(|_| Error::InvalidInput)?;

        // TODO: instead of opcode ids, operate based on function selectors from the INativeTokens interface

        // Handle the different opcodes
        match opcode_id {
            // BALANCEOF
            0xC0 => {
                // Extract the token id from the input
                let token_id = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the address from the input
                let address = consume_address_from(&mut input).map_err(|_| Error::InvalidInput)?;

                match evmctx.balance(token_id, address) {
                    Ok(balance) => {
                        Ok((gas_used, balance.0.to_be_bytes::<{ U256::BYTES }>().into()))
                    }
                    Err(_) => Err(Error::InvalidInput),
                }
            }

            // MINT
            0xC1 => {
                // Extract the recipient's address from the input
                let recipient =
                    consume_address_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the sub_id from the input
                let sub_id = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the amount from the input
                let amount = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                let minter = evmctx.env().tx.caller;
                if evmctx
                    .journaled_state
                    .mint(minter, recipient, sub_id, amount, &mut evmctx.db)
                {
                    Ok((gas_used, Bytes::new()))
                } else {
                    Err(Error::Other(String::from("Mint failed")))
                }
            }

            // BURN
            0xC2 => {
                // Extract the sub_id from the input
                let sub_id = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the amount from the input
                let amount = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                let burner = evmctx.env().tx.caller;
                if evmctx
                    .journaled_state
                    .burn(burner, sub_id, amount, &mut evmctx.db)
                {
                    Ok((gas_used, Bytes::new()))
                } else {
                    Err(Error::Other(String::from("Burn failed")))
                }
            }

            // MNTCALLVALUES
            0x2F => {
                let mut call_values: Vec<u8> = evmctx
                    .env
                    .tx
                    .transferred_tokens
                    .len()
                    .to_be_bytes()
                    .to_vec();
                for token in evmctx.env.tx.transferred_tokens.iter() {
                    call_values.append(token.id.to_be_bytes_vec().as_mut());
                    call_values.append(token.amount.to_be_bytes_vec().as_mut());
                }

                Ok((gas_used, Bytes::from(call_values)))
            }

            // 0xEE => MNTCALL
            // 0xF6 => MNTCREATE
            _ => Err(Error::InvalidInput),
        }
    }
}
