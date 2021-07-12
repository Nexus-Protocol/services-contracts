use cosmwasm_std::{
    attr, to_binary, Addr, CosmosMsg, Decimal, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, Storage, SubMsg, Uint128, WasmMsg,
};
use services::governance::{PollExecuteMsg, PollStatus, VoteOption, VoterInfo};

use crate::{
    querier::query_token_balance,
    state::{
        load_bank, load_config, load_poll, load_state, may_load_bank, poll_indexer_store,
        poll_voter_read, poll_voter_store, store_bank, store_config, store_poll, store_state,
        Config, ExecuteData, Poll, TokenManager,
    },
    utils,
};
use cw20::Cw20ExecuteMsg;

pub fn update_config(
    deps: DepsMut,
    mut current_config: Config,
    owner: Option<String>,
    quorum: Option<Decimal>,
    threshold: Option<Decimal>,
    voting_period: Option<u64>,
    timelock_period: Option<u64>,
    expiration_period: Option<u64>,
    proposal_deposit: Option<Uint128>,
    snapshot_period: Option<u64>,
) -> StdResult<Response> {
    if let Some(ref owner) = owner {
        current_config.owner = deps.api.addr_validate(owner)?;
    }

    if let Some(quorum) = quorum {
        current_config.quorum = quorum;
    }

    if let Some(threshold) = threshold {
        current_config.threshold = threshold;
    }

    if let Some(voting_period) = voting_period {
        current_config.voting_period = voting_period;
    }

    if let Some(timelock_period) = timelock_period {
        current_config.timelock_period = timelock_period;
    }

    if let Some(expiration_period) = expiration_period {
        current_config.expiration_period = expiration_period;
    }

    if let Some(proposal_deposit) = proposal_deposit {
        current_config.proposal_deposit = proposal_deposit;
    }

    if let Some(snapshot_period) = snapshot_period {
        current_config.snapshot_period = snapshot_period;
    }

    store_config(deps.storage, &current_config)?;
    Ok(Response::default())
}

pub fn stake_voting_tokens(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    config: &Config,
    sender: Addr,
    amount: Uint128,
) -> StdResult<Response> {
    if amount.is_zero() {
        return Err(StdError::generic_err("Insufficient funds sent"));
    }

    let mut token_manager = load_bank(deps.storage, &sender)?;
    let mut state = load_state(deps.storage)?;

    // balance already increased, so subtract deposit amount
    let psi_balance = query_token_balance(deps.as_ref(), &config.psi_token, &env.contract.address)?;
    let total_balance = psi_balance.checked_sub(state.total_deposit + amount)?;

    let share = if total_balance.is_zero() || state.total_share.is_zero() {
        amount
    } else {
        amount.multiply_ratio(state.total_share, total_balance)
    };

    token_manager.share += share;
    state.total_share += share;

    store_state(deps.storage, &state)?;
    store_bank(deps.storage, &sender, &token_manager)?;

    Ok(Response {
        messages: vec![],
        data: None,
        events: vec![],
        attributes: vec![
            attr("action", "staking"),
            attr("sender", sender),
            attr("share", share),
            attr("amount", amount),
        ],
    })
}

#[allow(clippy::too_many_arguments)]
pub fn create_poll(
    deps: DepsMut,
    env: Env,
    proposer: Addr,
    deposit_amount: Uint128,
    title: String,
    description: String,
    link: Option<String>,
    execute_msgs: Option<Vec<PollExecuteMsg>>,
) -> StdResult<Response> {
    utils::validate_title(&title)?;
    utils::validate_description(&description)?;
    utils::validate_link(&link)?;

    let config: Config = load_config(deps.storage)?;
    if deposit_amount < config.proposal_deposit {
        return Err(StdError::generic_err(format!(
            "Must deposit more than {} token",
            config.proposal_deposit
        )));
    }

    let mut state = load_state(deps.storage)?;
    let poll_id = state.poll_count + 1;

    // Increase poll count & total deposit amount
    state.poll_count += 1;
    state.total_deposit += deposit_amount;

    let mut data_list: Vec<ExecuteData> = vec![];
    let all_execute_data = if let Some(exe_msgs) = execute_msgs {
        for msgs in exe_msgs {
            let execute_data = ExecuteData {
                order: msgs.order,
                contract: deps.api.addr_validate(&msgs.contract)?,
                msg: msgs.msg,
            };
            data_list.push(execute_data)
        }
        Some(data_list)
    } else {
        None
    };

    let new_poll = Poll {
        id: poll_id,
        creator: proposer,
        status: PollStatus::InProgress,
        yes_votes: Uint128::zero(),
        no_votes: Uint128::zero(),
        end_height: env.block.height + config.voting_period,
        title,
        description,
        link,
        execute_data: all_execute_data,
        deposit_amount,
        total_balance_at_end_poll: None,
        staked_amount: None,
    };

    store_poll(deps.storage, &poll_id, &new_poll)?;
    poll_indexer_store(deps.storage, &PollStatus::InProgress)
        .save(&poll_id.to_be_bytes(), &true)?;

    store_state(deps.storage, &state)?;

    Ok(Response {
        events: vec![],
        messages: vec![],
        attributes: vec![
            attr("action", "create_poll"),
            attr("creator", new_poll.creator),
            attr("poll_id", poll_id),
            attr("end_height", new_poll.end_height),
        ],
        data: None,
    })
}

pub fn end_poll(deps: DepsMut, env: Env, poll_id: u64) -> StdResult<Response> {
    let mut a_poll: Poll = load_poll(deps.storage, &poll_id)?;

    if a_poll.status != PollStatus::InProgress {
        return Err(StdError::generic_err("Poll is not in progress"));
    }

    if a_poll.end_height > env.block.height {
        return Err(StdError::generic_err("Voting period has not expired"));
    }

    let no = a_poll.no_votes.u128();
    let yes = a_poll.yes_votes.u128();

    let tallied_weight = yes + no;

    let mut poll_status = PollStatus::Rejected;
    let mut rejected_reason = "";
    let mut passed = false;

    let mut messages: Vec<SubMsg> = vec![];
    let config = load_config(deps.storage)?;
    let mut state = load_state(deps.storage)?;

    let (quorum, staked_weight) = if state.total_share.u128() == 0 {
        (Decimal::zero(), Uint128::zero())
    } else if let Some(staked_amount) = a_poll.staked_amount {
        (
            Decimal::from_ratio(tallied_weight, staked_amount),
            staked_amount,
        )
    } else {
        let psi_balance =
            query_token_balance(deps.as_ref(), &config.psi_token, &env.contract.address)?;
        let staked_weight = psi_balance.checked_sub(state.total_deposit)?;

        (
            Decimal::from_ratio(tallied_weight, staked_weight),
            staked_weight,
        )
    };

    if tallied_weight == 0 || quorum < config.quorum {
        // Quorum: More than quorum of the total staked tokens at the end of the voting
        // period need to have participated in the vote.
        rejected_reason = "Quorum not reached";
    } else {
        if Decimal::from_ratio(yes, tallied_weight) > config.threshold {
            //Threshold: More than 50% of the tokens that participated in the vote
            // (after excluding “Abstain” votes) need to have voted in favor of the proposal (“Yes”).
            poll_status = PollStatus::Passed;
            passed = true;
        } else {
            rejected_reason = "Threshold not reached";
        }

        // Refunds deposit only when quorum is reached
        if !a_poll.deposit_amount.is_zero() {
            messages.push(SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: config.psi_token.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: a_poll.creator.to_string(),
                    amount: a_poll.deposit_amount,
                })?,
            })))
        }
    }

    // Decrease total deposit amount
    state.total_deposit = state.total_deposit.checked_sub(a_poll.deposit_amount)?;
    store_state(deps.storage, &state)?;

    // Update poll indexer
    poll_indexer_store(deps.storage, &PollStatus::InProgress).remove(&a_poll.id.to_be_bytes());
    poll_indexer_store(deps.storage, &poll_status).save(&a_poll.id.to_be_bytes(), &true)?;

    // Update poll status
    a_poll.status = poll_status;
    a_poll.total_balance_at_end_poll = Some(staked_weight);
    store_poll(deps.storage, &poll_id, &a_poll)?;

    Ok(Response {
        events: vec![],
        messages,
        attributes: vec![
            attr("action", "end_poll"),
            attr("poll_id", poll_id),
            attr("rejected_reason", rejected_reason),
            attr("passed", passed),
        ],
        data: None,
    })
}

pub fn execute_poll(deps: DepsMut, env: Env, poll_id: u64) -> StdResult<Response> {
    let config: Config = load_config(deps.storage)?;
    let mut a_poll = load_poll(deps.storage, &poll_id)?;

    if a_poll.status != PollStatus::Passed {
        return Err(StdError::generic_err("Poll is not in passed status"));
    }

    if a_poll.end_height + config.timelock_period > env.block.height {
        return Err(StdError::generic_err("Timelock period has not expired"));
    }

    poll_indexer_store(deps.storage, &PollStatus::Passed).remove(&poll_id.to_be_bytes());
    poll_indexer_store(deps.storage, &PollStatus::Executed).save(&poll_id.to_be_bytes(), &true)?;

    a_poll.status = PollStatus::Executed;
    store_poll(deps.storage, &poll_id, &a_poll)?;

    let mut messages: Vec<SubMsg> = vec![];
    if let Some(all_msgs) = a_poll.execute_data {
        let mut msgs = all_msgs;
        msgs.sort();
        for msg in msgs {
            messages.push(SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: msg.contract.to_string(),
                msg: msg.msg,
                funds: vec![],
            })))
        }
    } else {
        return Err(StdError::generic_err("The poll does not have execute_data"));
    }

    Ok(Response {
        events: vec![],
        messages,
        attributes: vec![attr("action", "execute_poll"), attr("poll_id", poll_id)],
        data: None,
    })
}

/// ExpirePoll is used to make the poll as expired state for querying purpose
pub fn expire_poll(deps: DepsMut, env: Env, poll_id: u64) -> StdResult<Response> {
    let config = load_config(deps.storage)?;
    let mut a_poll = load_poll(deps.storage, &poll_id)?;

    if a_poll.status != PollStatus::Passed {
        return Err(StdError::generic_err("Poll is not in passed status"));
    }

    if a_poll.execute_data.is_none() {
        return Err(StdError::generic_err(
            "Cannot make a text proposal to expired state",
        ));
    }

    if a_poll.end_height + config.expiration_period > env.block.height {
        return Err(StdError::generic_err("Expire height has not been reached"));
    }

    poll_indexer_store(deps.storage, &PollStatus::Passed).remove(&poll_id.to_be_bytes());
    poll_indexer_store(deps.storage, &PollStatus::Expired).save(&poll_id.to_be_bytes(), &true)?;

    a_poll.status = PollStatus::Expired;
    store_poll(deps.storage, &poll_id, &a_poll)?;

    Ok(Response {
        events: vec![],
        messages: vec![],
        attributes: vec![attr("action", "expire_poll"), attr("poll_id", poll_id)],
        data: None,
    })
}

/// SnapshotPoll is used to take a snapshot of the staked amount for quorum calculation
pub fn snapshot_poll(deps: DepsMut, env: Env, poll_id: u64) -> StdResult<Response> {
    let config = load_config(deps.storage)?;
    let mut a_poll = load_poll(deps.storage, &poll_id)?;

    if a_poll.status != PollStatus::InProgress {
        return Err(StdError::generic_err("Poll is not in progress"));
    }

    let time_to_end = a_poll.end_height - env.block.height;

    if time_to_end > config.snapshot_period {
        return Err(StdError::generic_err("Cannot snapshot at this height"));
    }

    if a_poll.staked_amount.is_some() {
        return Err(StdError::generic_err("Snapshot has already occurred"));
    }

    // store the current staked amount for quorum calculation
    let state = load_state(deps.storage)?;

    let psi_balance = query_token_balance(deps.as_ref(), &config.psi_token, &env.contract.address)?;
    let staked_amount = psi_balance.checked_sub(state.total_deposit)?;

    a_poll.staked_amount = Some(staked_amount);

    store_poll(deps.storage, &poll_id, &a_poll)?;

    Ok(Response {
        events: vec![],
        messages: vec![],
        attributes: vec![
            attr("action", "snapshot_poll"),
            attr("poll_id", poll_id),
            attr("staked_amount", staked_amount),
        ],
        data: None,
    })
}

pub fn cast_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    poll_id: u64,
    vote: VoteOption,
    amount: Uint128,
) -> StdResult<Response> {
    let config = load_config(deps.storage)?;
    let state = load_state(deps.storage)?;
    if poll_id == 0 || state.poll_count < poll_id {
        return Err(StdError::generic_err("Poll does not exist"));
    }

    let mut a_poll = load_poll(deps.storage, &poll_id)?;
    if a_poll.status != PollStatus::InProgress || env.block.height > a_poll.end_height {
        return Err(StdError::generic_err("Poll is not in progress"));
    }

    // Check the voter already has a vote on the poll
    if poll_voter_read(deps.storage, poll_id)
        //TODO: update cw-storage-plus to "0.7.0" and change this 'info.sender.as_bytes()'
        .load(&info.sender.to_string().as_bytes())
        .is_ok()
    {
        return Err(StdError::generic_err("User has already voted."));
    }

    let mut token_manager = load_bank(deps.storage, &info.sender)?;

    // convert share to amount
    let total_share = state.total_share;
    let psi_balance = query_token_balance(deps.as_ref(), &config.psi_token, &env.contract.address)?;
    let total_balance = psi_balance.checked_sub(state.total_deposit)?;

    if token_manager
        .share
        .multiply_ratio(total_balance, total_share)
        < amount
    {
        return Err(StdError::generic_err(
            "User does not have enough staked tokens.",
        ));
    }

    // update tally info
    if VoteOption::Yes == vote {
        a_poll.yes_votes += amount;
    } else {
        a_poll.no_votes += amount;
    }

    let vote_info = VoterInfo {
        vote,
        balance: amount,
    };
    token_manager
        .locked_balance
        .push((poll_id, vote_info.clone()));
    store_bank(deps.storage, &info.sender, &token_manager)?;

    // store poll voter && and update poll data
    //TODO: update cw-storage-plus to "0.7.0" and change this 'info.sender.as_bytes()'
    poll_voter_store(deps.storage, poll_id)
        .save(&info.sender.to_string().as_bytes(), &vote_info)?;

    // processing snapshot
    let time_to_end = a_poll.end_height - env.block.height;

    if time_to_end < config.snapshot_period && a_poll.staked_amount.is_none() {
        a_poll.staked_amount = Some(total_balance);
    }

    store_poll(deps.storage, &poll_id, &a_poll)?;

    Ok(Response {
        events: vec![],
        messages: vec![],
        attributes: vec![
            attr("action", "cast_vote"),
            attr("poll_id", poll_id),
            attr("amount", amount),
            attr("voter", info.sender),
            attr("vote_option", vote_info.vote),
        ],
        data: None,
    })
}

// Withdraw amount if not staked. By default all funds will be withdrawn.
pub fn withdraw_voting_tokens(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Option<Uint128>,
) -> StdResult<Response> {
    if let Some(mut token_manager) = may_load_bank(deps.storage, &info.sender)? {
        let config: Config = load_config(deps.storage)?;
        let mut state = load_state(deps.storage)?;
        let user_address = info.sender;

        // Load total share & total balance except proposal deposit amount
        let total_share = state.total_share.u128();
        let psi_balance =
            query_token_balance(deps.as_ref(), &config.psi_token, &env.contract.address)?;
        let total_balance = psi_balance.checked_sub(state.total_deposit)?.u128();

        let locked_balance =
            compute_locked_balance(deps.storage, &mut token_manager, &user_address)?;
        let locked_share = locked_balance * total_share / total_balance;
        let user_share = token_manager.share.u128();

        let withdraw_share = amount
            .map(|v| std::cmp::max(v.multiply_ratio(total_share, total_balance).u128(), 1u128))
            .unwrap_or_else(|| user_share - locked_share);
        let withdraw_amount = amount
            .map(|v| v.u128())
            .unwrap_or_else(|| withdraw_share * total_balance / total_share);

        if locked_share + withdraw_share > user_share {
            Err(StdError::generic_err(
                "User is trying to withdraw too many tokens.",
            ))
        } else {
            let share = user_share - withdraw_share;
            token_manager.share = Uint128::from(share);

            store_bank(deps.storage, &user_address, &token_manager)?;

            state.total_share = Uint128::from(total_share - withdraw_share);
            store_state(deps.storage, &state)?;

            Ok(Response {
                events: vec![],
                messages: vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: config.psi_token.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: user_address.to_string(),
                        amount: Uint128::new(withdraw_amount),
                    })?,
                    funds: vec![],
                }))],
                data: None,
                attributes: vec![
                    attr("action", "withdraw"),
                    attr("recipient", user_address),
                    attr("amount", withdraw_amount),
                ],
            })
        }
    } else {
        Err(StdError::generic_err("Nothing staked"))
    }
}

// removes not in-progress poll voter info & unlock tokens
// and returns the largest locked amount in participated polls.
fn compute_locked_balance(
    storage: &mut dyn Storage,
    token_manager: &mut TokenManager,
    voter: &Addr,
) -> StdResult<u128> {
    // filter out not in-progress polls
    token_manager.locked_balance.retain(|(poll_id, _)| {
        let poll: Poll = load_poll(storage, poll_id).unwrap();

        if poll.status != PollStatus::InProgress {
            // remove voter info from the poll
            poll_voter_store(storage, *poll_id).remove(&voter.to_string().as_bytes());
        }

        poll.status == PollStatus::InProgress
    });

    Ok(token_manager
        .locked_balance
        .iter()
        .map(|(_, v)| v.balance.u128())
        .max()
        .unwrap_or_default())
}
