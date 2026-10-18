//! `GET /query/protocol/market-stats`: each market's notional over the last day.

use df_hyperliquid_core::utils::Json;

use crate::utils::wire::{BadWire, bad, field, parse, u128_of};

/// Each instrument's 24 hour notional in collateral units, sorted by instrument id.
pub fn market_volumes(text: &str) -> Result<Vec<(u32, u128)>, BadWire> {
    let json = parse(text, "market_stats")?;
    let markets = field(&json, "markets")?;
    let Some(rows) = markets.as_object() else {
        return Err(bad("markets", markets));
    };
    let mut volumes = rows
        .iter()
        .map(|(id, row)| {
            let id = id.parse().map_err(|_| bad("market_id", &Json::String(id.clone())))?;
            Ok((id, u128_of(field(row, "volume_24h")?, "volume_24h")?))
        })
        .collect::<Result<Vec<_>, BadWire>>()?;
    volumes.sort_by_key(|(id, _)| *id);
    Ok(volumes)
}

#[cfg(test)]
#[path = "tests/volumes.rs"]
mod tests;
