use crate::{Address, Bytes, Log, State, B256, U256};
use alloc::{boxed::Box, vec::Vec};
use core::fmt;

/// Result of EVM execution.
pub type EVMResult<DBError> = EVMResultGeneric<ResultAndState, DBError>;

/// Generic result of EVM execution. Used to represent error and generic output.
pub type EVMResultGeneric<T, DBError> = core::result::Result<T, EVMError<DBError>>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ResultAndState {
    /// Status of execution
    pub result: ExecutionResult,
    /// State that got updated
    pub state: State,
}

/// Result of a transaction execution.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ExecutionResult {
    /// Returned successfully
    Success {
        reason: SuccessReason,
        gas_used: u64,
        gas_refunded: u64,
        logs: Vec<Log>,
        output: Output,
    },
    /// Reverted by `REVERT` opcode that doesn't spend all gas.
    Revert { gas_used: u64, output: Bytes },
    /// Reverted for various reasons. Spend all gas(equal to gas_limit).
    Halt { reason: HaltReason, gas_used: u64 },
}

impl ExecutionResult {
    /// Returns if transaction execution is successful.
    /// 1 indicates success, 0 indicates revert.
    /// <https://eips.ethereum.org/EIPS/eip-658>
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }

    /// Returns true if execution result is a Halt.
    pub fn is_halt(&self) -> bool {
        matches!(self, Self::Halt { .. })
    }

    //TODO: why isn't there an "is_revert()", too?

    /// Return logs, if execution is not successful, function will return empty vec.
    pub fn logs(&self) -> Vec<Log> {
        match self {
            Self::Success { logs, .. } => logs.clone(),
            _ => Vec::new(),
        }
    }

    /// Returns the output data of the execution.
    ///
    /// Returns `None` if the execution was halted.
    pub fn output(&self) -> Option<&Bytes> {
        match self {
            Self::Success { output, .. } => Some(output.data()),
            Self::Revert { output, .. } => Some(output),
            _ => None,
        }
    }

    /// Consumes the type and returns the output data of the execution.
    ///
    /// Returns `None` if the execution was halted.
    pub fn into_output(self) -> Option<Bytes> {
        match self {
            Self::Success { output, .. } => Some(output.into_data()),
            Self::Revert { output, .. } => Some(output),
            _ => None,
        }
    }

    /// Consumes the type and returns logs, if execution is not successful, function will return empty vec.
    pub fn into_logs(self) -> Vec<Log> {
        match self {
            Self::Success { logs, .. } => logs,
            _ => Vec::new(),
        }
    }

    pub fn gas_used(&self) -> u64 {
        let (Self::Success { gas_used, .. }
        | Self::Revert { gas_used, .. }
        | Self::Halt { gas_used, .. }) = self;

        *gas_used
    }
}

/// Output of a transaction execution.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Output {
    Call(Bytes),
    Create(Bytes, Option<Address>),
}

impl Output {
    /// Consumes the type and returns the output data of the execution output.
    pub fn into_data(self) -> Bytes {
        match self {
            Output::Call(data) => data,
            Output::Create(data, _) => data,
        }
    }

    /// Returns the output data of the execution output.
    pub fn data(&self) -> &Bytes {
        match self {
            Output::Call(data) => data,
            Output::Create(data, _) => data,
        }
    }
}

/// Main EVM error.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EVMError<DBError> {
    /// Transaction validation error.
    Transaction(InvalidTransactionReason),
    /// Header validation error.
    Header(InvalidHeaderReason),
    /// Database error.
    Database(DBError),
}

#[cfg(feature = "std")]
impl<DBError: fmt::Debug + fmt::Display> std::error::Error for EVMError<DBError> {}

impl<DBError: fmt::Display> fmt::Display for EVMError<DBError> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EVMError::Transaction(e) => write!(f, "Transaction error: {e:?}"),
            EVMError::Header(e) => write!(f, "Header error: {e:?}"),
            EVMError::Database(e) => write!(f, "Database error: {e}"),
        }
    }
}

impl<DBError> From<InvalidTransactionReason> for EVMError<DBError> {
    fn from(invalid: InvalidTransactionReason) -> Self {
        EVMError::Transaction(invalid)
    }
}

/// The reason for the transaction validation error.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum InvalidTransactionReason {
    /// When using the EIP-1559 fee model introduced in the London upgrade, transactions specify two primary fee fields:
    /// - `gas_max_fee`: The maximum total fee a user is willing to pay, inclusive of both base fee and priority fee.
    /// - `gas_priority_fee`: The extra amount a user is willing to give directly to the miner, often referred to as the "tip".
    ///
    /// Provided `gas_priority_fee` exceeds the total `gas_max_fee`.
    PriorityFeeGreaterThanMaxFee,
    /// EIP-1559: `gas_price` is less than `basefee`.
    GasPriceLessThanBasefee,
    /// `gas_limit` in the tx is bigger than `block_gas_limit`.
    CallerGasLimitMoreThanBlock,
    /// Initial gas for a Call is bigger than `gas_limit`.
    ///
    /// Initial gas for a Call contains:
    /// - initial stipend gas
    /// - gas for access list and input data
    CallGasCostMoreThanGasLimit,
    /// EIP-3607 Reject transactions from senders with deployed code
    RejectCallerWithCode,
    /// Transaction account doesn't have enough base asset to cover the transferred value and gas_limit*gas_price.
    NotEnoughBaseAssetBalanceForTransferAndMaxFee {
        fee: Box<U256>,
        balance: Box<U256>,
    },
    /// Transaction account doesn't have enough asset balance to cover the transferred value.
    NotEnoughAssetBalanceForTransfer {
        asset_id: B256,
        required_balance: U256,
        actual_balance: U256,
    },
    /// Overflow payment in transaction.
    OverflowPaymentInTransaction,

    //TODO: NonceOverflowInTransaction isn't being used anywhere
    /// Nonce overflows in transaction.
    NonceOverflowInTransaction,
    NonceTooHigh {
        tx: u64,
        state: u64,
    },
    NonceTooLow {
        tx: u64,
        state: u64,
    },
    /// EIP-3860: Limit and meter initcode
    CreateInitCodeSizeLimit,
    /// Transaction chain id does not match the config chain id.
    InvalidChainId,
    /// Access list is not supported for blocks before the Berlin hardfork.
    AccessListNotSupported,
    /// `max_fee_per_blob_gas` is not supported for blocks before the Cancun hardfork.
    MaxFeePerBlobGasNotSupported,
    /// `blob_hashes`/`blob_versioned_hashes` is not supported for blocks before the Cancun hardfork.
    BlobVersionedHashesNotSupported,
    /// Block `blob_gas_price` is greater than tx-specified `max_fee_per_blob_gas` after Cancun.
    BlobGasPriceGreaterThanMax,
    /// There should be at least one blob in Blob transaction.
    EmptyBlobs,
    /// Blob transaction can't be a create transaction.
    /// `to` must be present
    BlobCreateTransaction,
    /// Transaction has more then [`crate::MAX_BLOB_NUMBER_PER_BLOCK`] blobs
    TooManyBlobs,
    /// Blob transaction contains a versioned hash with an incorrect version
    BlobVersionNotSupported,
    /// Asset IDs in transaction are not unique
    AssetIdsNotUnique,
    /// Deposit transaction haults bubble up to the global main return handler,
    /// wiping state and only increasing the nonce + persisting the mint value.
    /// System transactions are not supported post-regolith hardfork.
    ///
    /// Before the Regolith hardfork, there was a special field in the `Deposit` transaction
    /// type that differentiated between `system` and `user` deposit transactions. This field
    /// was deprecated in the Regolith hardfork, and this error is thrown if a `Deposit` transaction
    /// is found with this field set to `true` after the hardfork activation.
    ///
    /// In addition, this error is internal, and bubbles up into a [HaltReason::FailedDeposit] error
    /// in the `revm` handler for the consumer to easily handle. This is due to a state transition
    /// rule on OP Stack chains where, if for any reason a deposit transaction fails, the transaction
    /// must still be included in the block, the sender nonce is bumped, the `mint` value persists, and
    /// special gas accounting rules are applied. Normally on L1, [EVMError::Transaction] errors
    /// are cause for non-inclusion, so a special [HaltReason] variant was introduced to handle this
    /// case for failed deposit transactions.
    #[cfg(feature = "optimism")]
    DepositSystemTxPostRegolith,
    /// Deposit transaction haults bubble up to the global main return handler, wiping state and
    /// only increasing the nonce + persisting the mint value.
    ///
    /// This is a catch-all error for any deposit transaction that is results in a [HaltReason] error
    /// post-regolith hardfork. This allows for a consumer to easily handle special cases where
    /// a deposit transaction fails during validation, but must still be included in the block.
    ///
    /// In addition, this error is internal, and bubbles up into a [HaltReason::FailedDeposit] error
    /// in the `revm` handler for the consumer to easily handle. This is due to a state transition
    /// rule on OP Stack chains where, if for any reason a deposit transaction fails, the transaction
    /// must still be included in the block, the sender nonce is bumped, the `mint` value persists, and
    /// special gas accounting rules are applied. Normally on L1, [EVMError::Transaction] errors
    /// are cause for non-inclusion, so a special [HaltReason] variant was introduced to handle this
    /// case for failed deposit transactions.
    #[cfg(feature = "optimism")]
    HaltedDepositPostRegolith,
}

#[cfg(feature = "std")]
impl std::error::Error for InvalidTransactionReason {}

impl fmt::Display for InvalidTransactionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidTransactionReason::PriorityFeeGreaterThanMaxFee => {
                write!(f, "Priority fee is greater than max fee")
            }
            InvalidTransactionReason::GasPriceLessThanBasefee => {
                write!(f, "Gas price is less than basefee")
            }
            InvalidTransactionReason::CallerGasLimitMoreThanBlock => {
                write!(f, "Caller gas limit exceeds the block gas limit")
            }
            InvalidTransactionReason::CallGasCostMoreThanGasLimit => {
                write!(f, "Call gas cost exceeds the gas limit")
            }
            InvalidTransactionReason::RejectCallerWithCode => {
                write!(f, "Reject transactions from senders with deployed code")
            }
            InvalidTransactionReason::NotEnoughBaseAssetBalanceForTransferAndMaxFee {
                fee,
                balance,
            } => {
                write!(f, "Lack of funds {} for max fee {}", balance, fee)
            }
            InvalidTransactionReason::OverflowPaymentInTransaction => {
                write!(f, "Overflow payment in transaction")
            }
            InvalidTransactionReason::NonceOverflowInTransaction => {
                write!(f, "Nonce overflow in transaction")
            }
            InvalidTransactionReason::NonceTooHigh { tx, state } => {
                write!(f, "Nonce {} too high: expected {}", tx, state)
            }
            InvalidTransactionReason::NonceTooLow { tx, state } => {
                write!(f, "Nonce {} too low: expected {}", tx, state)
            }
            InvalidTransactionReason::CreateInitCodeSizeLimit => {
                write!(f, "Create initcode size limit")
            }
            InvalidTransactionReason::InvalidChainId => write!(f, "Invalid chain id"),
            InvalidTransactionReason::AccessListNotSupported => {
                write!(f, "Access list not supported")
            }
            InvalidTransactionReason::MaxFeePerBlobGasNotSupported => {
                write!(f, "Max fee per blob gas not supported")
            }
            InvalidTransactionReason::BlobVersionedHashesNotSupported => {
                write!(f, "Blob versioned hashes not supported")
            }
            InvalidTransactionReason::BlobGasPriceGreaterThanMax => {
                write!(f, "Blob gas price is greater than max fee per blob gas")
            }
            InvalidTransactionReason::EmptyBlobs => write!(f, "Empty blobs"),
            InvalidTransactionReason::BlobCreateTransaction => write!(f, "Blob create transaction"),
            InvalidTransactionReason::TooManyBlobs => write!(f, "Too many blobs"),
            InvalidTransactionReason::BlobVersionNotSupported => {
                write!(f, "Blob version not supported")
            }
            InvalidTransactionReason::NotEnoughAssetBalanceForTransfer {
                asset_id,
                required_balance,
                actual_balance,
            } => {
                write!(f, "The account balance {actual_balance} of asset id {asset_id} is not enough to cover the required {required_balance}")
            }
            InvalidTransactionReason::AssetIdsNotUnique => {
                write!(f, "The ids of the submitted native assets are not unique")
            }

            #[cfg(feature = "optimism")]
            InvalidTransactionReason::DepositSystemTxPostRegolith => {
                write!(
                    f,
                    "Deposit system transactions post regolith hardfork are not supported"
                )
            }
            #[cfg(feature = "optimism")]
            InvalidTransactionReason::HaltedDepositPostRegolith => {
                write!(
                    f,
                    "Deposit transaction halted post-regolith. Error will be bubbled up to main return handler."
                )
            }
        }
    }
}

impl<DBError> From<InvalidHeaderReason> for EVMError<DBError> {
    fn from(invalid: InvalidHeaderReason) -> Self {
        EVMError::Header(invalid)
    }
}

/// Errors related to misconfiguration of a [`crate::env::BlockEnv`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum InvalidHeaderReason {
    /// `prevrandao` is not set for Merge and above.
    PrevrandaoNotSet,
    /// `excess_blob_gas` is not set for Cancun and above.
    ExcessBlobGasNotSet,
}

#[cfg(feature = "std")]
impl std::error::Error for InvalidHeaderReason {}

impl fmt::Display for InvalidHeaderReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidHeaderReason::PrevrandaoNotSet => write!(f, "Prevrandao not set"),
            InvalidHeaderReason::ExcessBlobGasNotSet => write!(f, "Excess blob gas not set"),
        }
    }
}

/// Reason a transaction successfully completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SuccessReason {
    Stop,
    Return,
}

/// Indicates that the EVM has experienced an exceptional halt. This causes execution to
/// immediately end with all gas being consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HaltReason {
    OutOfGas(OutOfGasError),
    OpcodeNotFound,
    InvalidFEOpcode,
    InvalidJump,
    NotActivated,
    StackUnderflow,
    StackOverflow,
    OutOfOffset,
    CreateCollision,
    PrecompileError,
    NonceOverflow,
    UnauthorizedCaller,
    /// Create init code size exceeds limit (runtime).
    CreateContractSizeLimit,
    /// Error on created contract that begins with EF
    CreateContractStartingWithEF,
    /// EIP-3860: Limit and meter initcode. Initcode size limit exceeded.
    CreateInitCodeSizeLimit,

    /* Internal Halts that can be only found inside Inspector */
    OverflowPayment,
    StateChangeDuringStaticCall,
    CallNotAllowedInsideStatic,
    OutOfFund,
    CallTooDeep,

    /* Optimism errors */
    #[cfg(feature = "optimism")]
    FailedDeposit,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum OutOfGasError {
    // Basic OOG error
    BasicOutOfGas,
    // Tried to expand past REVM limit
    MemoryLimit,
    // Basic OOG error from memory expansion
    Memory,
    // Precompile threw OOG error
    Precompile,
    // When performing something that takes a U256 and casts down to a u64, if its too large this would fire
    // i.e. in `as_usize_or_fail`
    InvalidOperand,
}
