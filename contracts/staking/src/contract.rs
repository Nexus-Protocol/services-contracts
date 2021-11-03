#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    attr, from_binary, to_binary, Addr, Api, Attribute, Binary, BlockInfo, CanonicalAddr, Coin,
    Decimal, Deps, DepsMut, Env, MessageInfo, QuerierWrapper, Response, StdError, StdResult,
    Storage, Uint128, WasmMsg,
};

use services::staking::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    StakerInfoResponse, StakingSchedule, StateResponse,
};

use crate::state::{
    read_config, read_staker_info, read_state, remove_staker_info, store_config, store_staker_info,
    store_state, Config, StakerInfo, State,
};

use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::pair::ExecuteMsg as PairExecuteMsg;
use terraswap::querier::{query_pair_info, query_token_balance};

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
            terraswap_factory: deps.api.addr_canonicalize(&msg.terraswap_factory)?,
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
        ExecuteMsg::MigrateStaking {
            new_staking_contract,
        } => {
            assert_owner_privilege(deps.storage, deps.api, info.sender)?;
            migrate_staking(deps, env, new_staking_contract)
        }

        ExecuteMsg::AutoStake {
            assets,
            slippage_tolerance,
        } => auto_stake(deps, env, info, assets, slippage_tolerance),

        ExecuteMsg::AutoStakeHook {
            staker_addr,
            prev_staking_token_amount,
        } => {
            let api = deps.api;
            auto_stake_hook(
                deps,
                env,
                info,
                api.addr_validate(&staker_addr)?,
                prev_staking_token_amount,
            )
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
        ("staker_addr", &sender_addr.to_string()),
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
            ("staker_addr", &info.sender.to_string()),
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

pub fn migrate_staking(
    deps: DepsMut,
    env: Env,
    new_staking_contract: String,
) -> StdResult<Response> {
    let mut config: Config = read_config(deps.storage)?;
    let mut state: State = read_state(deps.storage)?;

    let current_time = get_time(&env.block);
    // compute global reward, sets last_distributed to current_time
    compute_reward(&config, &mut state, current_time);

    let total_distribution_amount: Uint128 = config
        .distribution_schedule
        .iter()
        .map(|item| item.amount)
        .sum();

    let current_time = get_time(&env.block);
    // eliminate distribution slots that have not started
    config
        .distribution_schedule
        .retain(|slot| slot.start_time < current_time);

    let mut distributed_amount = Uint128::zero();
    for s in config.distribution_schedule.iter_mut() {
        if s.end_time < current_time {
            // all distributed
            distributed_amount += s.amount;
        } else {
            // partially distributed slot
            let time_period = s.end_time - s.start_time;
            let distribution_amount_per_time: Decimal = Decimal::from_ratio(s.amount, time_period);

            let passed_time = current_time - s.start_time;
            let distributed_amount_on_slot =
                distribution_amount_per_time * Uint128::from(passed_time as u128);
            distributed_amount += distributed_amount_on_slot;

            // modify distribution slot
            s.end_time = current_time;
            s.amount = distributed_amount_on_slot;
        }
    }

    // update config
    store_config(deps.storage, &config)?;
    // update state
    store_state(deps.storage, &state)?;

    let remaining_psi = total_distribution_amount.checked_sub(distributed_amount)?;

    let psi_token_addr = deps.api.addr_humanize(&config.psi_token)?;
    Ok(Response::new()
        .add_messages(vec![WasmMsg::Execute {
            contract_addr: psi_token_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: new_staking_contract,
                amount: remaining_psi,
            })?,
            funds: vec![],
        }])
        .add_attributes(vec![
            ("action", "migrate_staking"),
            ("distributed_amount", &distributed_amount.to_string()),
            ("remaining_amount", &remaining_psi.to_string()),
        ]))
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
    if state.last_distributed >= current_time {
        return;
    }

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

pub fn auto_stake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: [Asset; 2],
    slippage_tolerance: Option<Decimal>,
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    let terraswap_factory: Addr = deps.api.addr_humanize(&config.terraswap_factory)?;

    // query pair info to obtain pair contract address
    let asset_infos: [AssetInfo; 2] = [assets[0].info.clone(), assets[1].info.clone()];
    let terraswap_pair: PairInfo = query_pair_info(&deps.querier, terraswap_factory, &asset_infos)?;

    if config.staking_token
        != deps
            .api
            .addr_canonicalize(terraswap_pair.liquidity_token.as_str())?
    {
        return Err(StdError::generic_err("Invalid staking token"));
    }

    // get current lp token amount to later compute the recived amount
    let prev_staking_token_amount = query_token_balance(
        &deps.querier,
        deps.api.addr_validate(&terraswap_pair.liquidity_token)?,
        env.contract.address.clone(),
    )?;

    let asset_0_messages = asset_transfer_and_increase_allowance(
        &assets[0],
        &info,
        &env,
        terraswap_pair.contract_addr.to_string(),
    )?;
    let asset_1_messages = asset_transfer_and_increase_allowance(
        &assets[1],
        &info,
        &env,
        terraswap_pair.contract_addr.to_string(),
    )?;
    let (provide_liquidity_message, attributes) = assets_provide_liquidity_message(
        assets,
        terraswap_pair.contract_addr.to_string(),
        slippage_tolerance,
        &deps.querier,
    )?;

    // 1. Transfer token asset to staking contract
    // 2. Increase allowance of token for pair contract
    // 3. Provide liquidity
    // 4. Execute staking hook, will stake in the name of the sender
    Ok(Response::new()
        .add_messages(asset_0_messages)
        .add_messages(asset_1_messages)
        .add_message(provide_liquidity_message)
        .add_messages(vec![WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::AutoStakeHook {
                staker_addr: info.sender.to_string(),
                prev_staking_token_amount,
            })?,
            funds: vec![],
        }])
        .add_attribute("action", "auto_stake")
        .add_attributes(attributes))
}

fn asset_transfer_and_increase_allowance(
    asset: &Asset,
    info: &MessageInfo,
    env: &Env,
    pair_contract_addr: String,
) -> StdResult<Vec<WasmMsg>> {
    match &asset.info {
        AssetInfo::Token {
            contract_addr: token_addr,
        } => Ok(vec![
            WasmMsg::Execute {
                contract_addr: token_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: info.sender.to_string(),
                    recipient: env.contract.address.to_string(),
                    amount: asset.amount,
                })?,
                funds: vec![],
            },
            WasmMsg::Execute {
                contract_addr: token_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: pair_contract_addr,
                    amount: asset.amount,
                    expires: None,
                })?,
                funds: vec![],
            },
        ]),
        AssetInfo::NativeToken { .. } => {
            asset.assert_sent_native_token_balance(info)?;
            Ok(vec![])
        }
    }
}

fn assets_provide_liquidity_message(
    assets: [Asset; 2],
    pair_contract_addr: String,
    slippage_tolerance: Option<Decimal>,
    querier: &QuerierWrapper,
) -> StdResult<(WasmMsg, Vec<Attribute>)> {
    let mut result_assets: Vec<Asset> = Vec::with_capacity(2);
    let mut result_coins: Vec<Coin> = Vec::with_capacity(2);
    let mut attributes: Vec<Attribute> = Vec::with_capacity(4);

    for (index, asset) in assets.iter().enumerate() {
        match &asset.info {
            AssetInfo::Token {
                contract_addr: token_addr,
            } => {
                result_assets.push(asset.clone());
                attributes.push(attr(format!("asset_token_{}", index), token_addr));
            }

            AssetInfo::NativeToken { denom } => {
                let tax_amount = asset.compute_tax(&querier)?;
                let deducted_tax_amount = asset.amount.checked_sub(tax_amount)?;
                result_assets.push(Asset {
                    amount: deducted_tax_amount,
                    info: asset.info.clone(),
                });

                result_coins.push(Coin {
                    denom: denom.clone(),
                    amount: deducted_tax_amount,
                });
                attributes.push(attr(format!("native_token_{}", index), denom));
                attributes.push(attr(format!("tax_amount_{}", index), tax_amount));
            }
        }
    }

    if result_assets.len() != 2 {
        return Err(StdError::generic_err("wrong number of assets"));
    }

    let asset_1 = result_assets
        .pop()
        .ok_or(StdError::generic_err("wrong number of assets"))?;
    let asset_0 = result_assets
        .pop()
        .ok_or(StdError::generic_err("wrong number of assets"))?;

    Ok((
        WasmMsg::Execute {
            contract_addr: pair_contract_addr.to_string(),
            msg: to_binary(&PairExecuteMsg::ProvideLiquidity {
                assets: [asset_0, asset_1],
                slippage_tolerance,
                receiver: None,
            })?,
            funds: result_coins,
        },
        attributes,
    ))
}

pub fn auto_stake_hook(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    staker_addr: Addr,
    prev_staking_token_amount: Uint128,
) -> StdResult<Response> {
    // only can be called by itself
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }

    let config: Config = read_config(deps.storage)?;

    // stake all lp tokens received, compare with staking token amount before liquidity provision was executed
    let current_staking_token_amount = query_token_balance(
        &deps.querier,
        deps.api.addr_humanize(&config.staking_token)?,
        env.contract.address.clone(),
    )?;
    let amount_to_stake = current_staking_token_amount.checked_sub(prev_staking_token_amount)?;

    bond(deps, env, staker_addr, amount_to_stake)
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
        terraswap_factory: deps
            .api
            .addr_humanize(&state.terraswap_factory)?
            .to_string(),
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

#[entry_point]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
