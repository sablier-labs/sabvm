//! Handler related to Optimism chain

use super::mainnet;
use crate::{
    interpreter::{return_ok, return_revert, Gas, InstructionResult},
    optimism,
    primitives::{
        db::Database, Account, EVMError, Env, ExecutionResult, HaltReason, HashMap,
        InvalidTransactionReason, Output, ResultAndState, Spec, SpecId::REGOLITH, U256,
    },
    EvmContext,
};
use core::ops::Mul;

/// Handle output of the transaction
#[inline]
pub fn handle_call_return<SPEC: Spec>(
    env: &Env,
    call_result: InstructionResult,
    returned_gas: Gas,
) -> Gas {
    let is_deposit = env.tx.optimism.source_hash.is_some();
    let is_optimism = env.cfg.optimism;
    let tx_system = env.tx.optimism.is_system_transaction;
    let tx_gas_limit = env.tx.gas_limit;
    let is_regolith = SPEC::enabled(REGOLITH);
    // Spend the gas limit. Gas is reimbursed when the tx returns successfully.
    let mut gas = Gas::new(tx_gas_limit);
    gas.record_cost(tx_gas_limit);

    match call_result {
        return_ok!() => {
            // On Optimism, deposit transactions report gas usage uniquely to other
            // transactions due to them being pre-paid on L1.
            //
            // Hardfork Behavior:
            // - Bedrock (success path):
            //   - Deposit transactions (non-system) report their gas limit as the usage.
            //     No refunds.
            //   - Deposit transactions (system) report 0 gas used. No refunds.
            //   - Regular transactions report gas usage as normal.
            // - Regolith (success path):
            //   - Deposit transactions (all) report their gas used as normal. Refunds
            //     enabled.
            //   - Regular transactions report their gas used as normal.
            if is_optimism && (!is_deposit || is_regolith) {
                // For regular transactions prior to Regolith and all transactions after
                // Regolith, gas is reported as normal.
                gas.erase_cost(returned_gas.remaining());
                gas.record_refund(returned_gas.refunded());
            } else if is_deposit && tx_system.unwrap_or(false) {
                // System transactions were a special type of deposit transaction in
                // the Bedrock hardfork that did not incur any gas costs.
                gas.erase_cost(tx_gas_limit);
            }
        }
        return_revert!() => {
            // On Optimism, deposit transactions report gas usage uniquely to other
            // transactions due to them being pre-paid on L1.
            //
            // Hardfork Behavior:
            // - Bedrock (revert path):
            //   - Deposit transactions (all) report the gas limit as the amount of gas
            //     used on failure. No refunds.
            //   - Regular transactions receive a refund on remaining gas as normal.
            // - Regolith (revert path):
            //   - Deposit transactions (all) report the actual gas used as the amount of
            //     gas used on failure. Refunds on remaining gas enabled.
            //   - Regular transactions receive a refund on remaining gas as normal.
            if is_optimism && (!is_deposit || is_regolith) {
                gas.erase_cost(returned_gas.remaining());
            }
        }
        _ => {}
    }
    gas
}

#[inline]
pub fn calculate_gas_refund<SPEC: Spec>(env: &Env, gas: &Gas) -> u64 {
    let is_deposit = env.cfg.optimism && env.tx.optimism.source_hash.is_some();

    // Prior to Regolith, deposit transactions did not receive gas refunds.
    let is_gas_refund_disabled = env.cfg.optimism && is_deposit && !SPEC::enabled(REGOLITH);
    if is_gas_refund_disabled {
        0
    } else {
        mainnet::calculate_gas_refund::<SPEC>(env, gas)
    }
}

/// Reward beneficiary with gas fee.
#[inline]
pub fn reward_beneficiary<SPEC: Spec, DB: Database>(
    context: &mut EvmContext<'_, DB>,
    gas: &Gas,
) -> Result<(), EVMError<DB::Error>> {
    let is_deposit = context.env.cfg.optimism && context.env.tx.optimism.source_hash.is_some();
    let disable_coinbase_tip = context.env.cfg.optimism && is_deposit;

    // transfer fee to coinbase/beneficiary.
    if !disable_coinbase_tip {
        mainnet::reward_beneficiary::<SPEC, DB>(context, gas)?;
    }

    if context.env.cfg.optimism && !is_deposit {
        // If the transaction is not a deposit transaction, fees are paid out
        // to both the Base Fee Vault as well as the L1 Fee Vault.
        let Some(l1_block_info) = context.l1_block_info.clone() else {
            panic!("[OPTIMISM] Failed to load L1 block information.");
        };

        let Some(enveloped_tx) = &context.env.tx.optimism.enveloped_tx else {
            panic!("[OPTIMISM] Failed to load enveloped transaction.");
        };

        let l1_cost = l1_block_info.calculate_tx_l1_cost::<SPEC>(enveloped_tx);

        // Send the L1 cost of the transaction to the L1 Fee Vault.
        let Ok((l1_fee_vault_account, _)) = context
            .journaled_state
            .load_account(optimism::L1_FEE_RECIPIENT, context.db)
        else {
            panic!("[OPTIMISM] Failed to load L1 Fee Vault account");
        };
        l1_fee_vault_account.mark_touch();
        l1_fee_vault_account.info.increase_base_balance(l1_cost);

        // Send the base fee of the transaction to the Base Fee Vault.
        let Ok((base_fee_vault_account, _)) = context
            .journaled_state
            .load_account(optimism::BASE_FEE_RECIPIENT, context.db)
        else {
            panic!("[OPTIMISM] Failed to load Base Fee Vault account");
        };
        base_fee_vault_account.mark_touch();
        base_fee_vault_account.info.increase_base_balance(
            context
                .env
                .block
                .basefee
                .mul(U256::from(gas.spend() - gas.refunded() as u64)),
        );
    }
    Ok(())
}

/// Main return handle, returns the output of the transaction.
#[inline]
pub fn main_return<SPEC: Spec, DB: Database>(
    context: &mut EvmContext<'_, DB>,
    call_result: InstructionResult,
    output: Output,
    gas: &Gas,
) -> Result<ResultAndState, EVMError<DB::Error>> {
    let result = mainnet::main_return::<DB>(context, call_result, output, gas)?;

    if result.result.is_halt() {
        // Post-regolith, if the transaction is a deposit transaction and it haults,
        // we bubble up to the global return handler. The mint value will be persisted
        // and the caller nonce will be incremented there.
        let is_deposit = context.env.tx.optimism.source_hash.is_some();
        let optimism_regolith = context.env.cfg.optimism && SPEC::enabled(REGOLITH);
        if is_deposit && optimism_regolith {
            return Err(EVMError::Transaction(
                InvalidTransactionReason::HaltedDepositPostRegolith,
            ));
        }
    }
    Ok(result)
}
/// Optimism end handle changes output if the transaction is a deposit transaction.
/// Deposit transaction can't be reverted and is always successful.
#[inline]
pub fn end_handle<SPEC: Spec, DB: Database>(
    context: &mut EvmContext<'_, DB>,
    evm_output: Result<ResultAndState, EVMError<DB::Error>>,
) -> Result<ResultAndState, EVMError<DB::Error>> {
    evm_output.or_else(|err| {
        if matches!(err, EVMError::Transaction(_))
            && context.env().cfg.optimism
            && context.env().tx.optimism.source_hash.is_some()
        {
            // If the transaction is a deposit transaction and it failed
            // for any reason, the caller nonce must be bumped, and the
            // gas reported must be altered depending on the Hardfork. This is
            // also returned as a special Halt variant so that consumers can more
            // easily distinguish between a failed deposit and a failed
            // normal transaction.
            let caller = context.env().tx.caller;

            // Increment sender nonce and account balance for the mint amount. Deposits
            // always persist the mint amount, even if the transaction fails.
            let account = {
                let mut acc = Account::from(
                    context
                        .db
                        .basic(caller)
                        .unwrap_or_default()
                        .unwrap_or_default(),
                );
                acc.info.nonce = acc.info.nonce.saturating_add(1);
                acc.info.set_base_balance(
                    acc.info
                        .get_base_balance()
                        .saturating_add(U256::from(context.env().tx.optimism.mint.unwrap_or(0))),
                );
                acc.mark_touch();
                acc
            };
            let state = HashMap::from([(caller, account)]);

            // The gas used of a failed deposit post-regolith is the gas
            // limit of the transaction. pre-regolith, it is the gas limit
            // of the transaction for non system transactions and 0 for system
            // transactions.
            let is_system_tx = context
                .env()
                .tx
                .optimism
                .is_system_transaction
                .unwrap_or(false);
            let gas_used = if SPEC::enabled(REGOLITH) || !is_system_tx {
                context.env().tx.gas_limit
            } else {
                0
            };

            Ok(ResultAndState {
                result: ExecutionResult::Halt {
                    reason: HaltReason::FailedDeposit,
                    gas_used,
                },
                state,
            })
        } else {
            Err(err)
        }
    })
}

#[cfg(test)]
mod tests {
    use crate::primitives::{BedrockSpec, RegolithSpec, B256};

    use super::*;

    #[test]
    fn test_revert_gas() {
        let mut env = Env::default();
        env.tx.gas_limit = 100;
        env.cfg.optimism = true;
        env.tx.optimism.source_hash = None;

        let gas = handle_call_return::<BedrockSpec>(&env, InstructionResult::Revert, Gas::new(90));
        assert_eq!(gas.remaining(), 90);
        assert_eq!(gas.spend(), 10);
        assert_eq!(gas.refunded(), 0);
    }

    #[test]
    fn test_revert_gas_non_optimism() {
        let mut env = Env::default();
        env.tx.gas_limit = 100;
        env.cfg.optimism = false;
        env.tx.optimism.source_hash = None;

        let gas = handle_call_return::<BedrockSpec>(&env, InstructionResult::Revert, Gas::new(90));
        // else branch takes all gas.
        assert_eq!(gas.remaining(), 0);
        assert_eq!(gas.spend(), 100);
        assert_eq!(gas.refunded(), 0);
    }

    #[test]
    fn test_consume_gas() {
        let mut env = Env::default();
        env.tx.gas_limit = 100;
        env.cfg.optimism = true;
        env.tx.optimism.source_hash = Some(B256::ZERO);

        let gas = handle_call_return::<RegolithSpec>(&env, InstructionResult::Stop, Gas::new(90));
        assert_eq!(gas.remaining(), 90);
        assert_eq!(gas.spend(), 10);
        assert_eq!(gas.refunded(), 0);
    }

    #[test]
    fn test_consume_gas_with_refund() {
        let mut env = Env::default();
        env.tx.gas_limit = 100;
        env.cfg.optimism = true;
        env.tx.optimism.source_hash = Some(B256::ZERO);

        let mut ret_gas = Gas::new(90);
        ret_gas.record_refund(20);

        let gas = handle_call_return::<RegolithSpec>(&env, InstructionResult::Stop, ret_gas);
        assert_eq!(gas.remaining(), 90);
        assert_eq!(gas.spend(), 10);
        assert_eq!(gas.refunded(), 20);

        let gas = handle_call_return::<RegolithSpec>(&env, InstructionResult::Revert, ret_gas);
        assert_eq!(gas.remaining(), 90);
        assert_eq!(gas.spend(), 10);
        assert_eq!(gas.refunded(), 0);
    }

    #[test]
    fn test_consume_gas_sys_deposit_tx() {
        let mut env = Env::default();
        env.tx.gas_limit = 100;
        env.cfg.optimism = true;
        env.tx.optimism.source_hash = Some(B256::ZERO);

        let gas = handle_call_return::<BedrockSpec>(&env, InstructionResult::Stop, Gas::new(90));
        assert_eq!(gas.remaining(), 0);
        assert_eq!(gas.spend(), 100);
        assert_eq!(gas.refunded(), 0);
    }
}
