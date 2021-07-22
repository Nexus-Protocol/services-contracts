use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Decimal, Uint128};
use cw20::Cw20ReceiveMsg;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub psi_token: String,
    pub staking_token: String, // lp token of PSI-UST or nAsset-PSI pair contract
    pub distribution_schedule: Vec<(u64, u64, Uint128)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    Unbond {
        amount: Uint128,
    },
    /// Withdraw pending rewards
    Withdraw {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    Bond {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {
        block_height: Option<u64>,
    },
    StakerInfo {
        staker: String,
        block_height: Option<u64>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub psi_token: String,
    pub staking_token: String,
    pub distribution_schedule: Vec<(u64, u64, Uint128)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StateResponse {
    pub last_distributed: u64,
    pub total_bond_amount: Uint128,
    pub global_reward_index: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StakerInfoResponse {
    pub staker: String,
    pub reward_index: Decimal,
    pub bond_amount: Uint128,
    pub pending_reward: Uint128,
}
