use eframe::egui::{self, RichText};

use crate::strategy::Action;
use crate::ui::theme::Theme;

#[derive(Debug, Clone)]
pub struct PositionRow {
    pub symbol: String,
    pub action: Action,
    pub entry: f64,
    pub last: f64,
    pub size: f64,
    pub leverage: u32,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub unrealized_pnl: f64,
    pub unrealized_pct: f64,
}

impl PositionRow {
    pub fn new(symbol: &str, action: Action, entry: f64, last: f64, size: f64, leverage: u32) -> Self {
        let mut row = PositionRow {
            symbol: symbol.to_string(),
            action,
            entry,
            last,
            size,
            leverage,
            stop_loss: if action == Action::LONG { entry * 0.97 } else { entry * 1.03 },
            take_profit: if action == Action::LONG { entry * 1.05 } else { entry * 0.95 },
            unrealized_pnl: 0.0,
            unrealized_pct: 0.0,
        };
        row.recalculate();
        row
    }

    pub fn recalculate(&mut self) {
        let price_change = if self.action == Action::LONG {
            (self.last - self.entry) / self.entry
        } else {
            (self.entry - self.last) / self.entry
        };
        self.unrealized_pct = price_change * 100.0 * self.leverage as f64;
        self.unrealized_pnl = self.size * price_change * self.leverage as f64;
    }
}

/// Displays open positions with PnL tracking.
pub struct PositionsPanel {
    pub positions: Vec<PositionRow>,
}

impl PositionsPanel {
    pub fn new() -> Self {
        PositionsPanel {
            positions: Vec::new(),
        }
    }

    pub fn set_positions(&mut self, positions: Vec<PositionRow>) {
        self.positions = positions;
    }

    pub fn refresh_prices(&mut self, deltas: &[f64]) {
        for (row, delta) in self.positions.iter_mut().zip(deltas.iter()) {
            row.last += delta;
            row.recalculate();
        }
    }

    pub fn total_unrealized(&self) -> f64 {
        self.positions.iter().map(|p| p.unrealized_pnl).sum()
    }

    pub fn show(&self, ui: &mut egui::Ui, theme: &Theme) {
        ui.label(RichText::new("Open Positions").color(theme.green).strong());

        egui::Grid::new("positions_grid")
            .striped(true)
            .num_columns(7)
            .show(ui, |ui| {
                ui.label("Symbol");
                ui.label("Side");
                ui.label("Entry");
                ui.label("Last");
                ui.label("Size");
                ui.label("PnL");
                ui.label("Stops/TP");
                ui.end_row();

                for pos in &self.positions {
                    let pnl_color = if pos.unrealized_pnl >= 0.0 {
                        theme.green
                    } else {
                        theme.red
                    };
                    ui.label(RichText::new(&pos.symbol).strong());
                    ui.label(match pos.action {
                        Action::LONG => RichText::new("LONG").color(theme.green),
                        Action::SHORT => RichText::new("SHORT").color(theme.red),
                        Action::HOLD => RichText::new("HOLD").color(theme.yellow),
                    });
                    ui.label(format!("{:.2}", pos.entry));
                    ui.label(format!("{:.2}", pos.last));
                    ui.label(format!("${:.0}", pos.size));
                    ui.colored_label(
                        pnl_color,
                        format!(
                            "${:.0} ({:.1}%)",
                            pos.unrealized_pnl,
                            pos.unrealized_pct
                        ),
                    );
                    ui.label(format!(
                        "SL {:.2} | TP {:.2}",
                        pos.stop_loss, pos.take_profit
                    ));
                    ui.end_row();
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pnl_calculates_for_long_and_short() {
        let long = PositionRow::new("ETH", Action::LONG, 3000.0, 3300.0, 1000.0, 5);
        assert!(long.unrealized_pnl > 0.0);
        let mut short = PositionRow::new("SOL", Action::SHORT, 180.0, 190.0, 500.0, 3);
        assert!(short.unrealized_pnl < 0.0, "Short should lose when price rises");
        short.last = 170.0;
        short.recalculate();
        assert!(short.unrealized_pnl > 0.0, "Short should profit when price falls");
    }

    #[test]
    fn totals_sum_unrealized_pnl() {
        let mut panel = PositionsPanel::new();
        panel.set_positions(vec![
            PositionRow::new("BTC", Action::LONG, 30_000.0, 31_000.0, 1000.0, 3),
            PositionRow::new("ETH", Action::SHORT, 2_000.0, 1_950.0, 800.0, 2),
        ]);
        let total: f64 = panel.positions.iter().map(|p| p.unrealized_pnl).sum();
        assert_eq!(panel.total_unrealized(), total);
    }
}
