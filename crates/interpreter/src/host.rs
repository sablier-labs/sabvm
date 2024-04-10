use crate::primitives::{Address, Bytecode, Env, Log, B256, U256};

mod dummy;
pub use dummy::DummyHost;
use revm_primitives::BASE_ASSET_ID;

/// EVM context host.
pub trait Host {
    /// Returns a reference to the environment.
    fn env(&self) -> &Env;

    /// Returns a mutable reference to the environment.
    fn env_mut(&mut self) -> &mut Env;

    /// Load an account.
    ///
    /// Returns (is_cold, is_new_account)
    fn load_account(&mut self, address: Address) -> Option<(bool, bool)>;

    /// Get the block hash of the given block `number`.
    fn block_hash(&mut self, number: U256) -> Option<B256>;

    /// Get the base asset balance of `address` and if the account is cold.
    fn base_balance(&mut self, address: Address) -> Option<(U256, bool)> {
        self.balance(BASE_ASSET_ID, address)
    }

    /// Get code of `address` and if the account is cold.
    fn code(&mut self, address: Address) -> Option<(Bytecode, bool)>;

    /// Get code hash of `address` and if the account is cold.
    fn code_hash(&mut self, address: Address) -> Option<(B256, bool)>;

    /// Check whether the sender of the current tx is an EOA.
    fn is_tx_sender_eoa(&mut self) -> bool {
        let caller = self.env().tx.caller;
        self.code(caller).is_none()
    }

    /// Get storage value of `address` at `index` and if the account is cold.
    fn sload(&mut self, address: Address, index: U256) -> Option<(U256, bool)>;

    /// Set storage value of account address at index.
    ///
    /// Returns (original, present, new, is_cold).
    fn sstore(&mut self, address: Address, index: U256, value: U256) -> Option<SStoreResult>;

    /// Get the transient storage value of `address` at `index`.
    fn tload(&mut self, address: Address, index: U256) -> U256;

    /// Set the transient storage value of `address` at `index`.
    fn tstore(&mut self, address: Address, index: U256, value: U256);

    /// Emit a log owned by `address` with given `LogData`.
    fn log(&mut self, log: Log);

    /// Get asset balance of address and if account is cold loaded.
    fn balance(&mut self, asset_id: B256, address: Address) -> Option<(U256, bool)>;

    /// Mint a native asset.
    fn mint(&mut self, minter: Address, sub_id: B256, amount: U256) -> bool;

    /// Burn a native asset.
    fn burn(&mut self, burner: Address, sub_id: B256, amount: U256) -> bool;
}

/// Represents the result of an `sstore` operation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SStoreResult {
    /// Value of the storage when it is first read
    pub original_value: U256,
    /// Current value of the storage
    pub present_value: U256,
    /// New value that is set
    pub new_value: U256,
    /// Is storage slot loaded from database
    pub is_cold: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_host<H: Host + ?Sized>() {}

    #[test]
    fn object_safety() {
        assert_host::<DummyHost>();
        assert_host::<dyn Host>();
    }
}
