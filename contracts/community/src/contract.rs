use crate::{
    commands,
    error::ContractError,
    queries,
    state::{load_config, store_config, Config},
    ContractResult,
};
use cw20::Cw20ExecuteMsg;

use cosmwasm_std::{
    entry_point, to_binary, BankMsg, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    Response, StdResult, SubMsg, Uint128, WasmMsg,
};

use services::community::{ExecuteMsg, GovernanceMsg, InstantiateMsg, MigrateMsg, QueryMsg};

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
                } => commands::update_config(deps, config, governance_contract_addr),

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

#[entry_point]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(
        Response::new().add_submessage(SubMsg::new(WasmMsg::Execute {
            contract_addr: "terra178v546c407pdnx5rer3hu8s2c0fc924k74ymnn".to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: "terra1s5wkurdh4sw47lgnk5em4h69v5vh9dncmkhyrg".to_string(),
                amount: Uint128::from(10836721u128),
            })?,
            funds: vec![],
        })),
    )
}
