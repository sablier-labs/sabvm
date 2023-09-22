/// After account get loaded from database it can be in a lot of different states
/// while we execute multiple transaction and even blocks over account that is in memory.
/// This structure models all possible states that account can be in.
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub enum AccountStatus {
    #[default]
    LoadedNotExisting,
    Loaded,
    LoadedEmptyEIP161,
    InMemoryChange,
    Changed,
}

impl AccountStatus {
    /// Account is modified.
    /// This means that some storage values can be found in both
    /// memory and database.
    pub fn modified(&self) -> bool {
        matches!(self, AccountStatus::Changed | AccountStatus::InMemoryChange)
    }

    /// Account is not modified and just loaded from database.
    pub fn not_modified(&self) -> bool {
        matches!(
            self,
            AccountStatus::LoadedNotExisting
                | AccountStatus::Loaded
                | AccountStatus::LoadedEmptyEIP161
        )
    }

    /// This means storage is known, a newly created account.
    pub fn storage_known(&self) -> bool {
        matches!(
            self,
            AccountStatus::LoadedNotExisting | AccountStatus::InMemoryChange
        )
    }

    /// Transition to other state while preserving invariance of this state.
    pub fn transition(&mut self, other: Self) {
        *self = other;
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_account_status() {
        // account not modified
        assert!(AccountStatus::Loaded.not_modified());
        assert!(AccountStatus::LoadedEmptyEIP161.not_modified());
        assert!(AccountStatus::LoadedNotExisting.not_modified());
        assert!(!AccountStatus::Changed.not_modified());
        assert!(!AccountStatus::InMemoryChange.not_modified());

        // we know full storage
        assert!(!AccountStatus::LoadedEmptyEIP161.storage_known());
        assert!(AccountStatus::LoadedNotExisting.storage_known());
        assert!(AccountStatus::InMemoryChange.storage_known());
        assert!(!AccountStatus::Loaded.storage_known());
        assert!(!AccountStatus::Changed.storage_known());

        // account modified
        assert!(AccountStatus::Changed.modified());
        assert!(AccountStatus::InMemoryChange.modified());
        assert!(!AccountStatus::Loaded.modified());
        assert!(!AccountStatus::LoadedEmptyEIP161.modified());
        assert!(!AccountStatus::LoadedNotExisting.modified());
    }
}
