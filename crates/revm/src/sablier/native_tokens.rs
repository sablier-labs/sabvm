//! Stateful precompile to implement Native Tokens.
use crate::{
    interpreter::CallInputs,
    precompile::{Error, PrecompileResult},
    primitives::{utilities::bytes_parsing::*, Address, Bytes, EVMError, TokenTransfer, U256},
    ContextStatefulPrecompileMut, Database, InnerEvmContext,
};
use std::{string::String, vec::Vec};

pub const ADDRESS: Address = crate::sablier::u64_to_prefixed_address(1);

pub const MINT_SELECTOR: u32 = 0x836a1040; // The function selector of `mint(uint256 subID, address recipient, uint256 amount)`
pub const BURN_SELECTOR: u32 = 0x9eea5f66; // The function selector of `burn(uint256 subID, address tokenHolder, uint256 amount)`
pub const BALANCEOF_SELECTOR: u32 = 0x3656eec2; // The function selector of `balanceOf(uint256 tokenID, address account)`
pub const TRANSFER_SELECTOR: u32 = 0x095bcdb6; // The function selector of `transfer(address to, uint256 tokenID, uint256 amount)`

/// The base gas cost for the NativeTokens precompile operations.
pub const BASE_GAS_COST: u64 = 15;

pub struct NativeTokensContextPrecompile;

impl Clone for NativeTokensContextPrecompile {
    fn clone(&self) -> Self {
        NativeTokensContextPrecompile
    }
}

fn is_caller_eoa<DB: Database>(
    evmctx: &mut InnerEvmContext<DB>,
    caller: Address,
) -> Result<bool, EVMError<DB::Error>> {
    evmctx.code(caller).map(|(bytecode, _)| bytecode.is_empty())
}

impl<DB: Database> ContextStatefulPrecompileMut<DB> for NativeTokensContextPrecompile {
    fn call_mut(
        &mut self,
        inputs: &CallInputs,
        gas_limit: u64,
        evmctx: &mut InnerEvmContext<DB>,
    ) -> PrecompileResult {
        let gas_used = BASE_GAS_COST;
        if gas_used > gas_limit {
            return Err(Error::OutOfGas);
        }

        // Create a local mutable copy of the input bytes
        let mut input = inputs.input.clone();

        // Parse the input bytes, to figure out what opcode to execute
        let function_selector = consume_u32(&mut input).map_err(|_| Error::InvalidInput)?;

        // Handle the different opcodes
        match function_selector {
            BALANCEOF_SELECTOR => {
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

            MINT_SELECTOR => {
                if inputs.is_static {
                    return Err(Error::AttemptedStateChangeDuringStaticCall);
                }

                // Make sure that the caller is a contract
                let caller = inputs.target_address;
                if is_caller_eoa(evmctx, caller).map_err(|_| Error::UnauthorizedCaller)? {
                    return Err(Error::UnauthorizedCaller);
                }

                // Extract the sub_id from the input
                let sub_id = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the recipient's address from the input
                let recipient =
                    consume_address_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the amount from the input
                let amount = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                let minter = caller;
                if evmctx
                    .journaled_state
                    .mint(minter, recipient, sub_id, amount, &mut evmctx.db)
                {
                    Ok((gas_used, Bytes::new()))
                } else {
                    Err(Error::Other(String::from("Mint failed")))
                }
            }

            BURN_SELECTOR => {
                // TODO: consider forcing the to-be-burned tokens to be transferred as MNTs.
                // This would allow us to deduce the token ID from the call itself, as well as make the burning process more transparent to the caller
                // - and more secure (as e.g. we wouldn't have to deal with the situation when the caller doesn't have enough tokens to burn).

                if inputs.is_static {
                    return Err(Error::AttemptedStateChangeDuringStaticCall);
                }

                // Make sure that the caller is a contract
                let caller = inputs.target_address;
                if is_caller_eoa(evmctx, caller).map_err(|_| Error::UnauthorizedCaller)? {
                    return Err(Error::UnauthorizedCaller);
                }

                // Extract the sub_id from the input
                let sub_id = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the token holder address from the input
                let token_holder =
                    consume_address_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the amount from the input
                let amount = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                let burner = caller;
                if evmctx
                    .journaled_state
                    .burn(burner, sub_id, token_holder, amount, &mut evmctx.db)
                {
                    Ok((gas_used, Bytes::new()))
                } else {
                    Err(Error::Other(String::from("Burn failed")))
                }
            }

            TRANSFER_SELECTOR => {
                if inputs.is_static {
                    return Err(Error::AttemptedStateChangeDuringStaticCall);
                }

                // Make sure that the caller is a contract
                let caller = inputs.target_address;
                if is_caller_eoa(evmctx, caller).map_err(|_| Error::UnauthorizedCaller)? {
                    return Err(Error::UnauthorizedCaller);
                }

                // Extract the recipient's address from the input
                let recipient =
                    consume_address_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the token ID from the input
                let token_id = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the amount from the input
                let amount = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                let sender = caller;
                if evmctx
                    .journaled_state
                    .transfer(
                        &sender,
                        &recipient,
                        &vec![
                            (TokenTransfer {
                                id: token_id,
                                amount,
                            }),
                        ],
                        &mut evmctx.db,
                    )
                    .is_ok()
                {
                    Ok((gas_used, Bytes::new()))
                } else {
                    Err(Error::Other(String::from("Transfer failed")))
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

            // TRANSFER
            // TRANSFERANDCALL
            // TRANSFERMULTIPLE
            // TRANSFERMULTIPLEANDCALL
            // 0xF6 => MNTCREATE
            _ => Err(Error::InvalidInput),
        }
    }
}
