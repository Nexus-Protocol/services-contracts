use crate::{
    commands, queries,
    state::{load_config, load_tmp_poll_id, store_config, store_state, Config, State},
    utils,
};

use cosmwasm_std::{
    entry_point, from_binary, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Reply,
    Response, StdError, StdResult, Uint128,
};

use cw20::Cw20ReceiveMsg;
use services::governance::{
    AnyoneMsg, Cw20HookMsg, ExecuteMsg, GovernanceMsg, InstantiateMsg, QueryMsg, YourselfMsg,
};

pub(crate) const MIN_TITLE_LENGTH: usize = 4;
pub(crate) const MAX_TITLE_LENGTH: usize = 64;
pub(crate) const MIN_DESC_LENGTH: usize = 4;
pub(crate) const MAX_DESC_LENGTH: usize = 1024;
pub(crate) const MIN_LINK_LENGTH: usize = 12;
pub(crate) const MAX_LINK_LENGTH: usize = 128;

pub(crate) const POLL_EXECUTE_REPLY_ID: u64 = 1;

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    utils::validate_quorum(msg.quorum)?;
    utils::validate_threshold(msg.threshold)?;

    let config = Config {
        psi_token: Addr::unchecked(""),
        owner: info.sender,
        quorum: msg.quorum,
        threshold: msg.threshold,
        voting_period: msg.voting_period,
        timelock_period: msg.timelock_period,
        proposal_deposit: msg.proposal_deposit,
        snapshot_period: msg.snapshot_period,
    };

    let state = State {
        poll_count: 0,
        total_share: Uint128::zero(),
        total_deposit: Uint128::zero(),
    };

    store_config(deps.storage, &config)?;
    store_state(deps.storage, &state)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> StdResult<Response> {
    match msg.id {
        POLL_EXECUTE_REPLY_ID => {
            let poll_id: u64 = load_tmp_poll_id(deps.storage)?;
            commands::fail_poll(deps, poll_id)
        }
        _ => Err(StdError::generic_err("reply id is invalid")),
    }
}

#[entry_point]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Governance { governance_msg } => {
            let config: Config = load_config(deps.storage)?;
            if info.sender != config.owner {
                return Err(StdError::generic_err("unauthorized"));
            }

            match governance_msg {
                GovernanceMsg::UpdateConfig {
                    owner,
                    quorum,
                    threshold,
                    voting_period,
                    timelock_period,
                    proposal_deposit,
                    snapshot_period,
                } => commands::update_config(
                    deps,
                    config,
                    owner,
                    quorum,
                    threshold,
                    voting_period,
                    timelock_period,
                    proposal_deposit,
                    snapshot_period,
                ),
            }
        }

        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),

        ExecuteMsg::Anyone { anyone_msg } => match anyone_msg {
            AnyoneMsg::RegisterToken { psi_token } => commands::register_token(deps, psi_token),
            AnyoneMsg::WithdrawVotingTokens { amount } => {
                commands::withdraw_voting_tokens(deps, env, info, amount)
            }
            AnyoneMsg::CastVote {
                poll_id,
                vote,
                amount,
            } => commands::cast_vote(deps, env, info, poll_id, vote, amount),
            AnyoneMsg::EndPoll { poll_id } => commands::end_poll(deps, env, poll_id),
            AnyoneMsg::ExecutePoll { poll_id } => commands::execute_poll(deps, env, poll_id),
            AnyoneMsg::SnapshotPoll { poll_id } => commands::snapshot_poll(deps, env, poll_id),
        },

        ExecuteMsg::Yourself { yourself_msg } => {
            if info.sender != env.contract.address {
                return Err(StdError::generic_err("unauthorized"));
            }

            match yourself_msg {
                YourselfMsg::ExecutePollMsgs { poll_id } => {
                    commands::execute_poll_messages(deps, poll_id)
                }
            }
        }
    }
}

pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<Response> {
    // only asset contract can execute this message
    let config: Config = load_config(deps.storage)?;
    if config.psi_token != info.sender {
        return Err(StdError::generic_err("unauthorized"));
    }

    let real_sender = Addr::unchecked(cw20_msg.sender);
    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::StakeVotingTokens {}) => {
            commands::stake_voting_tokens(deps, env, info, &config, real_sender, cw20_msg.amount)
        }
        Ok(Cw20HookMsg::CreatePoll {
            title,
            description,
            link,
            execute_msgs,
            migrate_msgs,
        }) => commands::create_poll(
            deps,
            env,
            real_sender,
            cw20_msg.amount,
            title,
            description,
            link,
            execute_msgs,
            migrate_msgs,
        ),

        Err(err) => Err(err),
    }
}

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&queries::query_config(deps)?),
        QueryMsg::State {} => to_binary(&queries::query_state(deps)?),
        QueryMsg::Staker { address } => to_binary(&queries::query_staker(deps, env, address)?),
        QueryMsg::Poll { poll_id } => to_binary(&queries::query_poll(deps, poll_id)?),
        QueryMsg::Polls {
            filter,
            start_after,
            limit,
            order_by,
        } => to_binary(&queries::query_polls(
            deps,
            filter,
            start_after,
            limit,
            order_by,
        )?),
        QueryMsg::Voters {
            poll_id,
            start_after,
            limit,
            order_by,
        } => to_binary(&queries::query_voters(
            deps,
            poll_id,
            start_after,
            limit,
            order_by,
        )?),
    }
}
