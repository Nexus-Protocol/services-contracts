#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    from_binary, to_binary, Addr, Api, Binary, BlockInfo, CanonicalAddr, Decimal, Deps, DepsMut,
    Env, MessageInfo, Response, StdError, StdResult, Storage, Uint128, WasmMsg,
};

use services::staking::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg, StakerInfoResponse,
    StakingSchedule, StateResponse,
};

use crate::state::{
    read_config, read_staker_info, read_state, remove_staker_info, store_config, store_staker_info,
    store_state, Config, StakerInfo, State,
};

use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    store_config(
        deps.storage,
        &Config {
            owner: deps.api.addr_canonicalize(&msg.owner)?,
            psi_token: deps.api.addr_canonicalize(&msg.psi_token)?,
            staking_token: deps.api.addr_canonicalize(&msg.staking_token)?,
            distribution_schedule: msg.distribution_schedule,
        },
    )?;

    store_state(
        deps.storage,
        &State {
            last_distributed: get_time(&env.block),
            total_bond_amount: Uint128::zero(),
            global_reward_index: Decimal::zero(),
        },
    )?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Unbond { amount } => unbond(deps, env, info, amount),
        ExecuteMsg::Withdraw {} => withdraw(deps, env, info),
        ExecuteMsg::AddSchedules { schedules } => {
            assert_owner_privilege(deps.storage, deps.api, info.sender)?;
            add_schedules(deps, env, schedules)
        }
        ExecuteMsg::UpdateOwner { owner } => {
            assert_owner_privilege(deps.storage, deps.api, info.sender)?;
            update_owner(deps, owner)
        }
    }
}

fn assert_owner_privilege(storage: &dyn Storage, api: &dyn Api, sender: Addr) -> StdResult<()> {
    if read_config(storage)?.owner != api.addr_canonicalize(sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    Ok(())
}

pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;

    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::Bond {}) => {
            // only staking token contract can execute this message
            if config.staking_token != deps.api.addr_canonicalize(info.sender.as_str())? {
                return Err(StdError::generic_err("unauthorized"));
            }

            let cw20_sender = deps.api.addr_validate(&cw20_msg.sender)?;
            bond(deps, env, cw20_sender, cw20_msg.amount)
        }
        Err(_) => Err(StdError::generic_err("data should be given")),
    }
}

pub fn bond(deps: DepsMut, env: Env, sender_addr: Addr, amount: Uint128) -> StdResult<Response> {
    let current_time = get_time(&env.block);
    let sender_addr_raw: CanonicalAddr = deps.api.addr_canonicalize(sender_addr.as_str())?;

    let config: Config = read_config(deps.storage)?;
    let mut state: State = read_state(deps.storage)?;
    let mut staker_info: StakerInfo = read_staker_info(deps.storage, &sender_addr_raw)?;

    // Compute global reward & staker reward
    compute_reward(&config, &mut state, current_time);
    compute_staker_reward(&state, &mut staker_info)?;

    // Increase bond_amount
    increase_bond_amount(&mut state, &mut staker_info, amount);

    // Store updated state with staker's staker_info
    store_staker_info(deps.storage, &sender_addr_raw, &staker_info)?;
    store_state(deps.storage, &state)?;

    Ok(Response::new().add_attributes(vec![
        ("action", "bond"),
        ("owner", &sender_addr.to_string()),
        ("amount", &amount.to_string()),
    ]))
}

pub fn unbond(deps: DepsMut, env: Env, info: MessageInfo, amount: Uint128) -> StdResult<Response> {
    let current_time = get_time(&env.block);
    let config: Config = read_config(deps.storage)?;
    let sender_addr_raw: CanonicalAddr = deps.api.addr_canonicalize(info.sender.as_str())?;

    let mut state: State = read_state(deps.storage)?;
    let mut staker_info: StakerInfo = read_staker_info(deps.storage, &sender_addr_raw)?;

    if staker_info.bond_amount < amount {
        return Err(StdError::generic_err("Cannot unbond more than bond amount"));
    }

    // Compute global reward & staker reward
    compute_reward(&config, &mut state, current_time);
    compute_staker_reward(&state, &mut staker_info)?;

    // Decrease bond_amount
    decrease_bond_amount(&mut state, &mut staker_info, amount)?;

    // Store or remove updated rewards info
    // depends on the left pending reward and bond amount
    if staker_info.pending_reward.is_zero() && staker_info.bond_amount.is_zero() {
        remove_staker_info(deps.storage, &sender_addr_raw);
    } else {
        store_staker_info(deps.storage, &sender_addr_raw, &staker_info)?;
    }

    // Store updated state
    store_state(deps.storage, &state)?;

    Ok(Response::new()
        .add_message(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.staking_token)?.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount,
            })?,
            funds: vec![],
        })
        .add_attributes(vec![
            ("action", "unbond"),
            ("owner", &info.sender.to_string()),
            ("amount", &amount.to_string()),
        ]))
}

fn get_time(block: &BlockInfo) -> u64 {
    block.time.seconds()
}

// withdraw rewards to executor
pub fn withdraw(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let current_time = get_time(&env.block);
    let sender_addr_raw = deps.api.addr_canonicalize(info.sender.as_str())?;

    let config: Config = read_config(deps.storage)?;
    let mut state: State = read_state(deps.storage)?;
    let mut staker_info = read_staker_info(deps.storage, &sender_addr_raw)?;

    // Compute global reward & staker reward
    compute_reward(&config, &mut state, current_time);
    compute_staker_reward(&state, &mut staker_info)?;

    let amount = staker_info.pending_reward;
    staker_info.pending_reward = Uint128::zero();

    // Store or remove updated rewards info
    // depends on the left pending reward and bond amount
    if staker_info.bond_amount.is_zero() {
        remove_staker_info(deps.storage, &sender_addr_raw);
    } else {
        store_staker_info(deps.storage, &sender_addr_raw, &staker_info)?;
    }

    // Store updated state
    store_state(deps.storage, &state)?;

    Ok(Response::new()
        .add_message(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.psi_token)?.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount,
            })?,
            funds: vec![],
        })
        .add_attributes(vec![
            ("action", "withdraw"),
            ("owner", &info.sender.to_string()),
            ("amount", &amount.to_string()),
        ]))
}

pub fn add_schedules(
    deps: DepsMut,
    env: Env,
    mut new_schedules: Vec<StakingSchedule>,
) -> StdResult<Response> {
    let mut config = read_config(deps.storage)?;
    for schedule in new_schedules.iter() {
        if schedule.start_time < get_time(&env.block) {
            return Err(StdError::generic_err(
                "schedule start_time is smaller than current time",
            ));
        }
    }
    config.distribution_schedule.append(&mut new_schedules);

    store_config(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "add_schedules"))
}

pub fn update_owner(deps: DepsMut, new_owner: String) -> StdResult<Response> {
    let mut config = read_config(deps.storage)?;
    config.owner = deps.api.addr_canonicalize(&new_owner)?;
    store_config(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_owner"))
}

fn increase_bond_amount(state: &mut State, staker_info: &mut StakerInfo, amount: Uint128) {
    state.total_bond_amount += amount;
    staker_info.bond_amount += amount;
}

fn decrease_bond_amount(
    state: &mut State,
    staker_info: &mut StakerInfo,
    amount: Uint128,
) -> StdResult<()> {
    state.total_bond_amount = state.total_bond_amount.checked_sub(amount)?;
    staker_info.bond_amount = staker_info.bond_amount.checked_sub(amount)?;
    Ok(())
}

// compute distributed rewards and update global reward index
fn compute_reward(config: &Config, state: &mut State, current_time: u64) {
    if state.total_bond_amount.is_zero() {
        state.last_distributed = current_time;
        return;
    }

    let mut distributed_amount: Uint128 = Uint128::zero();
    for s in config.distribution_schedule.iter() {
        if s.start_time > current_time || s.end_time < state.last_distributed {
            continue;
        }

        let passed_time = std::cmp::min(s.end_time, current_time)
            - std::cmp::max(s.start_time, state.last_distributed);

        let time_period = s.end_time - s.start_time;
        let distribution_amount_per_time: Decimal = Decimal::from_ratio(s.amount, time_period);
        distributed_amount += distribution_amount_per_time * Uint128::from(passed_time as u128);
    }

    state.last_distributed = current_time;
    state.global_reward_index = state.global_reward_index
        + Decimal::from_ratio(distributed_amount, state.total_bond_amount);
}

// withdraw reward to pending reward
fn compute_staker_reward(state: &State, staker_info: &mut StakerInfo) -> StdResult<()> {
    let pending_reward = (staker_info.bond_amount * state.global_reward_index)
        .checked_sub(staker_info.bond_amount * staker_info.reward_index)?;

    staker_info.reward_index = state.global_reward_index;
    staker_info.pending_reward += pending_reward;
    Ok(())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State { time_seconds } => to_binary(&query_state(deps, time_seconds)?),
        QueryMsg::StakerInfo {
            staker,
            time_seconds,
        } => to_binary(&query_staker_info(deps, staker, time_seconds)?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let state = read_config(deps.storage)?;
    let resp = ConfigResponse {
        owner: deps.api.addr_humanize(&state.owner)?.to_string(),
        psi_token: deps.api.addr_humanize(&state.psi_token)?.to_string(),
        staking_token: deps.api.addr_humanize(&state.staking_token)?.to_string(),
        distribution_schedule: state.distribution_schedule,
    };

    Ok(resp)
}

pub fn query_state(deps: Deps, time_seconds: Option<u64>) -> StdResult<StateResponse> {
    let mut state: State = read_state(deps.storage)?;
    if let Some(time_seconds) = time_seconds {
        let config = read_config(deps.storage)?;
        compute_reward(&config, &mut state, time_seconds);
    }

    Ok(StateResponse {
        last_distributed: state.last_distributed,
        total_bond_amount: state.total_bond_amount,
        global_reward_index: state.global_reward_index,
    })
}

pub fn query_staker_info(
    deps: Deps,
    staker: String,
    time_seconds: Option<u64>,
) -> StdResult<StakerInfoResponse> {
    let staker_raw = deps.api.addr_canonicalize(&staker)?;

    let mut staker_info: StakerInfo = read_staker_info(deps.storage, &staker_raw)?;
    if let Some(time_seconds) = time_seconds {
        let config = read_config(deps.storage)?;
        let mut state = read_state(deps.storage)?;

        compute_reward(&config, &mut state, time_seconds);
        compute_staker_reward(&state, &mut staker_info)?;
    }

    Ok(StakerInfoResponse {
        staker,
        reward_index: staker_info.reward_index,
        bond_amount: staker_info.bond_amount,
        pending_reward: staker_info.pending_reward,
    })
}
