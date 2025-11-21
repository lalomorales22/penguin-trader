use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};

use crate::strategy::{Action, Conviction, MarketSignal};
use crate::ui::theme::Theme;

/// Displays market scanner output (Burry + Penguin scores).
pub struct ScannerPanel {
    pub signals: Vec<MarketSignal>,
    pub last_refreshed: Instant,
}

impl ScannerPanel {
    pub fn new() -> Self {
        let mut panel = ScannerPanel {
            signals: Vec::new(),
            last_refreshed: Instant::now() - Duration::from_secs(10),
        };
        panel.refresh();
        panel
    }

    pub fn refresh(&mut self) {
        self.signals = Self::mock_signals();
        self.last_refreshed = Instant::now();
    }

    pub fn show(&self, ui: &mut egui::Ui, theme: &Theme) {
        let elapsed = self.last_refreshed.elapsed().as_secs();
        ui.horizontal(|ui| {
            ui.label(RichText::new("AI Market Scanner").color(theme.green).strong());
            ui.label(format!("refreshed {}s ago", elapsed));
        });

        egui::Grid::new("scanner_grid")
            .striped(true)
            .num_columns(6)
            .show(ui, |ui| {
                ui.label(RichText::new("Symbol").color(theme.text));
                ui.label(RichText::new("Action").color(theme.text));
                ui.label(RichText::new("Conviction").color(theme.text));
                ui.label(RichText::new("Burry").color(theme.red));
                ui.label(RichText::new("Penguin").color(theme.green));
                ui.label(RichText::new("Reasoning").color(theme.text));
                ui.end_row();

                for signal in &self.signals {
                    ui.label(RichText::new(&signal.symbol).strong().color(theme.text));
                    let (text, color) = Self::action_style(signal.action, theme);
                    ui.label(RichText::new(text).color(color).strong());
                    ui.label(format!("{:?}", signal.conviction));
                    ui.colored_label(theme.red, format!("{}/5", signal.burry_score));
                    ui.colored_label(theme.green, format!("{}/5", signal.penguin_score));
                    ui.label(RichText::new(signal.reasoning.clone()).color(theme.text));
                    ui.end_row();
                }
            });
    }

    fn action_style(action: Action, theme: &Theme) -> (String, eframe::egui::Color32) {
        match action {
            Action::LONG => ("LONG".to_string(), theme.green),
            Action::SHORT => ("SHORT".to_string(), theme.red),
            Action::HOLD => ("HOLD".to_string(), theme.yellow),
        }
    }

    fn mock_signals() -> Vec<MarketSignal> {
        let coins = vec![
            ("BTCUSDT", 67500.0, -1.7, 1, 4),
            ("ETHUSDT", 3720.0, 3.2, 0, 3),
            ("SOLUSDT", 162.5, 5.5, 0, 4),
            ("XRPUSDT", 0.58, -0.4, 2, 2),
            ("ADAUSDT", 0.82, 9.1, 3, 1),
            ("LINKUSDT", 19.4, -6.3, 4, 0),
        ];

        coins
            .into_iter()
            .map(|(symbol, price, change, burry, penguin)| {
                let mut signal = MarketSignal::new(symbol.to_string());
                signal.current_price = price;
                signal.btc_price = 67000.0;
                signal.btc_change_24h = -1.1;
                signal.burry_score = burry;
                signal.penguin_score = penguin;
                signal.action = if burry >= penguin && burry >= 3 {
                    Action::SHORT
                } else if penguin >= 3 {
                    Action::LONG
                } else {
                    Action::HOLD
                };
                signal.conviction = match signal.action {
                    Action::LONG | Action::SHORT => {
                        if burry.max(penguin) >= 4 {
                            Conviction::Textbook
                        } else if burry.max(penguin) == 3 {
                            Conviction::Solid
                        } else if burry.max(penguin) == 2 {
                            Conviction::Watch
                        } else {
                            Conviction::No
                        }
                    }
                    Action::HOLD => Conviction::No,
                };
                signal.reasoning = if signal.action == Action::SHORT {
                    format!("Bearish exhaustion forming ({:.1}% change)", change)
                } else if signal.action == Action::LONG {
                    format!("BTC/alt divergence spotted ({:.1}% change)", change)
                } else {
                    "No strong setup".to_string()
                };
                signal.risk_level = if signal.conviction >= Conviction::Solid {
                    "medium".to_string()
                } else {
                    "low".to_string()
                };
                signal
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_updates_timestamp_and_data() {
        let mut panel = ScannerPanel::new();
        let first_update = panel.last_refreshed;
        std::thread::sleep(Duration::from_millis(5));
        panel.refresh();
        assert!(panel.last_refreshed > first_update);
        assert_eq!(panel.signals.len(), 6);
    }

    #[test]
    fn action_labels_use_theme_colors() {
        let theme = Theme::new();
        let (label, color) = ScannerPanel::action_style(Action::LONG, &theme);
        assert_eq!(label, "LONG");
        assert_eq!(color, theme.green);
    }
}
