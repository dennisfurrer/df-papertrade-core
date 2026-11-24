# Changelog

All notable changes to df-papertrade-core.

## Unreleased

Fields the relayer has added, read and used:

- `TradingState::account_oi_cap` (`accountOiCapRaw`): the most notional (margin x leverage, collateral units, 18 decimals) one account may hold on one market, long and short and queued opens together. $10M on 2026-11-24. `None` for 0 or absent.
- `TradingState::exchange` (`exchange`), an `ExchangeVersion`: `Next` for `"next"` (contract set 2), `Original` when absent (contract set 1), `Other` otherwise.
- `TradingState::off_hours` (`offHours`): the off-hours keeper has switched winning closes to the off-hours payout curve. Sent only while it is on, so absent reads as `false`. `TradingState::curve()` gives the curve in force, `None` unless the exchange is `Next`.
- `Account::oi_caps` (`oiCaps`), a list of `OiCap { instrument_id, cap }`: caps the relayer set for one account in place of the venue's, `None` market for every market, `None` cap for no cap. A patch that carries them replaces them.
- `Account::recent_nonces` (`recentNonces`), a `RecentNonces { exchange, staking }`: the latest nonce the relayer has seen the wallet use in each lane (`session` and `paper`).
- `PaperFigures::rebate_c36` (`paper.rebateC36`): contract set 2's mint constant `c` times 10^36.

New API:

- `Account::oi_cap`, `Account::oi_used` and `Account::oi_room`: the account's cap on a market, the notional it holds there and what it may still open.
- `Rates::current(state, builder_fee) -> Result<Rates, RatesError>`: the win fee, builder fee and curve a close signed now settles at, from the trading state.
- `Client::close_builder_fee(store, owner)`: the builder fee a close signed by the stored session key pays, read on chain. `Client::key_builder_fee(owner, key)`, `Client::registered_key(owner, key)` and `Client::builder_fee_of(builder)` are its parts.
- `Client::off_hours()`: the Exchange's `offHoursPayout()`.
- `chain::session_key` (`sessionKeys(user, key)` on the SessionKeyManager, as `RegisteredKey { expiry, builder }`), `chain::builder_fee_rate_of` (`feeRateOf(builder)` on the BuilderRegistry) and `chain::off_hours`.
- `Client::catch_up_nonces(store, owner, recent)`, `Nonces::catch_up` and `nonce::caught_up`: move a stored lane past the latest nonce the relayer has seen, within `nonce::CATCH_UP_WINDOW` (65,536), as papertrade.xyz does.
- `math::rebate_mint` and `math::rebate_rate`: PAPER contract set 2 mints for a loss basis, `supply x (e^(c x g) - 1)`, without the insolvency bonus. `PaperFigures::mint_for_loss` uses it.

Changed:

- `open_limits` and `check_open` take `account_room: Option<u128>` last (`Account::oi_room`, `None` for no cap or no account), and refuse or limit with the new `Limit::AccountOpenInterest` and `OpenRefusal::AccountOpenInterest`. `Client::open` passes the account's room.
- `PaperFigures::mint_rate` follows contract set 2 when `rebate_c36` is sent, about 10.47 PAPER a dollar on 2026-11-24, instead of contract set 1's curve.
- The recordings `trading_mainnet.json` and `summary.json` are the answers of 2026-11-24; `live_recent_nonces.txt` and `hyperevm/session_key.json`, `builder_fee_rate_of.json` and `off_hours.json` are new.

## 0.0.13-beta

- Depends on df-hyperliquid-core 0.1.1, the first stable release, instead of 0.0.10-beta.
- A signed intent carries a `bytes` field as `0x` and lowercase hex. df-hyperliquid-core 0.1 added `Field::Bytes`; no papertrade intent uses it yet.
- `Client::set_builder` and `Client::current_builder`: the builder code that registrations name can be changed at runtime, and clones of a client share it.

## 0.0.12-beta

- Contract set 2 and builder codes.

## 0.0.11-beta and earlier

See the [GitHub repository](https://github.com/dennisfurrer/df-papertrade-core) history.
