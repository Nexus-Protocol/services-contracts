use cosmwasm_std::testing::mock_dependencies;
use cosmwasm_std::testing::{mock_env, mock_info};
use services::community::{ExecuteMsg, GovernanceMsg, InstantiateMsg};

use crate::error::ContractError;
use crate::state::load_config;

#[test]
fn fail_to_change_config_if_sender_is_not_governance() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        governance_contract_addr: "addr0001".to_string(),
        psi_token_addr: "addr0002".to_string(),
    };

    let env = mock_env();
    let info = mock_info("addr0010", &[]);
    crate::contract::instantiate(deps.as_mut(), env, info, msg.clone()).unwrap();

    // ====================================
    // ====================================
    // ====================================

    let new_governance_contract_addr = Some("addr9998".to_string());

    let change_config_msg = ExecuteMsg::Governance {
        governance_msg: GovernanceMsg::UpdateConfig {
            governance_contract_addr: new_governance_contract_addr,
        },
    };

    let env = mock_env();
    let info = mock_info("addr0010", &[]);
    let res = crate::contract::execute(deps.as_mut(), env, info, change_config_msg);
    assert!(res.is_err());
    assert_eq!(ContractError::Unauthorized, res.err().unwrap());
}

#[test]
fn success_to_change_config_if_sender_governance() {
    let mut deps = mock_dependencies(&[]);
    let old_governance_addr = "addr0001".to_string();

    let msg = InstantiateMsg {
        governance_contract_addr: old_governance_addr.clone(),
        psi_token_addr: "addr0002".to_string(),
    };

    let env = mock_env();
    let info = mock_info("addr0010", &[]);
    crate::contract::instantiate(deps.as_mut(), env, info, msg.clone()).unwrap();

    // ====================================
    // ====================================
    // ====================================

    let new_governance_contract_addr = "addr9998".to_string();

    let change_config_msg = ExecuteMsg::Governance {
        governance_msg: GovernanceMsg::UpdateConfig {
            governance_contract_addr: Some(new_governance_contract_addr.clone()),
        },
    };

    let env = mock_env();
    let info = mock_info(&old_governance_addr, &[]);
    crate::contract::execute(deps.as_mut(), env, info, change_config_msg).unwrap();

    let config = load_config(&deps.storage).unwrap();
    assert_eq!(new_governance_contract_addr, config.governance_contract);
}
