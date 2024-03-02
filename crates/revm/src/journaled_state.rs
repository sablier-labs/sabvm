use crate::interpreter::InstructionResult;
use crate::primitives::{
    db::Database, hash_map::Entry, Account, Address, Asset, Bytecode, HashMap, Log, Spec,
    SpecId::*, State, StorageSlot, TransientStorage, B256, KECCAK_EMPTY, PRECOMPILE3, U256,
};
use core::mem;
use revm_interpreter::primitives::SpecId;

/// JournalState is internal EVM state that is used to contain state and track changes to that state.
/// It contains journal of changes that happened to state so that they can be reverted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct JournaledState {
    /// Current state.
    pub state: State,
    /// EIP 1153 transient storage
    pub transient_storage: TransientStorage,
    /// logs
    pub logs: Vec<Log>,
    /// how deep are we in call stack.
    pub depth: usize,
    /// journal with changes that happened between calls.
    pub journal: Vec<Vec<JournalEntry>>,
    /// Ethereum before EIP-161 differently defined empty and not-existing account
    /// Spec is needed for two things SpuriousDragon's `EIP-161 State clear`,
    /// and for Cancun's `EIP-6780: SELFDESTRUCT in same transaction`
    pub spec: SpecId,
    /// Precompiles addresses are used to check if loaded address
    /// should be considered cold or hot loaded. It is cloned from
    /// EvmContext to be directly accessed from JournaledState.
    ///
    /// Note that addresses are sorted.
    pub precompile_addresses: Vec<Address>,
}

impl JournaledState {
    /// Create new JournaledState.
    ///
    /// precompile_addresses is used to determine if address is precompile or not.
    ///
    /// Note: This function will journal state after Spurious Dragon fork.
    /// And will not take into account if account is not existing or empty.
    ///
    /// # Note
    ///
    /// Precompile addresses should be sorted.
    pub fn new(spec: SpecId, precompile_addresses: Vec<Address>) -> JournaledState {
        Self {
            state: HashMap::new(),
            transient_storage: TransientStorage::default(),
            logs: Vec::new(),
            journal: vec![vec![]],
            depth: 0,
            spec,
            precompile_addresses,
        }
    }

    /// Return reference to state.
    #[inline]
    pub fn state(&mut self) -> &mut State {
        &mut self.state
    }

    /// Mark account as touched as only touched accounts will be added to state.
    /// This is especially important for state clear where touched empty accounts needs to
    /// be removed from state.
    #[inline]
    pub fn touch(&mut self, address: &Address) {
        if let Some(account) = self.state.get_mut(address) {
            Self::touch_account(self.journal.last_mut().unwrap(), address, account);
        }
    }

    /// Mark account as touched.
    #[inline]
    fn touch_account(journal: &mut Vec<JournalEntry>, address: &Address, account: &mut Account) {
        if !account.is_touched() {
            journal.push(JournalEntry::AccountTouched { address: *address });
            account.mark_touch();
        }
    }

    /// Does cleanup and returns modified state.
    #[inline]
    pub fn finalize(&mut self) -> (State, Vec<Log>) {
        let state = mem::take(&mut self.state);

        let logs = mem::take(&mut self.logs);
        self.journal = vec![vec![]];
        self.depth = 0;
        (state, logs)
    }

    /// Returns the _loaded_ [Account] for the given address.
    ///
    /// This assumes that the account has already been loaded.
    ///
    /// # Panics
    ///
    /// Panics if the account has not been loaded and is missing from the state set.
    #[inline]
    pub fn account(&self, address: Address) -> &Account {
        self.state
            .get(&address)
            .expect("Account expected to be loaded") // Always assume that acc is already loaded
    }

    /// Returns call depth.
    #[inline]
    pub fn depth(&self) -> u64 {
        self.depth as u64
    }

    /// use it only if you know that acc is warm
    /// Assume account is warm
    #[inline]
    pub fn set_code(&mut self, address: Address, code: Bytecode) {
        self.touch(&address);

        self.journal
            .last_mut()
            .unwrap()
            .push(JournalEntry::CodeChange { address });

        let account = self.state.get_mut(&address).unwrap();
        account.info.code_hash = code.hash_slow();
        account.info.code = Some(code);
    }

    pub fn inc_nonce(&mut self, address: Address) -> Option<u64> {
        let account = self.state.get_mut(&address).unwrap();
        // Check if nonce is going to overflow.
        if account.info.nonce == u64::MAX {
            return None;
        }

        Self::touch_account(self.journal.last_mut().unwrap(), &address, account);
        self.journal
            .last_mut()
            .unwrap()
            .push(JournalEntry::NonceChange { address });

        account.info.nonce += 1;

        Some(account.info.nonce)
    }

    /// Transfers assets between 2 accounts. Returns error if sender balance is not enough.
    #[inline]
    pub fn transfer<DB: Database>(
        &mut self,
        from: &Address,
        to: &Address,
        assets: &Vec<Asset>,
        db: &mut DB,
    ) -> Result<(), InstructionResult> {
        // load accounts
        self.load_account(*from, db)
            .map_err(|_| InstructionResult::FatalExternalError)?;

        self.load_account(*to, db)
            .map_err(|_| InstructionResult::FatalExternalError)?;

        for asset in assets {
            let asset_id = asset.id;
            let asset_amount = asset.amount;

            // sub amount from
            let from_account = self.state.get_mut(from).unwrap();
            Self::touch_account(self.journal.last_mut().unwrap(), from, from_account);
            let from_balance = &mut from_account.info.get_balance(asset_id);
            *from_balance = from_balance
                .checked_sub(asset_amount)
                .ok_or(InstructionResult::OutOfFunds)?;

            // add amount to
            let to_account = self.state.get_mut(to).unwrap();
            Self::touch_account(self.journal.last_mut().unwrap(), to, to_account);
            let to_balance = &mut to_account.info.get_balance(asset_id);
            *to_balance = to_balance
                .checked_add(asset_amount)
                .ok_or(InstructionResult::OverflowPayment)?;
            // Overflow of U256 balance is not possible to happen on mainnet. We don't bother to return funds from from_acc.

            self.journal
                .last_mut()
                .unwrap()
                .push(JournalEntry::BalanceTransfer {
                    from: *from,
                    to: *to,
                    asset_id,
                    asset_amount,
                });
        }

        Ok(())
    }

    /// Create account or return false if collision is detected.
    ///
    /// There are few steps done:
    /// 1. Make created account warm loaded (AccessList) and this should
    ///     be done before subroutine checkpoint is created.
    /// 2. Check if there is collision of newly created account with existing one.
    /// 3. Mark created account as created.
    /// 4. Increment nonce of created account if SpuriousDragon is active
    /// 5. Add funds to the created account
    /// 6. Decrease balances of the caller account.
    ///
    /// # Panics
    ///
    /// Panics if the caller is not loaded inside of the EVM state.
    /// This is should have been done inside `create_inner`.
    #[inline]
    pub fn create_account_checkpoint<SPEC: Spec>(
        &mut self,
        caller: Address,
        address: Address,
        transferred_assets: &Vec<Asset>,
    ) -> Result<JournalCheckpoint, InstructionResult> {
        // Enter subroutine
        let checkpoint = self.checkpoint();

        // Newly created account is present, as we just loaded it.
        let account = self.state.get_mut(&address).unwrap();
        let last_journal = self.journal.last_mut().unwrap();

        // New account can be created if:
        // Bytecode is not empty.
        // Nonce is not zero
        // Account is not precompile.
        if account.info.code_hash != KECCAK_EMPTY
            || account.info.nonce != 0
            || self.precompile_addresses.binary_search(&address).is_ok()
        {
            self.checkpoint_revert(checkpoint);
            return Err(InstructionResult::CreateCollision);
        }

        // set account status to created.
        account.mark_created();

        // this entry will revert set nonce.
        last_journal.push(JournalEntry::AccountCreated { address });
        account.info.code = None;

        // Set all storages to default value. They need to be present to act as accessed slots in access list.
        // it shouldn't be possible for them to have different values then zero as code is not existing for this account,
        // but because tests can change that assumption we are doing it.
        let empty = StorageSlot::default();
        account
            .storage
            .iter_mut()
            .for_each(|(_, slot)| *slot = empty.clone());

        // touch account. This is important as for pre SpuriousDragon account could be
        // saved even empty.
        Self::touch_account(last_journal, &address, account);

        // EIP-161: State trie clearing (invariant-preserving alternative)
        if SPEC::enabled(SPURIOUS_DRAGON) {
            // nonce is going to be reset to zero in AccountCreated journal entry.
            account.info.nonce = 1;
        }

        for asset in transferred_assets {
            let asset_id = asset.id;
            let asset_amount = asset.amount;
            let account = self.state.get_mut(&address).unwrap();

            // Add asset amount to created account, as we already have target here.
            let Some(new_balance) = account.info.get_balance(asset_id).checked_add(asset_amount)
            else {
                self.checkpoint_revert(checkpoint);
                return Err(InstructionResult::OverflowPayment);
            };
            account.info.set_balance(asset_id, new_balance);

            // Sub asset amount from caller
            let caller_account = self.state.get_mut(&caller).unwrap();
            // Balance is already checked in `create_inner`, so it is safe to just subtract.
            caller_account.info.decrease_balance(asset_id, asset_amount);

            // add journal entry of the transferred asset
            last_journal.push(JournalEntry::BalanceTransfer {
                from: caller,
                to: address,
                asset_id,
                asset_amount,
            });
        }

        Ok(checkpoint)
    }

    /// Revert all changes that happened in given journal entries.
    #[inline]
    fn journal_revert(
        state: &mut State,
        transient_storage: &mut TransientStorage,
        journal_entries: Vec<JournalEntry>,
        is_spurious_dragon_enabled: bool,
    ) {
        for entry in journal_entries.into_iter().rev() {
            match entry {
                JournalEntry::AccountLoaded { address } => {
                    state.remove(&address);
                }
                JournalEntry::AccountTouched { address } => {
                    if is_spurious_dragon_enabled && address == PRECOMPILE3 {
                        continue;
                    }
                    // remove touched status
                    state.get_mut(&address).unwrap().unmark_touch();
                }
                JournalEntry::BalanceTransfer {
                    from,
                    to,
                    asset_id,
                    asset_amount,
                } => {
                    // we don't need to check overflow and underflow when adding and subtracting the balance.
                    let from = state.get_mut(&from).unwrap();
                    from.info.increase_balance(asset_id, asset_amount);
                    let to = state.get_mut(&to).unwrap();
                    to.info.decrease_balance(asset_id, asset_amount);
                }
                JournalEntry::NonceChange { address } => {
                    state.get_mut(&address).unwrap().info.nonce -= 1;
                }
                JournalEntry::AccountCreated { address } => {
                    let account = &mut state.get_mut(&address).unwrap();
                    account.unmark_created();
                    account.info.nonce = 0;
                }
                JournalEntry::StorageChange {
                    address,
                    key,
                    had_value,
                } => {
                    let storage = &mut state.get_mut(&address).unwrap().storage;
                    if let Some(had_value) = had_value {
                        storage.get_mut(&key).unwrap().present_value = had_value;
                    } else {
                        storage.remove(&key);
                    }
                }
                JournalEntry::TransientStorageChange {
                    address,
                    key,
                    had_value,
                } => {
                    let tkey = (address, key);
                    if had_value == U256::ZERO {
                        // if previous value is zero, remove it
                        transient_storage.remove(&tkey);
                    } else {
                        // if not zero, reinsert old value to transient storage.
                        transient_storage.insert(tkey, had_value);
                    }
                }
                JournalEntry::CodeChange { address } => {
                    let acc = state.get_mut(&address).unwrap();
                    acc.info.code_hash = KECCAK_EMPTY;
                    acc.info.code = None;
                }
                JournalEntry::AssetsMinted {
                    minter,
                    asset_id,
                    minted_amount,
                } => {
                    let minter_acc = state.get_mut(&minter).unwrap();
                    // TODO: is there a guarantee that the assets have, actually, been minted before the tx reverted?
                    minter_acc.info.decrease_balance(asset_id, minted_amount);
                }
                JournalEntry::AssetsBurned {
                    burner,
                    asset_id,
                    burned_amount,
                } => {
                    let receiver = state.get_mut(&burner).unwrap();
                    receiver.info.increase_balance(asset_id, burned_amount);
                }
            }
        }
    }

    /// Makes a checkpoint that in case of Revert can bring back state to this point.
    #[inline]
    pub fn checkpoint(&mut self) -> JournalCheckpoint {
        let checkpoint = JournalCheckpoint {
            log_i: self.logs.len(),
            journal_i: self.journal.len(),
        };
        self.depth += 1;
        self.journal.push(Default::default());
        checkpoint
    }

    /// Commit the checkpoint.
    #[inline]
    pub fn checkpoint_commit(&mut self) {
        self.depth -= 1;
    }

    /// Reverts all changes to state until given checkpoint.
    #[inline]
    pub fn checkpoint_revert(&mut self, checkpoint: JournalCheckpoint) {
        let is_spurious_dragon_enabled = SpecId::enabled(self.spec, SPURIOUS_DRAGON);
        let state = &mut self.state;
        let transient_storage = &mut self.transient_storage;
        self.depth -= 1;
        // iterate over last N journals sets and revert our global state
        let leng = self.journal.len();
        self.journal
            .iter_mut()
            .rev()
            .take(leng - checkpoint.journal_i)
            .for_each(|cs| {
                Self::journal_revert(
                    state,
                    transient_storage,
                    mem::take(cs),
                    is_spurious_dragon_enabled,
                )
            });

        self.logs.truncate(checkpoint.log_i);
        self.journal.truncate(checkpoint.journal_i);
    }

    /// Initial load of account. This load will not be tracked inside journal
    #[inline]
    pub fn initial_account_load<DB: Database>(
        &mut self,
        address: Address,
        slots: &[U256],
        db: &mut DB,
    ) -> Result<&mut Account, DB::Error> {
        // load or get account.
        let account = match self.state.entry(address) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(vac) => vac.insert(
                db.basic(address)?
                    .map(|i| i.into())
                    .unwrap_or(Account::new_not_existing()),
            ),
        };
        // preload storages.
        for slot in slots {
            if let Entry::Vacant(entry) = account.storage.entry(*slot) {
                let storage = db.storage(address, *slot)?;
                entry.insert(StorageSlot::new(storage));
            }
        }
        Ok(account)
    }

    /// load account into memory. return if it is cold or warm accessed
    #[inline]
    pub fn load_account<DB: Database>(
        &mut self,
        address: Address,
        db: &mut DB,
    ) -> Result<(&mut Account, bool), DB::Error> {
        Ok(match self.state.entry(address) {
            Entry::Occupied(entry) => (entry.into_mut(), false),
            Entry::Vacant(vac) => {
                let account = if let Some(account) = db.basic(address)? {
                    account.into()
                } else {
                    Account::new_not_existing()
                };

                // journal loading of account. AccessList touch.
                self.journal
                    .last_mut()
                    .unwrap()
                    .push(JournalEntry::AccountLoaded { address });

                // precompiles are warm loaded so we need to take that into account
                let is_cold = self.precompile_addresses.binary_search(&address).is_err();

                (vac.insert(account), is_cold)
            }
        })
    }

    /// Load account from database to JournaledState.
    ///
    /// Return boolean pair where first is `is_cold`` second bool `is_exists`.
    #[inline]
    pub fn load_account_exist<DB: Database>(
        &mut self,
        address: Address,
        db: &mut DB,
    ) -> Result<(bool, bool), DB::Error> {
        let is_spurious_dragon_enabled = SpecId::enabled(self.spec, SPURIOUS_DRAGON);
        let (acc, is_cold) = self.load_account(address, db)?;

        let exist = if is_spurious_dragon_enabled {
            !acc.is_empty()
        } else {
            let is_existing = !acc.is_loaded_as_not_existing();
            let is_touched = acc.is_touched();
            is_existing || is_touched
        };
        Ok((is_cold, exist))
    }

    /// Loads code.
    #[inline]
    pub fn load_code<DB: Database>(
        &mut self,
        address: Address,
        db: &mut DB,
    ) -> Result<(&mut Account, bool), DB::Error> {
        let (acc, is_cold) = self.load_account(address, db)?;
        if acc.info.code.is_none() {
            if acc.info.code_hash == KECCAK_EMPTY {
                let empty = Bytecode::new();
                acc.info.code = Some(empty);
            } else {
                let code = db.code_by_hash(acc.info.code_hash)?;
                acc.info.code = Some(code);
            }
        }
        Ok((acc, is_cold))
    }

    /// Load storage slot
    ///
    /// # Note
    ///
    /// Account is already present and loaded.
    #[inline]
    pub fn sload<DB: Database>(
        &mut self,
        address: Address,
        key: U256,
        db: &mut DB,
    ) -> Result<(U256, bool), DB::Error> {
        let account = self.state.get_mut(&address).unwrap(); // assume acc is warm
                                                             // only if account is created in this tx we can assume that storage is empty.
        let is_newly_created = account.is_created();
        let load = match account.storage.entry(key) {
            Entry::Occupied(occ) => (occ.get().present_value, false),
            Entry::Vacant(vac) => {
                // if storage was cleared, we don't need to ping db.
                let value = if is_newly_created {
                    U256::ZERO
                } else {
                    db.storage(address, key)?
                };
                // add it to journal as cold loaded.
                self.journal
                    .last_mut()
                    .unwrap()
                    .push(JournalEntry::StorageChange {
                        address,
                        key,
                        had_value: None,
                    });

                vac.insert(StorageSlot::new(value));

                (value, true)
            }
        };
        Ok(load)
    }

    /// Stores storage slot.
    /// And returns (original,present,new) slot value.
    ///
    /// Note:
    ///
    /// account should already be present in our state.
    #[inline]
    pub fn sstore<DB: Database>(
        &mut self,
        address: Address,
        key: U256,
        new: U256,
        db: &mut DB,
    ) -> Result<(U256, U256, U256, bool), DB::Error> {
        // assume that acc exists and load the slot.
        let (present, is_cold) = self.sload(address, key, db)?;
        let acc = self.state.get_mut(&address).unwrap();

        // if there is no original value in dirty return present value, that is our original.
        let slot = acc.storage.get_mut(&key).unwrap();

        // new value is same as present, we don't need to do anything
        if present == new {
            return Ok((slot.previous_or_original_value, present, new, is_cold));
        }

        self.journal
            .last_mut()
            .unwrap()
            .push(JournalEntry::StorageChange {
                address,
                key,
                had_value: Some(present),
            });
        // insert value into present state.
        slot.present_value = new;
        Ok((slot.previous_or_original_value, present, new, is_cold))
    }

    /// Read transient storage tied to the account.
    ///
    /// EIP-1153: Transient storage opcodes
    #[inline]
    pub fn tload(&mut self, address: Address, key: U256) -> U256 {
        self.transient_storage
            .get(&(address, key))
            .copied()
            .unwrap_or_default()
    }

    /// Store transient storage tied to the account.
    ///
    /// If values is different add entry to the journal
    /// so that old state can be reverted if that action is needed.
    ///
    /// EIP-1153: Transient storage opcodes
    #[inline]
    pub fn tstore(&mut self, address: Address, key: U256, new: U256) {
        let had_value = if new == U256::ZERO {
            // if new values is zero, remove entry from transient storage.
            // if previous values was some insert it inside journal.
            // If it is none nothing should be inserted.
            self.transient_storage.remove(&(address, key))
        } else {
            // insert values
            let previous_value = self
                .transient_storage
                .insert((address, key), new)
                .unwrap_or_default();

            // check if previous value is same
            if previous_value != new {
                // if it is different, insert previous values inside journal.
                Some(previous_value)
            } else {
                None
            }
        };

        if let Some(had_value) = had_value {
            // insert in journal only if value was changed.
            self.journal
                .last_mut()
                .unwrap()
                .push(JournalEntry::TransientStorageChange {
                    address,
                    key,
                    had_value,
                });
        }
    }

    /// push log into subroutine
    #[inline]
    pub fn log(&mut self, log: Log) {
        self.logs.push(log);
    }

    pub fn mint<DB: Database>(
        &mut self,
        minter: Address,
        asset_id: B256,
        amount: U256,
        db: &mut DB,
    ) -> bool {
        // TODO: is loading the account really necessary?
        if self.load_account(minter, db).is_err() {
            return false;
        }

        let account = self.state.get_mut(&minter).unwrap();
        let balance = account.info.get_balance(asset_id);
        if let Some(new_balance) = balance.checked_add(amount) {
            account.info.set_balance(asset_id, new_balance);
        } else {
            return false;
        }

        // add journal entry of the minted assets
        self.journal
            .last_mut()
            .unwrap()
            .push(JournalEntry::AssetsMinted {
                minter,
                asset_id,
                minted_amount: amount,
            });

        true
    }

    pub fn burn<DB: Database>(
        &mut self,
        burner: Address,
        asset_id: B256,
        amount: U256,
        db: &mut DB,
    ) -> bool {
        // TODO: is loading the account really necessary?
        if self.load_account(burner, db).is_err() {
            return false;
        }

        let account = self.state.get_mut(&burner).unwrap();
        let balance = account.info.get_balance(asset_id);
        if let Some(new_balance) = balance.checked_sub(amount) {
            account.info.set_balance(asset_id, new_balance);
        } else {
            return false;
        }

        // add journal entry of the minted assets
        self.journal
            .last_mut()
            .unwrap()
            .push(JournalEntry::AssetsBurned {
                burner,
                asset_id,
                burned_amount: amount,
            });

        true
    }
}

/// Journal entries that are used to track changes to the state and are used to revert it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum JournalEntry {
    /// Used to mark account that is warm inside EVM in regards to EIP-2929 AccessList.
    /// Action: We will add Account to state.
    /// Revert: we will remove account from state.
    AccountLoaded { address: Address },
    /// Loading account does not mean that account will need to be added to MerkleTree (touched).
    /// Only when account is called (to execute contract or transfer balance) only then account is made touched.
    /// Action: Mark account touched
    /// Revert: Unmark account touched
    AccountTouched { address: Address },
    /// Transfer balance between two accounts
    /// Action: Transfer balance
    /// Revert: Transfer balance back
    BalanceTransfer {
        from: Address,
        to: Address,
        asset_id: B256,
        asset_amount: U256,
    },
    /// Assets minted
    /// Action: Mint assets
    /// Revert: Remove minted assets
    AssetsMinted {
        minter: Address,
        asset_id: B256,
        minted_amount: U256,
    },
    /// Assets burned
    /// Action: Burn assets
    /// Revert: Refund the burned assets
    AssetsBurned {
        burner: Address,
        asset_id: B256,
        burned_amount: U256,
    },
    /// Increment nonce
    /// Action: Increment nonce by one
    /// Revert: Decrement nonce by one
    NonceChange {
        address: Address, //geth has nonce value,
    },
    /// Create account:
    /// Actions: Mark account as created
    /// Revert: Unmart account as created and reset nonce to zero.
    AccountCreated { address: Address },
    /// It is used to track both storage change and warm load of storage slot. For warm load in regard
    /// to EIP-2929 AccessList had_value will be None
    /// Action: Storage change or warm load
    /// Revert: Revert to previous value or remove slot from storage
    StorageChange {
        address: Address,
        key: U256,
        had_value: Option<U256>, //if none, storage slot was cold loaded from db and needs to be removed
    },
    /// It is used to track an EIP-1153 transient storage change.
    /// Action: Transient storage changed.
    /// Revert: Revert to previous value.
    TransientStorageChange {
        address: Address,
        key: U256,
        had_value: U256,
    },
    /// Code changed
    /// Action: Account code changed
    /// Revert: Revert to previous bytecode.
    CodeChange { address: Address },
}

/// SubRoutine checkpoint that will help us to go back from this
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct JournalCheckpoint {
    log_i: usize,
    journal_i: usize,
}
