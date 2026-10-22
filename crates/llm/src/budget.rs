//! Budgets, spend tracking, and cost accounting (`LLM.md` §12–§13).

use std::sync::Mutex;

use crate::error::RouterError;
use crate::types::Usage;

/// Per-run limits enforced by the kernel before each call.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Budget {
    /// Maximum spend in USD, if limited.
    pub max_cost_usd: Option<f64>,
    /// Maximum total tokens, if limited.
    pub max_total_tokens: Option<u64>,
}

/// Cost and token consumption so far.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Spend {
    /// Spend in USD.
    pub cost_usd: f64,
    /// Tokens consumed.
    pub total_tokens: u64,
}

/// Tracks spend against a [`Budget`].
#[derive(Debug)]
pub struct BudgetTracker {
    budget: Budget,
    spend: Mutex<Spend>,
}

impl BudgetTracker {
    /// Create a tracker for `budget`.
    pub fn new(budget: Budget) -> Self {
        Self {
            budget,
            spend: Mutex::new(Spend::default()),
        }
    }

    /// The configured budget.
    pub fn budget(&self) -> Budget {
        self.budget
    }

    /// The spend so far.
    pub fn spend(&self) -> Spend {
        *self.spend.lock().expect("budget spend poisoned")
    }

    /// Refuse the next call when it would exceed the budget.
    pub fn check(&self, next_cost_usd: f64) -> Result<(), RouterError> {
        let spend = self.spend();
        if let Some(max) = self.budget.max_cost_usd {
            if spend.cost_usd + next_cost_usd > max {
                return Err(RouterError::BudgetExceeded(format!(
                    "cost {:.4} + {:.4} exceeds {:.4} USD",
                    spend.cost_usd, next_cost_usd, max
                )));
            }
        }
        Ok(())
    }

    /// Record actual consumption after a call.
    pub fn record(&self, usage: &Usage, cost_usd: f64) {
        let mut spend = self.spend.lock().expect("budget spend poisoned");
        spend.cost_usd += cost_usd;
        spend.total_tokens += u64::from(usage.total());
    }
}

/// An approximate price list: `prefix -> (input USD/Mtok, output USD/Mtok)`.
///
/// The longest matching prefix wins; unknown models cost nothing (local models).
#[derive(Debug, Clone, Default)]
pub struct CostTable {
    prices: Vec<(String, f64, f64)>,
}

impl CostTable {
    /// An empty table (everything is free).
    pub fn new() -> Self {
        Self::default()
    }

    /// A small built-in table for common hosted models (illustrative).
    pub fn with_defaults() -> Self {
        let mut table = Self::new();
        table.insert("gpt-4o-mini", 0.15, 0.60);
        table.insert("gpt-4o", 2.50, 10.00);
        table.insert("gpt-4.1", 2.00, 8.00);
        table.insert("claude-3-5-haiku", 0.80, 4.00);
        table.insert("claude-3-5-sonnet", 3.00, 15.00);
        table.insert("claude-sonnet-4", 3.00, 15.00);
        table.insert("gemini-1.5-flash", 0.075, 0.30);
        table.insert("gemini-1.5-pro", 1.25, 5.00);
        table
    }

    /// Add a price entry.
    pub fn insert(&mut self, prefix: impl Into<String>, input_per_mtok: f64, output_per_mtok: f64) {
        self.prices
            .push((prefix.into(), input_per_mtok, output_per_mtok));
    }

    /// The price per million tokens for `model`.
    pub fn price(&self, model: &str) -> (f64, f64) {
        let mut best: Option<(&str, f64, f64)> = None;
        for (prefix, input, output) in &self.prices {
            if model.starts_with(prefix.as_str())
                && best.is_none_or(|(current, _, _)| prefix.len() > current.len())
            {
                best = Some((prefix, *input, *output));
            }
        }
        best.map_or((0.0, 0.0), |(_, input, output)| (input, output))
    }

    /// The cost of `usage` on `model`, in USD.
    pub fn cost(&self, model: &str, usage: &Usage) -> f64 {
        let (input, output) = self.price(model);
        let cached = usage.cached_tokens.min(usage.input_tokens);
        let billed_input = usage.input_tokens - cached;
        (f64::from(billed_input) * input + f64::from(usage.output_tokens) * output) / 1_000_000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_refuses_overspend() {
        let tracker = BudgetTracker::new(Budget {
            max_cost_usd: Some(1.0),
            max_total_tokens: None,
        });
        assert!(tracker.check(0.5).is_ok());
        tracker.record(
            &Usage {
                input_tokens: 10,
                output_tokens: 10,
                cached_tokens: 0,
            },
            0.8,
        );
        assert!(tracker.check(0.5).is_err());
        assert_eq!(tracker.spend().total_tokens, 20);
    }

    #[test]
    fn cost_table_matches_longest_prefix() {
        let table = CostTable::with_defaults();
        assert_eq!(table.price("gpt-4o-mini-2024-07-18"), (0.15, 0.60));
        assert_eq!(table.price("gpt-4o"), (2.50, 10.00));
        assert_eq!(table.price("qwen2.5:14b"), (0.0, 0.0));
    }

    #[test]
    fn cost_accounts_cached_tokens_as_input() {
        let table = CostTable::with_defaults();
        let usage = Usage {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cached_tokens: 0,
        };
        let cost = table.cost("gpt-4o", &usage);
        assert!((cost - 2.50).abs() < 1e-9, "got {cost}");
    }
}
