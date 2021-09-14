use crate::contract::{execute, instantiate};
use crate::state::read_config;
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{StdError, Uint128};
use services::staking::{ExecuteMsg, InstantiateMsg, StakingSchedule};

#[test]
fn fail_to_add_schedule_that_start_in_past() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000".to_string();

    let msg = InstantiateMsg {
        owner: owner.clone(),
        psi_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        terraswap_factory: "terraswap_factory0000".to_string(),
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
        terraswap_factory: "terraswap_factory0000".to_string(),
        distribution_schedule: vec![StakingSchedule::new(100, 110, Uint128::from(1000000u128))],
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

    // add new schedule from common user (not owner)
    let msg = ExecuteMsg::AddSchedules {
        schedules: vec![StakingSchedule::new(
            env.block.time.plus_seconds(10).seconds(),
            env.block.time.plus_seconds(20).seconds(),
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
fn successfully_add_schedule() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000".to_string();

    let msg = InstantiateMsg {
        owner: owner.clone(),
        psi_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        terraswap_factory: "terraswap_factory0000".to_string(),
        distribution_schedule: vec![StakingSchedule::new(100, 110, Uint128::from(1000000u128))],
    };

    let env = mock_env();
    let info = mock_info(&owner, &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

    // add new schedule
    let msg = ExecuteMsg::AddSchedules {
        schedules: vec![StakingSchedule::new(
            env.block.time.plus_seconds(10).seconds(),
            env.block.time.plus_seconds(20).seconds(),
            Uint128::from(1_000u128),
        )],
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let config = read_config(&deps.storage).unwrap();

    assert_eq!(
        vec![
            StakingSchedule::new(100, 110, Uint128::from(1000000u128)),
            StakingSchedule::new(
                env.block.time.plus_seconds(10).seconds(),
                env.block.time.plus_seconds(20).seconds(),
                Uint128::from(1_000u128)
            ),
        ],
        config.distribution_schedule
    );
}
