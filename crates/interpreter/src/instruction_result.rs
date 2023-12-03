use crate::primitives::{Eval, HaltReason};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum InstructionResult {
    // success codes
    #[default]
    Continue = 0x00,
    Stop,
    Return,
    SelfDestruct,

    // revert codes
    Revert = 0x10, // revert opcode
    CallTooDeep,
    OutOfFund,

    // Actions
    CallOrCreate = 0x20,

    // error codes
    OutOfGas = 0x50,
    MemoryOOG,
    MemoryLimitOOG,
    PrecompileOOG,
    InvalidOperandOOG,
    OpcodeNotFound,
    CallNotAllowedInsideStatic,
    StateChangeDuringStaticCall,
    InvalidFEOpcode,
    InvalidJump,
    NotActivated,
    StackUnderflow,
    StackOverflow,
    OutOfOffset,
    CreateCollision,
    OverflowPayment,
    PrecompileError,
    NonceOverflow,
    /// Create init code size exceeds limit (runtime).
    CreateContractSizeLimit,
    /// Error on created contract that begins with EF
    CreateContractStartingWithEF,
    /// EIP-3860: Limit and meter initcode. Initcode size limit exceeded.
    CreateInitCodeSizeLimit,

    /// Fatal external error. Returned by database.
    FatalExternalError,
}

impl InstructionResult {
    /// Returns whether the result is a success.
    #[inline]
    pub fn is_ok(self) -> bool {
        matches!(self, crate::return_ok!())
    }

    /// Returns whether the result is a revert.
    #[inline]
    pub fn is_revert(self) -> bool {
        matches!(self, crate::return_revert!())
    }

    /// Returns whether the result is an error.
    #[inline]
    pub fn is_error(self) -> bool {
        matches!(
            self,
            Self::OutOfGas
                | Self::MemoryOOG
                | Self::MemoryLimitOOG
                | Self::PrecompileOOG
                | Self::InvalidOperandOOG
                | Self::OpcodeNotFound
                | Self::CallNotAllowedInsideStatic
                | Self::StateChangeDuringStaticCall
                | Self::InvalidFEOpcode
                | Self::InvalidJump
                | Self::NotActivated
                | Self::StackUnderflow
                | Self::StackOverflow
                | Self::OutOfOffset
                | Self::CreateCollision
                | Self::OverflowPayment
                | Self::PrecompileError
                | Self::NonceOverflow
                | Self::CreateContractSizeLimit
                | Self::CreateContractStartingWithEF
                | Self::CreateInitCodeSizeLimit
                | Self::FatalExternalError
        )
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum SuccessOrHalt {
    Success(Eval),
    Revert,
    Halt(HaltReason),
    FatalExternalError,
    /// Internal instruction that signals Interpreter should continue running.
    InternalContinue,
    /// Internal instruction that signals subcall.
    InternalCallOrCreate,
}

impl SuccessOrHalt {
    /// Returns true if the transaction returned successfully without halts.
    #[inline]
    pub fn is_success(self) -> bool {
        matches!(self, SuccessOrHalt::Success(_))
    }

    /// Returns the [Eval] value if this a successful result
    #[inline]
    pub fn to_success(self) -> Option<Eval> {
        match self {
            SuccessOrHalt::Success(eval) => Some(eval),
            _ => None,
        }
    }

    /// Returns true if the transaction reverted.
    #[inline]
    pub fn is_revert(self) -> bool {
        matches!(self, SuccessOrHalt::Revert)
    }

    /// Returns true if the EVM has experienced an exceptional halt
    #[inline]
    pub fn is_halt(self) -> bool {
        matches!(self, SuccessOrHalt::Halt(_))
    }

    /// Returns the [Halt] value the EVM has experienced an exceptional halt
    #[inline]
    pub fn to_halt(self) -> Option<HaltReason> {
        match self {
            SuccessOrHalt::Halt(halt) => Some(halt),
            _ => None,
        }
    }
}

impl From<InstructionResult> for SuccessOrHalt {
    fn from(result: InstructionResult) -> Self {
        match result {
            InstructionResult::Continue => Self::InternalContinue, // used only in interpreter loop
            InstructionResult::Stop => Self::Success(Eval::Stop),
            InstructionResult::Return => Self::Success(Eval::Return),
            InstructionResult::SelfDestruct => Self::Success(Eval::SelfDestruct),
            InstructionResult::Revert => Self::Revert,
            InstructionResult::CallOrCreate => Self::InternalCallOrCreate, // used only in interpreter loop
            InstructionResult::CallTooDeep => Self::Halt(HaltReason::CallTooDeep), // not gonna happen for first call
            InstructionResult::OutOfFund => Self::Halt(HaltReason::OutOfFund), // Check for first call is done separately.
            InstructionResult::OutOfGas => Self::Halt(HaltReason::OutOfGas(
                revm_primitives::OutOfGasError::BasicOutOfGas,
            )),
            InstructionResult::MemoryLimitOOG => Self::Halt(HaltReason::OutOfGas(
                revm_primitives::OutOfGasError::MemoryLimit,
            )),
            InstructionResult::MemoryOOG => {
                Self::Halt(HaltReason::OutOfGas(revm_primitives::OutOfGasError::Memory))
            }
            InstructionResult::PrecompileOOG => Self::Halt(HaltReason::OutOfGas(
                revm_primitives::OutOfGasError::Precompile,
            )),
            InstructionResult::InvalidOperandOOG => Self::Halt(HaltReason::OutOfGas(
                revm_primitives::OutOfGasError::InvalidOperand,
            )),
            InstructionResult::OpcodeNotFound => Self::Halt(HaltReason::OpcodeNotFound),
            InstructionResult::CallNotAllowedInsideStatic => {
                Self::Halt(HaltReason::CallNotAllowedInsideStatic)
            } // first call is not static call
            InstructionResult::StateChangeDuringStaticCall => {
                Self::Halt(HaltReason::StateChangeDuringStaticCall)
            }
            InstructionResult::InvalidFEOpcode => Self::Halt(HaltReason::InvalidFEOpcode),
            InstructionResult::InvalidJump => Self::Halt(HaltReason::InvalidJump),
            InstructionResult::NotActivated => Self::Halt(HaltReason::NotActivated),
            InstructionResult::StackUnderflow => Self::Halt(HaltReason::StackUnderflow),
            InstructionResult::StackOverflow => Self::Halt(HaltReason::StackOverflow),
            InstructionResult::OutOfOffset => Self::Halt(HaltReason::OutOfOffset),
            InstructionResult::CreateCollision => Self::Halt(HaltReason::CreateCollision),
            InstructionResult::OverflowPayment => Self::Halt(HaltReason::OverflowPayment), // Check for first call is done separately.
            InstructionResult::PrecompileError => Self::Halt(HaltReason::PrecompileError),
            InstructionResult::NonceOverflow => Self::Halt(HaltReason::NonceOverflow),
            InstructionResult::CreateContractSizeLimit => {
                Self::Halt(HaltReason::CreateContractSizeLimit)
            }
            InstructionResult::CreateContractStartingWithEF => {
                Self::Halt(HaltReason::CreateContractSizeLimit)
            }
            InstructionResult::CreateInitCodeSizeLimit => {
                Self::Halt(HaltReason::CreateInitCodeSizeLimit)
            }
            InstructionResult::FatalExternalError => Self::FatalExternalError,
        }
    }
}

#[macro_export]
macro_rules! return_ok {
    () => {
        InstructionResult::Continue
            | InstructionResult::Stop
            | InstructionResult::Return
            | InstructionResult::SelfDestruct
    };
}

#[macro_export]
macro_rules! return_revert {
    () => {
        InstructionResult::Revert | InstructionResult::CallTooDeep | InstructionResult::OutOfFund
    };
}
