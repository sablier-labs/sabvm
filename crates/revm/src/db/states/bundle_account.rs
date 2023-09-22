use super::{
    reverts::AccountInfoRevert, AccountRevert, AccountStatus, RevertToSlot,
    StorageWithOriginalValues, TransitionAccount,
};
use revm_interpreter::primitives::{AccountInfo, StorageSlot, U256};
use revm_precompile::HashMap;

/// Account information focused on creating of database changesets
/// and Reverts.
///
/// Status is needed as to know from what state we are applying the TransitionAccount.
///
/// Original account info is needed to know if there was a change.
/// Same thing for storage with original value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundleAccount {
    pub info: Option<AccountInfo>,
    pub original_info: Option<AccountInfo>,
    /// Contains both original and present state.
    /// When extracting changeset we compare if original value is different from present value.
    /// If it is different we add it to changeset.
    pub storage: StorageWithOriginalValues,
    /// Account status.
    pub status: AccountStatus,
}

impl BundleAccount {
    /// Create new BundleAccount.
    pub fn new(
        original_info: Option<AccountInfo>,
        present_info: Option<AccountInfo>,
        storage: StorageWithOriginalValues,
        status: AccountStatus,
    ) -> Self {
        Self {
            info: present_info,
            original_info,
            storage,
            status,
        }
    }

    /// The approximate size of changes needed to store this account.
    /// `1 + storage_len`
    pub fn size_hint(&self) -> usize {
        1 + self.storage.len()
    }

    /// Return storage slot if it exists.
    ///
    /// In case we know that account is newly created, return `Some(U256::ZERO)`.
    pub fn storage_slot(&self, slot: U256) -> Option<U256> {
        let slot = self.storage.get(&slot).map(|s| s.present_value);
        if slot.is_some() {
            slot
        } else if self.status.storage_known() {
            Some(U256::ZERO)
        } else {
            None
        }
    }

    /// Fetch account info if it exist.
    pub fn account_info(&self) -> Option<AccountInfo> {
        self.info.clone()
    }

    /// Return true of account info was changed.
    pub fn is_info_changed(&self) -> bool {
        self.info != self.original_info
    }

    /// Return true if contract was changed
    pub fn is_contract_changed(&self) -> bool {
        self.info.as_ref().map(|a| a.code_hash) != self.original_info.as_ref().map(|a| a.code_hash)
    }

    /// Revert account to previous state and return true if account can be removed.
    pub fn revert(&mut self, revert: AccountRevert) -> bool {
        self.status = revert.previous_status;

        match revert.account {
            AccountInfoRevert::DoNothing => (),
            AccountInfoRevert::DeleteIt => {
                self.info = None;
                self.storage = HashMap::new();
                return true;
            }
            AccountInfoRevert::RevertTo(info) => self.info = Some(info),
        };
        // revert storage
        for (key, slot) in revert.storage {
            match slot {
                RevertToSlot::Some(value) => {
                    // Don't overwrite original values if present
                    // if storage is not present set original values as current value.
                    self.storage
                        .entry(key)
                        .or_insert(StorageSlot::new_changed(value, U256::ZERO))
                        .present_value = value;
                }
            }
        }
        false
    }

    /// Update to new state and generate AccountRevert that if applied to new state will
    /// revert it to previous state. If no revert is present, update is noop.
    pub fn update_and_create_revert(
        &mut self,
        transition: TransitionAccount,
    ) -> Option<AccountRevert> {
        let updated_info = transition.info;
        let updated_storage = transition.storage;
        let updated_status = transition.status;

        // the helper that extends this storage but preserves original value.
        let extend_storage =
            |this_storage: &mut StorageWithOriginalValues,
             storage_update: StorageWithOriginalValues| {
                for (key, value) in storage_update {
                    this_storage.entry(key).or_insert(value).present_value = value.present_value;
                }
            };

        let previous_storage_from_update =
            |updated_storage: &StorageWithOriginalValues| -> HashMap<U256, RevertToSlot> {
                updated_storage
                    .iter()
                    .filter(|s| s.1.previous_or_original_value != s.1.present_value)
                    .map(|(key, value)| {
                        (*key, RevertToSlot::Some(value.previous_or_original_value))
                    })
                    .collect()
            };

        // Needed for some reverts.
        let info_revert = if self.info != updated_info {
            AccountInfoRevert::RevertTo(self.info.clone().unwrap_or_default())
        } else {
            AccountInfoRevert::DoNothing
        };

        let account_revert = match updated_status {
            AccountStatus::Changed => {
                let previous_storage = previous_storage_from_update(&updated_storage);
                match self.status {
                    AccountStatus::Changed | AccountStatus::Loaded => {
                        // extend the storage. original values is not used inside bundle.
                        extend_storage(&mut self.storage, updated_storage);
                    }
                    AccountStatus::LoadedEmptyEIP161 => {
                        // Do nothing.
                        // Only change that can happen from LoadedEmpty to Changed is if balance
                        // is send to account. So we are only checking account change here.
                    }
                    _ => unreachable!("Invalid state transfer to Changed from {self:?}"),
                };
                let previous_status = self.status;
                self.status = AccountStatus::Changed;
                self.info = updated_info;
                Some(AccountRevert {
                    account: info_revert,
                    storage: previous_storage,
                    previous_status,
                })
            }
            AccountStatus::InMemoryChange => {
                let previous_storage = previous_storage_from_update(&updated_storage);
                let in_memory_info_revert = match self.status {
                    AccountStatus::Loaded | AccountStatus::InMemoryChange => {
                        // from loaded (Or LoadedEmpty) to InMemoryChange can happen if there is balance change
                        // or new created account but Loaded didn't have contract.
                        extend_storage(&mut self.storage, updated_storage);
                        info_revert
                    }
                    AccountStatus::LoadedEmptyEIP161 => {
                        self.storage = updated_storage;
                        info_revert
                    }
                    AccountStatus::LoadedNotExisting => {
                        self.storage = updated_storage;
                        AccountInfoRevert::DeleteIt
                    }
                    _ => unreachable!("Invalid change to InMemoryChange from {self:?}"),
                };
                let previous_status = self.status;
                self.status = AccountStatus::InMemoryChange;
                self.info = updated_info;
                Some(AccountRevert {
                    account: in_memory_info_revert,
                    storage: previous_storage,
                    previous_status,
                })
            }
            AccountStatus::Loaded
            | AccountStatus::LoadedNotExisting
            | AccountStatus::LoadedEmptyEIP161 => {
                // No changeset, maybe just update data
                // Do nothing for now.
                None
            }
        };

        account_revert.and_then(|acc| if acc.is_empty() { None } else { Some(acc) })
    }
}
