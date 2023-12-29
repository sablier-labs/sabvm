//! Mainnet related handlers.

use crate::{
    interpreter::{return_ok, return_revert, Gas, InstructionResult, SuccessOrHalt},
    primitives::{
        db::Database, EVMError, Env, ExecutionResult, Output, ResultAndState, Spec, SpecId::LONDON,
        U256,
    },
    EvmContext,
};

/// Handle output of the transaction
#[inline]
pub fn handle_call_return<SPEC: Spec>(
    env: &Env,
    call_result: InstructionResult,
    returned_gas: Gas,
) -> Gas {
    let tx_gas_limit = env.tx.gas_limit;
    // Spend the gas limit. Gas is reimbursed when the tx returns successfully.
    let mut gas = Gas::new(tx_gas_limit);
    gas.record_cost(tx_gas_limit);

    match call_result {
        return_ok!() => {
            gas.erase_cost(returned_gas.remaining());
            gas.record_refund(returned_gas.refunded());
        }
        return_revert!() => {
            gas.erase_cost(returned_gas.remaining());
        }
        _ => {}
    }
    gas
}

#[inline]
pub fn handle_reimburse_caller<SPEC: Spec, DB: Database>(
    context: &mut EvmContext<'_, DB>,
    gas: &Gas,
) -> Result<(), EVMError<DB::Error>> {
    let caller = context.env.tx.caller;
    let effective_gas_price = context.env.effective_gas_price();

    // return balance of not spend gas.
    let (caller_account, _) = context
        .journaled_state
        .load_account(caller, context.db)
        .map_err(EVMError::Database)?;

    let base_amount_to_reimburse = caller_account
        .info
        .get_base_balance()
        .saturating_add(effective_gas_price * U256::from(gas.remaining() + gas.refunded() as u64));

    caller_account
        .info
        .set_base_balance(base_amount_to_reimburse);

    Ok(())
}

/// Reward beneficiary with gas fee.
#[inline]
pub fn reward_beneficiary<SPEC: Spec, DB: Database>(
    context: &mut EvmContext<'_, DB>,
    gas: &Gas,
) -> Result<(), EVMError<DB::Error>> {
    let beneficiary = context.env.block.coinbase;
    let effective_gas_price = context.env.effective_gas_price();

    // transfer fee to coinbase/beneficiary.
    // EIP-1559 discard basefee for coinbase transfer. Basefee amount of gas is discarded.
    let coinbase_gas_price = if SPEC::enabled(LONDON) {
        effective_gas_price.saturating_sub(context.env.block.basefee)
    } else {
        effective_gas_price
    };

    let (coinbase_account, _) = context
        .journaled_state
        .load_account(beneficiary, context.db)
        .map_err(EVMError::Database)?;

    coinbase_account.mark_touch();
    let base_amount_to_reimburse = coinbase_account
        .info
        .get_base_balance()
        .saturating_add(coinbase_gas_price * U256::from(gas.spent() - gas.refunded() as u64));

    coinbase_account
        .info
        .set_base_balance(base_amount_to_reimburse);

    Ok(())
}

/// Calculate gas refund for transaction.
///
/// If config is set to disable gas refund, it will return 0.
///
/// If spec is set to london, it will decrease the maximum refund amount to 5th part of
/// gas spend. (Before london it was 2th part of gas spend)
#[inline]
pub fn calculate_gas_refund<SPEC: Spec>(env: &Env, gas: &Gas) -> u64 {
    if env.cfg.is_gas_refund_disabled() {
        0
    } else {
        // EIP-3529: Reduction in refunds
        let max_refund_quotient = if SPEC::enabled(LONDON) { 5 } else { 2 };
        (gas.refunded() as u64).min(gas.spent() / max_refund_quotient)
    }
}

//pub fn main_first_call

/// Main return handle, returns the output of the transaction.
#[inline]
pub fn main_return<DB: Database>(
    context: &mut EvmContext<'_, DB>,
    call_result: InstructionResult,
    output: Output,
    gas: &Gas,
) -> Result<ResultAndState, EVMError<DB::Error>> {
    // used gas with refund calculated.
    let gas_refunded = gas.refunded() as u64;
    let final_gas_used = gas.spent() - gas_refunded;

    // reset journal and return present state.
    let (state, logs) = context.journaled_state.finalize();

    let result = match call_result.into() {
        SuccessOrHalt::Success(reason) => ExecutionResult::Success {
            reason,
            gas_used: final_gas_used,
            gas_refunded,
            logs,
            output,
        },
        SuccessOrHalt::Revert => ExecutionResult::Revert {
            gas_used: final_gas_used,
            output: match output {
                Output::Call(return_value) => return_value,
                Output::Create(return_value, _) => return_value,
            },
        },
        SuccessOrHalt::Halt(reason) => ExecutionResult::Halt {
            reason,
            gas_used: final_gas_used,
        },
        SuccessOrHalt::FatalExternalError => {
            return Err(EVMError::Database(context.error.take().unwrap()));
        }
        // Only two internal return flags.
        SuccessOrHalt::InternalContinue | SuccessOrHalt::InternalCallOrCreate => {
            panic!("Internal return flags should remain internal {call_result:?}")
        }
    };

    Ok(ResultAndState { result, state })
}

/// Mainnet end handle does not change the output.
#[inline]
pub fn end_handle<DB: Database>(
    _context: &mut EvmContext<'_, DB>,
    evm_output: Result<ResultAndState, EVMError<DB::Error>>,
) -> Result<ResultAndState, EVMError<DB::Error>> {
    evm_output
}

#[cfg(test)]
mod tests {
    use revm_interpreter::primitives::CancunSpec;

    use super::*;

    #[test]
    fn test_consume_gas() {
        let mut env = Env::default();
        env.tx.gas_limit = 100;

        let gas = handle_call_return::<CancunSpec>(&env, InstructionResult::Stop, Gas::new(90));
        assert_eq!(gas.remaining(), 90);
        assert_eq!(gas.spent(), 10);
        assert_eq!(gas.refunded(), 0);
    }

    #[test]
    fn test_consume_gas_with_refund() {
        let mut env = Env::default();
        env.tx.gas_limit = 100;

        let mut return_gas = Gas::new(90);
        return_gas.record_refund(30);

        let gas = handle_call_return::<CancunSpec>(&env, InstructionResult::Stop, return_gas);
        assert_eq!(gas.remaining(), 90);
        assert_eq!(gas.spent(), 10);
        assert_eq!(gas.refunded(), 30);

        let gas = handle_call_return::<CancunSpec>(&env, InstructionResult::Revert, return_gas);
        assert_eq!(gas.remaining(), 90);
        assert_eq!(gas.spent(), 10);
        assert_eq!(gas.refunded(), 0);
    }

    #[test]
    fn test_revert_gas() {
        let mut env = Env::default();
        env.tx.gas_limit = 100;

        let gas = handle_call_return::<CancunSpec>(&env, InstructionResult::Revert, Gas::new(90));
        assert_eq!(gas.remaining(), 90);
        assert_eq!(gas.spent(), 10);
        assert_eq!(gas.refunded(), 0);
    }
}
