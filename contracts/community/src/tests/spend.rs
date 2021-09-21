use cosmwasm_std::testing::mock_dependencies;
use cosmwasm_std::testing::{mock_env, mock_info};
use cosmwasm_std::{to_binary, CosmosMsg, SubMsg, Uint128, WasmMsg};
use cw20::Cw20ExecuteMsg;
use services::community::{ExecuteMsg, GovernanceMsg, InstantiateMsg};

use crate::error::ContractError;

#[test]
fn test_spend() {
    let mut deps = mock_dependencies(&[]);
    let governance_contract_addr = "addr0001".to_string();
    let spend_amount = Uint128::new(2000);
    let psi_token_addr = "addr0002".to_string();

    let msg = InstantiateMsg {
        governance_contract_addr: governance_contract_addr.clone(),
        psi_token_addr: psi_token_addr.clone(),
    };

    let env = mock_env();
    let info = mock_info("addr0010", &[]);
    crate::contract::instantiate(deps.as_mut(), env, info, msg).unwrap();

    // ====================================
    // ====================================
    // ====================================

    // permission failed
    {
        let spend_msg = ExecuteMsg::Governance {
            governance_msg: GovernanceMsg::Spend {
                recipient: "addr0000".to_string(),
                amount: spend_amount,
            },
        };

        let env = mock_env();
        let info = mock_info("addr0010", &[]);
        let res = crate::contract::execute(deps.as_mut(), env.clone(), info.clone(), spend_msg);
        assert!(res.is_err());
        assert_eq!(res.err().unwrap(), ContractError::Unauthorized);
    }

    // OK
    {
        let recipient_addr = "addr0010".to_string();
        let spend_msg = ExecuteMsg::Governance {
            governance_msg: GovernanceMsg::Spend {
                recipient: recipient_addr.clone(),
                amount: spend_amount,
            },
        };

        let env = mock_env();
        let info = mock_info(&governance_contract_addr.clone(), &[]);
        let res = crate::contract::execute(deps.as_mut(), env, info, spend_msg).unwrap();
        assert_eq!(
            res.messages,
            vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: psi_token_addr.clone(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: recipient_addr.clone(),
                    amount: spend_amount,
                })
                .unwrap(),
            }))]
        );
    }
}
