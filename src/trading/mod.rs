use chrono::Utc;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::strategy::Action;
pub mod persistence;
pub use persistence::{ClosedTradeRecord, PortfolioRecord, PositionRecord, PositionSide, SharedAppData, SharedDataStore};

/// Open paper-trading position.
#[derive(Debug, Clone)]
pub struct PaperPosition {
    pub id: String,
    pub symbol: String,
    pub side: Action, // LONG or SHORT
    pub entry_price: f64,
    pub size: f64,
    pub leverage: u32,
    pub margin_used: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub unrealized_pnl: f64,
    pub unrealized_pnl_percent: f64,
    pub opened_at: String,
}

/// Closed trade record for paper mode.
#[derive(Debug, Clone)]
pub struct ClosedPaperTrade {
    pub id: String,
    pub symbol: String,
    pub side: Action,
    pub entry_price: f64,
    pub exit_price: f64,
    pub margin_used: f64,
    pub realized_pnl: f64,
    pub realized_pnl_percent: f64,
    pub hold_minutes: i64,
    pub closed_at: String,
}

/// Minimal paper engine used for integration tests and Phase 7 live/paper toggle.
#[derive(Debug, Default)]
pub struct PaperEngine {
    pub cash_balance: f64,
    pub realized_pnl: f64,
    prices: HashMap<String, f64>,
    pub open_positions: Vec<PaperPosition>,
    pub closed_trades: Vec<ClosedPaperTrade>,
}

impl PaperEngine {
    /// Create a new paper engine with starting cash.
    pub fn new(starting_balance: f64) -> Self {
        PaperEngine {
            cash_balance: starting_balance,
            realized_pnl: 0.0,
            prices: HashMap::new(),
            open_positions: Vec::new(),
            closed_trades: Vec::new(),
        }
    }

    /// Upsert a price and refresh unrealized PnL.
    pub fn set_price(&mut self, symbol: &str, price: f64) {
        self.prices.insert(symbol.to_string(), price);
        self.refresh_unrealized();
    }

    /// Open a paper position; returns position id.
    pub fn open_position(
        &mut self,
        symbol: &str,
        side: Action,
        margin_usd: f64,
        leverage: u32,
        stop_loss: f64,
        take_profit: f64,
    ) -> Result<String, String> {
        if !matches!(side, Action::LONG | Action::SHORT) {
            return Err("Only LONG or SHORT positions are supported".to_string());
        }
        if margin_usd <= 0.0 {
            return Err("Margin must be > 0".to_string());
        }
        if leverage == 0 {
            return Err("Leverage must be >= 1".to_string());
        }
        if margin_usd > self.cash_balance {
            return Err(format!(
                "Insufficient cash. Available ${:.2}, requested ${:.2}",
                self.cash_balance, margin_usd
            ));
        }
        let price = *self
            .prices
            .get(symbol)
            .ok_or_else(|| format!("Price for {} not set", symbol))?;

        let size = (margin_usd * leverage as f64) / price;
        let position_id = format!("pos_{}", Self::generate_id());

        self.cash_balance -= margin_usd;

        let position = PaperPosition {
            id: position_id.clone(),
            symbol: symbol.to_string(),
            side,
            entry_price: price,
            size,
            leverage,
            margin_used: margin_usd,
            stop_loss,
            take_profit,
            unrealized_pnl: 0.0,
            unrealized_pnl_percent: 0.0,
            opened_at: Utc::now().to_rfc3339(),
        };

        self.open_positions.push(position);
        self.refresh_unrealized();
        Ok(position_id)
    }

    /// Close a position by id, using current price or supplied exit price.
    pub fn close_position(
        &mut self,
        position_id: &str,
        exit_price: Option<f64>,
    ) -> Result<ClosedPaperTrade, String> {
        let idx = self
            .open_positions
            .iter()
            .position(|p| p.id == position_id)
            .ok_or_else(|| "Position not found".to_string())?;

        let position = self.open_positions.remove(idx);
        let price = exit_price
            .or_else(|| self.prices.get(&position.symbol).copied())
            .ok_or_else(|| format!("Price for {} not set", position.symbol))?;

        let pnl = match position.side {
            Action::LONG => (price - position.entry_price) * position.size,
            Action::SHORT => (position.entry_price - price) * position.size,
            Action::HOLD => 0.0,
        };

        let pnl_percent = if position.margin_used != 0.0 {
            (pnl / position.margin_used) * 100.0
        } else {
            0.0
        };

        self.cash_balance += position.margin_used + pnl;
        self.realized_pnl += pnl;

        let closed = ClosedPaperTrade {
            id: format!("close_{}", position.id),
            symbol: position.symbol.clone(),
            side: position.side,
            entry_price: position.entry_price,
            exit_price: price,
            margin_used: position.margin_used,
            realized_pnl: pnl,
            realized_pnl_percent: pnl_percent,
            hold_minutes: Self::minutes_held(&position.opened_at),
            closed_at: Utc::now().to_rfc3339(),
        };

        self.closed_trades.push(closed.clone());
        self.refresh_unrealized();
        Ok(closed)
    }

    /// Portfolio heat: % of equity locked in margin.
    pub fn portfolio_heat(&self) -> f64 {
        let locked_margin: f64 = self.open_positions.iter().map(|p| p.margin_used).sum();
        let equity = self.equity();
        if equity == 0.0 {
            0.0
        } else {
            (locked_margin / equity) * 100.0
        }
    }

    /// Current portfolio equity (cash + position equity).
    pub fn equity(&self) -> f64 {
        let position_equity: f64 = self
            .open_positions
            .iter()
            .map(|p| p.margin_used + p.unrealized_pnl)
            .sum();
        self.cash_balance + position_equity
    }

    fn refresh_unrealized(&mut self) {
        for position in &mut self.open_positions {
            let price = *self
                .prices
                .get(&position.symbol)
                .unwrap_or(&position.entry_price);

            let pnl = match position.side {
                Action::LONG => (price - position.entry_price) * position.size,
                Action::SHORT => (position.entry_price - price) * position.size,
                Action::HOLD => 0.0,
            };

            position.unrealized_pnl_percent = if position.margin_used != 0.0 {
                (pnl / position.margin_used) * 100.0
            } else {
                0.0
            };
            position.unrealized_pnl = pnl;
        }
    }

    fn generate_id() -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros();
        format!("{:x}", timestamp)
    }

    fn minutes_held(opened_at: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(opened_at)
            .ok()
            .map(|dt| (Utc::now() - dt.with_timezone(&Utc)).num_minutes())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_and_close_updates_equity() {
        let mut engine = PaperEngine::new(10_000.0);
        engine.set_price("ETHUSDT", 2000.0);
        let id = engine
            .open_position("ETHUSDT", Action::LONG, 1000.0, 5, 1800.0, 2300.0)
            .expect("open failed");
        let equity_after_open = engine.equity();
        assert!(engine.cash_balance < 10_000.0, "Cash reduced by margin");
        assert!(
            (equity_after_open - 10_000.0).abs() < 1e-6,
            "Equity stays constant when margin is locked"
        );

        engine.set_price("ETHUSDT", 2100.0);
        let closed = engine
            .close_position(&id, None)
            .expect("close failed");
        assert!(closed.realized_pnl > 0.0);
        assert!(engine.cash_balance > 10_000.0, "Profits added back to cash");
        assert!(engine.realized_pnl > 0.0);
    }
}
