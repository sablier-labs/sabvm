use super::inner_evm_context::InnerEvmContext;
use crate::{
    db::Database,
    interpreter::{
        interpreter::{CallInfo, ResultOrNewCall as InterpreterResultOrNewCallInfo},
        return_ok, CallInputs, CallValues, Contract, Gas, InstructionResult, Interpreter,
        InterpreterResult,
    },
    primitives::{
        Bytes, EVMError, Env, HashSet, ResultOrNewCall as PrecompileResultOrNewCallInfo, U256,
    },
    ContextPrecompiles, FrameOrResult, CALL_STACK_LIMIT,
};

use core::{
    fmt,
    ops::{Deref, DerefMut},
};
use std::boxed::Box;

/// EVM context that contains the inner EVM context and precompiles.
pub struct EvmContext<DB: Database> {
    /// Inner EVM context.
    pub inner: InnerEvmContext<DB>,
    /// Precompiles that are available for evm.
    pub precompiles: ContextPrecompiles<DB>,
}

impl<DB: Database + Clone> Clone for EvmContext<DB>
where
    DB::Error: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            precompiles: ContextPrecompiles::default(),
        }
    }
}

impl<DB> fmt::Debug for EvmContext<DB>
where
    DB: Database + fmt::Debug,
    DB::Error: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvmContext")
            .field("inner", &self.inner)
            .field("precompiles", &self.inner)
            .finish_non_exhaustive()
    }
}

impl<DB: Database> Deref for EvmContext<DB> {
    type Target = InnerEvmContext<DB>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<DB: Database> DerefMut for EvmContext<DB> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<DB: Database> EvmContext<DB> {
    /// Create new context with database.
    pub fn new(db: DB) -> Self {
        Self {
            inner: InnerEvmContext::new(db),
            precompiles: ContextPrecompiles::default(),
        }
    }

    /// Creates a new context with the given environment and database.
    #[inline]
    pub fn new_with_env(db: DB, env: Box<Env>) -> Self {
        Self {
            inner: InnerEvmContext::new_with_env(db, env),
            precompiles: ContextPrecompiles::default(),
        }
    }

    /// Sets the database.
    ///
    /// Note that this will ignore the previous `error` if set.
    #[inline]
    pub fn with_db<ODB: Database>(self, db: ODB) -> EvmContext<ODB> {
        EvmContext {
            inner: self.inner.with_db(db),
            precompiles: ContextPrecompiles::default(),
        }
    }

    /// Sets precompiles
    #[inline]
    pub fn set_precompiles(&mut self, precompiles: ContextPrecompiles<DB>) {
        // set warm loaded addresses.
        self.journaled_state.warm_preloaded_addresses =
            precompiles.addresses().copied().collect::<HashSet<_>>();
        self.precompiles = precompiles;
    }

    /// Call precompile contract
    #[inline]
    fn call_precompile(
        &mut self,
        inputs: &CallInputs,
        gas: Gas,
    ) -> Option<InterpreterResultOrNewCallInfo> {
        let out = self
            .precompiles
            .call(inputs, gas.limit(), &mut self.inner)?;

        let mut result = InterpreterResult {
            result: InstructionResult::Return,
            gas,
            output: Bytes::new(),
        };

        match out {
            Ok(call_or_result_info) => match call_or_result_info {
                PrecompileResultOrNewCallInfo::Call(primitive_call_info) => {
                    return Some(InterpreterResultOrNewCallInfo::NewCall(CallInfo {
                        target_address: primitive_call_info.target_address,
                        input_data: primitive_call_info.input_data,
                        call_values: CallValues::Transfer(primitive_call_info.token_transfers),
                    }));
                }
                PrecompileResultOrNewCallInfo::Result(interpreter_result) => {
                    let (gas_used, data) = (
                        interpreter_result.gas_used,
                        interpreter_result.returned_bytes,
                    );
                    if result.gas.record_cost(gas_used) {
                        result.result = InstructionResult::Return;
                        result.output = data;
                    } else {
                        result.result = InstructionResult::PrecompileOOG;
                    }
                }
            },
            Err(e) => {
                result.result = if e == crate::precompile::Error::OutOfGas {
                    InstructionResult::PrecompileOOG
                } else {
                    InstructionResult::PrecompileError
                };
            }
        }
        Some(InterpreterResultOrNewCallInfo::Result(result))
    }

    /// Make call frame
    #[inline]
    pub fn make_call_frame(
        &mut self,
        inputs: &CallInputs,
    ) -> Result<FrameOrResult, EVMError<DB::Error>> {
        let gas = Gas::new(inputs.gas_limit);

        let return_result = |instruction_result: InstructionResult| {
            Ok(FrameOrResult::new_call_result(
                InterpreterResult {
                    result: instruction_result,
                    gas,
                    output: Bytes::new(),
                },
                inputs.return_memory_offset.clone(),
            ))
        };

        // Check depth
        if self.journaled_state.depth() > CALL_STACK_LIMIT {
            return return_result(InstructionResult::CallTooDeep);
        }

        let (account, _) = self
            .inner
            .journaled_state
            .load_code(inputs.bytecode_address, &mut self.inner.db)?;
        let code_hash = account.info.code_hash();
        let bytecode = account.info.code.clone().unwrap_or_default();

        // Create subroutine checkpoint
        let checkpoint = self.journaled_state.checkpoint();

        // Touch address. For "EIP-158 State Clear", this will erase empty accounts.
        match &inputs.values {
            // if transfer value is zero, do the touch.
            CallValues::Transfer(values)
                if values.is_empty() || values.iter().all(|tt| tt.amount == U256::ZERO) =>
            {
                self.load_account(inputs.target_address)?;
                self.journaled_state.touch(&inputs.target_address);
            }
            CallValues::Transfer(values) => {
                // Transfer value from caller to called account
                if let Some(result) = self.inner.journaled_state.transfer(
                    &inputs.caller,
                    &inputs.target_address,
                    values,
                    &mut self.inner.db,
                )? {
                    self.journaled_state.checkpoint_revert(checkpoint);
                    return return_result(result);
                }
            }
            _ => {}
        };

        if let Some(result_or_call_info) = self.call_precompile(inputs, gas) {
            match result_or_call_info {
                InterpreterResultOrNewCallInfo::NewCall(call_info) => {
                    // Compose the new Call Frame to process
                    let (account, _) = self
                        .inner
                        .journaled_state
                        .load_code(call_info.target_address, &mut self.inner.db)?;
                    let code_hash = account.info.code_hash();
                    let bytecode = account.info.code.clone().unwrap_or_default();

                    let call_inputs = CallInputs {
                        input: call_info.input_data.clone(),
                        gas_limit: gas.limit(),
                        bytecode_address: call_info.target_address,
                        target_address: call_info.target_address,
                        caller: inputs.caller,
                        values: call_info.call_values,
                        scheme: revm_interpreter::CallScheme::Call,
                        is_eof: false,
                        is_static: inputs.is_static,
                        return_memory_offset: 0..0,
                    };

                    // Transfer value from caller to called account
                    if let Some(result) = self.inner.journaled_state.transfer(
                        &call_inputs.caller,
                        &call_inputs.target_address,
                        &call_inputs.values.get(),
                        &mut self.inner.db,
                    )? {
                        self.journaled_state.checkpoint_revert(checkpoint);
                        return return_result(result);
                    }

                    let contract = Contract::new_with_context(
                        call_info.input_data.clone(),
                        bytecode,
                        Some(code_hash),
                        &call_inputs,
                    );

                    // Create interpreter, execute the call and push new CallStackFrame.
                    Ok(FrameOrResult::new_call_frame(
                        call_inputs.return_memory_offset.clone(),
                        checkpoint,
                        Interpreter::new(contract, call_inputs.gas_limit, call_inputs.is_static),
                    ))
                }

                InterpreterResultOrNewCallInfo::Result(result) => {
                    if matches!(result.result, return_ok!()) {
                        self.journaled_state.checkpoint_commit();
                    } else {
                        self.journaled_state.checkpoint_revert(checkpoint);
                    }
                    Ok(FrameOrResult::new_call_result(
                        result,
                        inputs.return_memory_offset.clone(),
                    ))
                }
            }
        } else if !bytecode.is_empty() {
            let contract =
                Contract::new_with_context(inputs.input.clone(), bytecode, Some(code_hash), inputs);
            // Create interpreter and executes call and push new CallStackFrame.
            Ok(FrameOrResult::new_call_frame(
                inputs.return_memory_offset.clone(),
                checkpoint,
                Interpreter::new(contract, gas.limit(), inputs.is_static),
            ))
        } else {
            self.journaled_state.checkpoint_commit();
            return_result(InstructionResult::Stop)
        }
    }
}

/// Test utilities for the [`EvmContext`].
#[cfg(any(test, feature = "test-utils"))]
pub(crate) mod test_utils {
    use super::*;
    use crate::{
        db::{CacheDB, EmptyDB},
        journaled_state::JournaledState,
        primitives::{address, Address, SpecId, TokenBalances, B256},
    };

    /// Mock caller address.
    pub const MOCK_CALLER: Address = address!("0000000000000000000000000000000000000000");

    /// Creates `CallInputs` that calls a provided contract address from the mock caller.
    pub fn create_mock_call_inputs(to: Address) -> CallInputs {
        CallInputs {
            input: Bytes::new(),
            gas_limit: 0,
            bytecode_address: to,
            target_address: to,
            caller: MOCK_CALLER,
            values: CallValues::Transfer(Vec::new()),
            scheme: revm_interpreter::CallScheme::Call,
            is_eof: false,
            is_static: false,
            return_memory_offset: 0..0,
        }
    }

    /// Creates an evm context with a cache db backend.
    /// Additionally loads the mock caller account into the db,
    /// and sets the balances to the provided value.
    pub fn create_cache_db_evm_context_with_balances(
        env: Box<Env>,
        mut db: CacheDB<EmptyDB>,
        balances: TokenBalances,
    ) -> EvmContext<CacheDB<EmptyDB>> {
        db.insert_account_info(
            test_utils::MOCK_CALLER,
            crate::primitives::AccountInfo {
                nonce: 0,
                balances,
                code_hash: B256::default(),
                code: None,
            },
        );
        create_cache_db_evm_context(env, db)
    }

    /// Creates a cached db evm context.
    pub fn create_cache_db_evm_context(
        env: Box<Env>,
        db: CacheDB<EmptyDB>,
    ) -> EvmContext<CacheDB<EmptyDB>> {
        EvmContext {
            inner: InnerEvmContext {
                env,
                journaled_state: JournaledState::new(SpecId::CANCUN, HashSet::new()),
                db,
                error: Ok(()),
                #[cfg(feature = "optimism")]
                l1_block_info: None,
            },
            precompiles: ContextPrecompiles::default(),
        }
    }

    /// Returns a new `EvmContext` with an empty journaled state.
    pub fn create_empty_evm_context(env: Box<Env>, db: EmptyDB) -> EvmContext<EmptyDB> {
        EvmContext {
            inner: InnerEvmContext {
                env,
                journaled_state: JournaledState::new(SpecId::CANCUN, HashSet::new()),
                db,
                error: Ok(()),
                #[cfg(feature = "optimism")]
                l1_block_info: None,
            },
            precompiles: ContextPrecompiles::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{CacheDB, EmptyDB},
        primitives::{
            address, utilities::init_balances, AccountInfo, Bytecode, TokenTransfer, BASE_TOKEN_ID,
        },
        Frame, JournalEntry,
    };
    use std::boxed::Box;
    use test_utils::*;

    // Tests that the `EVMContext::make_call_frame` function returns an error if the
    // call stack is too deep.
    #[test]
    fn test_make_call_frame_stack_too_deep() {
        let env = Env::default();
        let db = EmptyDB::default();
        let mut context = test_utils::create_empty_evm_context(Box::new(env), db);
        context.journaled_state.depth = CALL_STACK_LIMIT as usize + 1;
        let contract = address!("dead10000000000000000000000000000001dead");
        let call_inputs = test_utils::create_mock_call_inputs(contract);
        let res = context.make_call_frame(&call_inputs);
        let Ok(FrameOrResult::Result(err)) = res else {
            panic!("Expected FrameOrResult::Result");
        };
        assert_eq!(
            err.interpreter_result().result,
            InstructionResult::CallTooDeep
        );
    }

    // Tests that the `EVMContext::make_call_frame` function returns an error if the
    // transfer fails on the journaled state. It also verifies that the revert was
    // checkpointed on the journaled state correctly.
    #[test]
    fn test_make_call_frame_transfer_revert() {
        let env = Env::default();
        let db = EmptyDB::default();
        let mut evm_context = test_utils::create_empty_evm_context(Box::new(env), db);
        let contract = address!("dead10000000000000000000000000000001dead");
        let mut call_inputs = test_utils::create_mock_call_inputs(contract);
        call_inputs.values = CallValues::Transfer(vec![
            (TokenTransfer {
                id: BASE_TOKEN_ID,
                amount: U256::from(1),
            }),
        ]);
        let res = evm_context.make_call_frame(&call_inputs);
        let Ok(FrameOrResult::Result(result)) = res else {
            panic!("Expected FrameOrResult::Result");
        };
        assert_eq!(
            result.interpreter_result().result,
            InstructionResult::OutOfFunds
        );
        let checkpointed = vec![vec![JournalEntry::AccountLoaded { address: contract }]];
        assert_eq!(evm_context.journaled_state.journal, checkpointed);
        assert_eq!(evm_context.journaled_state.depth, 0);
    }

    #[test]
    fn test_make_call_frame_missing_code_context() {
        let env = Env::default();
        let cdb = CacheDB::new(EmptyDB::default());
        let balances = init_balances(U256::from(3_000_000_000_u128));
        let mut context = create_cache_db_evm_context_with_balances(Box::new(env), cdb, balances);
        let contract = address!("dead10000000000000000000000000000001dead");
        let call_inputs = test_utils::create_mock_call_inputs(contract);
        let res = context.make_call_frame(&call_inputs);
        let Ok(FrameOrResult::Result(result)) = res else {
            panic!("Expected FrameOrResult::Result");
        };
        assert_eq!(result.interpreter_result().result, InstructionResult::Stop);
    }

    #[test]
    fn test_make_call_frame_succeeds() {
        let env = Env::default();
        let mut cdb = CacheDB::new(EmptyDB::default());
        let balances = init_balances(U256::from(3_000_000_000_u128));
        let by = Bytecode::new_raw(Bytes::from(vec![0x60, 0x00, 0x60, 0x00]));
        let contract = address!("dead10000000000000000000000000000001dead");
        cdb.insert_account_info(
            contract,
            AccountInfo {
                nonce: 0,
                balances: balances.clone(),
                code_hash: by.clone().hash_slow(),
                code: Some(by),
            },
        );
        let mut evm_context =
            create_cache_db_evm_context_with_balances(Box::new(env), cdb, balances);
        let call_inputs = test_utils::create_mock_call_inputs(contract);
        let res = evm_context.make_call_frame(&call_inputs);
        let Ok(FrameOrResult::Frame(Frame::Call(call_frame))) = res else {
            panic!("Expected FrameOrResult::Frame(Frame::Call(..))");
        };
        assert_eq!(call_frame.return_memory_range, 0..0,);
    }
}
