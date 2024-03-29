use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::OrderBy;
use cosmwasm_std::Uint128;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub owner: String,
    pub psi_token: String,
    pub genesis_time: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    UpdateConfig {
        owner: Option<String>,
        psi_token: Option<String>,
        genesis_time: Option<u64>,
    },
    RegisterVestingAccounts {
        vesting_accounts: Vec<VestingAccount>,
    },
    Claim {},
    ClaimFor {
        address: String,
    },
}

/// CONTRACT: end_time > start_time
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingAccount {
    pub address: String,
    pub schedules: Vec<VestingSchedule>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingInfo {
    pub schedules: Vec<VestingSchedule>,
    pub last_claim_time: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingSchedule {
    pub start_time: u64,
    pub end_time: u64,
    pub cliff_end_time: u64,
    pub amount: Uint128,
}

impl VestingSchedule {
    pub fn new(start_time: u64, end_time: u64, cliff_end_time: u64, amount: Uint128) -> Self {
        Self {
            start_time,
            end_time,
            cliff_end_time,
            amount,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    VestingAccount {
        address: String,
    },
    VestingAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
        order_by: Option<OrderBy>,
    },
    Claimable {
        address: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: String,
    pub psi_token: String,
    pub genesis_time: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingAccountResponse {
    pub address: String,
    pub info: VestingInfo,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VestingAccountsResponse {
    pub vesting_accounts: Vec<VestingAccountResponse>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ClaimableAmountResponse {
    pub address: String,
    pub claimable_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
