use core::ops::Range;

use crate::{
    interpreter::{CallInputs, CreateInputs, Interpreter},
    primitives::{db::Database, Address, Bytes, B256},
    EvmContext,
};
use auto_impl::auto_impl;

#[cfg(feature = "std")]
mod customprinter;
#[cfg(all(feature = "std", feature = "serde"))]
mod eip3155;
mod gas;
mod instruction;
mod noop;

pub use instruction::inspector_instruction;
use revm_interpreter::InterpreterResult;
/// [Inspector] implementations.
pub mod inspectors {
    #[cfg(feature = "std")]
    pub use super::customprinter::CustomPrintTracer;
    #[cfg(all(feature = "std", feature = "serde"))]
    pub use super::eip3155::TracerEip3155;
    pub use super::gas::GasInspector;
    pub use super::noop::NoOpInspector;
}

/// EVM [Interpreter] callbacks.
#[auto_impl(&mut, Box)]
pub trait Inspector<DB: Database> {
    /// Called before the interpreter is initialized.
    ///
    /// If `interp.instruction_result` is set to anything other than [crate::interpreter::InstructionResult::Continue] then the execution of the interpreter
    /// is skipped.
    #[inline]
    fn initialize_interp(&mut self, interp: &mut Interpreter, context: &mut EvmContext<'_, DB>) {
        let _ = interp;
        let _ = context;
    }

    /// Called on each step of the interpreter.
    ///
    /// Information about the current execution, including the memory, stack and more is available
    /// on `interp` (see [Interpreter]).
    ///
    /// # Example
    ///
    /// To get the current opcode, use `interp.current_opcode()`.
    #[inline]
    fn step(&mut self, interp: &mut Interpreter, context: &mut EvmContext<'_, DB>) {
        let _ = interp;
        let _ = context;
    }

    /// Called when a log is emitted.
    #[inline]
    fn log(
        &mut self,
        context: &mut EvmContext<'_, DB>,
        address: &Address,
        topics: &[B256],
        data: &Bytes,
    ) {
        let _ = context;
        let _ = address;
        let _ = topics;
        let _ = data;
    }

    /// Called after `step` when the instruction has been executed.
    ///
    /// Setting `interp.instruction_result` to anything other than [crate::interpreter::InstructionResult::Continue] alters the execution
    /// of the interpreter.
    #[inline]
    fn step_end(&mut self, interp: &mut Interpreter, context: &mut EvmContext<'_, DB>) {
        let _ = interp;
        let _ = context;
    }

    /// Called whenever a call to a contract is about to start.
    ///
    /// InstructionResulting anything other than [crate::interpreter::InstructionResult::Continue] overrides the result of the call.
    #[inline]
    fn call(
        &mut self,
        context: &mut EvmContext<'_, DB>,
        inputs: &mut CallInputs,
    ) -> Option<(InterpreterResult, Range<usize>)> {
        let _ = context;
        let _ = inputs;
        None
    }

    /// Called when a call to a contract has concluded.
    ///
    /// InstructionResulting anything other than the values passed to this function (`(ret, remaining_gas,
    /// out)`) will alter the result of the call.
    #[inline]
    fn call_end(
        &mut self,
        context: &mut EvmContext<'_, DB>,
        result: InterpreterResult,
    ) -> InterpreterResult {
        let _ = context;
        result
    }

    /// Called when a contract is about to be created.
    ///
    /// InstructionResulting anything other than [crate::interpreter::InstructionResult::Continue] overrides the result of the creation.
    #[inline]
    fn create(
        &mut self,
        context: &mut EvmContext<'_, DB>,
        inputs: &mut CreateInputs,
    ) -> Option<(InterpreterResult, Option<Address>)> {
        let _ = context;
        let _ = inputs;
        None
    }

    /// Called when a contract has been created.
    ///
    /// InstructionResulting anything other than the values passed to this function (`(ret, remaining_gas,
    /// address, out)`) will alter the result of the create.
    #[inline]
    fn create_end(
        &mut self,
        context: &mut EvmContext<'_, DB>,
        result: InterpreterResult,
        address: Option<Address>,
    ) -> (InterpreterResult, Option<Address>) {
        let _ = context;
        (result, address)
    }
}
