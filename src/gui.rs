use eframe::egui;
use penguin_trader::ui::DashboardApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        centered: true,
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 860.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Penguin Trader GUI",
        options,
        Box::new(|cc| Box::new(DashboardApp::new(cc))),
    )
}
