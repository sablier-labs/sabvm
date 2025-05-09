//! Database that is split on State and BlockHash traits.
pub mod block_hash;
pub mod state;

pub use block_hash::{BlockHash, BlockHashRef};
pub use state::{State, StateRef};

use super::DatabaseCommit;
use crate::{
    db::{Database, DatabaseRef},
    state::EvmState,
    AccountInfo, Address, Bytecode, B256, U256,
};
use std::vec::Vec;

#[derive(Debug)]
pub struct DatabaseComponents<S, BH> {
    pub state: S,
    pub block_hash: BH,
}

#[derive(Debug)]
pub enum DatabaseComponentError<SE, BHE> {
    State(SE),
    BlockHash(BHE),
}

impl<S: State, BH: BlockHash> Database for DatabaseComponents<S, BH> {
    type Error = DatabaseComponentError<S::Error, BH::Error>;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        self.state.basic(address).map_err(Self::Error::State)
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        self.state
            .code_by_hash(code_hash)
            .map_err(Self::Error::State)
    }

    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
        self.state
            .storage(address, index)
            .map_err(Self::Error::State)
    }

    fn block_hash(&mut self, number: U256) -> Result<B256, Self::Error> {
        self.block_hash
            .block_hash(number)
            .map_err(Self::Error::BlockHash)
    }

    fn get_token_ids(&self) -> Result<Vec<U256>, Self::Error> {
        self.state.get_token_ids().map_err(Self::Error::State)
    }

    fn is_token_id_valid(&self, token_id: U256) -> Result<bool, Self::Error> {
        self.state
            .is_token_id_valid(token_id)
            .map_err(Self::Error::State)
    }
}

impl<S: StateRef, BH: BlockHashRef> DatabaseRef for DatabaseComponents<S, BH> {
    type Error = DatabaseComponentError<S::Error, BH::Error>;

    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        self.state.basic(address).map_err(Self::Error::State)
    }

    fn code_by_hash_ref(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        self.state
            .code_by_hash(code_hash)
            .map_err(Self::Error::State)
    }

    fn storage_ref(&self, address: Address, index: U256) -> Result<U256, Self::Error> {
        self.state
            .storage(address, index)
            .map_err(Self::Error::State)
    }

    fn block_hash_ref(&self, number: U256) -> Result<B256, Self::Error> {
        self.block_hash
            .block_hash(number)
            .map_err(Self::Error::BlockHash)
    }

    fn is_token_id_valid_ref(&self, token_id: U256) -> Result<bool, Self::Error> {
        self.state
            .is_token_id_valid(token_id)
            .map_err(Self::Error::State)
    }

    fn get_token_ids_ref(&self) -> Result<Vec<U256>, Self::Error> {
        self.state.get_token_ids().map_err(Self::Error::State)
    }
}

impl<S: DatabaseCommit, BH: BlockHashRef> DatabaseCommit for DatabaseComponents<S, BH> {
    fn commit(&mut self, changes: EvmState) {
        self.state.commit(changes);
    }
}
