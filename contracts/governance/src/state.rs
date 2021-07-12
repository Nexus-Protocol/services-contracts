use cosmwasm_storage::{bucket, bucket_read, singleton, singleton_read, Bucket, ReadonlyBucket};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Binary, Decimal, StdResult, Storage, Uint128};
use services::common::OrderBy;
use services::governance::{PollStatus, VoterInfo};
use std::cmp::Ordering;

static KEY_CONFIG: &[u8] = b"config";
static KEY_STATE: &[u8] = b"state";

static PREFIX_POLL: &[u8] = b"poll";
static PREFIX_BANK: &[u8] = b"bank";

static PREFIX_POLL_INDEXER: &[u8] = b"poll_indexer";
static PREFIX_POLL_VOTER: &[u8] = b"poll_voter";

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
    singleton_read(storage, KEY_CONFIG).load()
}

pub fn store_config(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    singleton(storage, KEY_CONFIG).save(config)
}

pub fn load_state(storage: &dyn Storage) -> StdResult<State> {
    singleton_read(storage, KEY_STATE).load()
}

pub fn store_state(storage: &mut dyn Storage, state: &State) -> StdResult<()> {
    singleton(storage, KEY_STATE).save(state)
}

pub fn load_poll(storage: &dyn Storage, poll_id: &u64) -> StdResult<Poll> {
    bucket_read(storage, PREFIX_POLL).load(&poll_id.to_be_bytes())
}

pub fn load_poll_raw_key(storage: &dyn Storage, key: &[u8]) -> StdResult<Poll> {
    bucket_read(storage, PREFIX_POLL).load(key)
}

pub fn may_load_poll(storage: &dyn Storage, poll_id: &u64) -> StdResult<Option<Poll>> {
    bucket_read(storage, PREFIX_POLL).may_load(&poll_id.to_be_bytes())
}

pub fn store_poll(storage: &mut dyn Storage, poll_id: &u64, poll: &Poll) -> StdResult<()> {
    bucket(storage, PREFIX_POLL).save(&poll_id.to_be_bytes(), poll)
}

pub fn may_load_bank(storage: &dyn Storage, addr: &Addr) -> StdResult<Option<TokenManager>> {
    bucket_read(storage, PREFIX_BANK).may_load(addr.as_bytes())
}

pub fn load_bank(storage: &dyn Storage, addr: &Addr) -> StdResult<TokenManager> {
    may_load_bank(storage, addr).map(|res| res.unwrap_or_default())
}

pub fn store_bank(
    storage: &mut dyn Storage,
    addr: &Addr,
    token_manager: &TokenManager,
) -> StdResult<()> {
    bucket(storage, PREFIX_BANK).save(addr.as_bytes(), token_manager)
}

pub fn poll_indexer_store<'a>(
    storage: &'a mut dyn Storage,
    status: &PollStatus,
) -> Bucket<'a, bool> {
    Bucket::multilevel(
        storage,
        &[PREFIX_POLL_INDEXER, status.to_string().as_bytes()],
    )
}

pub fn poll_voter_store(storage: &mut dyn Storage, poll_id: u64) -> Bucket<VoterInfo> {
    Bucket::multilevel(storage, &[PREFIX_POLL_VOTER, &poll_id.to_be_bytes()])
}

pub fn poll_voter_read(storage: &dyn Storage, poll_id: u64) -> ReadonlyBucket<VoterInfo> {
    ReadonlyBucket::multilevel(storage, &[PREFIX_POLL_VOTER, &poll_id.to_be_bytes()])
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
        Some(OrderBy::Asc) => (calc_range_start_addr(start_after), None, OrderBy::Asc),
        _ => (None, calc_range_end_addr(start_after), OrderBy::Desc),
    };

    let voters: ReadonlyBucket<VoterInfo> =
        ReadonlyBucket::multilevel(storage, &[PREFIX_POLL_VOTER, &poll_id.to_be_bytes()]);

    voters
        .range(start.as_deref(), end.as_deref(), order_by.into())
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
        Some(OrderBy::Asc) => (calc_range_start(start_after), None, OrderBy::Asc),
        _ => (None, calc_range_end(start_after), OrderBy::Desc),
    };

    if let Some(status) = filter {
        let poll_indexer: ReadonlyBucket<bool> = ReadonlyBucket::multilevel(
            storage,
            &[PREFIX_POLL_INDEXER, status.to_string().as_bytes()],
        );

        poll_indexer
            .range(start.as_deref(), end.as_deref(), order_by.into())
            .take(limit)
            .map(|item| {
                let (k, _) = item?;
                load_poll_raw_key(storage, &k)
            })
            .collect()
    } else {
        let polls: ReadonlyBucket<Poll> = ReadonlyBucket::new(storage, PREFIX_POLL);

        polls
            .range(start.as_deref(), end.as_deref(), order_by.into())
            .take(limit)
            .map(|item| {
                let (_, v) = item?;
                Ok(v)
            })
            .collect()
    }
}

// this will set the first key after the provided key, by appending a 1 byte
fn calc_range_start(start_after: Option<u64>) -> Option<Vec<u8>> {
    start_after.map(|id| {
        let mut v = id.to_be_bytes().to_vec();
        v.push(0);
        v
    })
}

// this will set the first key after the provided key, by appending a 1 byte
fn calc_range_end(start_after: Option<u64>) -> Option<Vec<u8>> {
    start_after.map(|id| id.to_be_bytes().to_vec())
}

// this will set the first key after the provided key, by appending a 1 byte
fn calc_range_start_addr(start_after: Option<Addr>) -> Option<Vec<u8>> {
    start_after.map(|addr| {
        let mut v: Vec<u8> = addr.as_ref().into();
        v.push(0);
        v
    })
}

// this will set the first key after the provided key, by appending a 1 byte
fn calc_range_end_addr(start_after: Option<Addr>) -> Option<Vec<u8>> {
    start_after.map(|addr| addr.as_ref().into())
}
