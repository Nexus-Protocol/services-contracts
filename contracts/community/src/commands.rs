use cosmwasm_std::{to_binary, DepsMut, Response, Uint128, WasmMsg};

use crate::{
    state::{store_config, Config},
    ContractResult,
};
use cw20::Cw20ExecuteMsg;

pub fn update_config(
    deps: DepsMut,
    mut current_config: Config,
    governance_contract_addr: Option<String>,
) -> ContractResult<Response> {
    if let Some(ref governance_contract_addr) = governance_contract_addr {
        current_config.governance_contract = deps.api.addr_validate(governance_contract_addr)?;
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
    Ok(Response::new()
        .add_message(WasmMsg::Execute {
            contract_addr: config.psi_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: recipient.clone(),
                amount,
            })?,
        })
        .add_attributes(vec![
            ("action", "spend"),
            ("recipient", &recipient.to_string()),
            ("amount", &amount.to_string()),
        ]))
}
