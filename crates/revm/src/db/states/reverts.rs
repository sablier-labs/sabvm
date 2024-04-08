use super::{changes::PlainStorageRevert, AccountStatus, PlainStateReverts};
use core::ops::{Deref, DerefMut};
use revm_interpreter::primitives::{AccountInfo, Address, HashMap, U256};
use std::vec::Vec;

/// Contains reverts of multiple account in multiple transitions (Transitions as a block).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reverts(Vec<Vec<(Address, AccountRevert)>>);

impl Deref for Reverts {
    type Target = Vec<Vec<(Address, AccountRevert)>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Reverts {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Reverts {
    /// Create new reverts
    pub fn new(reverts: Vec<Vec<(Address, AccountRevert)>>) -> Self {
        Self(reverts)
    }

    /// Sort account inside transition by their address.
    pub fn sort(&mut self) {
        for revert in &mut self.0 {
            revert.sort_by_key(|(address, _)| *address);
        }
    }

    /// Extend reverts with other reverts.
    pub fn extend(&mut self, other: Reverts) {
        self.0.extend(other.0);
    }

    /// Consume reverts and create plain state reverts.
    ///
    /// Note that account are sorted by address.
    pub fn into_plain_state_reverts(mut self) -> PlainStateReverts {
        let mut state_reverts = PlainStateReverts::with_capacity(self.0.len());
        for reverts in self.0.drain(..) {
            // pessimistically pre-allocate assuming _all_ accounts changed.
            let mut accounts = Vec::with_capacity(reverts.len());
            let mut storage = Vec::with_capacity(reverts.len());
            for (address, revert_account) in reverts.into_iter() {
                match revert_account.account {
                    AccountInfoRevert::RevertTo(acc) => accounts.push((address, Some(acc))),
                    AccountInfoRevert::DeleteIt => accounts.push((address, None)),
                    AccountInfoRevert::DoNothing => (),
                }
                if revert_account.wipe_storage || !revert_account.storage.is_empty() {
                    storage.push(PlainStorageRevert {
                        address,
                        wiped: revert_account.wipe_storage,
                        storage_revert: revert_account.storage.into_iter().collect::<Vec<_>>(),
                    });
                }
            }
            state_reverts.accounts.push(accounts);
            state_reverts.storage.push(storage);
        }
        state_reverts
    }
}

/// Assumption is that Revert can return full state from any future state to any past state.
///
/// It is created when new account state is applied to old account state.
/// And it is used to revert new account state to the old account state.
///
/// AccountRevert is structured in this way as we need to save it inside database.
/// And we need to be able to read it from database.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct AccountRevert {
    pub account: AccountInfoRevert,
    pub storage: HashMap<U256, RevertToSlot>,
    pub previous_status: AccountStatus,
    pub wipe_storage: bool,
}

impl AccountRevert {
    /// The approximate size of changes needed to store this account revert.
    /// `1 + storage_reverts_len`
    pub fn size_hint(&self) -> usize {
        1 + self.storage.len()
    }

    /// Returns `true` if there is nothing to revert,
    /// by checking that:
    /// * both account info and storage have been left untouched
    /// * we don't need to wipe storage
    pub fn is_empty(&self) -> bool {
        self.account == AccountInfoRevert::DoNothing
            && self.storage.is_empty()
            && !self.wipe_storage
    }
}

/// Depending on previous state of account info this
/// will tell us what to do on revert.
#[derive(Clone, Default, Debug, PartialEq, Eq, Hash)]
pub enum AccountInfoRevert {
    #[default]
    /// Nothing changed
    DoNothing,
    /// Account was created and on revert we need to remove it with all storage.
    DeleteIt,
    /// Account was changed and on revert we need to put old state.
    RevertTo(AccountInfo),
}

/// So storage can have multiple types:
/// * Zero, on revert remove plain state.
/// * Value, on revert set this value
/// * Destroyed, should be removed on revert but on Revert set it as zero.
///
/// Note: It is completely different state if Storage is Zero or Some or if Storage was
/// Destroyed. Because if it is destroyed, previous values can be found in database or it can be zero.
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash)]
pub enum RevertToSlot {
    Some(U256),
    Destroyed,
}

impl RevertToSlot {
    pub fn to_previous_value(self) -> U256 {
        match self {
            RevertToSlot::Some(value) => value,
            RevertToSlot::Destroyed => U256::ZERO,
        }
    }
}
