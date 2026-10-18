//! Price table and cost estimates. Everything here is an ESTIMATE: prices
//! change, tiers differ, and the provider's invoice is the truth. The table is
//! user-editable.

use crate::event::Usage;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MICRO: u128 = 1_000_000;

/// Price per million tokens, in micro-USD (1 USD = 1_000_000).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Price {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    /// Five-minute cache write.
    pub cache_write: u64,
}

impl Price {
    /// Builds a price from USD per million tokens given as micro-USD.
    pub const fn new(input: u64, output: u64, cache_read: u64, cache_write: u64) -> Self {
        Self {
            input,
            output,
            cache_read,
            cache_write,
        }
    }
}

/// An estimated cost. `estimate` is always true; it travels with the number so
/// a UI cannot show it without the label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostEstimate {
    pub micro_usd: u64,
    pub estimate: bool,
}

impl CostEstimate {
    /// `~$0.0123`, always prefixed with a tilde.
    pub fn display(&self) -> String {
        let micro = self.micro_usd;
        format!("~${}.{:06}", micro / 1_000_000, micro % 1_000_000)
    }
}

/// Model id to price.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceTable {
    /// Where the numbers came from, shown next to the estimate.
    pub source: String,
    pub models: BTreeMap<String, Price>,
}

impl PriceTable {
    /// Built-in prices for the Claude 5.5 family, from the Anthropic pricing
    /// page (checked 2026-10-18). Haiku 5.5 uses the tier for prompts up to
    /// 100K tokens; larger prompts cost more and are underestimated.
    pub fn builtin() -> Self {
        let models = BTreeMap::from([
            (
                "claude-fable-5-1".to_owned(),
                Price::new(10_000_000, 50_000_000, 250_000, 12_500_000),
            ),
            (
                "claude-opus-5-5".to_owned(),
                Price::new(4_000_000, 20_000_000, 200_000, 5_000_000),
            ),
            (
                "claude-sonnet-5-5".to_owned(),
                Price::new(2_000_000, 10_000_000, 100_000, 2_500_000),
            ),
            (
                "claude-haiku-5-5".to_owned(),
                Price::new(100_000, 500_000, 10_000, 125_000),
            ),
        ]);
        Self {
            source: "Anthropic pricing page, 2026-10-18".to_owned(),
            models,
        }
    }

    /// Adds or replaces a model's price (the user's override).
    pub fn set(&mut self, model: &str, price: Price) {
        self.models.insert(model.to_owned(), price);
    }

    /// Estimates the cost of `usage` on `model`; `None` for an unknown model.
    pub fn estimate(&self, model: &str, usage: &Usage) -> Option<CostEstimate> {
        let p = self.models.get(model)?;
        let total: u128 = [
            (usage.input, p.input),
            (usage.output, p.output),
            (usage.cache_read, p.cache_read),
            (usage.cache_write, p.cache_write),
        ]
        .iter()
        .map(|(tokens, price)| u128::from(*tokens) * u128::from(*price))
        .sum();
        // Round up: an estimate should not flatter the user.
        let micro = total.div_ceil(MICRO);
        Some(CostEstimate {
            micro_usd: u64::try_from(micro).unwrap_or(u64::MAX),
            estimate: true,
        })
    }
}
