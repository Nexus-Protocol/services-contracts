use crate::{
    commands,
    error::ContractError,
    queries,
    state::{load_config, store_config, Config},
    ContractResult,
};

use cosmwasm_std::{
    entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
};

use services::community::{ExecuteMsg, GovernanceMsg, InstantiateMsg, QueryMsg};

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> ContractResult<Response> {
    let config = Config {
        governance_contract: deps.api.addr_validate(&msg.governance_contract_addr)?,
        psi_token: deps.api.addr_validate(&msg.psi_token_addr)?,
        spend_limit: msg.spend_limit,
    };

    store_config(deps.storage, &config)?;

    Ok(Response::default())
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> ContractResult<Response> {
    match msg {
        ExecuteMsg::Governance { governance_msg } => {
            let config: Config = load_config(deps.storage)?;
            if info.sender != config.governance_contract {
                return Err(ContractError::Unauthorized);
            }

            match governance_msg {
                GovernanceMsg::UpdateConfig {
                    governance_contract_addr,
                    spend_limit,
                } => commands::update_config(deps, config, governance_contract_addr, spend_limit),

                GovernanceMsg::Spend { recipient, amount } => {
                    commands::spend(deps, config, recipient, amount)
                }
            }
        }
    }
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&queries::query_config(deps)?),
    }
}
