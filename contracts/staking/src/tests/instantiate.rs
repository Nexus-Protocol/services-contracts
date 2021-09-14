use crate::contract::{execute, instantiate, query};
use crate::tests::mock_env_block_time;
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{from_binary, Decimal, StdError, Uint128};
use services::staking::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, StakingSchedule, StateResponse,
};

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner0000".to_string(),
        psi_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        terraswap_factory: "terraswap_factory0000".to_string(),
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
            terraswap_factory: "terraswap_factory0000".to_string(),
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
fn change_owner() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000".to_string();
    let new_owner = "owner0001".to_string();

    let msg = InstantiateMsg {
        owner: owner.clone(),
        psi_token: "psi_token".to_string(),
        staking_token: "staking0000".to_string(),
        terraswap_factory: "terraswap_factory0000".to_string(),
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
            terraswap_factory: "terraswap_factory0000".to_string(),
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
