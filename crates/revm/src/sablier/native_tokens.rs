//! Stateful precompile to implement Native Tokens.
use revm_precompile::{PrimitiveCallInfo, ResultInfo, ResultOrNewCall};

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
pub const TRANSFER_AND_CALL_SELECTOR: u32 = 0xd1c673e9; // The function selector of `transferAndCall(address recipientAndCallee, uint256 tokenID, uint256 amount, bytes calldata data)`
pub const TRANSFER_MULTIPLE_SELECTOR: u32 = 0x99583417; // The function selector of `transferMultiple(address to, uint256[] calldata tokenIDs, uint256[] calldata amounts)`
pub const TRANSFER_MULTIPLE_AND_CALL_SELECTOR: u32 = 0x822bbe4c; // The function selector of `transferMultipleAndCall(address recipientAndCallee, uint256[] calldata tokenIDs, uint256[] calldata, amounts bytes calldata data)`

/// The base gas cost for the NativeTokens precompile operations.
pub const BASE_GAS_COST: u64 = 15;

pub struct NativeTokensContextPrecompile;

impl Clone for NativeTokensContextPrecompile {
    fn clone(&self) -> Self {
        NativeTokensContextPrecompile
    }
}

fn is_address_eoa<DB: Database>(
    evmctx: &mut InnerEvmContext<DB>,
    address: Address,
) -> Result<bool, EVMError<DB::Error>> {
    evmctx
        .code(address)
        .map(|(bytecode, _)| bytecode.is_empty())
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
        let function_selector = consume_u32_from(&mut input).map_err(|_| Error::InvalidInput)?;

        // Handle the different opcodes
        match function_selector {
            BALANCEOF_SELECTOR => {
                // Extract the token id from the input
                let token_id = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the address from the input
                let address = consume_address_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // if the input has not been fully consumed by this point, it has been ill-formed
                if !input.is_empty() {
                    return Err(Error::InvalidInput);
                }

                match evmctx.balance(token_id, address) {
                    Ok(balance) => Ok(ResultOrNewCall::Result(ResultInfo {
                        gas_used,
                        returned_bytes: balance.0.to_be_bytes::<{ U256::BYTES }>().into(),
                    })),
                    Err(_) => Err(Error::InvalidInput),
                }
            }

            MINT_SELECTOR => {
                if inputs.is_static {
                    return Err(Error::AttemptedStateChangeDuringStaticCall);
                }

                // Make sure that the caller is a contract
                let caller = inputs.target_address;
                if is_address_eoa(evmctx, caller).map_err(|_| Error::UnauthorizedCaller)? {
                    return Err(Error::UnauthorizedCaller);
                }

                // Extract the sub_id from the input
                let sub_id = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the recipient's address from the input
                let recipient =
                    consume_address_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the amount from the input
                let amount = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // if the input has not been fully consumed by this point, it has been ill-formed
                if !input.is_empty() {
                    return Err(Error::InvalidInput);
                }

                let minter = caller;
                if evmctx
                    .journaled_state
                    .mint(minter, recipient, sub_id, amount, &mut evmctx.db)
                {
                    Ok(ResultOrNewCall::Result(ResultInfo {
                        gas_used,
                        returned_bytes: Bytes::new(),
                    }))
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
                if is_address_eoa(evmctx, caller).map_err(|_| Error::UnauthorizedCaller)? {
                    return Err(Error::UnauthorizedCaller);
                }

                // Extract the sub_id from the input
                let sub_id = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the token holder address from the input
                let token_holder =
                    consume_address_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the amount from the input
                let amount = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // if the input has not been fully consumed by this point, it has been ill-formed
                if !input.is_empty() {
                    return Err(Error::InvalidInput);
                }

                let burner = caller;
                if evmctx
                    .journaled_state
                    .burn(burner, sub_id, token_holder, amount, &mut evmctx.db)
                {
                    Ok(ResultOrNewCall::Result(ResultInfo {
                        gas_used,
                        returned_bytes: Bytes::new(),
                    }))
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
                if is_address_eoa(evmctx, caller).map_err(|_| Error::UnauthorizedCaller)? {
                    return Err(Error::UnauthorizedCaller);
                }

                // Extract the recipient's address from the input
                let recipient =
                    consume_address_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the token ID from the input
                let token_id = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the amount from the input
                let amount = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // if the input has not been fully consumed by this point, it has been ill-formed
                if !input.is_empty() {
                    return Err(Error::InvalidInput);
                }

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
                    Ok(ResultOrNewCall::Result(ResultInfo {
                        gas_used,
                        returned_bytes: Bytes::new(),
                    }))
                } else {
                    Err(Error::Other(String::from("Transfer failed")))
                }
            }

            TRANSFER_AND_CALL_SELECTOR => {
                if inputs.is_static {
                    return Err(Error::AttemptedStateChangeDuringStaticCall);
                }

                // Make sure that the caller is a contract
                let caller = inputs.target_address;
                if is_address_eoa(evmctx, caller).map_err(|_| Error::UnauthorizedCaller)? {
                    return Err(Error::UnauthorizedCaller);
                }

                // Extract the recipient's address from the input
                let recipient_and_callee =
                    consume_address_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Make sure that the callee is a contract
                if is_address_eoa(evmctx, recipient_and_callee).map_err(|_| Error::InvalidInput)? {
                    return Err(Error::InvalidInput);
                }

                // Extract the token ID from the input
                let token_id = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the amount from the input
                let amount = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract and ignore the calldata offset from the input
                let _ = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the byte size of the calldata from the input
                let calldata_size =
                    consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                let calldata_usize: usize = calldata_size.try_into().unwrap_or_default();

                // Extract the calldata from the input
                let mut calldata = consume_bytes_from(&mut input, calldata_usize)
                    .map_err(|_| Error::InvalidInput)?;

                // if the input has not been fully consumed by this point, it has been ill-formed
                if !input.is_empty() {
                    return Err(Error::InvalidInput);
                }

                // Renounce the 28-byte 0 prefix, forming the EVM word together with the 4-byte function selector
                calldata = calldata[28..].to_vec();

                // Signal to the external context that a Call to the callee must be performed,
                // transferring the MNTs and passing the calldata to it
                Ok(ResultOrNewCall::Call(PrimitiveCallInfo {
                    target_address: recipient_and_callee,
                    token_transfers: vec![
                        (TokenTransfer {
                            id: token_id,
                            amount,
                        }),
                    ],
                    input_data: calldata.into(),
                }))
            }

            TRANSFER_MULTIPLE_SELECTOR => {
                if inputs.is_static {
                    return Err(Error::AttemptedStateChangeDuringStaticCall);
                }

                // Make sure that the caller is a contract
                let caller = inputs.target_address;
                if is_address_eoa(evmctx, caller).map_err(|_| Error::UnauthorizedCaller)? {
                    return Err(Error::UnauthorizedCaller);
                }

                // Extract the recipient's address from the input
                let recipient =
                    consume_address_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract & ignore the token_ids offset
                consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;
                // Extract & ignore the transfer_amounts offset
                consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the length of the token IDs array from the input
                let token_ids_len =
                    consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the token IDs from the input
                let mut token_ids = Vec::new();
                let last_64_bits: &[u8] = &token_ids_len.to_be_bytes::<32>()[24..];
                let token_ids_len_u64 = u64::from_be_bytes(last_64_bits.try_into().unwrap());
                for _ in 0..token_ids_len_u64 {
                    token_ids.push(consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?);
                }

                // Make sure the token IDs are unique
                if token_ids.len()
                    != token_ids
                        .iter()
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                {
                    return Err(Error::InvalidInput);
                }

                // Extract the length of the token IDs array from the input
                let transfer_amounts_len =
                    consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                if token_ids_len != transfer_amounts_len {
                    return Err(Error::InvalidInput);
                }

                // Extract the transfer amounts from the input
                let mut transfer_amounts = Vec::new();
                let last_64_bits: &[u8] = &transfer_amounts_len.to_be_bytes::<32>()[24..];
                let transfer_amounts_len_u64 = u64::from_be_bytes(last_64_bits.try_into().unwrap());
                for _ in 0..transfer_amounts_len_u64 {
                    transfer_amounts
                        .push(consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?);
                }

                // if the input has not been fully consumed by this point, it has been ill-formed
                if !input.is_empty() {
                    return Err(Error::InvalidInput);
                }

                // Transform the passed token IDs & amounts into a vector of TokenTransfers
                let token_transfers = token_ids
                    .iter()
                    .zip(transfer_amounts.iter())
                    .map(|(id, amount)| TokenTransfer {
                        id: *id,
                        amount: *amount,
                    })
                    .collect::<Vec<TokenTransfer>>();

                let sender = caller;
                if evmctx
                    .journaled_state
                    .transfer(&sender, &recipient, &token_transfers, &mut evmctx.db)
                    .is_ok()
                {
                    Ok(ResultOrNewCall::Result(ResultInfo {
                        gas_used,
                        returned_bytes: Bytes::new(),
                    }))
                } else {
                    Err(Error::Other(String::from("Transfer failed")))
                }
            }

            TRANSFER_MULTIPLE_AND_CALL_SELECTOR => {
                if inputs.is_static {
                    return Err(Error::AttemptedStateChangeDuringStaticCall);
                }

                // Make sure that the caller is a contract
                let caller = inputs.target_address;
                if is_address_eoa(evmctx, caller).map_err(|_| Error::UnauthorizedCaller)? {
                    return Err(Error::UnauthorizedCaller);
                }

                // Extract the recipient's address from the input
                let recipient_and_callee =
                    consume_address_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Make sure that the callee is a contract
                if is_address_eoa(evmctx, recipient_and_callee).map_err(|_| Error::InvalidInput)? {
                    return Err(Error::InvalidInput);
                }

                // Extract & ignore the token_ids offset
                let _ = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract & ignore the transfer_amounts offset
                let _ = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract & ignore the calldata offset from the input
                let _ = consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Extract the length of the token IDs array from the input
                let token_ids_len =
                    consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                // Initialize the vector of TokenTransfers
                let capacity: usize = token_ids_len.try_into().unwrap_or_default();
                let mut token_transfers: Vec<TokenTransfer> = Vec::with_capacity(capacity);

                // Extract the token IDs from the input
                for _ in 0..capacity {
                    let token_id =
                        consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;
                    token_transfers.push(TokenTransfer {
                        id: token_id,
                        amount: U256::ZERO,
                    });
                }

                // Make sure the token IDs inside the vector are unique
                if token_transfers.len()
                    != token_transfers
                        .iter()
                        .map(|x| x.id)
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                {
                    return Err(Error::InvalidInput);
                }

                // Extract the length of the transfer array from the input
                let transfer_amounts_len =
                    consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                if token_ids_len != transfer_amounts_len {
                    return Err(Error::InvalidInput);
                }

                // Extract the transfer amounts from the input
                for transfer in token_transfers.iter_mut() {
                    transfer.amount =
                        consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;
                }

                // Extract the byte size of the calldata from the input
                let calldata_size =
                    consume_u256_from(&mut input).map_err(|_| Error::InvalidInput)?;

                let calldata_usize: usize = calldata_size.try_into().unwrap_or_default();

                // Extract the calldata from the input
                let mut calldata = consume_bytes_from(&mut input, calldata_usize)
                    .map_err(|_| Error::InvalidInput)?;

                // if the input has not been fully consumed by this point, it has been ill-formed
                if !input.is_empty() {
                    return Err(Error::InvalidInput);
                }

                // Renounce the 28-byte 0 prefix, forming the EVM word together with the 4-byte function selector
                calldata = calldata[28..].to_vec();

                // Signal to the external context that a Call to the callee must be performed,
                // transferring the MNTs and passing the calldata to it
                Ok(ResultOrNewCall::Call(PrimitiveCallInfo {
                    target_address: recipient_and_callee,
                    token_transfers,
                    input_data: calldata.into(),
                }))
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

                Ok(ResultOrNewCall::Result(ResultInfo {
                    gas_used,
                    returned_bytes: Bytes::from(call_values),
                }))
            }

            // TRANSFERMULTIPLEANDCALL
            // 0xF6 => MNTCREATE
            _ => Err(Error::InvalidInput),
        }
    }
}
