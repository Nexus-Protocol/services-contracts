mod mock_querier;

use crate::contract::{execute, instantiate, query, reply, POLL_EXECUTE_REPLY_ID};
use crate::state::{
    load_bank, load_config, load_poll_voter, load_state, load_tmp_poll_id, remove_poll_indexer,
    store_bank, store_poll, store_poll_indexer, store_poll_voter, Config, Poll, State,
    TokenManager,
};
use crate::tests::mock_querier::{mock_dependencies, WasmMockQuerier};

use crate::querier::query_token_balance;
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    attr, coins, from_binary, to_binary, Addr, ContractResult, CosmosMsg, Decimal, Env, OwnedDeps,
    Reply, Response, StdError, SubMsg, Timestamp, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use services::common::OrderBy;
use services::governance::{
    AnyoneMsg, ConfigResponse, Cw20HookMsg, ExecuteMsg, GovernanceMsg, InstantiateMsg,
    PollExecuteMsg, PollMigrateMsg, PollResponse, PollStatus, PollsResponse, QueryMsg,
    StakerResponse, VoteOption, VoterInfo, VotersResponse, VotersResponseItem, YourselfMsg,
};

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct MigrateMsg {
    id: u64,
}

const VOTING_TOKEN: &str = "voting_token";
const UTILITY_TOKEN: &str = "utility_token";
const PSI_NEXPRISM_STAKING: &str = "psi_nexprism_staking";
const TEST_CREATOR: &str = "creator";
const TEST_VOTER: &str = "voter1";
const TEST_VOTER_2: &str = "voter2";
const TEST_VOTER_3: &str = "voter3";
const DEFAULT_QUORUM: u64 = 30u64;
const DEFAULT_THRESHOLD: u64 = 50u64;
const DEFAULT_VOTING_PERIOD: u64 = 20000u64;
const DEFAULT_FIX_PERIOD: u64 = 10u64;
const DEFAULT_TIMELOCK_PERIOD: u64 = 10000u64;
const DEFAULT_PROPOSAL_DEPOSIT: u128 = 10000000000u128;

fn mock_init(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let msg = InstantiateMsg {
        quorum: Decimal::percent(DEFAULT_QUORUM),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: DEFAULT_VOTING_PERIOD,
        timelock_period: DEFAULT_TIMELOCK_PERIOD,
        proposal_deposit: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
        snapshot_period: DEFAULT_FIX_PERIOD,
        psi_nexprism_staking: PSI_NEXPRISM_STAKING.to_owned(),
    };

    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    instantiate(deps.as_mut(), env, info.clone(), msg)
        .expect("contract successfully handles InitMsg");
    let config = load_config(deps.as_ref().storage).unwrap();

    assert_eq!(
        config,
        Config {
            owner: Addr::unchecked(TEST_CREATOR.to_string()),
            psi_token: Addr::unchecked("".to_string()),
            quorum: Decimal::percent(DEFAULT_QUORUM),
            threshold: Decimal::percent(DEFAULT_THRESHOLD),
            voting_period: DEFAULT_VOTING_PERIOD,
            timelock_period: DEFAULT_TIMELOCK_PERIOD,
            proposal_deposit: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
            snapshot_period: DEFAULT_FIX_PERIOD,
            psi_nexprism_staking: Addr::unchecked(PSI_NEXPRISM_STAKING),
        }
    );

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::RegisterToken {
            psi_token: VOTING_TOKEN.to_string(),
        },
    };
    execute(deps.as_mut(), mock_env(), info.clone(), msg)
        .expect("contract successfully handles RegisterToken");

    let config = load_config(deps.as_ref().storage).unwrap();
    assert_eq!(
        config,
        Config {
            owner: Addr::unchecked(TEST_CREATOR.to_string()),
            psi_token: Addr::unchecked(VOTING_TOKEN.to_string()),
            quorum: Decimal::percent(DEFAULT_QUORUM),
            threshold: Decimal::percent(DEFAULT_THRESHOLD),
            voting_period: DEFAULT_VOTING_PERIOD,
            timelock_period: DEFAULT_TIMELOCK_PERIOD,
            proposal_deposit: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
            snapshot_period: DEFAULT_FIX_PERIOD,
            psi_nexprism_staking: Addr::unchecked(PSI_NEXPRISM_STAKING),
        }
    );

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::RegisterToken {
            psi_token: VOTING_TOKEN.to_string(),
        },
    };

    // can't change token_address
    let res = execute(deps.as_mut(), mock_env(), info, msg);
    if let StdError::GenericErr { msg } = res.err().unwrap() {
        assert_eq!("unauthorized", msg);
    } else {
        panic!("wrong error");
    }
}

fn mock_env_height(height: u64, time: u64) -> Env {
    let mut env = mock_env();
    env.block.height = height;
    env.block.time = Timestamp::from_seconds(time);
    env
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let state: State = load_state(deps.as_ref().storage).unwrap();
    assert_eq!(
        state,
        State {
            poll_count: 0,
            total_share: Uint128::zero(),
            total_deposit: Uint128::zero(),
        }
    );
}

#[test]
fn poll_not_found() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let res = query(deps.as_ref(), mock_env(), QueryMsg::Poll { poll_id: 1 });

    match res {
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Poll does not exist"),
        Err(e) => panic!("Unexpected error: {:?}", e),
        _ => panic!("Must return error"),
    }
}

#[test]
fn fails_init_invalid_quorum() {
    let mut deps = mock_dependencies(&[]);
    let env = mock_env();
    let info = mock_info("voter", &coins(11, VOTING_TOKEN));
    let msg = InstantiateMsg {
        quorum: Decimal::percent(101),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: DEFAULT_VOTING_PERIOD,
        timelock_period: DEFAULT_TIMELOCK_PERIOD,
        proposal_deposit: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
        snapshot_period: DEFAULT_FIX_PERIOD,
        psi_nexprism_staking: PSI_NEXPRISM_STAKING.to_owned(),
    };

    let res = instantiate(deps.as_mut(), env, info, msg);
    match res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "quorum must be 0 to 1"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_init_invalid_threshold() {
    let mut deps = mock_dependencies(&[]);
    let env = mock_env();
    let info = mock_info("voter", &coins(11, VOTING_TOKEN));
    let msg = InstantiateMsg {
        quorum: Decimal::percent(DEFAULT_QUORUM),
        threshold: Decimal::percent(101),
        voting_period: DEFAULT_VOTING_PERIOD,
        timelock_period: DEFAULT_TIMELOCK_PERIOD,
        proposal_deposit: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
        snapshot_period: DEFAULT_FIX_PERIOD,
        psi_nexprism_staking: PSI_NEXPRISM_STAKING.to_owned(),
    };

    let res = instantiate(deps.as_mut(), env, info, msg);
    match res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "threshold must be 0 to 1"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_create_poll_invalid_title() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let msg = create_poll_msg("a", "test", None, None, None);
    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    match res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Title too short"),
        Err(_) => panic!("Unknown error"),
    }

    let msg = create_poll_msg(
            "0123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234",
            "test",
            None,
            None,
            None,
        );

    match execute(deps.as_mut(), env, info, msg) {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Title too long"),
        Err(_) => panic!("Unknown error"),
    }
}

#[test]
fn fails_create_poll_invalid_description() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let msg = create_poll_msg("test", "a", None, None, None);
    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    match execute(deps.as_mut(), env.clone(), info.clone(), msg) {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Description too short"),
        Err(_) => panic!("Unknown error"),
    }

    let msg = create_poll_msg(
            "test",
            "012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678900123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012340123456789001234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012341234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123456",
            None,
            None,
            None,
        );

    match execute(deps.as_mut(), env, info, msg) {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Description too long"),
        Err(_) => panic!("Unknown error"),
    }
}

#[test]
fn fails_create_poll_invalid_link() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let msg = create_poll_msg("test", "test", Some("http://hih"), None, None);
    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    match execute(deps.as_mut(), env.clone(), info.clone(), msg) {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Link too short"),
        Err(_) => panic!("Unknown error"),
    }

    let msg = create_poll_msg(
            "test",
            "test",
            Some("0123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234012345678901234567890123456789012345678901234567890123456789012340123456789012345678901234567890123456789012345678901234567890123401234567890123456789012345678901234567890123456789012345678901234"),
            None,
            None,
        );

    match execute(deps.as_mut(), env, info, msg) {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Link too long"),
        Err(_) => panic!("Unknown error"),
    }
}

#[test]
fn fails_create_poll_invalid_deposit() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_CREATOR.to_string(),
        amount: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT - 1),
        msg: to_binary(&Cw20HookMsg::CreatePoll {
            title: "TESTTEST".to_string(),
            description: "TESTTEST".to_string(),
            link: None,
            execute_msgs: None,
            migrate_msgs: None,
        })
        .unwrap(),
    });
    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    match execute(deps.as_mut(), env, info, msg) {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(
            msg,
            format!("Must deposit more than {} token", DEFAULT_PROPOSAL_DEPOSIT)
        ),
        Err(_) => panic!("Unknown error"),
    }
}

fn create_poll_msg<A: Into<String>>(
    title: A,
    description: A,
    link: Option<A>,
    execute_msg: Option<Vec<PollExecuteMsg>>,
    migrate_msg: Option<Vec<PollMigrateMsg>>,
) -> ExecuteMsg {
    ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_CREATOR.to_string(),
        amount: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
        msg: to_binary(&Cw20HookMsg::CreatePoll {
            title: title.into(),
            description: description.into(),
            link: link.map(|l| l.into()),
            execute_msgs: execute_msg,
            migrate_msgs: migrate_msg,
        })
        .unwrap(),
    })
}

#[test]
fn happy_days_create_poll() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);
    let env = mock_env_height(0, 10000);
    let info = mock_info(VOTING_TOKEN, &[]);

    let msg = create_poll_msg("test", "test", None, None, None);

    let handle_res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_create_poll_result(
        1,
        env.block.time.plus_seconds(DEFAULT_VOTING_PERIOD).seconds(),
        TEST_CREATOR,
        handle_res,
        &deps,
    );
}

#[test]
fn query_polls() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);
    let env = mock_env_height(0, 0);
    let info = mock_info(VOTING_TOKEN, &[]);

    let exec_msg_bz = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(123),
    })
    .unwrap();

    let exec_msg_bz2 = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(12),
    })
    .unwrap();

    let exec_msg_bz3 = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(1),
    })
    .unwrap();

    let execute_msgs = vec![
        PollExecuteMsg {
            order: 1u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz,
        },
        PollExecuteMsg {
            order: 3u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz3,
        },
        PollExecuteMsg {
            order: 2u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz2,
        },
    ];

    let msg = create_poll_msg(
        "test".to_string(),
        "test".to_string(),
        Some("http://google.com".to_string()),
        Some(execute_msgs.clone()),
        None,
    );

    let _handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    let msg = create_poll_msg("test2", "test2", None, None, None);
    let _handle_res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    let res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::Polls {
            filter: None,
            start_after: None,
            limit: None,
            order_by: Some(OrderBy::Asc),
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(
        response.polls,
        vec![
            PollResponse {
                id: 1u64,
                creator: TEST_CREATOR.to_string(),
                status: PollStatus::InProgress,
                end_time: DEFAULT_VOTING_PERIOD,
                title: "test".to_string(),
                description: "test".to_string(),
                link: Some("http://google.com".to_string()),
                deposit_amount: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
                execute_data: Some(execute_msgs.clone()),
                migrate_data: None,
                yes_votes: Uint128::zero(),
                no_votes: Uint128::zero(),
                staked_amount: None,
                total_balance_at_end_poll: None,
            },
            PollResponse {
                id: 2u64,
                creator: TEST_CREATOR.to_string(),
                status: PollStatus::InProgress,
                end_time: DEFAULT_VOTING_PERIOD,
                title: "test2".to_string(),
                description: "test2".to_string(),
                link: None,
                deposit_amount: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
                execute_data: None,
                migrate_data: None,
                yes_votes: Uint128::zero(),
                no_votes: Uint128::zero(),
                staked_amount: None,
                total_balance_at_end_poll: None,
            },
        ]
    );

    let res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::Polls {
            filter: None,
            start_after: Some(1u64),
            limit: None,
            order_by: Some(OrderBy::Asc),
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(
        response.polls,
        vec![PollResponse {
            id: 2u64,
            creator: TEST_CREATOR.to_string(),
            status: PollStatus::InProgress,
            end_time: DEFAULT_VOTING_PERIOD,
            title: "test2".to_string(),
            description: "test2".to_string(),
            link: None,
            deposit_amount: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
            execute_data: None,
            migrate_data: None,
            yes_votes: Uint128::zero(),
            no_votes: Uint128::zero(),
            staked_amount: None,
            total_balance_at_end_poll: None,
        },]
    );

    let res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::Polls {
            filter: None,
            start_after: Some(2u64),
            limit: None,
            order_by: Some(OrderBy::Desc),
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(
        response.polls,
        vec![PollResponse {
            id: 1u64,
            creator: TEST_CREATOR.to_string(),
            status: PollStatus::InProgress,
            end_time: DEFAULT_VOTING_PERIOD,
            title: "test".to_string(),
            description: "test".to_string(),
            link: Some("http://google.com".to_string()),
            deposit_amount: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
            execute_data: Some(execute_msgs),
            migrate_data: None,
            yes_votes: Uint128::zero(),
            no_votes: Uint128::zero(),
            staked_amount: None,
            total_balance_at_end_poll: None,
        }]
    );

    let res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::Polls {
            filter: Some(PollStatus::InProgress),
            start_after: Some(1u64),
            limit: None,
            order_by: Some(OrderBy::Asc),
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(
        response.polls,
        vec![PollResponse {
            id: 2u64,
            creator: TEST_CREATOR.to_string(),
            status: PollStatus::InProgress,
            end_time: DEFAULT_VOTING_PERIOD,
            title: "test2".to_string(),
            description: "test2".to_string(),
            link: None,
            deposit_amount: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
            execute_data: None,
            migrate_data: None,
            yes_votes: Uint128::zero(),
            no_votes: Uint128::zero(),
            staked_amount: None,
            total_balance_at_end_poll: None,
        },]
    );

    let res = query(
        deps.as_ref(),
        env,
        QueryMsg::Polls {
            filter: Some(PollStatus::Passed),
            start_after: None,
            limit: None,
            order_by: None,
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(response.polls, vec![]);
}

#[test]
fn create_poll_no_quorum() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);
    let env = mock_env_height(0, 0);
    let info = mock_info(VOTING_TOKEN, &[]);

    let msg = create_poll_msg("test", "test", None, None, None);

    let handle_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_create_poll_result(1, DEFAULT_VOTING_PERIOD, TEST_CREATOR, handle_res, &deps);
}

#[test]
fn fails_end_poll_before_end_height() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);
    let env = mock_env_height(0, 0);
    let info = mock_info(VOTING_TOKEN, &[]);

    let msg = create_poll_msg("test", "test", None, None, None);

    let handle_res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_create_poll_result(1, DEFAULT_VOTING_PERIOD, TEST_CREATOR, handle_res, &deps);

    let res = query(deps.as_ref(), env.clone(), QueryMsg::Poll { poll_id: 1 }).unwrap();
    let value: PollResponse = from_binary(&res).unwrap();
    assert_eq!(DEFAULT_VOTING_PERIOD, value.end_time);

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::EndPoll { poll_id: 1 },
    };
    let handle_res = execute(deps.as_mut(), env, info, msg);

    match handle_res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Voting period has not expired"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn happy_days_end_poll() {
    const POLL_START_HEIGHT: u64 = 1000;
    const POLL_ID: u64 = 1;
    let stake_amount = 1000;

    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(&mut deps);
    let mut creator_env = mock_env_height(POLL_START_HEIGHT, 10000);
    let mut creator_info = mock_info(VOTING_TOKEN, &coins(2, VOTING_TOKEN));

    let exec_msg_bz = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(123),
    })
    .unwrap();

    let exec_msg_bz2 = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(12),
    })
    .unwrap();

    let exec_msg_bz3 = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(1),
    })
    .unwrap();

    //add three messages with different order
    let execute_msgs = vec![
        PollExecuteMsg {
            order: 3u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz3.clone(),
        },
        PollExecuteMsg {
            order: 2u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz2.clone(),
        },
        PollExecuteMsg {
            order: 1u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz.clone(),
        },
    ];

    let msg = create_poll_msg("test", "test", None, Some(execute_msgs), None);

    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap();

    assert_create_poll_result(
        1,
        creator_env
            .block
            .time
            .plus_seconds(DEFAULT_VOTING_PERIOD)
            .seconds(),
        TEST_CREATOR,
        execute_res,
        &deps,
    );

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new((stake_amount + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(stake_amount as u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_stake_tokens_result(
        stake_amount,
        DEFAULT_PROPOSAL_DEPOSIT,
        stake_amount,
        1,
        execute_res,
        &deps,
    );

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(stake_amount),
        },
    };
    let env = mock_env_height(POLL_START_HEIGHT, 10000);
    let info = mock_info(TEST_VOTER, &[]);
    let execute_res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "cast_vote"),
            attr("poll_id", POLL_ID.to_string()),
            attr("amount", "1000"),
            attr("voter", TEST_VOTER),
            attr("vote_option", "yes"),
        ]
    );

    // not in passed status
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::ExecutePoll { poll_id: 1 },
    };
    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap_err();
    match execute_res {
        StdError::GenericErr { msg, .. } => assert_eq!(msg, "Poll is not in passed status"),
        _ => panic!("DO NOT ENTER HERE"),
    }

    creator_info.sender = Addr::unchecked(TEST_CREATOR);
    creator_env.block.time = creator_env.block.time.plus_seconds(DEFAULT_VOTING_PERIOD);

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::EndPoll { poll_id: 1 },
    };

    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "end_poll"),
            attr("poll_id", "1"),
            attr("rejected_reason", ""),
            attr("passed", "true"),
        ]
    );
    assert_eq!(
        execute_res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: VOTING_TOKEN.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: TEST_CREATOR.to_string(),
                amount: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
            })
            .unwrap(),
            funds: vec![],
        }))]
    );

    // End poll will withdraw deposit balance
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new(stake_amount as u128),
        )],
    )]);

    // timelock_period has not expired
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::ExecutePoll { poll_id: 1 },
    };
    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap_err();
    match execute_res {
        StdError::GenericErr { msg, .. } => assert_eq!(msg, "Timelock period has not expired"),
        _ => panic!("DO NOT ENTER HERE"),
    }

    creator_env.block.time = creator_env.block.time.plus_seconds(DEFAULT_TIMELOCK_PERIOD);
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::ExecutePoll { poll_id: 1 },
    };
    let execute_res = execute(deps.as_mut(), creator_env.clone(), creator_info, msg).unwrap();
    assert_eq!(
        execute_res.messages,
        vec![SubMsg::reply_on_error(
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: creator_env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::Yourself {
                    yourself_msg: YourselfMsg::ExecutePollMsgs { poll_id: 1 }
                })
                .unwrap(),
                funds: vec![],
            }),
            1
        )]
    );

    let msg = ExecuteMsg::Yourself {
        yourself_msg: services::governance::YourselfMsg::ExecutePollMsgs { poll_id: 1 },
    };
    let contract_info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let execute_res = execute(deps.as_mut(), creator_env, contract_info, msg).unwrap();
    assert_eq!(
        execute_res.messages,
        vec![
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: exec_msg_bz,
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: exec_msg_bz2,
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: exec_msg_bz3,
                funds: vec![],
            }))
        ]
    );
    assert_eq!(
        execute_res.attributes,
        vec![attr("action", "execute_poll"), attr("poll_id", "1"),]
    );
    let tmp_poll_id = load_tmp_poll_id(&deps.storage).unwrap();
    assert_eq!(tmp_poll_id, 1);

    // Query executed polls
    let res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::Polls {
            filter: Some(PollStatus::Passed),
            start_after: None,
            limit: None,
            order_by: None,
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(response.polls.len(), 0);

    let res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::Polls {
            filter: Some(PollStatus::InProgress),
            start_after: None,
            limit: None,
            order_by: None,
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(response.polls.len(), 0);

    let res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::Polls {
            filter: Some(PollStatus::Executed),
            start_after: None,
            limit: None,
            order_by: Some(OrderBy::Desc),
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(response.polls.len(), 1);

    // voter info must be deleted
    let res = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::Voters {
            poll_id: 1,
            start_after: None,
            limit: None,
            order_by: None,
        },
    )
    .unwrap();
    let response: VotersResponse = from_binary(&res).unwrap();
    assert_eq!(response.voters.len(), 0);

    // staker locked token must be disappeared
    let res = query(
        deps.as_ref(),
        env,
        QueryMsg::Staker {
            address: TEST_VOTER.to_string(),
        },
    )
    .unwrap();
    let response: StakerResponse = from_binary(&res).unwrap();
    assert_eq!(
        response,
        StakerResponse {
            balance: Uint128::new(stake_amount),
            share: Uint128::new(stake_amount),
            locked_balance: vec![]
        }
    );

    // But the data is still in the store
    let voter = load_poll_voter(deps.as_ref().storage, 1u64, &Addr::unchecked(TEST_VOTER)).unwrap();
    assert_eq!(
        voter,
        VoterInfo {
            vote: VoteOption::Yes,
            balance: Uint128::new(stake_amount),
        }
    );

    let token_manager = load_bank(
        deps.as_ref().storage,
        &Addr::unchecked(TEST_VOTER.to_string()),
    )
    .unwrap();
    assert_eq!(
        token_manager.locked_balance,
        vec![(
            1u64,
            VoterInfo {
                vote: VoteOption::Yes,
                balance: Uint128::new(stake_amount),
            }
        )]
    );
}

#[test]
fn fail_poll() {
    const POLL_START_HEIGHT: u64 = 1000;
    const POLL_ID: u64 = 1;
    let stake_amount = 1000;

    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(&mut deps);
    let mut creator_env = mock_env_height(POLL_START_HEIGHT, 10000);
    let creator_info = mock_info(VOTING_TOKEN, &coins(2, VOTING_TOKEN));

    let exec_msg_bz = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(123),
    })
    .unwrap();
    let execute_msgs = vec![PollExecuteMsg {
        order: 1u64,
        contract: VOTING_TOKEN.to_string(),
        msg: exec_msg_bz.clone(),
    }];
    let msg = create_poll_msg("test", "test", None, Some(execute_msgs), None);

    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap();

    assert_create_poll_result(
        1,
        creator_env
            .block
            .time
            .plus_seconds(DEFAULT_VOTING_PERIOD)
            .seconds(),
        TEST_CREATOR,
        execute_res,
        &deps,
    );

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new((stake_amount + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(stake_amount as u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_stake_tokens_result(
        stake_amount,
        DEFAULT_PROPOSAL_DEPOSIT,
        stake_amount,
        1,
        execute_res,
        &deps,
    );

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(stake_amount),
        },
    };
    let env = mock_env_height(POLL_START_HEIGHT, 10000);
    let info = mock_info(TEST_VOTER, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "cast_vote"),
            attr("poll_id", POLL_ID.to_string()),
            attr("amount", "1000"),
            attr("voter", TEST_VOTER),
            attr("vote_option", "yes"),
        ]
    );

    creator_env.block.time = creator_env.block.time.plus_seconds(DEFAULT_VOTING_PERIOD);

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::EndPoll { poll_id: 1 },
    };
    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "end_poll"),
            attr("poll_id", "1"),
            attr("rejected_reason", ""),
            attr("passed", "true"),
        ]
    );
    assert_eq!(
        execute_res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: VOTING_TOKEN.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: TEST_CREATOR.to_string(),
                amount: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
            })
            .unwrap(),
            funds: vec![],
        }))]
    );

    // Execute Poll should send submsg ExecutePollMsgs
    creator_env.block.time = creator_env.block.time.plus_seconds(DEFAULT_TIMELOCK_PERIOD);
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::ExecutePoll { poll_id: 1 },
    };
    let execute_res = execute(deps.as_mut(), creator_env.clone(), creator_info, msg).unwrap();
    assert_eq!(
        execute_res.messages,
        vec![SubMsg::reply_on_error(
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: creator_env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::Yourself {
                    yourself_msg: YourselfMsg::ExecutePollMsgs { poll_id: 1 }
                })
                .unwrap(),
                funds: vec![],
            }),
            1
        )]
    );

    // ExecutePollMsgs should send poll messages
    let msg = ExecuteMsg::Yourself {
        yourself_msg: services::governance::YourselfMsg::ExecutePollMsgs { poll_id: 1 },
    };
    let contract_info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let execute_res = execute(deps.as_mut(), creator_env, contract_info, msg).unwrap();
    assert_eq!(
        execute_res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz,
            funds: vec![],
        }))]
    );
    let tmp_poll_id = load_tmp_poll_id(&deps.storage).unwrap();
    assert_eq!(tmp_poll_id, 1);

    let reply_msg = Reply {
        id: POLL_EXECUTE_REPLY_ID,
        result: ContractResult::Err("Error".to_string()),
    };
    //revert poll status update, cause 'execute_poll_messages' will be reverted in fail case
    remove_poll_indexer(&mut deps.storage, &PollStatus::Executed, 1);
    store_poll_indexer(&mut deps.storage, &PollStatus::Passed, 1).unwrap();
    //===
    let res = reply(deps.as_mut(), mock_env(), reply_msg).unwrap();
    assert_eq!(
        res.attributes,
        vec![attr("action", "fail_poll"), attr("poll_id", "1")]
    );

    let res = query(deps.as_ref(), mock_env(), QueryMsg::Poll { poll_id: 1 }).unwrap();
    let poll_res: PollResponse = from_binary(&res).unwrap();
    assert_eq!(poll_res.status, PollStatus::Failed);

    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::Polls {
            filter: Some(PollStatus::Failed),
            start_after: None,
            limit: None,
            order_by: Some(OrderBy::Desc),
        },
    )
    .unwrap();
    let polls_res: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(polls_res.polls[0], poll_res);

    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::Polls {
            filter: Some(PollStatus::Executed),
            start_after: None,
            limit: None,
            order_by: Some(OrderBy::Desc),
        },
    )
    .unwrap();
    let polls_res: PollsResponse = from_binary(&res).unwrap();
    assert!(polls_res.polls.is_empty());

    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::Polls {
            filter: Some(PollStatus::Passed),
            start_after: None,
            limit: None,
            order_by: Some(OrderBy::Desc),
        },
    )
    .unwrap();
    let polls_res: PollsResponse = from_binary(&res).unwrap();
    assert!(polls_res.polls.is_empty());
}

#[test]
fn end_poll_zero_quorum() {
    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(&mut deps);
    let mut creator_env = mock_env_height(1000, 10000);
    let mut creator_info = mock_info(VOTING_TOKEN, &[]);

    let execute_msgs = vec![PollExecuteMsg {
        order: 1u64,
        contract: VOTING_TOKEN.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Burn {
            amount: Uint128::new(123),
        })
        .unwrap(),
    }];

    let msg = create_poll_msg("test", "test", None, Some(execute_msgs), None);

    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap();
    assert_create_poll_result(
        1,
        creator_env
            .block
            .time
            .plus_seconds(DEFAULT_VOTING_PERIOD)
            .seconds(),
        TEST_CREATOR,
        execute_res,
        &deps,
    );
    let stake_amount = 100;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new(100u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(stake_amount as u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let info = mock_info(VOTING_TOKEN, &[]);
    execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::EndPoll { poll_id: 1 },
    };
    creator_env.block.time = creator_env.block.time.plus_seconds(DEFAULT_VOTING_PERIOD);
    creator_info.sender = Addr::unchecked(TEST_CREATOR);

    let execute_res = execute(deps.as_mut(), creator_env, creator_info, msg).unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "end_poll"),
            attr("poll_id", "1"),
            attr("rejected_reason", "Quorum not reached"),
            attr("passed", "false"),
        ]
    );

    assert!(execute_res.messages.is_empty());

    // Query rejected polls
    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::Polls {
            filter: Some(PollStatus::Rejected),
            start_after: None,
            limit: None,
            order_by: Some(OrderBy::Desc),
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(response.polls.len(), 1);

    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::Polls {
            filter: Some(PollStatus::InProgress),
            start_after: None,
            limit: None,
            order_by: None,
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(response.polls.len(), 0);

    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::Polls {
            filter: Some(PollStatus::Passed),
            start_after: None,
            limit: None,
            order_by: None,
        },
    )
    .unwrap();
    let response: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(response.polls.len(), 0);
}

#[test]
fn end_poll_quorum_rejected() {
    let mut deps = mock_dependencies(&coins(100, VOTING_TOKEN));
    mock_init(&mut deps);

    let msg = create_poll_msg("test", "test", None, None, None);
    let mut creator_env = mock_env();
    let mut creator_info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap();
    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "create_poll"),
            attr("creator", TEST_CREATOR),
            attr("poll_id", "1"),
            attr(
                "end_time",
                creator_env
                    .block
                    .time
                    .plus_seconds(DEFAULT_VOTING_PERIOD)
                    .seconds()
                    .to_string()
            ),
        ]
    );

    let stake_amount = 100;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new(100u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(stake_amount as u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_stake_tokens_result(
        stake_amount,
        DEFAULT_PROPOSAL_DEPOSIT,
        stake_amount,
        1,
        execute_res,
        &deps,
    );

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(10u128),
        },
    };
    let info = mock_info(TEST_VOTER, &[]);
    let execute_res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "cast_vote"),
            attr("poll_id", "1"),
            attr("amount", "10"),
            attr("voter", TEST_VOTER),
            attr("vote_option", "yes"),
        ]
    );

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::EndPoll { poll_id: 1 },
    };

    creator_info.sender = Addr::unchecked(TEST_CREATOR);
    creator_env.block.time = creator_env.block.time.plus_seconds(DEFAULT_VOTING_PERIOD);

    let execute_res = execute(deps.as_mut(), creator_env, info, msg).unwrap();
    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "end_poll"),
            attr("poll_id", "1"),
            attr("rejected_reason", "Quorum not reached"),
            attr("passed", "false"),
        ]
    );
}

#[test]
fn end_poll_quorum_rejected_noting_staked() {
    let mut deps = mock_dependencies(&coins(100, VOTING_TOKEN));
    mock_init(&mut deps);

    let msg = create_poll_msg("test", "test", None, None, None);
    let mut creator_env = mock_env();
    let mut creator_info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap();
    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "create_poll"),
            attr("creator", TEST_CREATOR),
            attr("poll_id", "1"),
            attr(
                "end_time",
                creator_env
                    .block
                    .time
                    .plus_seconds(DEFAULT_VOTING_PERIOD)
                    .seconds()
                    .to_string()
            ),
        ]
    );

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::EndPoll { poll_id: 1 },
    };
    creator_info.sender = Addr::unchecked(TEST_CREATOR);
    creator_env.block.time = creator_env.block.time.plus_seconds(DEFAULT_VOTING_PERIOD);

    let execute_res = execute(deps.as_mut(), creator_env, creator_info, msg).unwrap();
    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "end_poll"),
            attr("poll_id", "1"),
            attr("rejected_reason", "Quorum not reached"),
            attr("passed", "false"),
        ]
    );
}

#[test]
fn end_poll_nay_rejected() {
    let voter1_stake = 100;
    let voter2_stake = 1000;
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);
    let mut creator_env = mock_env();
    let mut creator_info = mock_info(VOTING_TOKEN, &coins(2, VOTING_TOKEN));

    let msg = create_poll_msg("test", "test", None, None, None);

    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap();
    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "create_poll"),
            attr("creator", TEST_CREATOR),
            attr("poll_id", "1"),
            attr(
                "end_time",
                creator_env
                    .block
                    .time
                    .plus_seconds(DEFAULT_VOTING_PERIOD)
                    .seconds()
                    .to_string()
            ),
        ]
    );

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new((voter1_stake + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(voter1_stake as u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_stake_tokens_result(
        voter1_stake,
        DEFAULT_PROPOSAL_DEPOSIT,
        voter1_stake,
        1,
        execute_res,
        &deps,
    );

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new((voter1_stake + voter2_stake + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER_2.to_string(),
        amount: Uint128::from(voter2_stake as u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_stake_tokens_result(
        voter1_stake + voter2_stake,
        DEFAULT_PROPOSAL_DEPOSIT,
        voter2_stake,
        1,
        execute_res,
        &deps,
    );

    let info = mock_info(TEST_VOTER_2, &[]);
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::No,
            amount: Uint128::from(voter2_stake),
        },
    };
    let execute_res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_cast_vote_success(TEST_VOTER_2, voter2_stake, 1, VoteOption::No, execute_res);

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::EndPoll { poll_id: 1 },
    };

    creator_info.sender = Addr::unchecked(TEST_CREATOR);
    creator_env.block.time = creator_env.block.time.plus_seconds(DEFAULT_VOTING_PERIOD);
    let execute_res = execute(deps.as_mut(), creator_env, creator_info, msg).unwrap();
    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "end_poll"),
            attr("poll_id", "1"),
            attr("rejected_reason", "Threshold not reached"),
            attr("passed", "false"),
        ]
    );
}

#[test]
fn fails_cast_vote_not_enough_staked() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);
    let env = mock_env_height(0, 0);
    let info = mock_info(VOTING_TOKEN, &[]);

    let msg = create_poll_msg("test", "test", None, None, None);

    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_create_poll_result(1, DEFAULT_VOTING_PERIOD, TEST_CREATOR, execute_res, &deps);

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new(10u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(10u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_stake_tokens_result(10, DEFAULT_PROPOSAL_DEPOSIT, 10, 1, execute_res, &deps);

    let env = mock_env_height(0, 0);
    let info = mock_info(TEST_VOTER, &coins(11, VOTING_TOKEN));
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(11u128),
        },
    };

    let res = execute(deps.as_mut(), env, info, msg);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => {
            assert_eq!(msg, "User does not have enough staked tokens.")
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn happy_days_cast_vote() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let env = mock_env_height(0, 0);
    let info = mock_info(VOTING_TOKEN, &[]);
    let msg = create_poll_msg("test", "test", None, None, None);

    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_create_poll_result(1, DEFAULT_VOTING_PERIOD, TEST_CREATOR, execute_res, &deps);

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new(11u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(11u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_stake_tokens_result(11, DEFAULT_PROPOSAL_DEPOSIT, 11, 1, execute_res, &deps);

    let env = mock_env_height(0, 10000);
    let info = mock_info(TEST_VOTER, &coins(11, VOTING_TOKEN));
    let amount = 10u128;
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(amount),
        },
    };

    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_cast_vote_success(TEST_VOTER, amount, 1, VoteOption::Yes, execute_res);

    // balance be double
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new(22u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    // Query staker
    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::Staker {
            address: TEST_VOTER.to_string(),
        },
    )
    .unwrap();
    let response: StakerResponse = from_binary(&res).unwrap();
    assert_eq!(
        response,
        StakerResponse {
            balance: Uint128::new(22u128),
            share: Uint128::new(11u128),
            locked_balance: vec![(
                1u64,
                VoterInfo {
                    vote: VoteOption::Yes,
                    balance: Uint128::from(amount),
                }
            )]
        }
    );

    // Query voters
    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::Voters {
            poll_id: 1,
            start_after: None,
            limit: None,
            order_by: Some(OrderBy::Desc),
        },
    )
    .unwrap();
    let response: VotersResponse = from_binary(&res).unwrap();
    assert_eq!(
        response.voters,
        vec![VotersResponseItem {
            voter: TEST_VOTER.to_string(),
            vote: VoteOption::Yes,
            balance: Uint128::from(amount),
        }]
    );

    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::Voters {
            poll_id: 1,
            start_after: Some(TEST_VOTER.to_string()),
            limit: None,
            order_by: None,
        },
    )
    .unwrap();
    let response: VotersResponse = from_binary(&res).unwrap();
    assert_eq!(response.voters.len(), 0);
}

#[test]
fn happy_days_withdraw_voting_tokens() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::new(11u128))],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(11u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_stake_tokens_result(11, 0, 11, 0, execute_res, &deps);

    let state: State = load_state(deps.as_ref().storage).unwrap();
    assert_eq!(
        state,
        State {
            poll_count: 0,
            total_share: Uint128::from(11u128),
            total_deposit: Uint128::zero(),
        }
    );

    // double the balance, only half will be withdrawn
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::new(22u128))],
    )]);

    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::WithdrawVotingTokens {
            amount: Some(Uint128::from(11u128)),
        },
    };

    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    let msg = execute_res.messages.get(0).expect("no message");

    assert_eq!(
        msg,
        &SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: VOTING_TOKEN.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: TEST_VOTER.to_string(),
                amount: Uint128::from(11u128),
            })
            .unwrap(),
            funds: vec![],
        }))
    );

    let state: State = load_state(&deps.storage).unwrap();
    assert_eq!(
        state,
        State {
            poll_count: 0,
            total_share: Uint128::from(6u128),
            total_deposit: Uint128::zero(),
        }
    );
}

#[test]
fn happy_days_withdraw_voting_tokens_all() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::new(11u128))],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(11u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_stake_tokens_result(11, 0, 11, 0, execute_res, &deps);

    let state: State = load_state(deps.as_ref().storage).unwrap();
    assert_eq!(
        state,
        State {
            poll_count: 0,
            total_share: Uint128::from(11u128),
            total_deposit: Uint128::zero(),
        }
    );

    // double the balance, all balance withdrawn
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::new(22u128))],
    )]);

    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::WithdrawVotingTokens { amount: None },
    };

    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    let msg = execute_res.messages.get(0).expect("no message");

    assert_eq!(
        msg,
        &SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: VOTING_TOKEN.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: TEST_VOTER.to_string(),
                amount: Uint128::from(22u128),
            })
            .unwrap(),
            funds: vec![],
        }))
    );

    let state: State = load_state(deps.as_ref().storage).unwrap();
    assert_eq!(
        state,
        State {
            poll_count: 0,
            total_share: Uint128::zero(),
            total_deposit: Uint128::zero(),
        }
    );
}

#[test]
fn withdraw_voting_tokens_remove_not_in_progress_poll_voter_info() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::new(11u128))],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(11u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_stake_tokens_result(11, 0, 11, 0, execute_res, &deps);

    // make fake polls; one in progress & one in passed
    store_poll(
        deps.as_mut().storage,
        1u64,
        &Poll {
            id: 1u64,
            creator: Addr::unchecked(""),
            status: PollStatus::InProgress,
            yes_votes: Uint128::zero(),
            no_votes: Uint128::zero(),
            end_time: 0u64,
            title: "title".to_string(),
            description: "description".to_string(),
            deposit_amount: Uint128::zero(),
            link: None,
            execute_data: None,
            migrate_data: None,
            total_balance_at_end_poll: None,
            staked_amount: None,
        },
    )
    .unwrap();

    store_poll(
        deps.as_mut().storage,
        2u64,
        &Poll {
            id: 1u64,
            creator: Addr::unchecked(""),
            status: PollStatus::Passed,
            yes_votes: Uint128::zero(),
            no_votes: Uint128::zero(),
            end_time: 0u64,
            title: "title".to_string(),
            description: "description".to_string(),
            deposit_amount: Uint128::zero(),
            link: None,
            execute_data: None,
            migrate_data: None,
            total_balance_at_end_poll: None,
            staked_amount: None,
        },
    )
    .unwrap();

    let voter_addr = Addr::unchecked(TEST_VOTER);
    store_poll_voter(
        &mut deps.storage,
        1u64,
        &voter_addr,
        &VoterInfo {
            vote: VoteOption::Yes,
            balance: Uint128::new(5u128),
        },
    )
    .unwrap();
    store_poll_voter(
        deps.as_mut().storage,
        2u64,
        &voter_addr,
        &VoterInfo {
            vote: VoteOption::Yes,
            balance: Uint128::new(5u128),
        },
    )
    .unwrap();

    store_bank(
        deps.as_mut().storage,
        &voter_addr,
        &TokenManager {
            share: Uint128::new(11u128),
            locked_balance: vec![
                (
                    1u64,
                    VoterInfo {
                        vote: VoteOption::Yes,
                        balance: Uint128::new(5u128),
                    },
                ),
                (
                    2u64,
                    VoterInfo {
                        vote: VoteOption::Yes,
                        balance: Uint128::new(5u128),
                    },
                ),
            ],
        },
    )
    .unwrap();

    // withdraw voting token must remove not in-progress votes infos from the store
    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::WithdrawVotingTokens {
            amount: Some(Uint128::from(5u128)),
        },
    };

    execute(deps.as_mut(), env, info, msg).unwrap();
    let voter = load_poll_voter(&deps.storage, 1u64, &voter_addr).unwrap();
    assert_eq!(
        voter,
        VoterInfo {
            vote: VoteOption::Yes,
            balance: Uint128::new(5u128),
        }
    );
    assert!(load_poll_voter(&deps.storage, 2u64, &voter_addr).is_err());

    let token_manager = load_bank(deps.as_ref().storage, &voter_addr).unwrap();
    assert_eq!(
        token_manager.locked_balance,
        vec![(
            1u64,
            VoterInfo {
                vote: VoteOption::Yes,
                balance: Uint128::new(5u128),
            }
        )]
    );
}

#[test]
fn fails_withdraw_voting_tokens_no_stake() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let env = mock_env();
    let info = mock_info(TEST_VOTER, &coins(11, VOTING_TOKEN));
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::WithdrawVotingTokens {
            amount: Some(Uint128::from(11u128)),
        },
    };

    let res = execute(deps.as_mut(), env, info, msg);

    match res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Nothing staked"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_withdraw_too_many_tokens() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::new(10u128))],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(10u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_stake_tokens_result(10, 0, 10, 0, execute_res, &deps);

    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::WithdrawVotingTokens {
            amount: Some(Uint128::from(11u128)),
        },
    };

    let execute_res = execute(deps.as_mut(), env, info, msg);

    match execute_res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => {
            assert_eq!(msg, "User is trying to withdraw too many tokens.")
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_cast_vote_twice() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let env = mock_env_height(0, 10000);
    let info = mock_info(VOTING_TOKEN, &coins(2, VOTING_TOKEN));

    let msg = create_poll_msg("test", "test", None, None, None);
    let execute_res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_create_poll_result(
        1,
        env.block.time.plus_seconds(DEFAULT_VOTING_PERIOD).seconds(),
        TEST_CREATOR,
        execute_res,
        &deps,
    );

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new(11u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(11u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_stake_tokens_result(11, DEFAULT_PROPOSAL_DEPOSIT, 11, 1, execute_res, &deps);

    let amount = 1u128;
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(amount),
        },
    };
    let env = mock_env_height(0, 10000);
    let info = mock_info(TEST_VOTER, &[]);
    let execute_res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_cast_vote_success(TEST_VOTER, amount, 1, VoteOption::Yes, execute_res);

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(amount),
        },
    };
    let execute_res = execute(deps.as_mut(), env, info, msg);

    match execute_res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "User has already voted."),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_cast_vote_without_poll() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 0,
            vote: VoteOption::Yes,
            amount: Uint128::from(1u128),
        },
    };
    let env = mock_env();
    let info = mock_info(TEST_VOTER, &coins(11, VOTING_TOKEN));

    let execute_res = execute(deps.as_mut(), env, info, msg);

    match execute_res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Poll does not exist"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn happy_days_stake_voting_tokens() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::new(11u128))],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(11u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_stake_tokens_result(11, 0, 11, 0, execute_res, &deps);
}

#[test]
fn fails_insufficient_funds() {
    let mut deps = mock_dependencies(&[]);

    // initialize the store
    mock_init(&mut deps);

    // insufficient token
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(0u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg);

    match execute_res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Insufficient funds sent"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn fails_staking_wrong_token() {
    let mut deps = mock_dependencies(&[]);

    // initialize the store
    mock_init(&mut deps);

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::new(11u128))],
    )]);

    // wrong token
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(11u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info("addr0044", &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg);

    match execute_res {
        Ok(_) => panic!("Must return error"),
        Err(StdError::GenericErr { msg }) => assert_eq!(msg, "unauthorized"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[test]
fn share_calculation() {
    let mut deps = mock_dependencies(&[]);

    // initialize the store
    mock_init(&mut deps);

    // create 100 share
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::new(100u128))],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    execute(deps.as_mut(), env, info, msg).unwrap();

    // add more balance(100) to make share:balance = 1:2
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new(200u128 + 100u128),
        )],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "staking"),
            attr("sender", TEST_VOTER),
            attr("share", "50"),
            attr("amount", "100"),
        ]
    );

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::WithdrawVotingTokens {
            amount: Some(Uint128::new(100u128)),
        },
    };
    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "withdraw"),
            attr("recipient", TEST_VOTER),
            attr("amount", "100"),
        ]
    );

    // 100 tokens withdrawn
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::new(200u128))],
    )]);

    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::Staker {
            address: TEST_VOTER.to_string(),
        },
    )
    .unwrap();
    let stake_info: StakerResponse = from_binary(&res).unwrap();
    assert_eq!(stake_info.share, Uint128::new(100));
    assert_eq!(stake_info.balance, Uint128::new(200));
    assert_eq!(stake_info.locked_balance, vec![]);
}

// helper to confirm the expected create_poll response
fn assert_create_poll_result(
    poll_id: u64,
    end_time: u64,
    creator: &str,
    handle_res: Response,
    deps: &OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
) {
    assert_eq!(
        handle_res.attributes,
        vec![
            attr("action", "create_poll"),
            attr("creator", creator),
            attr("poll_id", poll_id.to_string()),
            attr("end_time", end_time.to_string()),
        ]
    );

    //confirm poll count
    let state: State = load_state(deps.as_ref().storage).unwrap();
    assert_eq!(
        state,
        State {
            poll_count: 1,
            total_share: Uint128::zero(),
            total_deposit: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
        }
    );
}

fn assert_stake_tokens_result(
    total_share: u128,
    total_deposit: u128,
    new_share: u128,
    poll_count: u64,
    execute_res: Response,
    deps: &OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
) {
    assert_eq!(
        execute_res.attributes.get(2).expect("no log"),
        &attr("share", new_share.to_string())
    );

    let state: State = load_state(deps.as_ref().storage).unwrap();
    assert_eq!(
        state,
        State {
            poll_count,
            total_share: Uint128::new(total_share),
            total_deposit: Uint128::new(total_deposit),
        }
    );
}

fn assert_cast_vote_success(
    voter: &str,
    amount: u128,
    poll_id: u64,
    vote_option: VoteOption,
    execute_res: Response,
) {
    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "cast_vote"),
            attr("poll_id", poll_id.to_string()),
            attr("amount", amount.to_string()),
            attr("voter", voter),
            attr("vote_option", vote_option.to_string()),
        ]
    );
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    // update owner
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::Governance {
        governance_msg: GovernanceMsg::UpdateConfig {
            owner: Some("addr0001".to_string()),
            quorum: None,
            threshold: None,
            voting_period: None,
            timelock_period: None,
            proposal_deposit: None,
            snapshot_period: None,
            psi_nexprism_staking: None,
        },
    };

    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert!(execute_res.messages.is_empty());

    // it worked, let's query the state
    let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!("addr0001", config.owner.as_str());
    assert_eq!(Decimal::percent(DEFAULT_QUORUM), config.quorum);
    assert_eq!(Decimal::percent(DEFAULT_THRESHOLD), config.threshold);
    assert_eq!(DEFAULT_VOTING_PERIOD, config.voting_period);
    assert_eq!(DEFAULT_TIMELOCK_PERIOD, config.timelock_period);
    assert_eq!(DEFAULT_PROPOSAL_DEPOSIT, config.proposal_deposit.u128());

    // update left items
    let env = mock_env();
    let info = mock_info("addr0001", &[]);
    let msg = ExecuteMsg::Governance {
        governance_msg: GovernanceMsg::UpdateConfig {
            owner: None,
            quorum: Some(Decimal::percent(20)),
            threshold: Some(Decimal::percent(75)),
            voting_period: Some(20000u64),
            timelock_period: Some(20000u64),
            proposal_deposit: Some(Uint128::new(123u128)),
            snapshot_period: Some(11),
            psi_nexprism_staking: Some(PSI_NEXPRISM_STAKING.to_owned()),
        },
    };

    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert!(execute_res.messages.is_empty());

    // it worked, let's query the state
    let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!("addr0001", config.owner.as_str());
    assert_eq!(Decimal::percent(20), config.quorum);
    assert_eq!(Decimal::percent(75), config.threshold);
    assert_eq!(20000u64, config.voting_period);
    assert_eq!(20000u64, config.timelock_period);
    assert_eq!(123u128, config.proposal_deposit.u128());
    assert_eq!(11u64, config.snapshot_period);

    // Unauthorzied err
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::Governance {
        governance_msg: GovernanceMsg::UpdateConfig {
            owner: None,
            quorum: None,
            threshold: None,
            voting_period: None,
            timelock_period: None,
            proposal_deposit: None,
            snapshot_period: None,
            psi_nexprism_staking: None,
        },
    };

    let execute_res = execute(deps.as_mut(), env, info, msg);
    match execute_res {
        Err(StdError::GenericErr { msg }) => assert_eq!(msg, "unauthorized"),
        _ => panic!("Must return unauthorized error"),
    }
}

#[test]
fn add_several_execute_msgs() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);
    let env = mock_env_height(0, 10000);
    let info = mock_info(VOTING_TOKEN, &[]);

    let exec_msg_bz = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(123),
    })
    .unwrap();

    let exec_msg_bz2 = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(12),
    })
    .unwrap();

    let exec_msg_bz3 = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(1),
    })
    .unwrap();

    // push two execute msgs to the list
    let execute_msgs = vec![
        PollExecuteMsg {
            order: 1u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz,
        },
        PollExecuteMsg {
            order: 3u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz3,
        },
        PollExecuteMsg {
            order: 2u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz2,
        },
    ];

    let msg = create_poll_msg("test", "test", None, Some(execute_msgs.clone()), None);

    let execute_res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_create_poll_result(
        1,
        env.block.time.plus_seconds(DEFAULT_VOTING_PERIOD).seconds(),
        TEST_CREATOR,
        execute_res,
        &deps,
    );

    let res = query(deps.as_ref(), mock_env(), QueryMsg::Poll { poll_id: 1 }).unwrap();
    let value: PollResponse = from_binary(&res).unwrap();

    let response_execute_data = value.execute_data.unwrap();
    assert_eq!(response_execute_data.len(), 3);
    assert_eq!(response_execute_data, execute_msgs);
}

#[test]
fn execute_poll_with_order() {
    const POLL_START_HEIGHT: u64 = 1000;
    const POLL_ID: u64 = 1;
    let stake_amount = 1000;

    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(&mut deps);
    let mut creator_env = mock_env_height(POLL_START_HEIGHT, 10000);
    let mut creator_info = mock_info(VOTING_TOKEN, &coins(2, VOTING_TOKEN));

    let exec_msg_bz = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(10),
    })
    .unwrap();

    let exec_msg_bz2 = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(20),
    })
    .unwrap();

    let exec_msg_bz3 = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(30),
    })
    .unwrap();

    let exec_msg_bz4 = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(40),
    })
    .unwrap();

    let exec_msg_bz5 = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(50),
    })
    .unwrap();

    let migrate_msg_bz = to_binary(&MigrateMsg { id: 10 }).unwrap();
    let migrate_msg_bz2 = to_binary(&MigrateMsg { id: 20 }).unwrap();
    let migrate_msg_bz3 = to_binary(&MigrateMsg { id: 30 }).unwrap();
    let migrate_msg_bz4 = to_binary(&MigrateMsg { id: 40 }).unwrap();
    let migrate_msg_bz5 = to_binary(&MigrateMsg { id: 50 }).unwrap();

    //add five messages with different order
    let execute_msgs: Vec<PollExecuteMsg> = vec![
        PollExecuteMsg {
            order: 3u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz3.clone(),
        },
        PollExecuteMsg {
            order: 4u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz4.clone(),
        },
        PollExecuteMsg {
            order: 2u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz2.clone(),
        },
        PollExecuteMsg {
            order: 5u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz5.clone(),
        },
        PollExecuteMsg {
            order: 1u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz.clone(),
        },
    ];

    //and migrate messages
    let migrate_msgs = vec![
        PollMigrateMsg {
            order: 3u64,
            contract: VOTING_TOKEN.to_string(),
            msg: migrate_msg_bz3.clone(),
            new_code_id: 11,
        },
        PollMigrateMsg {
            order: 4u64,
            contract: VOTING_TOKEN.to_string(),
            msg: migrate_msg_bz4.clone(),
            new_code_id: 11,
        },
        PollMigrateMsg {
            order: 2u64,
            contract: VOTING_TOKEN.to_string(),
            msg: migrate_msg_bz2.clone(),
            new_code_id: 11,
        },
        PollMigrateMsg {
            order: 5u64,
            contract: VOTING_TOKEN.to_string(),
            msg: migrate_msg_bz5.clone(),
            new_code_id: 11,
        },
        PollMigrateMsg {
            order: 1u64,
            contract: VOTING_TOKEN.to_string(),
            msg: migrate_msg_bz.clone(),
            new_code_id: 11,
        },
    ];

    let msg = create_poll_msg(
        "test",
        "test",
        None,
        Some(execute_msgs.clone()),
        Some(migrate_msgs.clone()),
    );

    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap();

    assert_create_poll_result(
        1,
        creator_env
            .block
            .time
            .plus_seconds(DEFAULT_VOTING_PERIOD)
            .seconds(),
        TEST_CREATOR,
        execute_res,
        &deps,
    );

    let query_res = query(
        deps.as_ref(),
        creator_env.clone(),
        QueryMsg::Polls {
            filter: None,
            start_after: None,
            limit: None,
            order_by: Some(OrderBy::Desc),
        },
    )
    .unwrap();
    let query_response: PollsResponse = from_binary(&query_res).unwrap();
    assert_eq!(
        query_response.polls,
        vec![PollResponse {
            id: 1u64,
            creator: TEST_CREATOR.to_string(),
            status: PollStatus::InProgress,
            end_time: creator_env
                .block
                .time
                .plus_seconds(DEFAULT_VOTING_PERIOD)
                .seconds(),
            title: "test".to_string(),
            description: "test".to_string(),
            link: None,
            deposit_amount: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
            execute_data: Some(execute_msgs),
            migrate_data: Some(migrate_msgs),
            yes_votes: Uint128::zero(),
            no_votes: Uint128::zero(),
            staked_amount: None,
            total_balance_at_end_poll: None,
        }]
    );

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new((stake_amount + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(stake_amount as u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_stake_tokens_result(
        stake_amount,
        DEFAULT_PROPOSAL_DEPOSIT,
        stake_amount,
        1,
        execute_res,
        &deps,
    );

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(stake_amount),
        },
    };
    let env = mock_env_height(POLL_START_HEIGHT, 10000);
    let info = mock_info(TEST_VOTER, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "cast_vote"),
            attr("poll_id", POLL_ID.to_string()),
            attr("amount", "1000"),
            attr("voter", TEST_VOTER),
            attr("vote_option", "yes"),
        ]
    );

    creator_info.sender = Addr::unchecked(TEST_CREATOR);
    creator_env.block.time = creator_env.block.time.plus_seconds(DEFAULT_VOTING_PERIOD);

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::EndPoll { poll_id: 1 },
    };
    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "end_poll"),
            attr("poll_id", "1"),
            attr("rejected_reason", ""),
            attr("passed", "true"),
        ]
    );
    assert_eq!(
        execute_res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: VOTING_TOKEN.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: TEST_CREATOR.to_string(),
                amount: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
            })
            .unwrap(),
            funds: vec![],
        }))]
    );

    // End poll will withdraw deposit balance
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new(stake_amount as u128),
        )],
    )]);

    creator_env.block.time = creator_env.block.time.plus_seconds(DEFAULT_TIMELOCK_PERIOD);
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::ExecutePoll { poll_id: 1 },
    };
    let execute_res = execute(deps.as_mut(), creator_env.clone(), creator_info, msg).unwrap();
    assert_eq!(
        execute_res.messages,
        vec![SubMsg::reply_on_error(
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: creator_env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::Yourself {
                    yourself_msg: services::governance::YourselfMsg::ExecutePollMsgs { poll_id: 1 },
                })
                .unwrap(),
                funds: vec![],
            }),
            1
        )]
    );

    let msg = ExecuteMsg::Yourself {
        yourself_msg: services::governance::YourselfMsg::ExecutePollMsgs { poll_id: 1 },
    };
    let contract_info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let execute_res = execute(deps.as_mut(), creator_env, contract_info, msg).unwrap();
    assert_eq!(
        execute_res.messages,
        vec![
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: exec_msg_bz,
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: exec_msg_bz2,
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: exec_msg_bz3,
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: exec_msg_bz4,
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: exec_msg_bz5,
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Migrate {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: migrate_msg_bz,
                new_code_id: 11
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Migrate {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: migrate_msg_bz2,
                new_code_id: 11
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Migrate {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: migrate_msg_bz3,
                new_code_id: 11
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Migrate {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: migrate_msg_bz4,
                new_code_id: 11
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Migrate {
                contract_addr: VOTING_TOKEN.to_string(),
                msg: migrate_msg_bz5,
                new_code_id: 11
            })),
        ]
    );
    assert_eq!(
        execute_res.attributes,
        vec![attr("action", "execute_poll"), attr("poll_id", "1"),]
    );
    let tmp_poll_id = load_tmp_poll_id(&deps.storage).unwrap();
    assert_eq!(tmp_poll_id, 1);
}

#[test]
fn snapshot_poll() {
    let stake_amount = 1000;

    let mut deps = mock_dependencies(&coins(100, VOTING_TOKEN));
    mock_init(&mut deps);

    let msg = create_poll_msg("test", "test", None, None, None);
    let mut creator_env = mock_env();
    let creator_info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap();

    let poll_end_time = creator_env.block.time.plus_seconds(DEFAULT_VOTING_PERIOD);
    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "create_poll"),
            attr("creator", TEST_CREATOR),
            attr("poll_id", "1"),
            attr("end_time", poll_end_time.seconds().to_string()),
        ]
    );

    //must not be executed
    let snapshot_err = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        ExecuteMsg::Anyone {
            anyone_msg: AnyoneMsg::SnapshotPoll { poll_id: 1 },
        },
    )
    .unwrap_err();
    assert_eq!(
        StdError::generic_err("Cannot snapshot at this time",),
        snapshot_err
    );

    // change time
    creator_env.block.time = poll_end_time.minus_seconds(10);

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new((stake_amount + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let fix_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        ExecuteMsg::Anyone {
            anyone_msg: AnyoneMsg::SnapshotPoll { poll_id: 1 },
        },
    )
    .unwrap();

    assert_eq!(
        fix_res.attributes,
        vec![
            attr("action", "snapshot_poll"),
            attr("poll_id", "1"),
            attr("staked_amount", stake_amount.to_string()),
        ]
    );

    //must not be executed
    let snapshot_error = execute(
        deps.as_mut(),
        creator_env,
        creator_info,
        ExecuteMsg::Anyone {
            anyone_msg: AnyoneMsg::SnapshotPoll { poll_id: 1 },
        },
    )
    .unwrap_err();
    assert_eq!(
        StdError::generic_err("Snapshot has already occurred"),
        snapshot_error
    );
}

#[test]
fn happy_days_cast_vote_with_snapshot() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let env = mock_env_height(0, 0);
    let info = mock_info(VOTING_TOKEN, &[]);
    let msg = create_poll_msg("test", "test", None, None, None);

    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_create_poll_result(1, DEFAULT_VOTING_PERIOD, TEST_CREATOR, execute_res, &deps);

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new(11u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(11u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_stake_tokens_result(11, DEFAULT_PROPOSAL_DEPOSIT, 11, 1, execute_res, &deps);

    //cast_vote without snapshot
    let env = mock_env_height(0, 0);
    let info = mock_info(TEST_VOTER, &coins(11, VOTING_TOKEN));
    let amount = 10u128;

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(amount),
        },
    };

    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_cast_vote_success(TEST_VOTER, amount, 1, VoteOption::Yes, execute_res);

    // balance be double
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new(22u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    let res = query(deps.as_ref(), mock_env(), QueryMsg::Poll { poll_id: 1 }).unwrap();
    let value: PollResponse = from_binary(&res).unwrap();
    assert_eq!(value.staked_amount, None);
    let end_time = value.end_time;

    //cast another vote
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER_2.to_string(),
        amount: Uint128::from(11u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    execute(deps.as_mut(), env, info, msg).unwrap();

    // another voter cast a vote
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(10u128),
        },
    };
    let env = mock_env_height(0, end_time - 9);
    let info = mock_info(TEST_VOTER_2, &[]);
    let execute_res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_cast_vote_success(TEST_VOTER_2, amount, 1, VoteOption::Yes, execute_res);

    let res = query(deps.as_ref(), mock_env(), QueryMsg::Poll { poll_id: 1 }).unwrap();
    let value: PollResponse = from_binary(&res).unwrap();
    assert_eq!(value.staked_amount, Some(Uint128::new(22)));

    // snanpshot poll will not go through
    let snap_error = execute(
        deps.as_mut(),
        env,
        info,
        ExecuteMsg::Anyone {
            anyone_msg: AnyoneMsg::SnapshotPoll { poll_id: 1 },
        },
    )
    .unwrap_err();
    assert_eq!(
        StdError::generic_err("Snapshot has already occurred"),
        snap_error
    );

    // balance be double
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new(33u128 + DEFAULT_PROPOSAL_DEPOSIT),
        )],
    )]);

    // another voter cast a vote but the snapshot is already occurred
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER_3.to_string(),
        amount: Uint128::from(11u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    execute(deps.as_mut(), env, info, msg).unwrap();
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(10u128),
        },
    };
    let env = mock_env_height(0, end_time - 8);
    let info = mock_info(TEST_VOTER_3, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_cast_vote_success(TEST_VOTER_3, amount, 1, VoteOption::Yes, execute_res);

    let res = query(deps.as_ref(), mock_env(), QueryMsg::Poll { poll_id: 1 }).unwrap();
    let value: PollResponse = from_binary(&res).unwrap();
    assert_eq!(value.staked_amount, Some(Uint128::new(22)));
}

#[test]
fn fails_end_poll_quorum_inflation_without_snapshot_poll() {
    const POLL_START_TIME: u64 = 1000;
    const POLL_ID: u64 = 1;
    let stake_amount = 1000;

    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(&mut deps);

    let mut creator_env = mock_env_height(0, POLL_START_TIME);
    let mut creator_info = mock_info(VOTING_TOKEN, &coins(2, VOTING_TOKEN));

    let exec_msg_bz = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(123),
    })
    .unwrap();

    //add two messages
    let execute_msgs = vec![
        PollExecuteMsg {
            order: 1u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz.clone(),
        },
        PollExecuteMsg {
            order: 2u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz,
        },
    ];

    let msg = create_poll_msg("test", "test", None, Some(execute_msgs), None);

    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap();

    assert_create_poll_result(
        1,
        creator_env
            .block
            .time
            .plus_seconds(DEFAULT_VOTING_PERIOD)
            .seconds(),
        TEST_CREATOR,
        execute_res,
        &deps,
    );

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new((stake_amount + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(stake_amount as u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_stake_tokens_result(
        stake_amount,
        DEFAULT_PROPOSAL_DEPOSIT,
        stake_amount,
        1,
        execute_res,
        &deps,
    );

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(stake_amount),
        },
    };
    let env = mock_env_height(0, POLL_START_TIME);
    let info = mock_info(TEST_VOTER, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "cast_vote"),
            attr("poll_id", POLL_ID.to_string()),
            attr("amount", "1000"),
            attr("voter", TEST_VOTER),
            attr("vote_option", "yes"),
        ]
    );

    creator_env.block.time = creator_env
        .block
        .time
        .plus_seconds(DEFAULT_VOTING_PERIOD - 10);

    // did not SnapshotPoll

    // staked amount get increased 10 times
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new(((10 * stake_amount) + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    //cast another vote
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER_2.to_string(),
        amount: Uint128::from(8 * stake_amount as u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    execute(deps.as_mut(), env, info, msg).unwrap();

    // another voter cast a vote
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(stake_amount),
        },
    };
    let env = mock_env_height(creator_env.block.height, 10000);
    let info = mock_info(TEST_VOTER_2, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "cast_vote"),
            attr("poll_id", POLL_ID.to_string()),
            attr("amount", "1000"),
            attr("voter", TEST_VOTER_2),
            attr("vote_option", "yes"),
        ]
    );

    creator_info.sender = Addr::unchecked(TEST_CREATOR);
    creator_env.block.time = creator_env.block.time.plus_seconds(10);

    // quorum must reach
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::EndPoll { poll_id: 1 },
    };
    let execute_res = execute(deps.as_mut(), creator_env, creator_info, msg).unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "end_poll"),
            attr("poll_id", "1"),
            attr("rejected_reason", "Quorum not reached"),
            attr("passed", "false"),
        ]
    );

    let res = query(deps.as_ref(), mock_env(), QueryMsg::Poll { poll_id: 1 }).unwrap();
    let value: PollResponse = from_binary(&res).unwrap();
    assert_eq!(
        10 * stake_amount,
        value.total_balance_at_end_poll.unwrap().u128()
    );
}

#[test]
fn happy_days_end_poll_with_controlled_quorum() {
    const POLL_START_TIME: u64 = 1000;
    const POLL_ID: u64 = 1;
    let stake_amount = 1000;

    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(&mut deps);

    let mut creator_env = mock_env_height(0, POLL_START_TIME);
    let mut creator_info = mock_info(VOTING_TOKEN, &coins(2, VOTING_TOKEN));

    let exec_msg_bz = to_binary(&Cw20ExecuteMsg::Burn {
        amount: Uint128::new(123),
    })
    .unwrap();

    //add two messages
    let execute_msgs = vec![
        PollExecuteMsg {
            order: 1u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz.clone(),
        },
        PollExecuteMsg {
            order: 2u64,
            contract: VOTING_TOKEN.to_string(),
            msg: exec_msg_bz,
        },
    ];

    let msg = create_poll_msg("test", "test", None, Some(execute_msgs), None);

    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap();

    assert_create_poll_result(
        1,
        creator_env
            .block
            .time
            .plus_seconds(DEFAULT_VOTING_PERIOD)
            .seconds(),
        TEST_CREATOR,
        execute_res,
        &deps,
    );

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new((stake_amount + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(stake_amount as u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_stake_tokens_result(
        stake_amount,
        DEFAULT_PROPOSAL_DEPOSIT,
        stake_amount,
        1,
        execute_res,
        &deps,
    );

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(stake_amount),
        },
    };
    let env = mock_env_height(0, POLL_START_TIME);
    let info = mock_info(TEST_VOTER, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "cast_vote"),
            attr("poll_id", POLL_ID.to_string()),
            attr("amount", "1000"),
            attr("voter", TEST_VOTER),
            attr("vote_option", "yes"),
        ]
    );

    creator_env.block.time = creator_env
        .block
        .time
        .plus_seconds(DEFAULT_VOTING_PERIOD - 10);

    // send SnapshotPoll
    let fix_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        ExecuteMsg::Anyone {
            anyone_msg: AnyoneMsg::SnapshotPoll { poll_id: 1 },
        },
    )
    .unwrap();

    assert_eq!(
        fix_res.attributes,
        vec![
            attr("action", "snapshot_poll"),
            attr("poll_id", "1"),
            attr("staked_amount", stake_amount.to_string()),
        ]
    );

    // staked amount get increased 10 times
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::new(((10 * stake_amount) + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    //cast another vote
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER_2.to_string(),
        amount: Uint128::from(8 * stake_amount as u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    execute(deps.as_mut(), env, info, msg).unwrap();

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(8 * stake_amount),
        },
    };
    let env = mock_env_height(creator_env.block.height, 10000);
    let info = mock_info(TEST_VOTER_2, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "cast_vote"),
            attr("poll_id", POLL_ID.to_string()),
            attr("amount", "8000"),
            attr("voter", TEST_VOTER_2),
            attr("vote_option", "yes"),
        ]
    );

    creator_info.sender = Addr::unchecked(TEST_CREATOR);
    creator_env.block.time = creator_env.block.time.plus_seconds(10);

    // quorum must reach
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::EndPoll { poll_id: 1 },
    };
    let execute_res = execute(deps.as_mut(), creator_env, creator_info, msg).unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "end_poll"),
            attr("poll_id", "1"),
            attr("rejected_reason", ""),
            attr("passed", "true"),
        ]
    );
    assert_eq!(
        execute_res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: VOTING_TOKEN.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: TEST_CREATOR.to_string(),
                amount: Uint128::new(DEFAULT_PROPOSAL_DEPOSIT),
            })
            .unwrap(),
            funds: vec![],
        }))]
    );

    let res = query(deps.as_ref(), mock_env(), QueryMsg::Poll { poll_id: 1 }).unwrap();
    let value: PollResponse = from_binary(&res).unwrap();
    assert_eq!(
        stake_amount,
        value.total_balance_at_end_poll.unwrap().u128()
    );

    assert_eq!(value.yes_votes.u128(), 9 * stake_amount);

    // actual staked amount is 10 times bigger than staked amount
    let actual_staked_weight = query_token_balance(
        deps.as_ref(),
        &Addr::unchecked(VOTING_TOKEN),
        &Addr::unchecked(MOCK_CONTRACT_ADDR),
    )
    .unwrap()
    .checked_sub(Uint128::new(DEFAULT_PROPOSAL_DEPOSIT))
    .unwrap();

    assert_eq!(actual_staked_weight.u128(), (10 * stake_amount))
}

#[test]
fn execute_poll_with_empty_execute_data() {
    const POLL_START_HEIGHT: u64 = 1000;
    const POLL_ID: u64 = 1;
    let stake_amount = 1000;

    let mut deps = mock_dependencies(&coins(1000, VOTING_TOKEN));
    mock_init(&mut deps);
    let mut creator_env = mock_env_height(POLL_START_HEIGHT, 10000);
    let mut creator_info = mock_info(VOTING_TOKEN, &coins(2, VOTING_TOKEN));

    let msg = create_poll_msg("test", "test", None, None, None);

    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap();

    assert_create_poll_result(
        1,
        creator_env
            .block
            .time
            .plus_seconds(DEFAULT_VOTING_PERIOD)
            .seconds(),
        TEST_CREATOR,
        execute_res,
        &deps,
    );

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::from((stake_amount + DEFAULT_PROPOSAL_DEPOSIT) as u128),
        )],
    )]);

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::from(stake_amount as u128),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });

    let info = mock_info(VOTING_TOKEN, &[]);
    let execute_res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_stake_tokens_result(
        stake_amount,
        DEFAULT_PROPOSAL_DEPOSIT,
        stake_amount,
        1,
        execute_res,
        &deps,
    );

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::CastVote {
            poll_id: 1,
            vote: VoteOption::Yes,
            amount: Uint128::from(stake_amount),
        },
    };
    let env = mock_env_height(POLL_START_HEIGHT, 10000);
    let info = mock_info(TEST_VOTER, &[]);
    let execute_res = execute(deps.as_mut(), env, info, msg).unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "cast_vote"),
            attr("poll_id", POLL_ID.to_string().as_str()),
            attr("amount", "1000"),
            attr("voter", TEST_VOTER),
            attr("vote_option", "yes"),
        ]
    );

    creator_info.sender = Addr::unchecked(TEST_CREATOR);
    creator_env.block.time = creator_env.block.time.plus_seconds(DEFAULT_VOTING_PERIOD);

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::EndPoll { poll_id: 1 },
    };
    let execute_res = execute(
        deps.as_mut(),
        creator_env.clone(),
        creator_info.clone(),
        msg,
    )
    .unwrap();

    assert_eq!(
        execute_res.attributes,
        vec![
            attr("action", "end_poll"),
            attr("poll_id", "1"),
            attr("rejected_reason", ""),
            attr("passed", "true"),
        ]
    );
    assert_eq!(
        execute_res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: VOTING_TOKEN.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: TEST_CREATOR.to_string(),
                amount: Uint128::from(DEFAULT_PROPOSAL_DEPOSIT),
            })
            .unwrap(),
            funds: vec![],
        }))]
    );

    // End poll will withdraw deposit balance
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::from(stake_amount as u128),
        )],
    )]);

    creator_env.block.time = creator_env.block.time.plus_seconds(DEFAULT_TIMELOCK_PERIOD);
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::ExecutePoll { poll_id: 1 },
    };
    let execute_err = execute(deps.as_mut(), creator_env, creator_info, msg).unwrap_err();
    match execute_err {
        StdError::GenericErr { msg, .. } => {
            assert_eq!(msg, "The poll does not have executable data")
        }
        _ => panic!("DO NOT ENTER HERE"),
    }

    let res = query(deps.as_ref(), mock_env(), QueryMsg::Poll { poll_id: 1 }).unwrap();
    let poll_res: PollResponse = from_binary(&res).unwrap();
    assert_eq!(poll_res.status, PollStatus::Passed);

    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::Polls {
            filter: Some(PollStatus::Passed),
            start_after: None,
            limit: None,
            order_by: Some(OrderBy::Desc),
        },
    )
    .unwrap();
    let polls_res: PollsResponse = from_binary(&res).unwrap();
    assert_eq!(polls_res.polls[0], poll_res);
}

#[test]
fn lock_zero_tokens_fails() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::LockTokensForUtility {
            amount: Some(Uint128::zero()),
        },
    };

    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let resp = execute(deps.as_mut(), env, info, msg);

    assert_eq!(
        Err(StdError::generic_err("Trying to lock zero tokens")),
        resp
    );
}

#[test]
fn lock_tokens_fails_when_no_utility() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::LockTokensForUtility {
            amount: Some(Uint128::new(100)),
        },
    };

    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let resp = execute(deps.as_mut(), env, info, msg);

    assert_eq!(
        Err(StdError::not_found("nexus_governance::state::Utility")),
        resp
    );
}

#[test]
fn init_utility_fails_when_not_owner() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let msg = ExecuteMsg::Governance {
        governance_msg: GovernanceMsg::InitUtility {
            token: UTILITY_TOKEN.to_owned(),
        },
    };
    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let resp = execute(deps.as_mut(), env, info, msg);

    assert_eq!(Err(StdError::generic_err("unauthorized")), resp);
}

#[test]
fn destroy_utility_fails_when_not_owner() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let msg = ExecuteMsg::Governance {
        governance_msg: GovernanceMsg::DestroyUtility {},
    };
    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let resp = execute(deps.as_mut(), env, info, msg);

    assert_eq!(Err(StdError::generic_err("unauthorized")), resp);
}

#[test]
fn lock_tokens_fails_when_no_tokens() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let msg = ExecuteMsg::Governance {
        governance_msg: GovernanceMsg::InitUtility {
            token: UTILITY_TOKEN.to_owned(),
        },
    };
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    execute(deps.as_mut(), env, info, msg).unwrap();

    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::LockTokensForUtility {
            amount: Some(Uint128::new(100)),
        },
    };
    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let resp = execute(deps.as_mut(), env, info, msg);

    assert_eq!(Err(StdError::generic_err("No tokens to lock")), resp);
}

#[test]
fn lock_tokens_fails_when_less_available() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::new(10))],
    )]);

    let msg = ExecuteMsg::Governance {
        governance_msg: GovernanceMsg::InitUtility {
            token: UTILITY_TOKEN.to_owned(),
        },
    };
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    execute(deps.as_mut(), env, info, msg).unwrap();

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::new(10),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });
    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    execute(deps.as_mut(), env, info, msg).unwrap();

    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::LockTokensForUtility {
            amount: Some(Uint128::new(11)),
        },
    };
    let resp = execute(deps.as_mut(), env, info, msg);

    assert_eq!(
        Err(StdError::generic_err("Not enough tokens to lock")),
        resp
    );
}

#[test]
fn unlock_tokens_with_not_utility_tokens_fails() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let msg = ExecuteMsg::Governance {
        governance_msg: GovernanceMsg::InitUtility {
            token: UTILITY_TOKEN.to_owned(),
        },
    };
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    execute(deps.as_mut(), env, info, msg).unwrap();

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::new(10),
        msg: to_binary(&Cw20HookMsg::UnlockTokensForUtility {}).unwrap(),
    });
    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let resp = execute(deps.as_mut(), env, info, msg);

    assert_eq!(Err(StdError::generic_err("Unauthorized")), resp);
}

#[test]
fn unlock_more_tokens_then_locked_fails() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    let msg = ExecuteMsg::Governance {
        governance_msg: GovernanceMsg::InitUtility {
            token: UTILITY_TOKEN.to_owned(),
        },
    };
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    execute(deps.as_mut(), env, info, msg).unwrap();

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::new(10),
        msg: to_binary(&Cw20HookMsg::UnlockTokensForUtility {}).unwrap(),
    });
    let env = mock_env();
    let info = mock_info(UTILITY_TOKEN, &[]);
    let resp = execute(deps.as_mut(), env, info, msg);

    assert_eq!(
        Err(StdError::generic_err("Not enough locked tokens to unlock")),
        resp
    );
}

#[test]
fn withdraw_locked_tokens_fails() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::new(10))],
    )]);

    let msg = ExecuteMsg::Governance {
        governance_msg: GovernanceMsg::InitUtility {
            token: UTILITY_TOKEN.to_owned(),
        },
    };
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    execute(deps.as_mut(), env, info, msg).unwrap();

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::new(10),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });
    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    execute(deps.as_mut(), env, info, msg).unwrap();

    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::LockTokensForUtility {
            amount: Some(Uint128::new(7)),
        },
    };
    execute(deps.as_mut(), env, info, msg).unwrap();

    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::WithdrawVotingTokens {
            amount: Some(Uint128::new(4)),
        },
    };
    let resp = execute(deps.as_mut(), env, info, msg);

    assert_eq!(
        Err(StdError::generic_err(
            "User is trying to withdraw tokens locked for utility."
        )),
        resp
    );
}

#[test]
fn happy_utility_usage() {
    let mut deps = mock_dependencies(&[]);
    mock_init(&mut deps);

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::new(10))],
    )]);

    let msg = ExecuteMsg::Governance {
        governance_msg: GovernanceMsg::InitUtility {
            token: UTILITY_TOKEN.to_owned(),
        },
    };
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    execute(deps.as_mut(), env, info, msg).unwrap();

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::new(10),
        msg: to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap(),
    });
    let env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    execute(deps.as_mut(), env, info, msg).unwrap();

    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::LockTokensForUtility {
            amount: Some(Uint128::new(7)),
        },
    };
    let resp = execute(deps.as_mut(), env, info, msg);
    assert_eq!(
        Ok(Response::new()
            .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: UTILITY_TOKEN.to_owned(),
                msg: to_binary(&Cw20ExecuteMsg::Mint {
                    recipient: TEST_VOTER.to_owned(),
                    amount: Uint128::new(7),
                })
                .unwrap(),
                funds: vec![],
            }))
            .add_attribute("action", "lock_tokens_for_utility")
            .add_attribute("amount", "7")),
        resp
    );

    let query_resp: Uint128 = from_binary(
        &query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::UtilityLock {
                address: TEST_VOTER.to_owned(),
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(Uint128::new(7), query_resp);

    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::LockTokensForUtility { amount: None },
    };
    let resp = execute(deps.as_mut(), env, info, msg);
    assert_eq!(
        Ok(Response::new()
            .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: UTILITY_TOKEN.to_owned(),
                msg: to_binary(&Cw20ExecuteMsg::Mint {
                    recipient: TEST_VOTER.to_owned(),
                    amount: Uint128::new(3),
                })
                .unwrap(),
                funds: vec![],
            }))
            .add_attribute("action", "lock_tokens_for_utility")
            .add_attribute("amount", "3")),
        resp
    );

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: Uint128::new(2),
        msg: to_binary(&Cw20HookMsg::UnlockTokensForUtility {}).unwrap(),
    });
    let env = mock_env();
    let info = mock_info(UTILITY_TOKEN, &[]);
    let resp = execute(deps.as_mut(), env, info, msg);
    assert_eq!(
        Ok(Response::new()
            .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: UTILITY_TOKEN.to_owned(),
                msg: to_binary(&Cw20ExecuteMsg::Burn {
                    amount: Uint128::new(2)
                })
                .unwrap(),
                funds: vec![],
            }))
            .add_attribute("action", "unlock_tokens_for_utility")
            .add_attribute("amount", "2")),
        resp
    );

    let msg = ExecuteMsg::Governance {
        governance_msg: GovernanceMsg::DestroyUtility {},
    };
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    execute(deps.as_mut(), env, info, msg).unwrap();

    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::Anyone {
        anyone_msg: AnyoneMsg::WithdrawVotingTokens { amount: None },
    };
    let resp = execute(deps.as_mut(), env, info, msg);
    assert_eq!(
        Ok(Response::new()
            .add_message(WasmMsg::Execute {
                contract_addr: VOTING_TOKEN.to_owned(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: TEST_VOTER.to_owned(),
                    amount: Uint128::new(10),
                })
                .unwrap(),
                funds: vec![],
            })
            .add_attributes(vec![
                ("action", "withdraw"),
                ("recipient", TEST_VOTER),
                ("amount", "10"),
            ])),
        resp
    );
}
