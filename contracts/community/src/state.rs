use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, StdResult, Storage, Uint128};
use cw_storage_plus::Item;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub governance_contract: Addr,
    pub psi_token: Addr,
    pub spend_limit: Uint128,
}

const CONFIG: Item<Config> = Item::new("config");

pub fn load_config(storage: &dyn Storage) -> StdResult<Config> {
    CONFIG.load(storage)
}

pub fn store_config(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    CONFIG.save(storage, config)
}
