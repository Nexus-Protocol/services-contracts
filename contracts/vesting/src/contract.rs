#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    attr, to_binary, Addr, Api, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, Storage, SubMsg, Uint128, WasmMsg,
};

use crate::state::{
    read_config, read_vesting_info, read_vesting_infos, store_config, store_vesting_info, Config,
};
use cw20::Cw20ExecuteMsg;
use services::common::OrderBy;
use services::vesting::{
    ClaimableAmountResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, VestingAccount,
    VestingAccountResponse, VestingAccountsResponse, VestingInfo, VestingSchedule,
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    store_config(
        deps.storage,
        &Config {
            owner: deps.api.addr_canonicalize(&msg.owner)?,
            psi_token: deps.api.addr_canonicalize(&msg.psi_token)?,
            genesis_time: msg.genesis_time,
        },
    )?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg.clone() {
        ExecuteMsg::Claim {} => claim(deps, env, info),
        _ => {
            assert_owner_privilege(deps.storage, deps.api, info.sender)?;
            match msg {
                ExecuteMsg::UpdateConfig {
                    owner,
                    psi_token,
                    genesis_time,
                } => update_config(deps, owner, psi_token, genesis_time),
                ExecuteMsg::RegisterVestingAccounts { vesting_accounts } => {
                    register_vesting_accounts(deps, vesting_accounts)
                }
                _ => panic!("DO NOT ENTER HERE"),
            }
        }
    }
}

fn assert_owner_privilege(storage: &dyn Storage, api: &dyn Api, sender: Addr) -> StdResult<()> {
    if read_config(storage)?.owner != api.addr_canonicalize(sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    Ok(())
}

pub fn update_config(
    deps: DepsMut,
    owner: Option<String>,
    psi_token: Option<String>,
    genesis_time: Option<u64>,
) -> StdResult<Response> {
    let mut config = read_config(deps.storage)?;
    if let Some(owner) = owner {
        config.owner = deps.api.addr_canonicalize(&owner)?;
    }

    if let Some(psi_token) = psi_token {
        config.psi_token = deps.api.addr_canonicalize(&psi_token)?;
    }

    if let Some(genesis_time) = genesis_time {
        config.genesis_time = genesis_time;
    }

    store_config(deps.storage, &config)?;

    Ok(Response {
        messages: vec![],
        attributes: vec![attr("action", "update_config")],
        events: vec![],
        data: None,
    })
}

fn assert_vesting_schedules(vesting_schedules: &[VestingSchedule]) -> StdResult<()> {
    for vesting_schedule in vesting_schedules.iter() {
        if vesting_schedule.start_time >= vesting_schedule.end_time {
            return Err(StdError::generic_err(
                "end_time must bigger than start_time",
            ));
        }

        if vesting_schedule.start_time > vesting_schedule.cliff_end_time {
            return Err(StdError::generic_err(
                "cliff_end_time must bigger or equal than start_time",
            ));
        }
    }

    Ok(())
}

pub fn register_vesting_accounts(
    deps: DepsMut,
    vesting_accounts: Vec<VestingAccount>,
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    for vesting_account in vesting_accounts.iter() {
        assert_vesting_schedules(&vesting_account.schedules)?;

        let vesting_address = deps.api.addr_canonicalize(&vesting_account.address)?;
        store_vesting_info(
            deps.storage,
            &vesting_address,
            &VestingInfo {
                last_claim_time: config.genesis_time,
                schedules: vesting_account.schedules.clone(),
            },
        )?;
    }

    Ok(Response {
        messages: vec![],
        attributes: vec![attr("action", "register_vesting_accounts")],
        events: vec![],
        data: None,
    })
}

pub fn claim(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let current_time = env.block.time.nanos() / 1_000_000_000;
    let address = info.sender;
    let address_raw = deps.api.addr_canonicalize(&address.to_string())?;

    let config: Config = read_config(deps.storage)?;
    let mut vesting_info: VestingInfo = read_vesting_info(deps.storage, &address_raw)?;

    let claim_amount = compute_claim_amount(current_time, &vesting_info);
    let messages: Vec<SubMsg> = if claim_amount.is_zero() {
        vec![]
    } else {
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.psi_token)?.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: address.to_string(),
                amount: claim_amount,
            })?,
        }))]
    };

    vesting_info.last_claim_time = current_time;
    store_vesting_info(deps.storage, &address_raw, &vesting_info)?;

    Ok(Response {
        messages,
        attributes: vec![
            attr("action", "claim"),
            attr("address", address),
            attr("claim_amount", claim_amount),
            attr("last_claim_time", current_time),
        ],
        events: vec![],
        data: None,
    })
}

fn compute_claim_amount(current_time: u64, vesting_info: &VestingInfo) -> Uint128 {
    let mut claimable_amount: Uint128 = Uint128::zero();
    for s in vesting_info.schedules.iter() {
        if s.start_time > current_time
            || s.end_time < vesting_info.last_claim_time
            || s.cliff_end_time > current_time
        {
            continue;
        }

        // min(s.end_time, current_time) - max(s.start_time, last_claim_time)
        let mut passed_time = std::cmp::min(s.end_time, current_time)
            - std::cmp::max(s.start_time, vesting_info.last_claim_time);
        if vesting_info.last_claim_time < s.cliff_end_time
            && vesting_info.last_claim_time > s.start_time
        {
            passed_time += vesting_info.last_claim_time - s.start_time;
        }

        // prevent zero time_period case
        let time_period = s.end_time - s.start_time;
        let release_amount_per_time: Decimal = Decimal::from_ratio(s.amount, time_period);

        claimable_amount += Uint128::from(passed_time as u128) * release_amount_per_time;
    }

    claimable_amount
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => Ok(to_binary(&query_config(deps)?)?),
        QueryMsg::VestingAccount { address } => {
            Ok(to_binary(&query_vesting_account(deps, address)?)?)
        }
        QueryMsg::VestingAccounts {
            start_after,
            limit,
            order_by,
        } => Ok(to_binary(&query_vesting_accounts(
            deps,
            start_after,
            limit,
            order_by,
        )?)?),
        QueryMsg::Claimable { address } => {
            Ok(to_binary(&query_claimable_amount(deps, env, address)?)?)
        }
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let state = read_config(deps.storage)?;
    let resp = ConfigResponse {
        owner: deps.api.addr_humanize(&state.owner)?.to_string(),
        psi_token: deps.api.addr_humanize(&state.psi_token)?.to_string(),
        genesis_time: state.genesis_time,
    };

    Ok(resp)
}

pub fn query_vesting_account(deps: Deps, address: String) -> StdResult<VestingAccountResponse> {
    let info = read_vesting_info(deps.storage, &deps.api.addr_canonicalize(&address)?)?;
    let resp = VestingAccountResponse { address, info };

    Ok(resp)
}

pub fn query_vesting_accounts(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<VestingAccountsResponse> {
    let vesting_infos = if let Some(start_after) = start_after {
        read_vesting_infos(
            deps.storage,
            Some(deps.api.addr_canonicalize(&start_after)?),
            limit,
            order_by,
        )?
    } else {
        read_vesting_infos(deps.storage, None, limit, order_by)?
    };

    let vesting_account_responses: StdResult<Vec<VestingAccountResponse>> = vesting_infos
        .iter()
        .map(|vesting_account| {
            Ok(VestingAccountResponse {
                address: deps.api.addr_humanize(&vesting_account.0)?.to_string(),
                info: vesting_account.1.clone(),
            })
        })
        .collect();

    Ok(VestingAccountsResponse {
        vesting_accounts: vesting_account_responses?,
    })
}

pub fn query_claimable_amount(
    deps: Deps,
    env: Env,
    address: String,
) -> StdResult<ClaimableAmountResponse> {
    let info = read_vesting_info(deps.storage, &deps.api.addr_canonicalize(&address)?)?;
    let current_time = env.block.time.nanos() / 1_000_000_000;
    let claimable_amount = compute_claim_amount(current_time, &info);
    let resp = ClaimableAmountResponse {
        address,
        claimable_amount,
    };

    Ok(resp)
}

#[test]
fn test_assert_vesting_schedules() {
    // valid
    assert_vesting_schedules(&[
        VestingSchedule::new(100u64, 101u64, 100u64, Uint128::from(100u128)),
        VestingSchedule::new(100u64, 110u64, 101u64, Uint128::from(100u128)),
        VestingSchedule::new(100u64, 200u64, 300u64, Uint128::from(100u128)),
    ])
    .unwrap();

    // invalid: start_time equals to end_time
    let res = assert_vesting_schedules(&[
        VestingSchedule::new(100u64, 100u64, 100u64, Uint128::from(100u128)),
        VestingSchedule::new(100u64, 110u64, 100u64, Uint128::from(100u128)),
        VestingSchedule::new(100u64, 200u64, 100u64, Uint128::from(100u128)),
    ]);
    match res {
        Err(StdError::GenericErr { msg, .. }) => {
            assert_eq!(msg, "end_time must bigger than start_time")
        }
        _ => panic!("DO NOT ENTER HERE"),
    }

    // invalid: cliff_end_time lesser than start_time
    let res = assert_vesting_schedules(&[
        VestingSchedule::new(100u64, 110u64, 100u64, Uint128::from(100u128)),
        VestingSchedule::new(100u64, 150u64, 90u64, Uint128::from(100u128)),
        VestingSchedule::new(100u64, 200u64, 100u64, Uint128::from(100u128)),
    ]);
    match res {
        Err(StdError::GenericErr { msg, .. }) => {
            assert_eq!(msg, "cliff_end_time must bigger or equal than start_time")
        }
        _ => panic!("DO NOT ENTER HERE"),
    }
}
