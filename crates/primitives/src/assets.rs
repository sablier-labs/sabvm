use hashbrown::HashMap;

use crate::{B256, BASE_ASSET_ID, U256};

pub type Balances = HashMap<B256, U256>;

pub fn init_balances(base_asset_balance: u128) -> Balances {
    let mut balances = Balances::new();
    balances.insert(BASE_ASSET_ID, U256::from(base_asset_balance));
    balances
}
