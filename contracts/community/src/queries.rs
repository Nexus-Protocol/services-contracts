use cosmwasm_std::{Deps, StdResult};
use services::community::ConfigResponse;

use crate::state::load_config;
use crate::state::Config;

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config: Config = load_config(deps.storage)?;
    Ok(ConfigResponse {
        governance_contract_addr: config.governance_contract.to_string(),
        psi_token_addr: config.psi_token.to_string(),
    })
}
