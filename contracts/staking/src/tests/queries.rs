use crate::contract::{execute, instantiate, query};
use crate::tests::mock_env_block_time;
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{from_binary, to_binary, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use services::staking::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg, StakerInfoResponse, StakingSchedule,
};

#[test]
fn staker_info_seconds_lesser_than_last_distributed() {
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
                    time_seconds: Some(env.block.time.seconds()),
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

    //try to query with `time_seconds` is lesser than `last_distributed`
    assert_eq!(
        from_binary::<StakerInfoResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::StakerInfo {
                    staker: "addr0000".to_string(),
                    time_seconds: Some(env.block.time.seconds() - 10),
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
}
