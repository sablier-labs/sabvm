//! State database component from [`crate::db::Database`]
//! it is used inside [`crate::db::DatabaseComponents`]

use crate::{AccountInfo, Address, Bytecode, B256, U256};
use auto_impl::auto_impl;
use core::ops::Deref;
use std::{sync::Arc, vec::Vec};

#[auto_impl(&mut, Box)]
pub trait State {
    type Error;

    /// Get basic account information.
    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error>;

    /// Get account code by its hash
    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error>;

    /// Get storage value of address at index.
    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error>;

    /// Get the supported token ids
    fn get_token_ids(&self) -> Result<Vec<U256>, Self::Error>;

    /// Check if token id is valid
    fn is_token_id_valid(&self, token_id: U256) -> Result<bool, Self::Error>;
}

#[auto_impl(&, &mut, Box, Rc, Arc)]
pub trait StateRef {
    type Error;

    /// Get basic account information.
    fn basic(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error>;

    /// Get account code by its hash
    fn code_by_hash(&self, code_hash: B256) -> Result<Bytecode, Self::Error>;

    /// Get storage value of address at index.
    fn storage(&self, address: Address, index: U256) -> Result<U256, Self::Error>;

    /// Get the supported token ids
    fn get_token_ids(&self) -> Result<Vec<U256>, Self::Error>;

    /// Check if token id is valid
    fn is_token_id_valid(&self, token_id: U256) -> Result<bool, Self::Error>;
}

impl<T> State for &T
where
    T: StateRef,
{
    type Error = <T as StateRef>::Error;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        StateRef::basic(*self, address)
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        StateRef::code_by_hash(*self, code_hash)
    }

    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
        StateRef::storage(*self, address, index)
    }

    /// Get the supported token ids
    fn get_token_ids(&self) -> Result<Vec<U256>, Self::Error> {
        StateRef::get_token_ids(*self)
    }

    /// Check if token id is valid
    fn is_token_id_valid(&self, token_id: U256) -> Result<bool, Self::Error> {
        StateRef::is_token_id_valid(*self, token_id)
    }
}

impl<T> State for Arc<T>
where
    T: StateRef,
{
    type Error = <T as StateRef>::Error;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        self.deref().basic(address)
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        self.deref().code_by_hash(code_hash)
    }

    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
        self.deref().storage(address, index)
    }

    fn get_token_ids(&self) -> Result<Vec<U256>, Self::Error> {
        self.deref().get_token_ids()
    }

    fn is_token_id_valid(&self, token_id: U256) -> Result<bool, Self::Error> {
        self.deref().is_token_id_valid(token_id)
    }
}
