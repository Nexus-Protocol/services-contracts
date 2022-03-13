use cosmwasm_std::{Deps, Env, StdError, StdResult, Uint128};
use services::{
    common::OrderBy,
    governance::{
        ConfigResponse, PollExecuteMsg, PollMigrateMsg, PollResponse, PollStatus, PollsResponse,
        StakerResponse, StateResponse, VotersResponse, VotersResponseItem,
    },
};

use crate::{
    querier::query_token_balance,
    state::{
        load_bank, load_config, load_locked_tokens_for_utility, load_poll, load_state,
        load_utility, may_load_poll, read_poll_voters, read_polls, Config, Poll,
    },
};

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config: Config = load_config(deps.storage)?;
    Ok(ConfigResponse {
        owner: config.owner.to_string(),
        psi_token: config.psi_token.to_string(),
        quorum: config.quorum,
        threshold: config.threshold,
        voting_period: config.voting_period,
        timelock_period: config.timelock_period,
        proposal_deposit: config.proposal_deposit,
        snapshot_period: config.snapshot_period,
        utility_token: load_utility(deps.storage).map(|u| u.token.to_string()).ok(),
    })
}

pub fn query_state(deps: Deps) -> StdResult<StateResponse> {
    let state = load_state(deps.storage)?;
    Ok(StateResponse {
        poll_count: state.poll_count,
        total_share: state.total_share,
        total_deposit: state.total_deposit,
    })
}

pub fn query_poll(deps: Deps, poll_id: u64) -> StdResult<PollResponse> {
    let poll = may_load_poll(deps.storage, poll_id)?;
    if let Some(poll) = poll {
        let execute_messages: Option<Vec<PollExecuteMsg>> = poll.execute_data.map(|exe_msgs| {
            exe_msgs
                .iter()
                .map(|msg| PollExecuteMsg {
                    order: msg.order,
                    contract: msg.contract.to_string(),
                    msg: msg.msg.clone(),
                })
                .collect()
        });

        let migrate_messages: Option<Vec<PollMigrateMsg>> = poll.migrate_data.map(|migrate_msgs| {
            migrate_msgs
                .iter()
                .map(|msg| PollMigrateMsg {
                    order: msg.order,
                    contract: msg.contract.to_string(),
                    msg: msg.msg.clone(),
                    new_code_id: msg.new_code_id,
                })
                .collect()
        });

        Ok(PollResponse {
            id: poll.id,
            creator: poll.creator.to_string(),
            status: poll.status,
            end_time: poll.end_time,
            title: poll.title,
            description: poll.description,
            link: poll.link,
            deposit_amount: poll.deposit_amount,
            execute_data: execute_messages,
            migrate_data: migrate_messages,
            yes_votes: poll.yes_votes,
            no_votes: poll.no_votes,
            staked_amount: poll.staked_amount,
            total_balance_at_end_poll: poll.total_balance_at_end_poll,
        })
    } else {
        Err(StdError::generic_err("Poll does not exist"))
    }
}

pub fn query_polls(
    deps: Deps,
    filter: Option<PollStatus>,
    start_after: Option<u64>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<PollsResponse> {
    let polls = read_polls(deps.storage, filter, start_after, limit, order_by)?;

    let poll_responses: StdResult<Vec<PollResponse>> = polls
        .into_iter()
        .map(|poll| {
            let execute_messages: Option<Vec<PollExecuteMsg>> = poll.execute_data.map(|exe_msgs| {
                exe_msgs
                    .iter()
                    .map(|msg| PollExecuteMsg {
                        order: msg.order,
                        contract: msg.contract.to_string(),
                        msg: msg.msg.clone(),
                    })
                    .collect()
            });

            let migrate_messages: Option<Vec<PollMigrateMsg>> =
                poll.migrate_data.map(|migrate_msgs| {
                    migrate_msgs
                        .iter()
                        .map(|msg| PollMigrateMsg {
                            order: msg.order,
                            contract: msg.contract.to_string(),
                            msg: msg.msg.clone(),
                            new_code_id: msg.new_code_id,
                        })
                        .collect()
                });

            Ok(PollResponse {
                id: poll.id,
                creator: poll.creator.to_string(),
                status: poll.status.clone(),
                end_time: poll.end_time,
                title: poll.title.to_string(),
                description: poll.description.to_string(),
                link: poll.link.clone(),
                deposit_amount: poll.deposit_amount,
                execute_data: execute_messages,
                migrate_data: migrate_messages,
                yes_votes: poll.yes_votes,
                no_votes: poll.no_votes,
                staked_amount: poll.staked_amount,
                total_balance_at_end_poll: poll.total_balance_at_end_poll,
            })
        })
        .collect();

    Ok(PollsResponse {
        polls: poll_responses?,
    })
}

pub fn query_voters(
    deps: Deps,
    poll_id: u64,
    start_after: Option<String>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<VotersResponse> {
    let poll = may_load_poll(deps.storage, poll_id)?;
    if let Some(poll) = poll {
        let voters = if poll.status != PollStatus::InProgress {
            vec![]
        } else if let Some(start_after) = start_after {
            let start_after_address = deps.api.addr_validate(&start_after)?;
            read_poll_voters(
                deps.storage,
                poll_id,
                Some(start_after_address),
                limit,
                order_by,
            )?
        } else {
            read_poll_voters(deps.storage, poll_id, None, limit, order_by)?
        };

        let voters_response: StdResult<Vec<VotersResponseItem>> = voters
            .iter()
            .map(|voter_info| {
                Ok(VotersResponseItem {
                    voter: voter_info.0.to_string(),
                    vote: voter_info.1.vote.clone(),
                    balance: voter_info.1.balance,
                })
            })
            .collect();

        Ok(VotersResponse {
            voters: voters_response?,
        })
    } else {
        Err(StdError::generic_err("Poll does not exist"))
    }
}

pub fn query_staker(deps: Deps, env: Env, address: String) -> StdResult<StakerResponse> {
    let address = deps.api.addr_validate(&address)?;
    let config = load_config(deps.storage)?;
    let state = load_state(deps.storage)?;
    let mut token_manager = load_bank(deps.storage, &address)?;

    // filter out not in-progress polls
    token_manager.locked_balance.retain(|(poll_id, _)| {
        let poll: Poll = load_poll(deps.storage, *poll_id).unwrap();
        poll.status == PollStatus::InProgress
    });

    let psi_token = query_token_balance(deps, &config.psi_token, &env.contract.address)?;
    let total_balance = psi_token.checked_sub(state.total_deposit)?;

    let balance = if !state.total_share.is_zero() {
        token_manager
            .share
            .multiply_ratio(total_balance, state.total_share)
    } else {
        Uint128::zero()
    };
    Ok(StakerResponse {
        balance,
        share: token_manager.share,
        locked_balance: token_manager.locked_balance,
    })
}

pub fn query_utility_lock(deps: Deps, address: String) -> StdResult<Uint128> {
    load_locked_tokens_for_utility(deps.storage, &deps.api.addr_validate(&address)?)
}
