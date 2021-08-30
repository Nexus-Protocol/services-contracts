use cosmwasm_std::{Addr, Binary, Deps, QueryRequest, StdResult, Uint128, WasmQuery};
use cosmwasm_storage::to_length_prefixed;

pub fn query_token_balance(
    deps: Deps,
    contract_addr: &Addr,
    account_addr: &Addr,
) -> StdResult<Uint128> {
    // load balance form the cw20 token contract version 0.6+
    Ok(deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Raw {
            contract_addr: contract_addr.to_string(),
            key: Binary::from(concat(
                &to_length_prefixed(b"balance"),
                account_addr.as_bytes(),
            )),
        }))
        .unwrap_or_else(|_| Uint128::zero()))
}

#[inline]
fn concat(namespace: &[u8], key: &[u8]) -> Vec<u8> {
    let mut k = namespace.to_vec();
    k.extend_from_slice(key);
    k
}
