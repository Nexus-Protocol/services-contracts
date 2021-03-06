use error::ContractError;

pub mod commands;
pub mod contract;
pub mod error;
pub mod queries;
pub mod state;

#[cfg(test)]
mod tests;

type ContractResult<T> = Result<T, ContractError>;
