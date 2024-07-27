use crate::primitives::{Address, Bytes, TokenTransfer, TransactTo, TxEnv, BASE_TOKEN_ID, U256};
use core::ops::Range;
use std::boxed::Box;
use std::vec;
use std::vec::Vec;

/// Inputs for a call.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CallInputs {
    /// The call data of the call.
    pub input: Bytes,
    /// The return memory offset where the output of the call is written.
    ///
    /// In EOF, this range is invalid as EOF calls do not write output to memory.
    pub return_memory_offset: Range<usize>,
    /// The gas limit of the call.
    pub gas_limit: u64,
    /// The account address of bytecode that is going to be executed.
    ///
    /// Previously `context.code_address`.
    pub bytecode_address: Address,
    /// Target address, this account storage is going to be modified.
    ///
    /// Previously `context.address`.
    pub target_address: Address,
    /// This caller is invoking the call.
    ///
    /// Previously `context.caller`.
    pub caller: Address,
    /// Call values.
    ///
    /// NOTE: These values may not necessarily be transferred from caller to callee, see [`CallValues`].
    ///
    /// Previously `transfer.value` or `context.apparent_value`.
    pub values: CallValues,
    /// The call scheme.
    ///
    /// Previously `context.scheme`.
    pub scheme: CallScheme,
    /// Whether the call is a static call, or is initiated inside a static call.
    pub is_static: bool,
    /// Whether the call is initiated from EOF bytecode.
    pub is_eof: bool,
}

impl CallInputs {
    /// Creates new call inputs.
    ///
    /// Returns `None` if the transaction is not a call.
    pub fn new(tx_env: &TxEnv, gas_limit: u64) -> Option<Self> {
        let TransactTo::Call(target_address) = tx_env.transact_to else {
            return None;
        };
        Some(CallInputs {
            input: tx_env.data.clone(),
            gas_limit,
            target_address,
            bytecode_address: target_address,
            caller: tx_env.caller,
            values: CallValues::Transfer(tx_env.transferred_tokens.clone()),
            scheme: CallScheme::Call,
            is_static: false,
            is_eof: false,
            return_memory_offset: 0..0,
        })
    }

    /// Creates new boxed call inputs.
    ///
    /// Returns `None` if the transaction is not a call.
    pub fn new_boxed(tx_env: &TxEnv, gas_limit: u64) -> Option<Box<Self>> {
        Self::new(tx_env, gas_limit).map(Box::new)
    }

    /// Returns `true` if the call will transfer a non-zero value.
    #[inline]
    pub fn transfers_value(&self) -> bool {
        self.values.transfer().iter().any(|x| x.amount > U256::ZERO)
    }

    /// Returns the transfer value.
    ///
    /// This is the value that is transferred from caller to callee, see [`CallValues`].
    #[inline]
    pub fn transfer_value(&self) -> Vec<TokenTransfer> {
        self.values.transfer()
    }

    /// Returns the **apparent** call value.
    ///
    /// This value is not actually transferred, see [`CallValues`].
    #[inline]
    pub fn apparent_value(&self) -> Vec<TokenTransfer> {
        self.values.apparent()
    }

    /// Returns the address of the transfer source account.
    ///
    /// This is only meaningful if `transfers_value` is `true`.
    #[inline]
    pub const fn transfer_from(&self) -> Address {
        self.caller
    }

    /// Returns the address of the transfer target account.
    ///
    /// This is only meaningful if `transfers_value` is `true`.
    #[inline]
    pub const fn transfer_to(&self) -> Address {
        self.target_address
    }

    /// Returns the call values, regardless of the transfer type.
    ///
    /// NOTE: this values may not necessarily be transferred from caller to callee, see [`CallValues`].
    #[inline]
    pub fn call_values(&self) -> Vec<TokenTransfer> {
        self.values.get()
    }
}

/// Call scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CallScheme {
    /// `CALL`.
    Call,
    /// `CALLCODE`
    CallCode,
    /// `DELEGATECALL`
    DelegateCall,
    /// `STATICCALL`
    StaticCall,
}

/// Call values.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CallValues {
    /// Concrete value, transferred from caller to callee at the end of the transaction.
    Transfer(Vec<TokenTransfer>),
    /// Apparent value, that is **not** actually transferred.
    ///
    /// Set when in a `DELEGATECALL` call type, and used by the `CALLVALUE` opcode.
    Apparent(Vec<TokenTransfer>),
}

impl Default for CallValues {
    #[inline]
    fn default() -> Self {
        CallValues::Transfer(vec![
            (TokenTransfer {
                id: BASE_TOKEN_ID,
                amount: U256::ZERO,
            }),
        ])
    }
}

impl CallValues {
    /// Returns the call value, regardless of the type.
    #[inline]
    pub fn get(&self) -> Vec<TokenTransfer> {
        match self {
            Self::Transfer(values) | Self::Apparent(values) => values.clone(),
        }
    }

    /// Returns the transferred value, if any.
    #[inline]
    pub fn transfer(&self) -> Vec<TokenTransfer> {
        match self {
            Self::Transfer(values) => values.clone(),
            Self::Apparent(_) => Vec::new(),
        }
    }

    /// Returns whether the call value will be transferred.
    #[inline]
    pub const fn is_transfer(&self) -> bool {
        matches!(self, Self::Transfer(_))
    }

    /// Returns the apparent value, if any.
    #[inline]
    pub fn apparent(&self) -> Vec<TokenTransfer> {
        match self {
            Self::Transfer(_) => Vec::new(),
            Self::Apparent(values) => values.clone(),
        }
    }

    /// Returns whether the call value is apparent, and not actually transferred.
    #[inline]
    pub const fn is_apparent(&self) -> bool {
        matches!(self, Self::Apparent(_))
    }
}
