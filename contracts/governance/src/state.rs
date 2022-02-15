use cw_storage_plus::{Bound, Item, Map, U64Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Binary, Decimal, Order, StdResult, Storage, Uint128};
use cw0::{calc_range_end, calc_range_start};
use services::common::OrderBy;
use services::governance::{PollStatus, VoterInfo};
use std::cmp::Ordering;

static KEY_CONFIG: Item<Config> = Item::new("config");
static KEY_STATE: Item<State> = Item::new("state");
static TMP_POLL_ID: Item<u64> = Item::new("tmp_poll_id");
static BANK: Map<&Addr, TokenManager> = Map::new("bank");

static POLL: Map<U64Key, Poll> = Map::new("poll");
//key: poll_status.to_string + poll_id
static POLL_INDEXER: Map<(String, U64Key), bool> = Map::new("poll_indexer");

//key: poll_id + poll_voter_addr
static POLL_VOTER: Map<(U64Key, &Addr), VoterInfo> = Map::new("poll_voter");

static UTILITY: Item<Utility> = Item::new("utility");
static LOCKED_TOKENS_FOR_UTILITY: Map<&Addr, Uint128> = Map::new("locked_tokens_for_utility");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub psi_token: Addr,
    pub quorum: Decimal,
    pub threshold: Decimal,
    pub voting_period: u64,
    pub timelock_period: u64,
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
pub struct Utility {
    pub token: Addr,
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
    pub end_time: u64,
    pub title: String,
    pub description: String,
    pub link: Option<String>,
    pub execute_data: Option<Vec<ExecuteData>>,
    pub migrate_data: Option<Vec<MigrateData>>,
    pub deposit_amount: Uint128,
    /// Total balance at the end poll
    pub total_balance_at_end_poll: Option<Uint128>,
    pub staked_amount: Option<Uint128>,
}

impl Poll {
    pub fn contain_messages(&self) -> bool {
        let execute_messages_is_empty = if let Some(data) = &self.execute_data {
            data.is_empty()
        } else {
            true
        };
        let migration_messages_is_empty = if let Some(data) = &self.migrate_data {
            data.is_empty()
        } else {
            true
        };

        return !execute_messages_is_empty || !migration_messages_is_empty;
    }
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

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct MigrateData {
    pub order: u64,
    pub contract: Addr,
    pub new_code_id: u64,
    pub msg: Binary,
}

impl Eq for MigrateData {}

impl Ord for MigrateData {
    fn cmp(&self, other: &Self) -> Ordering {
        self.order.cmp(&other.order)
    }
}

impl PartialOrd for MigrateData {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for MigrateData {
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

pub fn load_utility(storage: &dyn Storage) -> StdResult<Utility> {
    UTILITY.load(storage)
}

pub fn store_utility(storage: &mut dyn Storage, utility: &Utility) -> StdResult<()> {
    UTILITY.save(storage, utility)
}

pub fn remove_utility(storage: &mut dyn Storage) {
    UTILITY.remove(storage);
}

pub fn store_tmp_poll_id(storage: &mut dyn Storage, tmp_poll_id: u64) -> StdResult<()> {
    TMP_POLL_ID.save(storage, &tmp_poll_id)
}

pub fn load_tmp_poll_id(storage: &dyn Storage) -> StdResult<u64> {
    TMP_POLL_ID.load(storage)
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

pub fn load_locked_tokens_for_utility(storage: &dyn Storage, addr: &Addr) -> StdResult<Uint128> {
    LOCKED_TOKENS_FOR_UTILITY
        .may_load(storage, addr)
        .map(|res| res.unwrap_or_default())
}

pub fn store_locked_tokens_for_utility(
    storage: &mut dyn Storage,
    addr: &Addr,
    amount: Uint128,
) -> StdResult<()> {
    LOCKED_TOKENS_FOR_UTILITY.save(storage, addr, &amount)
}

pub fn clear_locked_tokens_for_utility(storage: &mut dyn Storage) {
    let keys: Vec<_> = storage
        .range(None, None, Order::Ascending)
        .map(|(key, _)| key)
        .collect();
    for key in keys {
        storage.remove(&key);
    }
}

#[cfg(test)]
mod test {
    use crate::state::VoterInfo;
    use cosmwasm_std::{testing::mock_dependencies, to_binary};
    use cw20::Cw20ExecuteMsg;
    use services::governance::VoteOption;

    use super::*;
    const LIMIT: usize = 30;
    const ELEMENTS_IN_GROUP: usize = 100;
    const POLLS_ID_COUNT: usize = 5;
    const TOTAL_ELEMENTS_COUNT: usize = ELEMENTS_IN_GROUP * POLLS_ID_COUNT;

    impl Default for Poll {
        fn default() -> Self {
            Poll {
                id: 0u64,
                creator: Addr::unchecked(""),
                status: PollStatus::Failed,
                yes_votes: Uint128::zero(),
                no_votes: Uint128::zero(),
                end_time: 0u64,
                title: String::default(),
                description: String::default(),
                link: None,
                execute_data: None,
                migrate_data: None,
                deposit_amount: Uint128::zero(),
                total_balance_at_end_poll: None,
                staked_amount: None,
            }
        }
    }

    fn addr_from_i(i: usize) -> Addr {
        Addr::unchecked(format!("addr{:0>8}", i))
    }

    fn voter_info_from_i(i: usize) -> VoterInfo {
        VoterInfo {
            balance: Uint128::from(i as u128),
            vote: VoteOption::Yes,
        }
    }

    fn poll_from_i(i: usize) -> Poll {
        Poll {
            id: i as u64,
            ..Poll::default()
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
                let first_element_index_in_group = j * LIMIT + poll_id * ELEMENTS_IN_GROUP;
                let start_after = if j == 0 {
                    None
                } else {
                    Some(addr_from_i(first_element_index_in_group - 1))
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
                    let global_index = first_element_index_in_group + i;
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

    #[test]
    fn load_polls_with_range_start_works_as_expected() {
        let total_elements_count = 100;
        let mut deps = mock_dependencies(&[]);
        for i in 0..total_elements_count {
            let poll_id = i as u64;
            let poll = poll_from_i(i);
            store_poll(&mut deps.storage, poll_id, &poll).unwrap();
        }

        for j in 0..4 {
            let first_element_index = j * LIMIT;
            let start_after: Option<u64> = if j == 0 {
                None
            } else {
                Some((first_element_index - 1) as u64)
            };

            let polls = read_polls(
                &deps.storage,
                None,
                start_after,
                Some(LIMIT as u32),
                Some(OrderBy::Asc),
            )
            .unwrap();

            for (i, poll) in polls.into_iter().enumerate() {
                let global_index = (first_element_index + i) as u64;
                assert_eq!(poll.id, global_index);
            }
        }
    }

    #[test]
    fn load_polls_with_range_end_works_as_expected() {
        let total_elements_count = 100;
        let mut deps = mock_dependencies(&[]);
        for i in 0..total_elements_count {
            let poll_id = i as u64;
            let poll = poll_from_i(i);
            store_poll(&mut deps.storage, poll_id, &poll).unwrap();
        }

        for j in 0..4 {
            let last_element_index = total_elements_count - j * LIMIT;
            let end_before = Some(last_element_index as u64);

            let polls = read_polls(
                &deps.storage,
                None,
                end_before,
                Some(LIMIT as u32),
                Some(OrderBy::Desc),
            )
            .unwrap();

            for (i, poll) in polls.into_iter().enumerate() {
                let global_index = (last_element_index - i - 1) as u64;
                assert_eq!(poll.id, global_index);
            }
        }
    }

    fn create_and_save_poll(storage: &mut dyn Storage, poll_id: usize, poll_status: PollStatus) {
        let mut poll = poll_from_i(poll_id);
        poll.status = poll_status;
        store_poll(storage, poll_id as u64, &poll).unwrap();
        store_poll_indexer(storage, &poll.status, poll_id as u64).unwrap();
    }

    #[test]
    fn load_polls_with_filter_by_status_range_start_works_as_expected() {
        let mut deps = mock_dependencies(&[]);
        let executed_ids: Vec<usize> = (0..30).filter(|elem| elem % 3 == 0).collect();
        let passed_ids: Vec<usize> = executed_ids.iter().map(|x| x + 1).collect();
        let rejected_ids: Vec<usize> = executed_ids.iter().map(|x| x + 2).collect();

        for i in passed_ids.iter() {
            create_and_save_poll(&mut deps.storage, *i, PollStatus::Passed);
        }

        for i in executed_ids.iter() {
            create_and_save_poll(&mut deps.storage, *i, PollStatus::Executed);
        }

        for i in rejected_ids.iter() {
            create_and_save_poll(&mut deps.storage, *i, PollStatus::Rejected);
        }

        let local_limit = 5;

        // get Executed polls
        {
            let local_status = PollStatus::Executed;
            let start_after_on_step_2 = 12;
            for j in 0..2 {
                let start_after: Option<u64> = if j == 0 {
                    None
                } else {
                    Some(start_after_on_step_2)
                };

                let polls = read_polls(
                    &deps.storage,
                    Some(local_status.clone()),
                    start_after,
                    Some(local_limit as u32),
                    Some(OrderBy::Asc),
                )
                .unwrap();

                for (i, poll) in polls.into_iter().enumerate() {
                    let expected_id = executed_ids[i + (j * local_limit)];
                    assert_eq!(poll.status, local_status);
                    assert_eq!(poll.id, expected_id as u64);
                }
            }
        }

        // get Passed polls
        {
            let local_status = PollStatus::Passed;
            let start_after_on_step_2 = 13;
            for j in 0..2 {
                let start_after: Option<u64> = if j == 0 {
                    None
                } else {
                    Some(start_after_on_step_2)
                };

                let polls = read_polls(
                    &deps.storage,
                    Some(local_status.clone()),
                    start_after,
                    Some(local_limit as u32),
                    Some(OrderBy::Asc),
                )
                .unwrap();

                for (i, poll) in polls.into_iter().enumerate() {
                    let expected_id = passed_ids[i + (j * local_limit)];
                    assert_eq!(poll.status, local_status);
                    assert_eq!(poll.id, expected_id as u64);
                }
            }
        }

        // get Rejected polls
        {
            let local_status = PollStatus::Rejected;
            let start_after_on_step_2 = 14;
            for j in 0..2 {
                let start_after: Option<u64> = if j == 0 {
                    None
                } else {
                    Some(start_after_on_step_2)
                };

                let polls = read_polls(
                    &deps.storage,
                    Some(local_status.clone()),
                    start_after,
                    Some(local_limit as u32),
                    Some(OrderBy::Asc),
                )
                .unwrap();

                for (i, poll) in polls.into_iter().enumerate() {
                    let expected_id = rejected_ids[i + (j * local_limit)];
                    assert_eq!(poll.status, local_status);
                    assert_eq!(poll.id, expected_id as u64);
                }
            }
        }
    }

    #[test]
    fn load_polls_with_filter_by_status_range_end_works_as_expected() {
        let mut deps = mock_dependencies(&[]);
        let elems_count_in_group = 10;
        let executed_ids: Vec<usize> = (0..(elems_count_in_group * 3))
            .filter(|elem| elem % 3 == 0)
            .collect();
        let passed_ids: Vec<usize> = executed_ids.iter().map(|x| x + 1).collect();
        let rejected_ids: Vec<usize> = executed_ids.iter().map(|x| x + 2).collect();

        for i in passed_ids.iter() {
            create_and_save_poll(&mut deps.storage, *i, PollStatus::Passed);
        }

        for i in executed_ids.iter() {
            create_and_save_poll(&mut deps.storage, *i, PollStatus::Executed);
        }

        for i in rejected_ids.iter() {
            create_and_save_poll(&mut deps.storage, *i, PollStatus::Rejected);
        }

        let local_limit = 5;

        // get Executed polls
        {
            let local_status = PollStatus::Executed;
            let end_before_on_step_2 = 15;
            for j in 0..2 {
                let end_before: Option<u64> = if j == 0 {
                    None
                } else {
                    Some(end_before_on_step_2)
                };

                let polls = read_polls(
                    &deps.storage,
                    Some(local_status.clone()),
                    end_before,
                    Some(local_limit as u32),
                    Some(OrderBy::Desc),
                )
                .unwrap();

                for (i, poll) in polls.into_iter().enumerate() {
                    let expected_id =
                        executed_ids[elems_count_in_group - i - (j * local_limit) - 1];
                    assert_eq!(poll.status, local_status);
                    assert_eq!(poll.id, expected_id as u64);
                }
            }
        }

        // get Passed polls
        {
            let local_status = PollStatus::Passed;
            let end_before_on_step_2 = 16;
            for j in 0..2 {
                let end_before: Option<u64> = if j == 0 {
                    None
                } else {
                    Some(end_before_on_step_2)
                };

                let polls = read_polls(
                    &deps.storage,
                    Some(local_status.clone()),
                    end_before,
                    Some(local_limit as u32),
                    Some(OrderBy::Desc),
                )
                .unwrap();

                for (i, poll) in polls.into_iter().enumerate() {
                    let expected_id = passed_ids[elems_count_in_group - i - (j * local_limit) - 1];
                    assert_eq!(poll.status, local_status);
                    assert_eq!(poll.id, expected_id as u64);
                }
            }
        }

        // get Rejected polls
        {
            let local_status = PollStatus::Rejected;
            let end_before_on_step_2 = 17;
            for j in 0..2 {
                let end_before: Option<u64> = if j == 0 {
                    None
                } else {
                    Some(end_before_on_step_2)
                };

                let polls = read_polls(
                    &deps.storage,
                    Some(local_status.clone()),
                    end_before,
                    Some(local_limit as u32),
                    Some(OrderBy::Desc),
                )
                .unwrap();

                for (i, poll) in polls.into_iter().enumerate() {
                    let expected_id =
                        rejected_ids[elems_count_in_group - i - (j * local_limit) - 1];
                    assert_eq!(poll.status, local_status);
                    assert_eq!(poll.id, expected_id as u64);
                }
            }
        }
    }

    #[test]
    fn poll_contain_messages_test() {
        let mut poll = Poll::default();
        poll.execute_data = None;
        poll.migrate_data = None;
        assert_eq!(poll.contain_messages(), false);

        poll.migrate_data = None;
        poll.execute_data = Some(vec![]);
        assert_eq!(poll.contain_messages(), false);

        poll.execute_data = None;
        poll.migrate_data = Some(vec![]);
        assert_eq!(poll.contain_messages(), false);

        poll.execute_data = Some(vec![]);
        poll.migrate_data = Some(vec![]);
        assert_eq!(poll.contain_messages(), false);

        poll.execute_data = Some(vec![]);
        poll.migrate_data = Some(vec![]);
        assert_eq!(poll.contain_messages(), false);

        let exec_msg = ExecuteData {
            order: 1u64,
            contract: Addr::unchecked("som_contract"),
            msg: to_binary(&Cw20ExecuteMsg::Burn {
                amount: Uint128::new(30),
            })
            .unwrap(),
        };

        let migrate_msg = MigrateData {
            order: 1u64,
            contract: Addr::unchecked("som_contract"),
            msg: to_binary(&Cw20ExecuteMsg::Burn {
                amount: Uint128::new(30),
            })
            .unwrap(),
            new_code_id: 11,
        };

        poll.execute_data = Some(vec![exec_msg.clone()]);
        poll.migrate_data = None;
        assert_eq!(poll.contain_messages(), true);

        poll.execute_data = None;
        poll.migrate_data = Some(vec![migrate_msg.clone()]);
        assert_eq!(poll.contain_messages(), true);

        poll.execute_data = Some(vec![exec_msg]);
        poll.migrate_data = Some(vec![migrate_msg]);
        assert_eq!(poll.contain_messages(), true);
    }
}
