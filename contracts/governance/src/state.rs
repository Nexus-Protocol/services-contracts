use cw_storage_plus::{Bound, Item, Map, U64Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Binary, Decimal, StdResult, Storage, Uint128};
use cw0::{calc_range_end, calc_range_start};
use services::common::OrderBy;
use services::governance::{PollStatus, VoterInfo};
use std::cmp::Ordering;

static KEY_CONFIG: Item<Config> = Item::new("config");
static KEY_STATE: Item<State> = Item::new("state");
static BANK: Map<&Addr, TokenManager> = Map::new("bank");

static POLL: Map<U64Key, Poll> = Map::new("poll");
//key: poll_status.to_string + poll_id
static POLL_INDEXER: Map<(String, U64Key), bool> = Map::new("poll_indexer");

//key: poll_id + poll_voter_addr
static POLL_VOTER: Map<(U64Key, &Addr), VoterInfo> = Map::new("poll_voter");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub psi_token: Addr,
    pub quorum: Decimal,
    pub threshold: Decimal,
    pub voting_period: u64,
    pub timelock_period: u64,
    pub expiration_period: u64,
    pub proposal_deposit: Uint128,
    pub snapshot_period: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub poll_count: u64,
    pub total_share: Uint128,
    pub total_deposit: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TokenManager {
    pub share: Uint128,                        // total staked balance
    pub locked_balance: Vec<(u64, VoterInfo)>, // maps poll_id to weight voted
}

impl Default for TokenManager {
    fn default() -> Self {
        let locked_balance: Vec<(u64, VoterInfo)> = vec![];
        Self {
            share: Uint128::default(),
            locked_balance,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Poll {
    pub id: u64,
    pub creator: Addr,
    pub status: PollStatus,
    pub yes_votes: Uint128,
    pub no_votes: Uint128,
    pub end_height: u64,
    pub title: String,
    pub description: String,
    pub link: Option<String>,
    pub execute_data: Option<Vec<ExecuteData>>,
    pub deposit_amount: Uint128,
    /// Total balance at the end poll
    pub total_balance_at_end_poll: Option<Uint128>,
    pub staked_amount: Option<Uint128>,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct ExecuteData {
    pub order: u64,
    pub contract: Addr,
    pub msg: Binary,
}
impl Eq for ExecuteData {}

impl Ord for ExecuteData {
    fn cmp(&self, other: &Self) -> Ordering {
        self.order.cmp(&other.order)
    }
}

impl PartialOrd for ExecuteData {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ExecuteData {
    fn eq(&self, other: &Self) -> bool {
        self.order == other.order
    }
}

pub fn load_config(storage: &dyn Storage) -> StdResult<Config> {
    KEY_CONFIG.load(storage)
}

pub fn store_config(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    KEY_CONFIG.save(storage, config)
}

pub fn load_state(storage: &dyn Storage) -> StdResult<State> {
    KEY_STATE.load(storage)
}

pub fn store_state(storage: &mut dyn Storage, state: &State) -> StdResult<()> {
    KEY_STATE.save(storage, state)
}

pub fn load_poll(storage: &dyn Storage, poll_id: u64) -> StdResult<Poll> {
    load_poll_internal(storage, poll_id.into())
}

pub fn load_poll_internal(storage: &dyn Storage, poll_id: U64Key) -> StdResult<Poll> {
    POLL.load(storage, poll_id)
}

pub fn may_load_poll(storage: &dyn Storage, poll_id: u64) -> StdResult<Option<Poll>> {
    POLL.may_load(storage, poll_id.into())
}

pub fn store_poll(storage: &mut dyn Storage, poll_id: u64, poll: &Poll) -> StdResult<()> {
    POLL.save(storage, poll_id.into(), poll)
}

pub fn may_load_bank(storage: &dyn Storage, addr: &Addr) -> StdResult<Option<TokenManager>> {
    BANK.may_load(storage, addr)
}

pub fn load_bank(storage: &dyn Storage, addr: &Addr) -> StdResult<TokenManager> {
    may_load_bank(storage, addr).map(|res| res.unwrap_or_default())
}

pub fn store_bank(
    storage: &mut dyn Storage,
    addr: &Addr,
    token_manager: &TokenManager,
) -> StdResult<()> {
    BANK.save(storage, addr, token_manager)
}

pub fn store_poll_indexer(
    storage: &mut dyn Storage,
    status: &PollStatus,
    poll_id: u64,
) -> StdResult<()> {
    POLL_INDEXER.save(storage, (status.to_string(), poll_id.into()), &true)
}

pub fn remove_poll_indexer(storage: &mut dyn Storage, status: &PollStatus, poll_id: u64) {
    POLL_INDEXER.remove(storage, (status.to_string(), poll_id.into()))
}

pub fn store_poll_voter(
    storage: &mut dyn Storage,
    poll_id: u64,
    voter: &Addr,
    voter_info: &VoterInfo,
) -> StdResult<()> {
    POLL_VOTER.save(storage, (poll_id.into(), voter), voter_info)
}

pub fn remove_poll_voter(storage: &mut dyn Storage, poll_id: u64, voter: &Addr) {
    POLL_VOTER.remove(storage, (poll_id.into(), voter))
}

pub fn load_poll_voter(storage: &dyn Storage, poll_id: u64, voter: &Addr) -> StdResult<VoterInfo> {
    POLL_VOTER.load(storage, (poll_id.into(), voter))
}

pub fn read_poll_voters(
    storage: &dyn Storage,
    poll_id: u64,
    start_after: Option<Addr>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<Vec<(Addr, VoterInfo)>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let (start, end, order_by) = match order_by {
        Some(OrderBy::Asc) => (
            calc_range_start(start_after).map(Bound::exclusive),
            None,
            OrderBy::Asc,
        ),
        _ => (
            None,
            calc_range_end(start_after).map(Bound::exclusive),
            OrderBy::Desc,
        ),
    };

    POLL_VOTER
        .prefix(poll_id.into())
        .range(storage, start, end, order_by.into())
        .take(limit)
        .map(|item| {
            let (k, v) = item?;
            let address_str = std::str::from_utf8(&k)?;
            Ok((Addr::unchecked(address_str), v))
        })
        .collect()
}

const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;
pub fn read_polls(
    storage: &dyn Storage,
    filter: Option<PollStatus>,
    start_after: Option<u64>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<Vec<Poll>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let (start, end, order_by) = match order_by {
        Some(OrderBy::Asc) => (
            calc_range_start_u64(start_after).map(Bound::exclusive),
            None,
            OrderBy::Asc,
        ),
        _ => (
            None,
            calc_range_end_u64(start_after).map(Bound::exclusive),
            OrderBy::Desc,
        ),
    };

    if let Some(status) = filter {
        POLL_INDEXER
            .prefix(status.to_string())
            .range(storage, start, end, order_by.into())
            .take(limit)
            .map(|item| {
                let (k, _) = item?;
                load_poll_internal(storage, k.into())
            })
            .collect()
    } else {
        POLL.range(storage, start, end, order_by.into())
            .take(limit)
            .map(|item| {
                let (_, v) = item?;
                Ok(v)
            })
            .collect()
    }
}

// this will set the first key after the provided key, by appending a 0 byte
fn calc_range_start_u64(start_after: Option<u64>) -> Option<Vec<u8>> {
    start_after.map(|id| {
        let mut v = id.to_be_bytes().to_vec();
        v.push(0);
        v
    })
}

fn calc_range_end_u64(start_after: Option<u64>) -> Option<Vec<u8>> {
    start_after.map(|id| id.to_be_bytes().to_vec())
}

#[cfg(test)]
mod test {
    use crate::state::VoterInfo;
    use cosmwasm_std::testing::mock_dependencies;
    use services::governance::VoteOption;

    use super::*;
    const LIMIT: usize = 30;
    const ELEMENTS_IN_GROUP: usize = 100;
    const POLLS_ID_COUNT: usize = 5;
    const TOTAL_ELEMENTS_COUNT: usize = ELEMENTS_IN_GROUP * POLLS_ID_COUNT;

    fn addr_from_i(i: usize) -> Addr {
        Addr::unchecked(format!("addr{:0>8}", i))
    }

    fn voter_info_from_i(i: usize) -> VoterInfo {
        VoterInfo {
            balance: Uint128::from(i as u128),
            vote: VoteOption::Yes,
        }
    }

    fn poll_id_from_i(i: usize) -> u64 {
        (i / ELEMENTS_IN_GROUP) as u64
    }

    #[test]
    fn load_voter_info_with_range_start_works_as_expected() {
        let mut deps = mock_dependencies(&[]);
        for i in 0..TOTAL_ELEMENTS_COUNT {
            let voter_addr = addr_from_i(i);
            let voter_info = voter_info_from_i(i);
            let poll_id = poll_id_from_i(i);
            store_poll_voter(&mut deps.storage, poll_id, &voter_addr, &voter_info).unwrap();
        }

        let max_j = (ELEMENTS_IN_GROUP / LIMIT) + 1;
        for poll_id in 0..POLLS_ID_COUNT {
            for j in 0..max_j {
                let start_after = if j == 0 {
                    None
                } else {
                    Some(addr_from_i(j * LIMIT + poll_id * ELEMENTS_IN_GROUP - 1))
                };

                let voters_info = read_poll_voters(
                    &deps.storage,
                    poll_id as u64,
                    start_after,
                    Some(LIMIT as u32),
                    Some(OrderBy::Asc),
                )
                .unwrap();

                for (i, (addr, voters_info)) in voters_info.into_iter().enumerate() {
                    let global_index = j * LIMIT + i + poll_id * ELEMENTS_IN_GROUP;
                    assert_eq!(addr, addr_from_i(global_index));
                    assert_eq!(voters_info.balance, Uint128::from(global_index as u128));
                }
            }
        }
    }

    #[test]
    fn load_voter_info_with_range_end_works_as_expected() {
        let mut deps = mock_dependencies(&[]);
        for i in 0..TOTAL_ELEMENTS_COUNT {
            let voter_addr = addr_from_i(i);
            let voter_info = voter_info_from_i(i);
            let poll_id = poll_id_from_i(i);
            store_poll_voter(&mut deps.storage, poll_id, &voter_addr, &voter_info).unwrap();
        }

        let max_j = (ELEMENTS_IN_GROUP / LIMIT) + 1;
        for poll_id in 0..POLLS_ID_COUNT {
            for j in 0..max_j {
                let last_element_index_in_group = TOTAL_ELEMENTS_COUNT
                    - j * LIMIT
                    - ELEMENTS_IN_GROUP * (POLLS_ID_COUNT - poll_id - 1);
                let end_before = Some(addr_from_i(last_element_index_in_group));

                let voters_info = read_poll_voters(
                    &deps.storage,
                    poll_id as u64,
                    end_before.clone(),
                    Some(LIMIT as u32),
                    Some(OrderBy::Desc),
                )
                .unwrap();

                for (i, (addr, voters_info)) in voters_info.into_iter().enumerate() {
                    let global_index = last_element_index_in_group - i - 1;
                    assert_eq!(addr, addr_from_i(global_index));
                    assert_eq!(voters_info.balance, Uint128::from(global_index as u128));
                }
            }
        }
    }
}
