## Service Contracts

1. [airdrop](#airdrop)
2. [community](#community)
3. [governance](#governance)
4. [staking](#staking)
5. [vesting](#vesting)

## airdrop

The Airdrop contract is for airdropping PSI tokens to ANC stakers.

## community

The Community Contract holds the funds of the Community Pool, which can be spent through a governance poll.

## governance

The Gov Contract contains logic for holding polls and Nexus Token (PSI) staking, and allows the Nexus Protocol to be governed by its users in a decentralized manner. After the initial bootstrapping of Nexus Protocol contracts, the Gov Contract is assigned to be the owner of itself and other contracts.

New proposals for change are submitted as polls, and are voted on by PSI stakers through the voting procedure. Polls can contain messages that can be executed directly without changing the Nexus Protocol code.

## staking

The Staking Contract contains the logic for LP Token staking and reward distribution. PSI tokens allocated for as liquidity incentives are distributed pro-rata to stakers of the PSI-UST Terraswap pair LP token.

## vesting

The Vesting Contract contains logic for distributing the token according to the specified vesting schedules for multiple accounts. Each account can have a different vesting schedules, and the accounts can claim a token at any time after the schedule has passed.
