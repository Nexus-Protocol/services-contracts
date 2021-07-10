use cosmwasm_std::{attr, to_binary, CosmosMsg, DepsMut, Response, StdError, Uint128, WasmMsg};

use crate::{
    state::{store_config, Config},
    ContractResult,
};
use cw20_base::msg::ExecuteMsg as Cw20ExecuteMsg;

pub fn update_config(
    deps: DepsMut,
    mut current_config: Config,
    governance_contract_addr: Option<String>,
    spend_limit: Option<Uint128>,
) -> ContractResult<Response> {
    if let Some(ref governance_contract_addr) = governance_contract_addr {
        current_config.governance_contract = deps.api.addr_validate(governance_contract_addr)?;
    }

    if let Some(spend_limit) = spend_limit {
        current_config.spend_limit = spend_limit;
    }

    store_config(deps.storage, &current_config)?;
    Ok(Response::default())
}

/// Spend
/// Governance can execute spend operation to send
/// `amount` of PSI token to `recipient` for community purpose
pub fn spend(
    _deps: DepsMut,
    config: Config,
    recipient: String,
    amount: Uint128,
) -> ContractResult<Response> {
    if config.spend_limit < amount {
        return Err(StdError::generic_err("Cannot spend more than spend_limit").into());
    }

    Ok(Response {
        submessages: vec![],
        messages: vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.psi_token.to_string(),
            send: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: recipient.clone(),
                amount,
            })?,
        })],
        attributes: vec![
            attr("action", "spend"),
            attr("recipient", recipient),
            attr("amount", amount),
        ],
        data: None,
    })
}
