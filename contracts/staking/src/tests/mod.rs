mod add_schedule;
mod auto_staking;
mod deposit_and_withdraw;
mod instantiate;
mod migrate_staking;
mod queries;

use cosmwasm_std::testing::{mock_env, MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Addr, Coin, ContractResult, Decimal, OwnedDeps, Querier,
    QuerierResult, QueryRequest, SystemError, SystemResult, Uint128, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;
use serde::Deserialize;
use terra_cosmwasm::{TaxCapResponse, TaxRateResponse, TerraQuery, TerraQueryWrapper, TerraRoute};
use terraswap::asset::{AssetInfo, PairInfo};

fn mock_env_block_time() -> u64 {
    mock_env().block.time.seconds()
}

pub struct WasmMockQuerier {
    base: MockQuerier<TerraQueryWrapper>,
    pair_addr: Addr,
    lp_token_addr: Addr,
    token_balance: Uint128,
    tax: (Decimal, Uint128),
}

pub fn mock_dependencies_with_querier(
    contract_balance: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let custom_querier: WasmMockQuerier =
        WasmMockQuerier::new(MockQuerier::new(&[(MOCK_CONTRACT_ADDR, contract_balance)]));

    OwnedDeps {
        api: MockApi::default(),
        storage: MockStorage::default(),
        querier: custom_querier,
    }
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<TerraQueryWrapper> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                })
            }
        };
        self.handle_query(&request)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MockQueryMsg {
    Pair { asset_infos: [AssetInfo; 2] },
    Balance { address: String },
}

impl WasmMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<TerraQueryWrapper>) -> QuerierResult {
        match &request {
            QueryRequest::Custom(TerraQueryWrapper { route, query_data }) => {
                if route == &TerraRoute::Treasury {
                    match query_data {
                        TerraQuery::TaxRate {} => {
                            let res = TaxRateResponse { rate: self.tax.0 };
                            SystemResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        TerraQuery::TaxCap { .. } => {
                            let res = TaxCapResponse { cap: self.tax.1 };
                            SystemResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        _ => panic!("DO NOT ENTER HERE"),
                    }
                } else {
                    panic!("DO NOT ENTER HERE")
                }
            }
            QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: _,
                msg,
            }) => match from_binary(msg).unwrap() {
                MockQueryMsg::Pair { asset_infos } => {
                    SystemResult::Ok(ContractResult::from(to_binary(&PairInfo {
                        asset_infos,
                        contract_addr: self.pair_addr.to_string(),
                        liquidity_token: self.lp_token_addr.to_string(),
                    })))
                }
                MockQueryMsg::Balance { address: _ } => {
                    SystemResult::Ok(ContractResult::from(to_binary(&cw20::BalanceResponse {
                        balance: self.token_balance,
                    })))
                }
            },

            QueryRequest::Wasm(WasmQuery::Raw {
                contract_addr: _,
                key,
            }) => {
                let key: &[u8] = key.as_slice();
                let prefix_balance = to_length_prefixed(b"balance").to_vec();
                if key[..prefix_balance.len()].to_vec() == prefix_balance {
                    SystemResult::Ok(ContractResult::from(to_binary(&self.token_balance)))
                } else {
                    panic!("DO NOT ENTER HERE")
                }
            }
            _ => self.base.handle_query(request),
        }
    }
}

impl WasmMockQuerier {
    pub fn new(base: MockQuerier<TerraQueryWrapper>) -> Self {
        WasmMockQuerier {
            base,
            pair_addr: Addr::unchecked(""),
            lp_token_addr: Addr::unchecked(""),
            token_balance: Uint128::zero(),
            tax: (Decimal::percent(1), Uint128::new(1000000)),
        }
    }

    pub fn with_pair_info(&mut self, pair_addr: Addr) {
        self.pair_addr = pair_addr;
    }

    pub fn with_lp_token(&mut self, lp_token_addr: Addr) {
        self.lp_token_addr = lp_token_addr;
    }

    pub fn with_token_balance(&mut self, token_balance: Uint128) {
        self.token_balance = token_balance;
    }
}
