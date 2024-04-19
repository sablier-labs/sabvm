mod call_helpers;

pub use call_helpers::{calc_call_gas, get_memory_input_and_out_ranges};

use crate::{
    gas::{self, COLD_ACCOUNT_ACCESS_COST, WARM_STORAGE_READ_COST},
    interpreter::{Interpreter, InterpreterAction},
    primitives::{Bytes, Log, LogData, Spec, SpecId::*, B256, U256},
    CallContext, CallInputs, CallScheme, CreateInputs, CreateScheme, Host, InstructionResult,
    SStoreResult, Transfer, MAX_INITCODE_SIZE,
};
use core::cmp::min;
use revm_primitives::{Asset, BASE_ASSET_ID, BLOCK_HASH_HISTORY};

/// EIP-1884: Repricing for trie-size-dependent opcodes
pub fn selfbalance<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    check!(interpreter, ISTANBUL);
    gas!(interpreter, gas::LOW);
    let Some((balance, _)) = host.base_balance(interpreter.contract.address) else {
        interpreter.instruction_result = InstructionResult::FatalExternalError;
        return;
    };

    push!(interpreter, balance);
}

/// EIP-1884: Repricing for trie-size-dependent opcodes
pub fn self_mna_balances<H: Host + ?Sized, SPEC: Spec>(
    interpreter: &mut Interpreter,
    host: &mut H,
) {
    check!(interpreter, ISTANBUL);
    gas!(interpreter, gas::LOW);

    for asset_id in interpreter.asset_ids.iter() {
        // Get the balance of the contract for the asset_id
        let Some((balance, _)) = host.balance(*asset_id, interpreter.contract.address) else {
            interpreter.instruction_result = InstructionResult::FatalExternalError;
            return;
        };

        // Push balance and asset_id to the stack
        push!(interpreter, balance);
        push!(interpreter, *asset_id);
    }

    // Push the number of assets to the stack
    push!(interpreter, U256::from(interpreter.asset_ids.len() as u64));
}

pub fn extcodesize<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    pop_address!(interpreter, address);
    let Some((code, is_cold)) = host.code(address) else {
        interpreter.instruction_result = InstructionResult::FatalExternalError;
        return;
    };
    if SPEC::enabled(BERLIN) {
        gas!(
            interpreter,
            if is_cold {
                COLD_ACCOUNT_ACCESS_COST
            } else {
                WARM_STORAGE_READ_COST
            }
        );
    } else if SPEC::enabled(TANGERINE) {
        gas!(interpreter, 700);
    } else {
        gas!(interpreter, 20);
    }

    push!(interpreter, U256::from(code.len()));
}

/// EIP-1052: EXTCODEHASH opcode
pub fn extcodehash<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    check!(interpreter, CONSTANTINOPLE);
    pop_address!(interpreter, address);
    let Some((code_hash, is_cold)) = host.code_hash(address) else {
        interpreter.instruction_result = InstructionResult::FatalExternalError;
        return;
    };
    if SPEC::enabled(BERLIN) {
        gas!(
            interpreter,
            if is_cold {
                COLD_ACCOUNT_ACCESS_COST
            } else {
                WARM_STORAGE_READ_COST
            }
        );
    } else if SPEC::enabled(ISTANBUL) {
        gas!(interpreter, 700);
    } else {
        gas!(interpreter, 400);
    }
    push_b256!(interpreter, code_hash);
}

pub fn extcodecopy<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    pop_address!(interpreter, address);
    pop!(interpreter, memory_offset, code_offset, len_u256);

    let Some((code, is_cold)) = host.code(address) else {
        interpreter.instruction_result = InstructionResult::FatalExternalError;
        return;
    };

    let len = as_usize_or_fail!(interpreter, len_u256);
    gas_or_fail!(
        interpreter,
        gas::extcodecopy_cost(SPEC::SPEC_ID, len as u64, is_cold)
    );
    if len == 0 {
        return;
    }
    let memory_offset = as_usize_or_fail!(interpreter, memory_offset);
    let code_offset = min(as_usize_saturated!(code_offset), code.len());
    resize_memory!(interpreter, memory_offset, len);

    // Note: this can't panic because we resized memory to fit.
    interpreter
        .shared_memory
        .set_data(memory_offset, code_offset, len, code.bytes());
}

pub fn blockhash<H: Host + ?Sized>(interpreter: &mut Interpreter, host: &mut H) {
    gas!(interpreter, gas::BLOCKHASH);
    pop_top!(interpreter, number);

    if let Some(diff) = host.env().block.number.checked_sub(*number) {
        let diff = as_usize_saturated!(diff);
        // blockhash should push zero if number is same as current block number.
        if diff <= BLOCK_HASH_HISTORY && diff != 0 {
            let Some(hash) = host.block_hash(*number) else {
                interpreter.instruction_result = InstructionResult::FatalExternalError;
                return;
            };
            *number = U256::from_be_bytes(hash.0);
            return;
        }
    }
    *number = U256::ZERO;
}

pub fn sload<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    pop_top!(interpreter, index);

    let Some((value, is_cold)) = host.sload(interpreter.contract.address, *index) else {
        interpreter.instruction_result = InstructionResult::FatalExternalError;
        return;
    };
    gas!(interpreter, gas::sload_cost(SPEC::SPEC_ID, is_cold));
    *index = value;
}

pub fn sstore<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    check_staticcall!(interpreter);

    pop!(interpreter, index, value);
    let Some(SStoreResult {
        original_value: original,
        present_value: old,
        new_value: new,
        is_cold,
    }) = host.sstore(interpreter.contract.address, index, value)
    else {
        interpreter.instruction_result = InstructionResult::FatalExternalError;
        return;
    };
    gas_or_fail!(interpreter, {
        let remaining_gas = interpreter.gas.remaining();
        gas::sstore_cost(SPEC::SPEC_ID, original, old, new, remaining_gas, is_cold)
    });
    refund!(
        interpreter,
        gas::sstore_refund(SPEC::SPEC_ID, original, old, new)
    );
}

/// EIP-1153: Transient storage opcodes
/// Store value to transient storage
pub fn tstore<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    check!(interpreter, CANCUN);
    check_staticcall!(interpreter);
    gas!(interpreter, gas::WARM_STORAGE_READ_COST);

    pop!(interpreter, index, value);

    host.tstore(interpreter.contract.address, index, value);
}

/// EIP-1153: Transient storage opcodes
/// Load value from transient storage
pub fn tload<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    check!(interpreter, CANCUN);
    gas!(interpreter, gas::WARM_STORAGE_READ_COST);

    pop_top!(interpreter, index);

    *index = host.tload(interpreter.contract.address, *index);
}

pub fn log<const N: usize, H: Host + ?Sized>(interpreter: &mut Interpreter, host: &mut H) {
    check_staticcall!(interpreter);

    pop!(interpreter, offset, len);
    let len = as_usize_or_fail!(interpreter, len);
    gas_or_fail!(interpreter, gas::log_cost(N as u8, len as u64));
    let data = if len == 0 {
        Bytes::new()
    } else {
        let offset = as_usize_or_fail!(interpreter, offset);
        resize_memory!(interpreter, offset, len);
        Bytes::copy_from_slice(interpreter.shared_memory.slice(offset, len))
    };

    if interpreter.stack.len() < N {
        interpreter.instruction_result = InstructionResult::StackUnderflow;
        return;
    }

    let mut topics = Vec::with_capacity(N);
    for _ in 0..N {
        // SAFETY: stack bounds already checked few lines above
        topics.push(B256::from(unsafe { interpreter.stack.pop_unsafe() }));
    }

    let log = Log {
        address: interpreter.contract.address,
        data: LogData::new(topics, data).expect("LogData should have <=4 topics"),
    };

    host.log(log);
}

fn pop_transferred_assets(interpreter: &mut Interpreter, transferred_assets: &mut Vec<Asset>) {
    pop!(interpreter, nr_of_transferred_assets);
    let nr_of_transferred_assets = as_usize_or_fail!(interpreter, nr_of_transferred_assets);

    for _ in 0..nr_of_transferred_assets {
        pop!(interpreter, asset_id, value);
        transferred_assets.push(Asset {
            id: asset_id,
            amount: value,
        });
    }
}

pub fn create<const IS_CREATE2: bool, H: Host + ?Sized, SPEC: Spec>(
    interpreter: &mut Interpreter,
    host: &mut H,
) {
    create_inner::<IS_CREATE2, H, SPEC>(interpreter, host, false);
}

pub fn mna_create<const IS_CREATE2: bool, H: Host + ?Sized, SPEC: Spec>(
    interpreter: &mut Interpreter,
    host: &mut H,
) {
    create_inner::<IS_CREATE2, H, SPEC>(interpreter, host, true);
}

fn create_inner<const IS_CREATE2: bool, H: Host + ?Sized, SPEC: Spec>(
    interpreter: &mut Interpreter,
    host: &mut H,
    is_mna_create: bool,
) {
    // Dev: deploying smart contracts is not allowed for general public
    // TODO: implement a way to allow deploying smart contracts by Sablier
    interpreter.instruction_result = InstructionResult::NotActivated;

    check_staticcall!(interpreter);

    // EIP-1014: Skinny CREATE2
    if IS_CREATE2 {
        check!(interpreter, PETERSBURG);
    }

    let mut transferred_assets = Vec::<Asset>::new();

    if is_mna_create {
        pop_transferred_assets(interpreter, transferred_assets.as_mut());
    } else {
        pop!(interpreter, value);
        if value != U256::ZERO {
            transferred_assets.push(Asset {
                id: BASE_ASSET_ID,
                amount: value,
            });
        }
    }

    pop!(interpreter, code_offset, len);
    let len = as_usize_or_fail!(interpreter, len);

    let mut code = Bytes::new();
    if len != 0 {
        // EIP-3860: Limit and meter initcode
        if SPEC::enabled(SHANGHAI) {
            // Limit is set as double of max contract bytecode size
            let max_initcode_size = host
                .env()
                .cfg
                .limit_contract_code_size
                .map(|limit| limit.saturating_mul(2))
                .unwrap_or(MAX_INITCODE_SIZE);
            if len > max_initcode_size {
                interpreter.instruction_result = InstructionResult::CreateInitCodeSizeLimit;
                return;
            }
            gas!(interpreter, gas::initcode_cost(len as u64));
        }

        let code_offset = as_usize_or_fail!(interpreter, code_offset);
        resize_memory!(interpreter, code_offset, len);
        code = Bytes::copy_from_slice(interpreter.shared_memory.slice(code_offset, len));
    }

    // EIP-1014: Skinny CREATE2
    let scheme = if IS_CREATE2 {
        pop!(interpreter, salt);
        gas_or_fail!(interpreter, gas::create2_cost(len as u64));
        CreateScheme::Create2 { salt }
    } else {
        gas!(interpreter, gas::CREATE);
        CreateScheme::Create
    };

    let mut gas_limit = interpreter.gas().remaining();

    // EIP-150: Gas cost changes for IO-heavy operations
    if SPEC::enabled(TANGERINE) {
        // take remaining gas and deduce l64 part of it.
        gas_limit -= gas_limit / 64
    }
    gas!(interpreter, gas_limit);

    // Call host to interact with target contract
    interpreter.next_action = InterpreterAction::Create {
        inputs: Box::new(CreateInputs {
            caller: interpreter.contract.address,
            scheme,
            transferred_assets,
            init_code: code,
            gas_limit,
        }),
    };
    interpreter.instruction_result = InstructionResult::CallOrCreate;
}

pub fn call<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    call_inner::<H, SPEC>(interpreter, host, false);
}

pub fn mna_call<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    call_inner::<H, SPEC>(interpreter, host, true);
}

fn call_inner<H: Host + ?Sized, SPEC: Spec>(
    interpreter: &mut Interpreter,
    host: &mut H,
    is_mna_call: bool,
) {
    pop!(interpreter, local_gas_limit);
    pop_address!(interpreter, to);
    // max gas limit is not possible in real ethereum situation.
    let local_gas_limit = u64::try_from(local_gas_limit).unwrap_or(u64::MAX);

    let mut transferred_assets = Vec::<Asset>::new();

    if is_mna_call {
        pop_transferred_assets(interpreter, transferred_assets.as_mut());
    } else {
        pop!(interpreter, value);
        if value != U256::ZERO {
            transferred_assets.push(Asset {
                id: BASE_ASSET_ID,
                amount: value,
            });
        }
    }

    if interpreter.is_static && !transferred_assets.is_empty() {
        interpreter.instruction_result = InstructionResult::CallNotAllowedInsideStatic;
        return;
    }

    let Some((input, return_memory_offset)) = get_memory_input_and_out_ranges(interpreter) else {
        return;
    };

    let Some(mut gas_limit) = calc_call_gas::<H, SPEC>(
        interpreter,
        host,
        to,
        !transferred_assets.is_empty(),
        local_gas_limit,
        true,
        true,
    ) else {
        return;
    };

    gas!(interpreter, gas_limit);

    // add call stipend if there is value to be transferred.
    if !transferred_assets.is_empty() {
        gas_limit = gas_limit.saturating_add(gas::CALL_STIPEND);
    }

    // Call host to interact with target contract
    interpreter.next_action = InterpreterAction::Call {
        inputs: Box::new(CallInputs {
            contract: to,
            transfer: Transfer {
                source: interpreter.contract.address,
                target: to,
                assets: transferred_assets.clone(),
            },
            input,
            gas_limit,
            context: CallContext {
                address: to,
                caller: interpreter.contract.address,
                code_address: to,
                apparent_assets: transferred_assets.clone(),
                scheme: CallScheme::Call,
            },
            is_static: interpreter.is_static,
            return_memory_offset,
        }),
    };
    interpreter.instruction_result = InstructionResult::CallOrCreate;
}

pub fn call_code<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    call_code_inner::<H, SPEC>(interpreter, host, false);
}

pub fn mna_call_code<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    call_code_inner::<H, SPEC>(interpreter, host, true);
}

fn call_code_inner<H: Host + ?Sized, SPEC: Spec>(
    interpreter: &mut Interpreter,
    host: &mut H,
    is_mna_call_code: bool,
) {
    pop!(interpreter, local_gas_limit);
    pop_address!(interpreter, to);
    // max gas limit is not possible in real ethereum situation.
    let local_gas_limit = u64::try_from(local_gas_limit).unwrap_or(u64::MAX);

    let mut transferred_assets = Vec::<Asset>::new();

    if is_mna_call_code {
        pop_transferred_assets(interpreter, transferred_assets.as_mut());
    } else {
        pop!(interpreter, value);
        if value != U256::ZERO {
            transferred_assets.push(Asset {
                id: BASE_ASSET_ID,
                amount: value,
            });
        }
    }

    let Some((input, return_memory_offset)) = get_memory_input_and_out_ranges(interpreter) else {
        return;
    };

    let Some(mut gas_limit) = calc_call_gas::<H, SPEC>(
        interpreter,
        host,
        to,
        !transferred_assets.is_empty(),
        local_gas_limit,
        true,
        false,
    ) else {
        return;
    };

    gas!(interpreter, gas_limit);

    // add call stipend if there is value to be transferred.
    if !transferred_assets.is_empty() {
        gas_limit = gas_limit.saturating_add(gas::CALL_STIPEND);
    }

    // Call host to interact with target contract
    interpreter.next_action = InterpreterAction::Call {
        inputs: Box::new(CallInputs {
            contract: to,
            transfer: Transfer {
                source: interpreter.contract.address,
                target: interpreter.contract.address,
                assets: transferred_assets.clone(),
            },
            input,
            gas_limit,
            context: CallContext {
                address: interpreter.contract.address,
                caller: interpreter.contract.address,
                code_address: to,
                apparent_assets: transferred_assets.clone(),
                scheme: CallScheme::CallCode,
            },
            is_static: interpreter.is_static,
            return_memory_offset,
        }),
    };
    interpreter.instruction_result = InstructionResult::CallOrCreate;
}

pub fn delegate_call<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    check!(interpreter, HOMESTEAD);
    pop!(interpreter, local_gas_limit);
    pop_address!(interpreter, to);
    // max gas limit is not possible in real ethereum situation.
    let local_gas_limit = u64::try_from(local_gas_limit).unwrap_or(u64::MAX);

    let Some((input, return_memory_offset)) = get_memory_input_and_out_ranges(interpreter) else {
        return;
    };

    let Some(gas_limit) =
        calc_call_gas::<H, SPEC>(interpreter, host, to, false, local_gas_limit, false, false)
    else {
        return;
    };

    gas!(interpreter, gas_limit);

    // Call host to interact with target contract
    interpreter.next_action = InterpreterAction::Call {
        inputs: Box::new(CallInputs {
            contract: to,
            // This is dummy send for StaticCall and DelegateCall,
            // it should do nothing and not touch anything.
            transfer: Transfer {
                source: interpreter.contract.address,
                target: interpreter.contract.address,
                assets: Vec::new(),
            },
            input,
            gas_limit,
            context: CallContext {
                address: interpreter.contract.address,
                caller: interpreter.contract.caller,
                code_address: to,
                apparent_assets: interpreter.contract.call_assets.clone(),
                scheme: CallScheme::DelegateCall,
            },
            is_static: interpreter.is_static,
            return_memory_offset,
        }),
    };
    interpreter.instruction_result = InstructionResult::CallOrCreate;
}

pub fn static_call<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    check!(interpreter, BYZANTIUM);
    pop!(interpreter, local_gas_limit);
    pop_address!(interpreter, to);
    // max gas limit is not possible in real ethereum situation.
    let local_gas_limit = u64::try_from(local_gas_limit).unwrap_or(u64::MAX);

    let Some((input, return_memory_offset)) = get_memory_input_and_out_ranges(interpreter) else {
        return;
    };

    let Some(gas_limit) =
        calc_call_gas::<H, SPEC>(interpreter, host, to, false, local_gas_limit, false, true)
    else {
        return;
    };
    gas!(interpreter, gas_limit);

    // Call host to interact with target contract
    interpreter.next_action = InterpreterAction::Call {
        inputs: Box::new(CallInputs {
            contract: to,
            // This is dummy send for StaticCall and DelegateCall,
            // it should do nothing and not touch anything.
            transfer: Transfer {
                source: interpreter.contract.address,
                target: interpreter.contract.address,
                assets: Vec::new(),
            },
            input,
            gas_limit,
            context: CallContext {
                address: to,
                caller: interpreter.contract.address,
                code_address: to,
                apparent_assets: Vec::new(),
                scheme: CallScheme::StaticCall,
            },
            is_static: true,
            return_memory_offset,
        }),
    };
    interpreter.instruction_result = InstructionResult::CallOrCreate;
}

pub fn balance<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    pop_address!(interpreter, address);
    push!(interpreter, BASE_ASSET_ID);
    push_b256!(interpreter, address.into_word());

    mna_balance::<H, SPEC>(interpreter, host);
}

pub fn mna_balance<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    pop_address!(interpreter, address);
    pop!(interpreter, asset_id);

    let Some((balance, is_cold)) = host.balance(asset_id, address) else {
        interpreter.instruction_result = InstructionResult::FatalExternalError;
        return;
    };

    gas!(
        interpreter,
        if SPEC::enabled(ISTANBUL) {
            // EIP-1884: Repricing for trie-size-dependent opcodes
            gas::account_access_gas(SPEC::SPEC_ID, is_cold)
        } else if SPEC::enabled(TANGERINE) {
            400
        } else {
            20
        }
    );

    push!(interpreter, balance);
}

pub fn mint<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    // TODO: implement minting allowance just for Sablier
    // Only allow minting for contracts (not EOAs)
    if host.is_tx_sender_eoa() {
        interpreter.instruction_result = InstructionResult::UnauthorizedCaller;
        return;
    }

    pop!(interpreter, sub_id, amount);
    if !host.mint(interpreter.contract.address, sub_id, amount) {
        interpreter.instruction_result = InstructionResult::FatalExternalError;
        return;
    };

    gas_or_fail!(interpreter, { gas::mint_cost() });
}

pub fn burn<H: Host + ?Sized, SPEC: Spec>(interpreter: &mut Interpreter, host: &mut H) {
    // TODO: implement burning allowance just for Sablier
    // Only allow burning for contracts (not EOAs)
    if host.is_tx_sender_eoa() {
        interpreter.instruction_result = InstructionResult::UnauthorizedCaller;
        return;
    }

    pop!(interpreter, sub_id, amount);
    if !host.burn(interpreter.contract.address, sub_id, amount) {
        interpreter.instruction_result = InstructionResult::FatalExternalError;
        return;
    };

    gas_or_fail!(interpreter, { gas::burn_cost() });
}
