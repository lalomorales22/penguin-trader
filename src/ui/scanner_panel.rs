use eframe::egui;
use egui_extras::{Column, TableBuilder};
use serde::Deserialize;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use crate::ui::theme::Theme;

// --- Data Structures ---

#[derive(Debug, Deserialize, Clone)]
struct CoinData {
    id: String,
    symbol: String,
    current_price: f64,
    market_cap: Option<f64>,
    total_volume: Option<f64>,
    price_change_percentage_24h: Option<f64>,
    #[serde(default)]
    sparkline_in_7d: Sparkline,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct Sparkline {
    price: Vec<f64>,
}

#[derive(Clone)]
pub struct CoinMetrics {
    pub id: String,
    pub symbol: String,
    pub price: f64,
    pub change_24h: f64,
    pub market_cap: f64,
    pub volume: f64,
    pub vol_mcap_ratio: f64, // Smarter metric: Volume / Market Cap
    pub rsi_1h: f64,
    pub rsi_6h: f64,
    pub rsi_24h: f64,
    pub price_history: Vec<f64>, 
    pub sma_24h: f64,
    pub is_stablecoin: bool,
}

pub enum ScannerMessage {
    FullSnapshot(Vec<CoinMetrics>),
    StatusUpdate(String),
    Error(String),
    AddCoin(String),
}

// --- Math Logic ---

fn calculate_rsi(prices: &[f64], period: usize) -> f64 {
    if prices.len() < period + 1 { return 50.0; }
    let mut gains = 0.0;
    let mut losses = 0.0;
    for i in 1..=period {
        let change = prices[prices.len() - 1 - (period - i)] - prices[prices.len() - 1 - (period - i + 1)];
        if change > 0.0 { gains += change; } else { losses -= change; }
    }
    let avg_gain = gains / period as f64;
    let avg_loss = losses / period as f64;
    if avg_loss == 0.0 { return 100.0; }
    let rs = avg_gain / avg_loss;
    100.0 - (100.0 / (1.0 + rs))
}

fn calculate_sma(prices: &[f64], period: usize) -> f64 {
    if prices.len() < period { return 0.0; }
    let sum: f64 = prices.iter().rev().take(period).sum();
    sum / period as f64
}

fn resample_prices(prices: &[f64], interval: usize) -> Vec<f64> {
    prices.iter().step_by(interval).cloned().collect()
}

fn format_large_number(n: f64) -> String {
    if n >= 1_000_000_000.0 {
        format!("${:.2}B", n / 1_000_000_000.0)
    } else if n >= 1_000_000.0 {
        format!("${:.2}M", n / 1_000_000.0)
    } else if n >= 1_000.0 {
        format!("${:.2}K", n / 1_000.0)
    } else {
        format!("${:.2}", n)
    }
}

// --- Networking ---

async fn fetch_coins(ids: &[String]) -> Result<Vec<CoinMetrics>, Box<dyn std::error::Error>> {
    let id_str = ids.join(",");
    let url = format!("https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&ids={}&order=market_cap_desc&per_page=250&page=1&sparkline=true", id_str);
    
    let client = reqwest::Client::builder().user_agent("trader-watcher-v2").build()?;
    let resp = client.get(url).send().await?;
    let data: Vec<CoinData> = resp.json().await?;

    let metrics: Vec<CoinMetrics> = data.into_iter().map(|c| {
        let prices = &c.sparkline_in_7d.price;
        
        let rsi_1h = calculate_rsi(prices, 14);
        let prices_6h = resample_prices(prices, 6);
        let rsi_6h = calculate_rsi(&prices_6h, 14);
        let prices_24h = resample_prices(prices, 24);
        let rsi_24h = calculate_rsi(&prices_24h, 5);
        let sma_24h = calculate_sma(prices, 24);
        
        let vol = c.total_volume.unwrap_or(0.0);
        let cap = c.market_cap.unwrap_or(1.0); // avoid div by 0

        let is_stable = ["tether", "usd-coin", "dai"].contains(&c.id.as_str());

        CoinMetrics {
            id: c.id,
            symbol: c.symbol.to_uppercase(),
            price: c.current_price,
            change_24h: c.price_change_percentage_24h.unwrap_or(0.0),
            market_cap: cap,
            volume: vol,
            vol_mcap_ratio: vol / cap,
            rsi_1h,
            rsi_6h,
            rsi_24h,
            price_history: prices.clone(),
            sma_24h,
            is_stablecoin: is_stable,
        }
    }).collect();

    Ok(metrics)
}

// --- UI Helpers ---

fn draw_sparkline(ui: &mut egui::Ui, prices: &[f64], color: egui::Color32) {
    let (response, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::hover());
    let rect = response.rect;
    if prices.is_empty() { return; }
    let min_price = prices.iter().fold(f64::INFINITY, |a, &b| a.min(b));
    let max_price = prices.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
    let range = max_price - min_price;
    let points: Vec<egui::Pos2> = prices.iter().enumerate().map(|(i, &p)| {
        let x = rect.min.x + (i as f32 / (prices.len() - 1) as f32) * rect.width();
        let y = rect.max.y - ((p - min_price) as f32 / range as f32) * rect.height();
        egui::Pos2::new(x, y)
    }).collect();
    painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, color)));
}

// --- Panel ---

pub struct ScannerPanel {
    pub coins: Vec<CoinMetrics>,
    pub filtered_coins: Vec<CoinMetrics>,
    
    // Search/Filter State
    pub search_query: String,
    pub hide_stablecoins: bool,
    pub show_snipe: bool,
    pub show_overbought: bool,
    
    // New Filters
    pub min_rsi: f64,
    pub max_rsi: f64,
    pub min_vol_mcap: f64,

    // Add Token State
    pub new_token_input: String,
    pub tx_cmd: Sender<ScannerMessage>,

    pub last_refreshed: Instant,
    pub status_message: String,
    rx: Receiver<ScannerMessage>,
}

impl ScannerPanel {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let (tx_cmd, rx_cmd) = std::sync::mpsc::channel(); // Command channel to background task

        // Spawn fetch task
        tokio::spawn(async move {
            let mut ids: Vec<String> = vec![
                "bitcoin", "ethereum", "solana", "ripple", "binancecoin", "cardano", "tether", 
                "usd-coin", "dai", "fetch-ai", "render-token", "bittensor", "near", 
                "internet-computer", "the-graph", "singularitynet", "ocean-protocol", 
                "akash-network", "injective-protocol", "phantom", "chainlink", "polkadot", 
                "avalanche", "pepe", "dogecoin", "shiba-inu", "sui", "aptos"
            ].into_iter().map(String::from).collect();

            // Initial fetch
            let _ = tx.send(ScannerMessage::StatusUpdate("Fetching coin data (CoinGecko)...".to_string()));
            if let Ok(coins) = fetch_coins(&ids).await {
                let _ = tx.send(ScannerMessage::FullSnapshot(coins));
                let _ = tx.send(ScannerMessage::StatusUpdate("Data loaded (CoinGecko)".to_string()));
            } else {
                 let _ = tx.send(ScannerMessage::Error("Failed to fetch initial data from CoinGecko".to_string()));
            }

            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let _ = tx.send(ScannerMessage::StatusUpdate("Refreshing data (CoinGecko)...".to_string()));
                        match fetch_coins(&ids).await {
                            Ok(coins) => {
                                let _ = tx.send(ScannerMessage::FullSnapshot(coins));
                                 let _ = tx.send(ScannerMessage::StatusUpdate("Data refreshed (CoinGecko)".to_string()));
                            },
                            Err(e) => {
                                let _ = tx.send(ScannerMessage::Error(format!("API Error: {}", e)));
                            }
                        }
                    }
                    // We need to check for commands. std::sync::mpsc::Receiver is blocking and not async-aware.
                    // However, since we are in a tokio task, we should ideally use tokio::sync::mpsc.
                    // BUT, the UI uses std::sync::mpsc for its receiver (rx).
                    // The command channel (rx_cmd) is consumed INSIDE this async task.
                    // So rx_cmd should be a tokio::sync::mpsc::Receiver if we want to await it in select!.
                    // Let's assume we switch to tokio channel for commands.
                }
                
                // Hack for std::mpsc inside async loop: check non-blocking
                while let Ok(cmd) = rx_cmd.try_recv() {
                    match cmd {
                        ScannerMessage::AddCoin(id) => {
                            if !ids.contains(&id) {
                                ids.push(id.clone());
                                let _ = tx.send(ScannerMessage::StatusUpdate(format!("Adding {}...", id)));
                                // Force immediate refresh
                                match fetch_coins(&ids).await {
                                    Ok(coins) => {
                                        let _ = tx.send(ScannerMessage::FullSnapshot(coins));
                                        let _ = tx.send(ScannerMessage::StatusUpdate(format!("Added {}", id)));
                                    },
                                    Err(e) => {
                                        let _ = tx.send(ScannerMessage::Error(format!("Failed to add coin: {}", e)));
                                        // Revert if failed (optional, but cleaner)
                                        ids.pop(); 
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        });

        Self {
            coins: Vec::new(),
            filtered_coins: Vec::new(),
            search_query: String::new(),
            hide_stablecoins: false,
            show_snipe: false,
            show_overbought: false,
            min_rsi: 0.0,
            max_rsi: 100.0,
            min_vol_mcap: 0.0,
            new_token_input: String::new(),
            tx_cmd,
            last_refreshed: Instant::now(),
            status_message: "Initializing CoinGecko feed...".to_string(),
            rx,
        }
    }

    // kept for compatibility with dashboard.rs tick(), though it's mostly handled by rx now
    pub fn refresh(&mut self) {
       self.process_messages();
    }

    fn update_filters(&mut self) {
        self.filtered_coins = self.coins.iter().filter(|c| {
            let matches_search = self.search_query.is_empty() || c.symbol.contains(&self.search_query.to_uppercase());
            let matches_stable = !self.hide_stablecoins || !c.is_stablecoin;
            
            let is_snipe = c.rsi_1h < 30.0 && c.vol_mcap_ratio > 0.1;
            let is_overbought = c.rsi_1h > 75.0;

            let matches_snipe = !self.show_snipe || is_snipe;
            let matches_overbought = !self.show_overbought || is_overbought;

            let matches_rsi = c.rsi_1h >= self.min_rsi && c.rsi_1h <= self.max_rsi;
            let matches_vol = c.vol_mcap_ratio >= self.min_vol_mcap;

            matches_search && matches_stable && matches_snipe && matches_overbought && matches_rsi && matches_vol
        }).cloned().collect();
    }

    pub fn process_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                ScannerMessage::FullSnapshot(new_coins) => {
                    self.coins = new_coins;
                    self.last_refreshed = Instant::now();
                }
                ScannerMessage::StatusUpdate(status) => {
                    self.status_message = status;
                }
                ScannerMessage::Error(err) => {
                    self.status_message = format!("Error: {}", err);
                }
                _ => {}
            }
            self.update_filters();
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, _theme: &Theme) {
        self.process_messages();

        ui.heading("AI Market Scanner (Powered by CoinGecko)");
        
        ui.collapsing("Filters & Settings", |ui| {
            ui.horizontal(|ui| {
                ui.label("Search:");
                if ui.text_edit_singleline(&mut self.search_query).changed() { self.update_filters(); }
                ui.separator();
                
                // Add Token
                ui.label("Add Token (ID):");
                let response = ui.text_edit_singleline(&mut self.new_token_input);
                if ui.button("Add").clicked() || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    if !self.new_token_input.is_empty() {
                        let _ = self.tx_cmd.send(ScannerMessage::AddCoin(self.new_token_input.clone()));
                        self.new_token_input.clear();
                    }
                }
            });
            
            ui.separator();
            
            ui.horizontal(|ui| {
                if ui.checkbox(&mut self.hide_stablecoins, "Hide Stablecoins").changed() { self.update_filters(); }
                if ui.checkbox(&mut self.show_snipe, "🔥 Show SNIPE Only").changed() { self.update_filters(); }
                if ui.checkbox(&mut self.show_overbought, "⚠️ Show OVERBOUGHT Only").changed() { self.update_filters(); }
            });

            ui.separator();

            // Sliders
            ui.horizontal(|ui| {
                ui.label("RSI Range:");
                if ui.add(egui::Slider::new(&mut self.min_rsi, 0.0..=100.0).text("Min")).changed() { self.update_filters(); }
                if ui.add(egui::Slider::new(&mut self.max_rsi, 0.0..=100.0).text("Max")).changed() { self.update_filters(); }
            });
            ui.horizontal(|ui| {
                ui.label("Min Vol/Cap:");
                if ui.add(egui::Slider::new(&mut self.min_vol_mcap, 0.0..=1.0).text("Ratio")).changed() { self.update_filters(); }
            });
        });
        
        ui.separator();

        // Updated time
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&self.status_message).color(egui::Color32::LIGHT_BLUE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
               ui.label(format!("Updated: {}s ago", self.last_refreshed.elapsed().as_secs())); 
            });
        });

        if self.filtered_coins.is_empty() {
             ui.add_space(20.0);
             ui.label("No coins found matching filters. Waiting for data...");
        } else {
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(60.0)) // Symbol
                .column(Column::initial(80.0)) // Sparkline
                .column(Column::initial(80.0)) // Price
                .column(Column::initial(80.0)) // Market Cap
                .column(Column::initial(80.0)) // Volume
                .column(Column::initial(60.0)) // RSI 1h
                .column(Column::initial(60.0)) // Vol/Cap
                .column(Column::remainder())   // Signal
                .header(20.0, |mut header| {
                    header.col(|ui| { ui.strong("Symbol"); });
                    header.col(|ui| { ui.strong("Trend (7d)"); });
                    header.col(|ui| { ui.strong("Price"); });
                    header.col(|ui| { ui.strong("Mkt Cap"); });
                    header.col(|ui| { ui.strong("24h Vol"); });
                    header.col(|ui| { ui.strong("RSI (1h)"); });
                    header.col(|ui| { ui.strong("Vol/Cap"); });
                    header.col(|ui| { ui.strong("Smart Signal"); });
                })
                .body(|mut body| {
                    for coin in &self.filtered_coins {
                        body.row(30.0, |mut row| {
                            row.col(|ui| { 
                                ui.strong(&coin.symbol); 
                                if coin.is_stablecoin { ui.label("💵"); }
                            });
                            
                            row.col(|ui| {
                                let color = if coin.change_24h >= 0.0 { egui::Color32::GREEN } else { egui::Color32::RED };
                                draw_sparkline(ui, &coin.price_history, color);
                            });

                            row.col(|ui| { 
                                let price_text = if coin.price < 1.0 { format!("${:.6}", coin.price) } else { format!("${:.2}", coin.price) };
                                
                                // Stablecoin Peg Alert
                                if coin.is_stablecoin && (coin.price < 0.99 || coin.price > 1.01) {
                                    ui.colored_label(egui::Color32::RED, format!("⚠ {}", price_text));
                                } else {
                                    ui.label(price_text); 
                                }
                            });

                            row.col(|ui| {
                                ui.label(format_large_number(coin.market_cap));
                            });

                            row.col(|ui| {
                                ui.label(format_large_number(coin.volume));
                            });

                            row.col(|ui| {
                                let color = if coin.rsi_1h < 30.0 { egui::Color32::GREEN } else if coin.rsi_1h > 70.0 { egui::Color32::RED } else { egui::Color32::GRAY };
                                ui.colored_label(color, format!("{:.0}", coin.rsi_1h));
                            });

                            row.col(|ui| {
                                // Vol/Cap > 0.1 usually implies high activity
                                let color = if coin.vol_mcap_ratio > 0.15 { egui::Color32::YELLOW } else { egui::Color32::GRAY };
                                ui.colored_label(color, format!("{:.2}", coin.vol_mcap_ratio));
                            });

                            row.col(|ui| {
                                if coin.is_stablecoin {
                                    if coin.price < 0.99 { ui.colored_label(egui::Color32::RED, "DE-PEGGED"); }
                                    else { ui.label("Stable"); }
                                } else {
                                    if coin.rsi_1h < 30.0 && coin.vol_mcap_ratio > 0.1 {
                                        ui.colored_label(egui::Color32::GREEN, "★ SNIPE");
                                    } else if coin.rsi_1h > 75.0 {
                                        ui.colored_label(egui::Color32::RED, "OVERBOUGHT");
                                    } else {
                                        ui.label("-");
                                    }
                                }
                            });
                        });
                    }
                });
        }
    }
}
