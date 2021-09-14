use crate::contract::{execute, instantiate, query};
use crate::tests::mock_env_block_time;
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{from_binary, to_binary, CosmosMsg, StdError, SubMsg, Uint128, WasmMsg};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use services::staking::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg, StakingSchedule,
};

#[test]
fn test_migrate_staking() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000".to_string();

    let msg = InstantiateMsg {
        owner: owner.clone(),
        psi_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        terraswap_factory: "terraswap_factory0000".to_string(),
        distribution_schedule: vec![
            StakingSchedule::new(
                mock_env_block_time(),
                mock_env_block_time() + 100,
                Uint128::from(1000000u128),
            ),
            StakingSchedule::new(
                mock_env_block_time() + 100,
                mock_env_block_time() + 200,
                Uint128::from(10000000u128),
            ),
        ],
    };

    let info = mock_info("addr0000", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    // bond 100 tokens
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond {}).unwrap(),
    });
    let info = mock_info("staking0000", &[]);
    let mut env = mock_env();
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    // 100 seconds passed
    // 1,000,000 rewards distributed
    env.block.time = env.block.time.plus_seconds(100);
    let info = mock_info("addr0000", &[]);

    let msg = ExecuteMsg::Withdraw {};
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "reward0000".to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: "addr0000".to_string(),
                amount: Uint128::from(1000000u128),
            })
            .unwrap(),
            funds: vec![],
        }))]
    );

    // execute migration after 50 seconds
    env.block.time = env.block.time.plus_seconds(50);

    let msg = ExecuteMsg::MigrateStaking {
        new_staking_contract: "newstaking0000".to_string(),
    };

    // unauthorized attempt
    let info = mock_info("notgov0000", &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    match res {
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "unauthorized"),
        _ => panic!("Must return unauthorized error"),
    }

    // successful attempt
    let info = mock_info(&owner, &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap();

    assert_eq!(
        res.attributes,
        vec![
            ("action", "migrate_staking"),
            ("distributed_amount", "6000000"), // 1000000 + (10000000 / 2)
            ("remaining_amount", "5000000")    // 11,000,000 - 6000000
        ]
    );

    assert_eq!(
        res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "reward0000".to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: "newstaking0000".to_string(),
                amount: Uint128::from(5000000u128),
            })
            .unwrap(),
            funds: vec![],
        }))]
    );

    // query config
    let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(
        config,
        ConfigResponse {
            owner: owner.clone(),
            psi_token: "reward0000".to_string(),
            staking_token: "staking0000".to_string(),
            terraswap_factory: "terraswap_factory0000".to_string(),
            distribution_schedule: vec![
                StakingSchedule::new(
                    mock_env_block_time(),
                    mock_env_block_time() + 100,
                    Uint128::from(1000000u128),
                ),
                StakingSchedule::new(
                    mock_env_block_time() + 100,
                    mock_env_block_time() + 150,
                    Uint128::from(5000000u128),
                )
            ]
        }
    );
}
