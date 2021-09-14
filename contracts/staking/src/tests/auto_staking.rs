use crate::contract::{execute, instantiate};
use crate::tests::{mock_dependencies_with_querier, mock_env_block_time};
use cosmwasm_std::testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{attr, to_binary, Addr, Coin, CosmosMsg, StdError, SubMsg, Uint128, WasmMsg};
use cw20::Cw20ExecuteMsg;
use services::staking::{ExecuteMsg, InstantiateMsg, StakingSchedule};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::ExecuteMsg as PairExecuteMsg;

#[test]
fn wrong_assets() {
    let mut deps = mock_dependencies_with_querier(&[]);
    deps.querier.with_pair_info(Addr::unchecked("pair"));
    deps.querier.with_lp_token(Addr::unchecked("lp_token"));

    let msg = InstantiateMsg {
        owner: "owner".to_string(),
        psi_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        terraswap_factory: "terraswap_factory0000".to_string(),
        distribution_schedule: vec![
            StakingSchedule::new(
                mock_env_block_time(),
                mock_env_block_time() + 100,
                Uint128::from(1000000u128),
            ),
            StakingSchedule::new(
                mock_env_block_time() + 100,
                mock_env_block_time() + 200,
                Uint128::from(10000000u128),
            ),
        ],
    };

    let info = mock_info("addr", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    let msg = ExecuteMsg::AutoStake {
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::new(100u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "ukrw".to_string(),
                },
                amount: Uint128::new(100u128),
            },
        ],
        slippage_tolerance: None,
    };
    let info = mock_info(
        "addr0000",
        &[
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(100u128),
            },
            Coin {
                denom: "ukrw".to_string(),
                amount: Uint128::new(100u128),
            },
        ],
    );
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
    assert_eq!(res, StdError::generic_err("Invalid staking token"));
}

#[test]
fn attempt_with_no_coins() {
    let mut deps = mock_dependencies_with_querier(&[]);
    deps.querier.with_pair_info(Addr::unchecked("pair"));
    let staking_token = "staking0000".to_string();
    deps.querier
        .with_lp_token(Addr::unchecked(staking_token.clone()));

    let msg = InstantiateMsg {
        owner: "owner".to_string(),
        psi_token: "reward0000".to_string(),
        staking_token: staking_token.clone(),
        terraswap_factory: "terraswap_factory0000".to_string(),
        distribution_schedule: vec![
            StakingSchedule::new(
                mock_env_block_time(),
                mock_env_block_time() + 100,
                Uint128::from(1000000u128),
            ),
            StakingSchedule::new(
                mock_env_block_time() + 100,
                mock_env_block_time() + 200,
                Uint128::from(10000000u128),
            ),
        ],
    };

    let info = mock_info("addr", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    let msg = ExecuteMsg::AutoStake {
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::new(100u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: "asset".to_string(),
                },
                amount: Uint128::new(1u128),
            },
        ],
        slippage_tolerance: None,
    };

    // sending no coins
    let info = mock_info("addr0000", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg.clone()).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err(
            "Native token balance mismatch between the argument and the transferred"
        )
    );
}

#[test]
fn native_plus_token() {
    let mut deps = mock_dependencies_with_querier(&[]);
    deps.querier.with_pair_info(Addr::unchecked("pair"));
    let staking_token = "staking0000".to_string();
    deps.querier
        .with_lp_token(Addr::unchecked(staking_token.clone()));
    let staker_addr = "addr0000".to_string();

    let msg = InstantiateMsg {
        owner: "owner".to_string(),
        psi_token: "reward0000".to_string(),
        staking_token: staking_token.clone(),
        terraswap_factory: "terraswap_factory0000".to_string(),
        distribution_schedule: vec![
            StakingSchedule::new(
                mock_env_block_time(),
                mock_env_block_time() + 100,
                Uint128::from(1000000u128),
            ),
            StakingSchedule::new(
                mock_env_block_time() + 100,
                mock_env_block_time() + 200,
                Uint128::from(10000000u128),
            ),
        ],
    };

    let info = mock_info("addr", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    let msg = ExecuteMsg::AutoStake {
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::new(100u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: "asset".to_string(),
                },
                amount: Uint128::new(1u128),
            },
        ],
        slippage_tolerance: None,
    };

    let info = mock_info(
        &staker_addr,
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::new(100u128),
        }],
    );
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "asset".to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: staker_addr.clone(),
                    recipient: MOCK_CONTRACT_ADDR.to_string(),
                    amount: Uint128::new(1u128),
                })
                .unwrap(),
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "asset".to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: "pair".to_string(),
                    amount: Uint128::new(1),
                    expires: None,
                })
                .unwrap(),
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "pair".to_string(),
                msg: to_binary(&PairExecuteMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uusd".to_string()
                            },
                            amount: Uint128::new(99u128),
                        },
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: "asset".to_string()
                            },
                            amount: Uint128::new(1u128),
                        },
                    ],
                    slippage_tolerance: None,
                    receiver: None,
                })
                .unwrap(),
                funds: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::new(99u128), // 1% tax
                }],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MOCK_CONTRACT_ADDR.to_string(),
                msg: to_binary(&ExecuteMsg::AutoStakeHook {
                    staker_addr: staker_addr.clone(),
                    prev_staking_token_amount: Uint128::new(0),
                })
                .unwrap(),
                funds: vec![],
            }))
        ]
    );
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "auto_stake"),
            attr("native_token_0", "uusd"),
            attr("tax_amount_0", "1"),
            attr("asset_token_1", "asset"),
        ]
    );

    deps.querier.with_token_balance(Uint128::new(100u128)); // recive 100 lptoken

    let msg = ExecuteMsg::AutoStakeHook {
        staker_addr: staker_addr.clone(),
        prev_staking_token_amount: Uint128::new(0),
    };

    // unauthorized attempt
    let info = mock_info(&staker_addr, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg.clone()).unwrap_err();
    assert_eq!(res, StdError::generic_err("unauthorized"));

    // successfull attempt
    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "bond"),
            attr("staker_addr", staker_addr.clone()),
            attr("amount", "100"),
        ]
    );
}

#[test]
fn native_plus_native() {
    let mut deps = mock_dependencies_with_querier(&[]);
    deps.querier.with_pair_info(Addr::unchecked("pair"));
    let staking_token = "staking0000".to_string();
    deps.querier
        .with_lp_token(Addr::unchecked(staking_token.clone()));
    let staker_addr = "addr0000".to_string();

    let msg = InstantiateMsg {
        owner: "owner".to_string(),
        psi_token: "reward0000".to_string(),
        staking_token: staking_token.clone(),
        terraswap_factory: "terraswap_factory0000".to_string(),
        distribution_schedule: vec![
            StakingSchedule::new(
                mock_env_block_time(),
                mock_env_block_time() + 100,
                Uint128::from(1000000u128),
            ),
            StakingSchedule::new(
                mock_env_block_time() + 100,
                mock_env_block_time() + 200,
                Uint128::from(10000000u128),
            ),
        ],
    };

    let info = mock_info("addr", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    let msg = ExecuteMsg::AutoStake {
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::new(100u128),
            },
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "ukrw".to_string(),
                },
                amount: Uint128::new(100u128),
            },
        ],
        slippage_tolerance: None,
    };

    let info = mock_info(
        &staker_addr,
        &[
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128::new(100u128),
            },
            Coin {
                denom: "ukrw".to_string(),
                amount: Uint128::new(100u128),
            },
        ],
    );

    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "pair".to_string(),
                msg: to_binary(&PairExecuteMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uusd".to_string()
                            },
                            amount: Uint128::new(99u128),
                        },
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "ukrw".to_string()
                            },
                            amount: Uint128::new(99u128),
                        },
                    ],
                    slippage_tolerance: None,
                    receiver: None,
                })
                .unwrap(),
                funds: vec![
                    Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128::new(99u128), // 1% tax
                    },
                    Coin {
                        denom: "ukrw".to_string(),
                        amount: Uint128::new(99u128), // 1% tax
                    }
                ],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MOCK_CONTRACT_ADDR.to_string(),
                msg: to_binary(&ExecuteMsg::AutoStakeHook {
                    staker_addr: staker_addr.clone(),
                    prev_staking_token_amount: Uint128::new(0),
                })
                .unwrap(),
                funds: vec![],
            }))
        ]
    );
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "auto_stake"),
            attr("native_token_0", "uusd"),
            attr("tax_amount_0", "1"),
            attr("native_token_1", "ukrw"),
            attr("tax_amount_1", "1"),
        ]
    );

    deps.querier.with_token_balance(Uint128::new(100u128)); // recive 100 lptoken

    let msg = ExecuteMsg::AutoStakeHook {
        staker_addr: staker_addr.clone(),
        prev_staking_token_amount: Uint128::new(0),
    };

    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "bond"),
            attr("staker_addr", staker_addr.clone()),
            attr("amount", "100"),
        ]
    );
}

#[test]
fn token_plus_token() {
    let mut deps = mock_dependencies_with_querier(&[]);
    deps.querier.with_pair_info(Addr::unchecked("pair"));
    let staking_token = "staking0000".to_string();
    deps.querier
        .with_lp_token(Addr::unchecked(staking_token.clone()));
    let staker_addr = "addr0000".to_string();

    let msg = InstantiateMsg {
        owner: "owner".to_string(),
        psi_token: "reward0000".to_string(),
        staking_token: staking_token.clone(),
        terraswap_factory: "terraswap_factory0000".to_string(),
        distribution_schedule: vec![
            StakingSchedule::new(
                mock_env_block_time(),
                mock_env_block_time() + 100,
                Uint128::from(1000000u128),
            ),
            StakingSchedule::new(
                mock_env_block_time() + 100,
                mock_env_block_time() + 200,
                Uint128::from(10000000u128),
            ),
        ],
    };

    let info = mock_info("addr", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    let msg = ExecuteMsg::AutoStake {
        assets: [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: "asset_0".to_string(),
                },
                amount: Uint128::new(1u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: "asset_1".to_string(),
                },
                amount: Uint128::new(1u128),
            },
        ],
        slippage_tolerance: None,
    };

    let info = mock_info(&staker_addr, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "asset_0".to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: staker_addr.clone(),
                    recipient: MOCK_CONTRACT_ADDR.to_string(),
                    amount: Uint128::new(1u128),
                })
                .unwrap(),
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "asset_0".to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: "pair".to_string(),
                    amount: Uint128::new(1),
                    expires: None,
                })
                .unwrap(),
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "asset_1".to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: staker_addr.clone(),
                    recipient: MOCK_CONTRACT_ADDR.to_string(),
                    amount: Uint128::new(1u128),
                })
                .unwrap(),
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "asset_1".to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: "pair".to_string(),
                    amount: Uint128::new(1),
                    expires: None,
                })
                .unwrap(),
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "pair".to_string(),
                msg: to_binary(&PairExecuteMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: "asset_0".to_string()
                            },
                            amount: Uint128::new(1u128),
                        },
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: "asset_1".to_string()
                            },
                            amount: Uint128::new(1u128),
                        },
                    ],
                    slippage_tolerance: None,
                    receiver: None,
                })
                .unwrap(),
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MOCK_CONTRACT_ADDR.to_string(),
                msg: to_binary(&ExecuteMsg::AutoStakeHook {
                    staker_addr: staker_addr.clone(),
                    prev_staking_token_amount: Uint128::new(0),
                })
                .unwrap(),
                funds: vec![],
            }))
        ]
    );
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "auto_stake"),
            attr("asset_token_0", "asset_0"),
            attr("asset_token_1", "asset_1"),
        ]
    );

    deps.querier.with_token_balance(Uint128::new(100u128)); // recive 100 lptoken

    let msg = ExecuteMsg::AutoStakeHook {
        staker_addr: staker_addr.clone(),
        prev_staking_token_amount: Uint128::new(0),
    };

    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "bond"),
            attr("staker_addr", staker_addr.clone()),
            attr("amount", "100"),
        ]
    );
}
