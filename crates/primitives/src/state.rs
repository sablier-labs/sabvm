use crate::{Address, Bytecode, B256, BASE_ASSET_ID, KECCAK_EMPTY, U256};
use bitflags::bitflags;
use core::hash::{Hash, Hasher};
use hashbrown::HashMap;

/// EVM State is a mapping from addresses to accounts.
pub type State = HashMap<Address, Account>;

/// Structure used for EIP-1153 transient storage.
pub type TransientStorage = HashMap<(Address, U256), U256>;

/// An account's Storage is a mapping from 256-bit integer keys to [StorageSlot]s.
pub type Storage = HashMap<U256, StorageSlot>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Account {
    /// Balances, nonce, and code.
    pub info: AccountInfo,
    /// Storage cache
    pub storage: Storage,
    /// Account status flags.
    pub status: AccountStatus,
}

// The `bitflags!` macro generates `struct`s that manage a set of flags.
bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[cfg_attr(feature = "serde", serde(transparent))]
    pub struct AccountStatus: u8 {
        /// When account is loaded but not touched or interacted with.
        /// This is the default state.
        const Loaded = 0b00000000;
        /// When account is newly created we will not access database
        /// to fetch storage values
        const Created = 0b00000001;
        /// If account is marked for self destruction.
        const SelfDestructed = 0b00000010;
        /// Only when account is marked as touched we will save it to database.
        const Touched = 0b00000100;
        /// used only for pre spurious dragon hardforks where existing and empty were two separate states.
        /// it became same state after EIP-161: State trie clearing
        const LoadedAsNotExisting = 0b0001000;
    }
}

impl Default for AccountStatus {
    fn default() -> Self {
        Self::Loaded
    }
}

impl Account {
    /// Create new account and mark it as non existing.
    pub fn new_not_existing() -> Self {
        Self {
            info: AccountInfo::default(),
            storage: HashMap::new(),
            status: AccountStatus::LoadedAsNotExisting,
        }
    }

    /// Mark account as self destructed.
    pub fn mark_selfdestruct(&mut self) {
        self.status |= AccountStatus::SelfDestructed;
    }

    /// Unmark account as self destructed.
    pub fn unmark_selfdestruct(&mut self) {
        self.status -= AccountStatus::SelfDestructed;
    }

    /// Is account marked for self destruct.
    pub fn is_selfdestructed(&self) -> bool {
        self.status.contains(AccountStatus::SelfDestructed)
    }

    /// Mark account as touched
    pub fn mark_touch(&mut self) {
        self.status |= AccountStatus::Touched;
    }

    /// Unmark the touch flag.
    pub fn unmark_touch(&mut self) {
        self.status -= AccountStatus::Touched;
    }

    /// If account status is marked as touched.
    pub fn is_touched(&self) -> bool {
        self.status.contains(AccountStatus::Touched)
    }

    /// Mark account as newly created.
    pub fn mark_created(&mut self) {
        self.status |= AccountStatus::Created;
    }

    /// Unmark created flag.
    pub fn unmark_created(&mut self) {
        self.status -= AccountStatus::Created;
    }

    /// Is account loaded as not existing from database
    /// This is needed for pre spurious dragon hardforks where
    /// existing and empty were two separate states.
    pub fn is_loaded_as_not_existing(&self) -> bool {
        self.status.contains(AccountStatus::LoadedAsNotExisting)
    }

    /// Is account newly created in this transaction.
    pub fn is_created(&self) -> bool {
        self.status.contains(AccountStatus::Created)
    }

    /// Is account empty.
    pub fn is_empty(&self) -> bool {
        self.info.is_empty()
    }

    /// Returns an iterator over the storage slots that have been changed.
    ///
    /// See also [StorageSlot::is_changed]
    pub fn changed_storage_slots(&self) -> impl Iterator<Item = (&U256, &StorageSlot)> {
        self.storage.iter().filter(|(_, slot)| slot.is_changed())
    }
}

impl From<AccountInfo> for Account {
    fn from(info: AccountInfo) -> Self {
        Self {
            info,
            storage: HashMap::new(),
            status: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StorageSlot {
    pub previous_or_original_value: U256,
    /// When loaded with sload present value is set to original value
    pub present_value: U256,
}

impl StorageSlot {
    pub fn new(original: U256) -> Self {
        Self {
            previous_or_original_value: original,
            present_value: original,
        }
    }

    pub fn new_changed(previous_or_original_value: U256, present_value: U256) -> Self {
        Self {
            previous_or_original_value,
            present_value,
        }
    }

    /// Returns true if the present value differs from the original value
    pub fn is_changed(&self) -> bool {
        self.previous_or_original_value != self.present_value
    }

    pub fn original_value(&self) -> U256 {
        self.previous_or_original_value
    }

    pub fn present_value(&self) -> U256 {
        self.present_value
    }
}

pub type Balances = HashMap<B256, U256>;

/// The account information.
#[derive(Clone, Debug, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AccountInfo {
    /// Asset balances.
    pub balances: Balances,
    /// Account nonce.
    pub nonce: u64,
    /// code hash,
    pub code_hash: B256,
    /// code: if None, `code_by_hash` will be used to fetch it if code needs to be loaded from
    /// inside of `revm`.
    pub code: Option<Bytecode>,
}

impl Default for AccountInfo {
    fn default() -> Self {
        Self {
            balances: HashMap::new(),
            code_hash: KECCAK_EMPTY,
            code: Some(Bytecode::new()),
            nonce: 0,
        }
    }
}

impl PartialEq for AccountInfo {
    fn eq(&self, other: &Self) -> bool {
        if self.nonce != other.nonce
            || self.code_hash != other.code_hash
            || self.balances.len() != other.balances.len()
        {
            return false;
        }

        // Check whether the balances of the accounts are the same.
        self.balances == other.balances
    }
}

impl Hash for AccountInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        //TODO: take into account the other Native Assets, as well
        self.get_balance(BASE_ASSET_ID).hash(state);
        self.nonce.hash(state);
        self.code_hash.hash(state);
    }
}

impl AccountInfo {
    pub fn new(balances: Balances, nonce: u64, code_hash: B256, code: Bytecode) -> Self {
        Self {
            balances,
            nonce,
            code: Some(code),
            code_hash,
        }
    }

    pub fn from_balances(balances: Balances) -> Self {
        AccountInfo {
            balances,
            ..Default::default()
        }
    }

    /// Returns account info without the code.
    pub fn without_code(mut self) -> Self {
        self.take_bytecode();
        self
    }

    /// Returns if an account is empty.
    ///
    /// An account is empty if the following conditions are met.
    /// - code hash is zero or set to the Keccak256 hash of the empty string `""`
    /// - the balances of the Account haven't been set
    /// - nonce is zero
    pub fn is_empty(&self) -> bool {
        let code_empty = self.code_hash == KECCAK_EMPTY || self.code_hash == B256::ZERO;
        self.balances.len() == 0 && self.nonce == 0 && code_empty
    }

    /// Returns `true` if the account is not empty.
    pub fn exists(&self) -> bool {
        !self.is_empty()
    }

    /// Returns `true` if account has no nonce and code.
    pub fn has_no_code_and_nonce(&self) -> bool {
        self.is_empty_code_hash() && self.nonce == 0
    }

    /// Return bytecode hash associated with this account.
    /// If account does not have code, it return's `KECCAK_EMPTY` hash.
    pub fn code_hash(&self) -> B256 {
        self.code_hash
    }

    /// Returns true if the code hash is the Keccak256 hash of the empty string `""`.
    #[inline]
    pub fn is_empty_code_hash(&self) -> bool {
        self.code_hash == KECCAK_EMPTY
    }

    /// Decreases the `asset_id` balance of the account, wrapping around `0` on underflow.
    pub fn decrease_balance(&mut self, balance: U256) -> Option<U256> {
        let current_base_balance = self.get_base_balance();
        self.balances
            .insert(BASE_ASSET_ID, current_base_balance.wrapping_sub(balance))
    }

    /// Decreases the `asset_id` balance of the account, saturating at zero.
    pub fn decrease_balance_saturating(&mut self, balance: U256) -> Option<U256> {
        let current_base_balance = self.get_base_balance();
        self.balances
            .insert(BASE_ASSET_ID, current_base_balance.saturating_sub(balance))
    }

    /// Decreases the base asset balance of the account, wrapping around `0` on underflow.
    pub fn decrease_base_balance(&mut self, balance: U256) -> Option<U256> {
        let current_base_balance = self.get_base_balance();
        self.balances
            .insert(BASE_ASSET_ID, current_base_balance.wrapping_sub(balance))
    }

    /// Decreases the base asset balance of the account, saturating at zero.
    pub fn decrease_base_balance_saturating(&mut self, balance: U256) -> Option<U256> {
        let current_base_balance = self.get_base_balance();
        self.balances
            .insert(BASE_ASSET_ID, current_base_balance.saturating_sub(balance))
    }

    /// Returns the balance of `asset_id`, defaulting to zero if none is set.
    pub fn get_balance(&self, asset_id: B256) -> U256 {
        self.balances.get(&asset_id).copied().unwrap_or_default()
    }

    /// Returns the balance of the base asset, defaulting to zero if none is set.
    pub fn get_base_balance(&self) -> U256 {
        self.balances
            .get(&BASE_ASSET_ID)
            .copied()
            .unwrap_or_default()
    }

    /// Increases the `asset_id` balance of the account, wrapping around `U256::MAX` on overflow.
    pub fn increase_balance(&mut self, asset_id: B256, value: U256) -> Option<U256> {
        let current_balance = self.get_balance(asset_id);
        self.balances
            .insert(asset_id, current_balance.wrapping_add(value))
    }

    /// Increases the `asset_id` balance of the account, saturating at `U256::MAX`.
    pub fn increase_balance_saturating(&mut self, asset_id: B256, value: U256) -> Option<U256> {
        let current_balance = self.get_balance(asset_id);
        self.balances
            .insert(asset_id, current_balance.saturating_add(value))
    }

    /// Increases the base asset balance of the account, wrapping around `U256::MAX` on overflow.
    pub fn increase_base_balance(&mut self, value: U256) -> Option<U256> {
        let current_base_balance = self.get_base_balance();
        self.balances
            .insert(BASE_ASSET_ID, current_base_balance.wrapping_add(value))
    }

    /// Increases the base asset balance of the account, saturating at `U256::MAX`.
    pub fn increase_base_balance_saturating(&mut self, value: U256) -> Option<U256> {
        let current_base_balance = self.get_base_balance();
        self.balances
            .insert(BASE_ASSET_ID, current_base_balance.saturating_add(value))
    }

    pub fn set_balance(&mut self, asset_id: B256, balance: U256) -> Option<U256> {
        self.balances.insert(asset_id, balance)
    }

    pub fn set_base_balance(&mut self, balance: U256) -> Option<U256> {
        self.balances.insert(BASE_ASSET_ID, balance)
    }

    /// Take bytecode from account. Code will be set to None.
    pub fn take_bytecode(&mut self) -> Option<Bytecode> {
        self.code.take()
    }
}

#[cfg(test)]
mod tests {
    use crate::{Account, KECCAK_EMPTY, U256};

    #[test]
    fn account_is_empty_balance() {
        let mut account = Account::default();
        assert!(account.is_empty());

        account.info.set_base_balance(U256::from(1));
        assert!(!account.is_empty());
    }

    #[test]
    fn account_is_empty_nonce() {
        let mut account = Account::default();
        assert!(account.is_empty());

        account.info.nonce = 1;
        assert!(!account.is_empty());

        account.info.nonce = 0;
        assert!(account.is_empty());
    }

    #[test]
    fn account_is_empty_code_hash() {
        let mut account = Account::default();
        assert!(account.is_empty());

        account.info.code_hash = [1; 32].into();
        assert!(!account.is_empty());

        account.info.code_hash = [0; 32].into();
        assert!(account.is_empty());

        account.info.code_hash = KECCAK_EMPTY;
        assert!(account.is_empty());
    }

    #[test]
    fn account_state() {
        let mut account = Account::default();

        assert!(!account.is_touched());

        account.mark_touch();
        assert!(account.is_touched());
    }
}
