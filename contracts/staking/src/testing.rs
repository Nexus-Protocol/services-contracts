use crate::contract::{execute, instantiate, query};
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{
    from_binary, to_binary, CosmosMsg, Decimal, StdError, SubMsg, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use services::staking::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg, StakerInfoResponse,
    StakingSchedule, StateResponse,
};

fn mock_env_block_time() -> u64 {
    mock_env().block.time.seconds()
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner0000".to_string(),
        psi_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        distribution_schedule: vec![StakingSchedule::new(100, 200, Uint128::from(1000000u128))],
    };

    let info = mock_info("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    // it worked, let's query the state
    let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(
        config,
        ConfigResponse {
            owner: "owner0000".to_string(),
            psi_token: "reward0000".to_string(),
            staking_token: "staking0000".to_string(),
            distribution_schedule: vec![StakingSchedule::new(100, 200, Uint128::from(1000000u128))],
        }
    );

    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::State { time_seconds: None },
    )
    .unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(
        state,
        StateResponse {
            last_distributed: mock_env_block_time(),
            total_bond_amount: Uint128::zero(),
            global_reward_index: Decimal::zero(),
        }
    );
}

#[test]
fn test_bond_tokens() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner0000".to_string(),
        psi_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
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

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond {}).unwrap(),
    });

    let info = mock_info("staking0000", &[]);
    let mut env = mock_env();
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    assert_eq!(
        from_binary::<StakerInfoResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::StakerInfo {
                    staker: "addr0000".to_string(),
                    time_seconds: None,
                },
            )
            .unwrap(),
        )
        .unwrap(),
        StakerInfoResponse {
            staker: "addr0000".to_string(),
            reward_index: Decimal::zero(),
            pending_reward: Uint128::zero(),
            bond_amount: Uint128::from(100u128),
        }
    );

    assert_eq!(
        from_binary::<StateResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::State { time_seconds: None }
            )
            .unwrap()
        )
        .unwrap(),
        StateResponse {
            total_bond_amount: Uint128::from(100u128),
            global_reward_index: Decimal::zero(),
            last_distributed: mock_env_block_time(),
        }
    );

    // bond 100 more tokens
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond {}).unwrap(),
    });
    env.block.time = env.block.time.plus_seconds(10);

    let _res = execute(deps.as_mut(), env, info, msg).unwrap();

    assert_eq!(
        from_binary::<StakerInfoResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::StakerInfo {
                    staker: "addr0000".to_string(),
                    time_seconds: None,
                },
            )
            .unwrap(),
        )
        .unwrap(),
        StakerInfoResponse {
            staker: "addr0000".to_string(),
            reward_index: Decimal::from_ratio(1000u128, 1u128),
            pending_reward: Uint128::from(100000u128),
            bond_amount: Uint128::from(200u128),
        }
    );

    assert_eq!(
        from_binary::<StateResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::State { time_seconds: None }
            )
            .unwrap()
        )
        .unwrap(),
        StateResponse {
            total_bond_amount: Uint128::from(200u128),
            global_reward_index: Decimal::from_ratio(1000u128, 1u128),
            last_distributed: mock_env_block_time() + 10,
        }
    );

    // failed with unautorized
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond {}).unwrap(),
    });

    let info = mock_info("staking0001", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg);
    match res {
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "unauthorized"),
        _ => panic!("Must return unauthorized error"),
    }
}

#[test]
fn test_unbond() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner0000".to_string(),
        psi_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
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
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    // unbond 150 tokens; failed
    let msg = ExecuteMsg::Unbond {
        amount: Uint128::from(150u128),
    };

    let info = mock_info("addr0000", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
    match res {
        StdError::GenericErr { msg, .. } => {
            assert_eq!(msg, "Cannot unbond more than bond amount");
        }
        _ => panic!("Must return generic error"),
    };

    // normal unbond
    let msg = ExecuteMsg::Unbond {
        amount: Uint128::from(100u128),
    };

    let info = mock_info("addr0000", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "staking0000".to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: "addr0000".to_string(),
                amount: Uint128::from(100u128),
            })
            .unwrap(),
            funds: vec![],
        }))]
    );
}

#[test]
fn test_compute_reward() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000".to_string();

    let msg = InstantiateMsg {
        owner: owner.clone(),
        psi_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        distribution_schedule: vec![
            StakingSchedule::new(
                mock_env_block_time(),
                mock_env_block_time() + 100,
                Uint128::from(1_000_000u128),
            ),
            StakingSchedule::new(
                mock_env_block_time() + 100,
                mock_env_block_time() + 200,
                Uint128::from(10_000_000u128),
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
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    // 100 seconds passed
    // 1,000,000 rewards distributed
    env.block.time = env.block.time.plus_seconds(100);

    // bond 100 more tokens
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond {}).unwrap(),
    });
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        from_binary::<StakerInfoResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::StakerInfo {
                    staker: "addr0000".to_string(),
                    time_seconds: None,
                },
            )
            .unwrap()
        )
        .unwrap(),
        StakerInfoResponse {
            staker: "addr0000".to_string(),
            reward_index: Decimal::from_ratio(10000u128, 1u128),
            pending_reward: Uint128::from(1000000u128),
            bond_amount: Uint128::from(200u128),
        }
    );

    // 10 seconds passed
    // 1,000,000 rewards distributed
    env.block.time = env.block.time.plus_seconds(10);
    let info = mock_info("addr0000", &[]);

    // unbond
    let msg = ExecuteMsg::Unbond {
        amount: Uint128::from(100u128),
    };
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        from_binary::<StakerInfoResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::StakerInfo {
                    staker: "addr0000".to_string(),
                    time_seconds: None,
                },
            )
            .unwrap()
        )
        .unwrap(),
        StakerInfoResponse {
            staker: "addr0000".to_string(),
            reward_index: Decimal::from_ratio(15000u64, 1u64),
            pending_reward: Uint128::from(2000000u128),
            bond_amount: Uint128::from(100u128),
        }
    );

    // query future block
    assert_eq!(
        from_binary::<StakerInfoResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::StakerInfo {
                    staker: "addr0000".to_string(),
                    time_seconds: Some(env.block.time.seconds() + 10),
                },
            )
            .unwrap()
        )
        .unwrap(),
        StakerInfoResponse {
            staker: "addr0000".to_string(),
            reward_index: Decimal::from_ratio(25000u64, 1u64),
            pending_reward: Uint128::from(3000000u128),
            bond_amount: Uint128::from(100u128),
        }
    );

    // add new schedule
    let msg = ExecuteMsg::AddSchedules {
        schedules: vec![StakingSchedule::new(
            env.block.time.seconds(),
            env.block.time.plus_seconds(100).seconds(),
            Uint128::from(1_000u128),
        )],
    };
    let owner_info = mock_info(&owner, &[]);
    let _res = execute(deps.as_mut(), env.clone(), owner_info, msg).unwrap();

    assert_eq!(
        from_binary::<StakerInfoResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::StakerInfo {
                    staker: "addr0000".to_string(),
                    time_seconds: None,
                },
            )
            .unwrap()
        )
        .unwrap(),
        StakerInfoResponse {
            staker: "addr0000".to_string(),
            reward_index: Decimal::from_ratio(15000u64, 1u64),
            pending_reward: Uint128::from(2000000u128),
            bond_amount: Uint128::from(100u128),
        }
    );

    // query future block (+10)
    // 1,000,000 rewards distributed from schedule from InitMsg
    // 100 rewards distributed from new schedule (1_000 on 100 blocks, means 100 for 10 blocks)
    assert_eq!(
        from_binary::<StakerInfoResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::StakerInfo {
                    staker: "addr0000".to_string(),
                    time_seconds: Some(env.block.time.seconds() + 10),
                },
            )
            .unwrap()
        )
        .unwrap(),
        StakerInfoResponse {
            staker: "addr0000".to_string(),
            reward_index: Decimal::from_ratio(25001u64, 1u64),
            pending_reward: Uint128::from(3000000u128) + Uint128::from(1_00u128),
            bond_amount: Uint128::from(100u128),
        }
    );
}

#[test]
fn fail_to_add_schedule_that_start_in_past() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000".to_string();

    let msg = InstantiateMsg {
        owner: owner.clone(),
        psi_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        distribution_schedule: vec![StakingSchedule::new(100, 110, Uint128::from(1000000u128))],
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    let msg = ExecuteMsg::AddSchedules {
        schedules: vec![StakingSchedule::new(
            env.block.time.minus_seconds(1).seconds(),
            env.block.time.plus_seconds(10).seconds(),
            Uint128::from(1_000u128),
        )],
    };
    let owner_info = mock_info(&owner, &[]);
    let res = execute(deps.as_mut(), env.clone(), owner_info, msg);

    assert!(res.is_err());
    if let StdError::GenericErr { msg } = res.err().unwrap() {
        assert_eq!("schedule start_time is smaller than current time", msg);
    } else {
        panic!("wrong error");
    }
}

#[test]
fn fail_to_add_schedule_from_non_owner() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner0000".to_string(),
        psi_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        distribution_schedule: vec![StakingSchedule::new(100, 110, Uint128::from(1000000u128))],
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

    // add new schedule from common user (not owner)
    let msg = ExecuteMsg::AddSchedules {
        schedules: vec![StakingSchedule::new(
            env.block.time.minus_seconds(1).seconds(),
            env.block.time.plus_seconds(1).seconds(),
            Uint128::from(1_000u128),
        )],
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_err());
    if let StdError::GenericErr { msg } = res.err().unwrap() {
        assert_eq!("unauthorized", msg);
    } else {
        panic!("wrong error");
    }
}

#[test]
fn test_withdraw() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner0000".to_string(),
        psi_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
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
    let res = execute(deps.as_mut(), env, info, msg).unwrap();

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
}

#[test]
fn change_owner() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000".to_string();
    let new_owner = "owner0001".to_string();

    let msg = InstantiateMsg {
        owner: owner.clone(),
        psi_token: "psi_token".to_string(),
        staking_token: "staking0000".to_string(),
        distribution_schedule: vec![StakingSchedule::new(100, 110, Uint128::from(1000000u128))],
    };

    let info = mock_info("addr0000", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    let msg = ExecuteMsg::UpdateOwner {
        owner: new_owner.clone(),
    };
    let info = mock_info(&owner, &[]);
    let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap();

    assert_eq!(
        from_binary::<ConfigResponse>(
            &query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap()
        )
        .unwrap(),
        ConfigResponse {
            owner: new_owner.clone(),
            psi_token: "psi_token".to_string(),
            staking_token: "staking0000".to_string(),
            distribution_schedule: vec![StakingSchedule::new(100, 110, Uint128::from(1000000u128))],
        }
    );

    //try to change owner again, but from old owner
    let res = execute(deps.as_mut(), mock_env(), info.clone(), msg);
    assert!(res.is_err());
    if let StdError::GenericErr { msg } = res.err().unwrap() {
        assert_eq!("unauthorized", msg);
    } else {
        panic!("wrong error");
    }
}

#[test]
fn test_migrate_staking() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000".to_string();

    let msg = InstantiateMsg {
        owner: owner.clone(),
        psi_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
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
