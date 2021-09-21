use cosmwasm_std::testing::mock_dependencies;
use cosmwasm_std::testing::{mock_env, mock_info};
use services::community::InstantiateMsg;

use crate::state::{load_config, Config};

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        governance_contract_addr: "addr0001".to_string(),
        psi_token_addr: "addr0002".to_string(),
    };

    let env = mock_env();
    let info = mock_info("addr0010", &[]);
    crate::contract::instantiate(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();

    let config: Config = load_config(deps.as_ref().storage).unwrap();
    assert_eq!(msg.governance_contract_addr, config.governance_contract);
    assert_eq!(msg.psi_token_addr, config.psi_token);
}
