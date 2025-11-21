use chrono::Utc;
use eframe::egui::{self, RichText};

use crate::ui::theme::Theme;
use crate::wallet::{ChartPoint, ChartSnapshot, ChartView};

/// GUI tab for viewing and saving chart snapshots backed by the shared DB.
pub struct ChartDeckPanel {
    pub charts: Vec<ChartSnapshot>,
    pub status: Option<String>,
    filter_text: String,
    asset_input: String,
    wallet_label_input: String,
    overlays_input: String,
    view: ChartView,
    pending_save: Option<ChartSnapshot>,
    pending_delete: Option<String>,
}

impl ChartDeckPanel {
    pub fn new() -> Self {
        ChartDeckPanel {
            charts: Vec::new(),
            status: None,
            filter_text: String::new(),
            asset_input: "BTCUSDT".to_string(),
            wallet_label_input: "Main wallet".to_string(),
            overlays_input: "RSI, Volume".to_string(),
            view: ChartView::Line,
            pending_save: None,
            pending_delete: None,
        }
    }

    pub fn set_charts(&mut self, charts: Vec<ChartSnapshot>) {
        self.charts = charts;
    }

    pub fn add_or_replace(&mut self, snapshot: ChartSnapshot) {
        if let Some(idx) = self.charts.iter().position(|c| c.id == snapshot.id) {
            self.charts[idx] = snapshot;
        } else {
            self.charts.push(snapshot);
        }
    }

    pub fn take_pending_save(&mut self) -> Option<ChartSnapshot> {
        self.pending_save.take()
    }

    pub fn take_pending_delete(&mut self) -> Option<String> {
        self.pending_delete.take()
    }

    fn build_snapshot(&self, mode: &str) -> ChartSnapshot {
        let overlays = self
            .overlays_input
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        let now = Utc::now().to_rfc3339();
        ChartSnapshot {
            id: format!("chart_{}", Self::timestamp_id()),
            asset: self.asset_input.trim().to_uppercase(),
            wallet_label: Some(self.wallet_label_input.clone()),
            overlays,
            view: self.view.clone(),
            mode: mode.to_string(),
            data: Self::sample_points(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    fn sample_points() -> Vec<ChartPoint> {
        let now = Utc::now().timestamp();
        (0..6)
            .map(|i| ChartPoint {
                timestamp: now - (5 - i) * 60,
                price: 100.0 + (i as f64 * 0.75),
                volume: Some(25.0 + (i as f64 * 5.0)),
            })
            .collect()
    }

    fn timestamp_id() -> String {
        format!("{:x}", Utc::now().timestamp_micros())
    }

    pub fn show(&mut self, ui: &mut egui::Ui, theme: &Theme, mode: &str) {
        ui.horizontal(|ui| {
            ui.label("Asset");
            ui.text_edit_singleline(&mut self.asset_input);
            ui.label("Wallet");
            ui.text_edit_singleline(&mut self.wallet_label_input);
            ui.label("Overlays");
            ui.text_edit_singleline(&mut self.overlays_input);
            ui.label("View");
            ui.radio_value(&mut self.view, ChartView::Line, "Line");
            ui.radio_value(&mut self.view, ChartView::Bar, "Bar");
            if ui
                .button(RichText::new("Save Snapshot").color(theme.green).strong())
                .clicked()
            {
                let snapshot = self.build_snapshot(mode);
                self.pending_save = Some(snapshot);
                self.status = Some("Saving snapshot to shared DB...".to_string());
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Filter");
            ui.text_edit_singleline(&mut self.filter_text);
            ui.label(RichText::new(format!("Mode: {}", mode)).color(theme.yellow));
        });

        ui.separator();
        self.render_table(ui, theme);

        if let Some(status) = &self.status {
            ui.separator();
            ui.label(RichText::new(status).color(theme.blue));
        }
    }

    fn render_table(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        let filter = self.filter_text.to_lowercase();
        egui::Grid::new("chart_deck_grid")
            .striped(true)
            .num_columns(6)
            .show(ui, |ui| {
                ui.label("Asset");
                ui.label("Wallet");
                ui.label("View");
                ui.label("Overlays");
                ui.label("Updated");
                ui.label("");
                ui.end_row();

                for chart in self.charts.iter() {
                    if !filter.is_empty() {
                        let haystack = format!(
                            "{} {} {}",
                            chart.asset,
                            chart.wallet_label.clone().unwrap_or_default(),
                            chart.mode
                        )
                        .to_lowercase();
                        if !haystack.contains(&filter) {
                            continue;
                        }
                    }
                    ui.label(RichText::new(&chart.asset).strong());
                    ui.label(chart.wallet_label.clone().unwrap_or_else(|| "-".to_string()));
                    let view_label = match chart.view {
                        ChartView::Line => "Line",
                        ChartView::Bar => "Bar",
                    };
                    ui.label(view_label);
                    ui.label(chart.overlays.join(", "));
                    ui.label(&chart.updated_at);
                    if ui
                        .button(RichText::new("Delete").color(theme.red).strong())
                        .clicked()
                    {
                        self.pending_delete = Some(chart.id.clone());
                    }
                    ui.end_row();
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_snapshot_includes_overlays_and_mode() {
        let mut panel = ChartDeckPanel::new();
        panel.asset_input = "ethusdt".to_string();
        panel.overlays_input = "RSI, Heatmap".to_string();
        let snap = panel.build_snapshot("paper");
        assert_eq!(snap.asset, "ETHUSDT");
        assert_eq!(snap.mode, "paper");
        assert_eq!(snap.overlays.len(), 2);
    }
}
