use eframe::egui;
use egui_extras::{Column, TableBuilder};
use serde::Deserialize;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use crate::ui::theme::Theme;

// --- DexScreener API Data Structures ---

#[derive(Debug, Deserialize, Clone)]
struct PairData {
    #[serde(rename = "chainId")]
    chain_id: String,
    #[serde(rename = "dexId")]
    dex_id: String,
    #[allow(dead_code)]
    url: String,
    #[serde(rename = "pairAddress")]
    pair_address: String,
    #[serde(rename = "baseToken")]
    base_token: TokenInfo,
    #[allow(dead_code)]
    #[serde(rename = "quoteToken")]
    quote_token: TokenInfo,
    #[allow(dead_code)]
    #[serde(rename = "priceNative")]
    price_native: Option<String>,
    #[serde(rename = "priceUsd")]
    price_usd: Option<String>,
    txns: Option<TxnData>,
    volume: Option<VolumeData>,
    #[serde(rename = "priceChange")]
    price_change: Option<PriceChange>,
    liquidity: Option<LiquidityData>,
    #[serde(rename = "fdv")]
    fdv: Option<f64>,
    #[serde(rename = "pairCreatedAt")]
    pair_created_at: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
struct TokenInfo {
    address: String,
    name: String,
    symbol: String,
}

#[derive(Debug, Deserialize, Clone)]
struct TxnData {
    #[allow(dead_code)]
    #[serde(default)]
    m5: Option<TxnCounts>,
    #[serde(default)]
    h1: Option<TxnCounts>,
    #[allow(dead_code)]
    #[serde(default)]
    h6: Option<TxnCounts>,
    #[allow(dead_code)]
    #[serde(default)]
    h24: Option<TxnCounts>,
}

#[derive(Debug, Deserialize, Clone)]
struct TxnCounts {
    buys: u64,
    sells: u64,
}

#[derive(Debug, Deserialize, Clone)]
struct VolumeData {
    #[serde(default)]
    h24: f64,
    #[allow(dead_code)]
    #[serde(default)]
    h6: f64,
    #[allow(dead_code)]
    #[serde(default)]
    h1: f64,
    #[allow(dead_code)]
    #[serde(default)]
    m5: f64,
}

#[derive(Debug, Deserialize, Clone)]
struct PriceChange {
    #[allow(dead_code)]
    #[serde(default)]
    m5: f64,
    #[serde(default)]
    h1: f64,
    #[serde(default)]
    h6: f64,
    #[serde(default)]
    h24: f64,
}

#[derive(Debug, Deserialize, Clone)]
struct LiquidityData {
    #[serde(default)]
    usd: f64,
    #[allow(dead_code)]
    #[serde(default)]
    base: f64,
    #[allow(dead_code)]
    #[serde(default)]
    quote: f64,
}

// Response wrapper
#[derive(Debug, Deserialize)]
struct PairsResponse {
    #[serde(default)]
    pairs: Vec<PairData>,
}

// --- Internal Token Metrics ---

#[derive(Clone)]
pub struct DexTokenMetrics {
    pub chain: String,
    pub symbol: String,
    pub name: String,
    pub address: String,
    pub pair_address: String,
    pub price_usd: f64,
    pub price_change_1h: f64,
    pub price_change_6h: f64,
    pub price_change_24h: f64,
    pub volume_24h: f64,
    pub liquidity_usd: f64,
    pub fdv: f64,
    pub txn_count_1h: u64,
    pub buy_sell_ratio_1h: f64,
    pub age_hours: f64,
    pub dex: String,
}

pub enum DexScreenerMessage {
    FullSnapshot(Vec<DexTokenMetrics>),
    StatusUpdate(String),
    Error(String),
    SearchToken(String),
}

// --- Math & Formatting ---

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

fn format_price(price: f64) -> String {
    if price < 0.000001 {
        format!("${:.10}", price)
    } else if price < 0.01 {
        format!("${:.8}", price)
    } else if price < 1.0 {
        format!("${:.6}", price)
    } else {
        format!("${:.2}", price)
    }
}

// --- DexScreener API Calls ---

async fn fetch_latest_pairs() -> Result<Vec<PairData>, Box<dyn std::error::Error>> {
    let url = "https://api.dexscreener.com/latest/dex/pairs";
    let client = reqwest::Client::builder()
        .user_agent("penguin-trader-v1")
        .build()?;
    let resp = client.get(url).send().await?;

    if resp.status().is_success() {
        let pairs_resp: PairsResponse = resp.json().await?;
        Ok(pairs_resp.pairs)
    } else {
        Err(format!("API returned status: {}", resp.status()).into())
    }
}

async fn search_pairs(query: &str) -> Result<Vec<PairData>, Box<dyn std::error::Error>> {
    let url = format!("https://api.dexscreener.com/latest/dex/search?q={}", query);
    let client = reqwest::Client::builder()
        .user_agent("penguin-trader-v1")
        .build()?;
    let resp = client.get(&url).send().await?;

    if resp.status().is_success() {
        let pairs_resp: PairsResponse = resp.json().await?;
        Ok(pairs_resp.pairs)
    } else {
        Err(format!("API returned status: {}", resp.status()).into())
    }
}

fn pair_to_metrics(pair: PairData) -> DexTokenMetrics {
    let price_usd = pair.price_usd
        .and_then(|p| p.parse::<f64>().ok())
        .unwrap_or(0.0);

    let volume_24h = pair.volume.as_ref().map(|v| v.h24).unwrap_or(0.0);
    let liquidity_usd = pair.liquidity.as_ref().map(|l| l.usd).unwrap_or(0.0);

    let price_change_1h = pair.price_change.as_ref().map(|p| p.h1).unwrap_or(0.0);
    let price_change_6h = pair.price_change.as_ref().map(|p| p.h6).unwrap_or(0.0);
    let price_change_24h = pair.price_change.as_ref().map(|p| p.h24).unwrap_or(0.0);

    let txn_1h = pair.txns.as_ref()
        .and_then(|t| t.h1.as_ref())
        .map(|t| t.buys + t.sells)
        .unwrap_or(0);

    let buy_sell_ratio = pair.txns.as_ref()
        .and_then(|t| t.h1.as_ref())
        .map(|t| {
            if t.sells > 0 {
                t.buys as f64 / t.sells as f64
            } else {
                t.buys as f64
            }
        })
        .unwrap_or(0.0);

    let age_hours = pair.pair_created_at.map(|timestamp| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        ((now - timestamp) as f64) / 3600.0
    }).unwrap_or(0.0);

    DexTokenMetrics {
        chain: pair.chain_id,
        symbol: pair.base_token.symbol,
        name: pair.base_token.name,
        address: pair.base_token.address,
        pair_address: pair.pair_address,
        price_usd,
        price_change_1h,
        price_change_6h,
        price_change_24h,
        volume_24h,
        liquidity_usd,
        fdv: pair.fdv.unwrap_or(0.0),
        txn_count_1h: txn_1h,
        buy_sell_ratio_1h: buy_sell_ratio,
        age_hours,
        dex: pair.dex_id,
    }
}

// --- Panel ---

pub struct DexScreenerPanel {
    pub tokens: Vec<DexTokenMetrics>,
    pub filtered_tokens: Vec<DexTokenMetrics>,

    // Filter State
    pub search_query: String,
    pub min_liquidity: f64,
    pub min_volume_24h: f64,
    pub max_age_hours: f64,
    pub show_new_only: bool,
    pub show_trending_only: bool,
    pub show_high_volume_only: bool,
    pub selected_chain: String,

    // Search Token Input
    pub search_token_input: String,
    pub tx_cmd: Sender<DexScreenerMessage>,

    pub last_refreshed: Instant,
    pub status_message: String,
    rx: Receiver<DexScreenerMessage>,
}

impl DexScreenerPanel {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let (tx_cmd, rx_cmd) = std::sync::mpsc::channel();

        // Spawn fetch task
        tokio::spawn(async move {
            // Initial fetch
            let _ = tx.send(DexScreenerMessage::StatusUpdate("Fetching latest pairs (DexScreener)...".to_string()));

            match fetch_latest_pairs().await {
                Ok(pairs) => {
                    let metrics: Vec<DexTokenMetrics> = pairs.into_iter()
                        .take(100) // Limit to first 100 to avoid overwhelming UI
                        .map(pair_to_metrics)
                        .collect();
                    let _ = tx.send(DexScreenerMessage::FullSnapshot(metrics));
                    let _ = tx.send(DexScreenerMessage::StatusUpdate("Data loaded (DexScreener)".to_string()));
                }
                Err(e) => {
                    let _ = tx.send(DexScreenerMessage::Error(format!("Failed to fetch: {}", e)));
                }
            }

            let mut interval = tokio::time::interval(Duration::from_secs(30));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let _ = tx.send(DexScreenerMessage::StatusUpdate("Refreshing data (DexScreener)...".to_string()));
                        match fetch_latest_pairs().await {
                            Ok(pairs) => {
                                let metrics: Vec<DexTokenMetrics> = pairs.into_iter()
                                    .take(100)
                                    .map(pair_to_metrics)
                                    .collect();
                                let _ = tx.send(DexScreenerMessage::FullSnapshot(metrics));
                                let _ = tx.send(DexScreenerMessage::StatusUpdate("Data refreshed (DexScreener)".to_string()));
                            }
                            Err(e) => {
                                let _ = tx.send(DexScreenerMessage::Error(format!("API Error: {}", e)));
                            }
                        }
                    }
                }

                // Check for search commands
                while let Ok(cmd) = rx_cmd.try_recv() {
                    match cmd {
                        DexScreenerMessage::SearchToken(query) => {
                            let _ = tx.send(DexScreenerMessage::StatusUpdate(format!("Searching for {}...", query)));
                            match search_pairs(&query).await {
                                Ok(pairs) => {
                                    let metrics: Vec<DexTokenMetrics> = pairs.into_iter()
                                        .take(50)
                                        .map(pair_to_metrics)
                                        .collect();
                                    let count = metrics.len();
                                    let _ = tx.send(DexScreenerMessage::FullSnapshot(metrics));
                                    let _ = tx.send(DexScreenerMessage::StatusUpdate(format!("Found {} results for {}", count, query)));
                                }
                                Err(e) => {
                                    let _ = tx.send(DexScreenerMessage::Error(format!("Search failed: {}", e)));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        });

        Self {
            tokens: Vec::new(),
            filtered_tokens: Vec::new(),
            search_query: String::new(),
            min_liquidity: 0.0,
            min_volume_24h: 0.0,
            max_age_hours: 999999.0,
            show_new_only: false,
            show_trending_only: false,
            show_high_volume_only: false,
            selected_chain: "all".to_string(),
            search_token_input: String::new(),
            tx_cmd,
            last_refreshed: Instant::now(),
            status_message: "Initializing DexScreener feed...".to_string(),
            rx,
        }
    }

    pub fn refresh(&mut self) {
        self.process_messages();
    }

    fn update_filters(&mut self) {
        self.filtered_tokens = self.tokens.iter().filter(|t| {
            let matches_search = self.search_query.is_empty()
                || t.symbol.to_lowercase().contains(&self.search_query.to_lowercase())
                || t.name.to_lowercase().contains(&self.search_query.to_lowercase());

            let matches_chain = self.selected_chain == "all"
                || t.chain.to_lowercase() == self.selected_chain.to_lowercase();

            let matches_liquidity = t.liquidity_usd >= self.min_liquidity;
            let matches_volume = t.volume_24h >= self.min_volume_24h;
            let matches_age = t.age_hours <= self.max_age_hours;

            // Signal filters
            let is_new = t.age_hours < 24.0;
            let is_trending = t.price_change_1h > 10.0 && t.volume_24h > 50000.0;
            let is_high_volume = t.volume_24h > 100000.0;

            let matches_new = !self.show_new_only || is_new;
            let matches_trending = !self.show_trending_only || is_trending;
            let matches_high_vol = !self.show_high_volume_only || is_high_volume;

            matches_search && matches_chain && matches_liquidity && matches_volume
                && matches_age && matches_new && matches_trending && matches_high_vol
        }).cloned().collect();
    }

    pub fn process_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                DexScreenerMessage::FullSnapshot(new_tokens) => {
                    self.tokens = new_tokens;
                    self.last_refreshed = Instant::now();
                }
                DexScreenerMessage::StatusUpdate(status) => {
                    self.status_message = status;
                }
                DexScreenerMessage::Error(err) => {
                    self.status_message = format!("Error: {}", err);
                }
                _ => {}
            }
            self.update_filters();
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, _theme: &Theme) {
        self.process_messages();

        ui.heading("🔍 DexScreener - Live Token Scout");

        ui.collapsing("Filters & Search", |ui| {
            ui.horizontal(|ui| {
                ui.label("Search Token:");
                if ui.text_edit_singleline(&mut self.search_query).changed() {
                    self.update_filters();
                }
                ui.separator();

                // Deep search
                ui.label("API Search:");
                let response = ui.text_edit_singleline(&mut self.search_token_input);
                if ui.button("🔎 Search").clicked() || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    if !self.search_token_input.is_empty() {
                        let _ = self.tx_cmd.send(DexScreenerMessage::SearchToken(self.search_token_input.clone()));
                        self.search_token_input.clear();
                    }
                }
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Chain:");
                egui::ComboBox::from_id_source("chain_filter")
                    .selected_text(&self.selected_chain)
                    .show_ui(ui, |ui| {
                        let chains = vec!["all", "ethereum", "solana", "bsc", "polygon", "avalanche", "arbitrum", "optimism", "base"];
                        for chain in chains {
                            if ui.selectable_value(&mut self.selected_chain, chain.to_string(), chain).clicked() {
                                self.update_filters();
                            }
                        }
                    });

                ui.separator();

                if ui.checkbox(&mut self.show_new_only, "🆕 New (< 24h)").changed() { self.update_filters(); }
                if ui.checkbox(&mut self.show_trending_only, "🔥 Trending").changed() { self.update_filters(); }
                if ui.checkbox(&mut self.show_high_volume_only, "📊 High Volume").changed() { self.update_filters(); }
            });

            ui.separator();

            // Sliders
            ui.horizontal(|ui| {
                ui.label("Min Liquidity:");
                if ui.add(egui::Slider::new(&mut self.min_liquidity, 0.0..=1_000_000.0)
                    .logarithmic(true)
                    .text("USD")).changed() {
                    self.update_filters();
                }
            });

            ui.horizontal(|ui| {
                ui.label("Min 24h Volume:");
                if ui.add(egui::Slider::new(&mut self.min_volume_24h, 0.0..=10_000_000.0)
                    .logarithmic(true)
                    .text("USD")).changed() {
                    self.update_filters();
                }
            });

            ui.horizontal(|ui| {
                ui.label("Max Age:");
                if ui.add(egui::Slider::new(&mut self.max_age_hours, 1.0..=720.0)
                    .text("hours")).changed() {
                    self.update_filters();
                }
            });
        });

        ui.separator();

        // Status and updated time
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&self.status_message).color(egui::Color32::LIGHT_BLUE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
               ui.label(format!("Updated: {}s ago | {} tokens",
                   self.last_refreshed.elapsed().as_secs(),
                   self.filtered_tokens.len()));
            });
        });

        if self.filtered_tokens.is_empty() {
             ui.add_space(20.0);
             ui.label("No tokens found matching filters. Waiting for data or try adjusting filters...");
        } else {
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(80.0))  // Chain
                .column(Column::initial(80.0))  // Symbol
                .column(Column::initial(100.0)) // Price
                .column(Column::initial(80.0))  // 1h %
                .column(Column::initial(80.0))  // 24h %
                .column(Column::initial(100.0)) // Volume
                .column(Column::initial(100.0)) // Liquidity
                .column(Column::initial(80.0))  // Age
                .column(Column::initial(80.0))  // Txns 1h
                .column(Column::remainder())     // Signal
                .header(20.0, |mut header| {
                    header.col(|ui| { ui.strong("Chain"); });
                    header.col(|ui| { ui.strong("Symbol"); });
                    header.col(|ui| { ui.strong("Price"); });
                    header.col(|ui| { ui.strong("1h %"); });
                    header.col(|ui| { ui.strong("24h %"); });
                    header.col(|ui| { ui.strong("24h Vol"); });
                    header.col(|ui| { ui.strong("Liquidity"); });
                    header.col(|ui| { ui.strong("Age"); });
                    header.col(|ui| { ui.strong("Txns 1h"); });
                    header.col(|ui| { ui.strong("Signal"); });
                })
                .body(|mut body| {
                    for token in &self.filtered_tokens {
                        body.row(30.0, |mut row| {
                            row.col(|ui| {
                                ui.label(token.chain.to_uppercase());
                            });

                            row.col(|ui| {
                                ui.strong(&token.symbol);
                            });

                            row.col(|ui| {
                                ui.label(format_price(token.price_usd));
                            });

                            row.col(|ui| {
                                let color = if token.price_change_1h > 0.0 {
                                    egui::Color32::GREEN
                                } else {
                                    egui::Color32::RED
                                };
                                ui.colored_label(color, format!("{:+.1}%", token.price_change_1h));
                            });

                            row.col(|ui| {
                                let color = if token.price_change_24h > 0.0 {
                                    egui::Color32::GREEN
                                } else {
                                    egui::Color32::RED
                                };
                                ui.colored_label(color, format!("{:+.1}%", token.price_change_24h));
                            });

                            row.col(|ui| {
                                ui.label(format_large_number(token.volume_24h));
                            });

                            row.col(|ui| {
                                let color = if token.liquidity_usd > 100000.0 {
                                    egui::Color32::GREEN
                                } else if token.liquidity_usd > 10000.0 {
                                    egui::Color32::YELLOW
                                } else {
                                    egui::Color32::RED
                                };
                                ui.colored_label(color, format_large_number(token.liquidity_usd));
                            });

                            row.col(|ui| {
                                let age_text = if token.age_hours < 1.0 {
                                    format!("{:.0}m", token.age_hours * 60.0)
                                } else if token.age_hours < 24.0 {
                                    format!("{:.1}h", token.age_hours)
                                } else {
                                    format!("{:.1}d", token.age_hours / 24.0)
                                };

                                let color = if token.age_hours < 1.0 {
                                    egui::Color32::LIGHT_GREEN
                                } else if token.age_hours < 24.0 {
                                    egui::Color32::YELLOW
                                } else {
                                    egui::Color32::GRAY
                                };

                                ui.colored_label(color, age_text);
                            });

                            row.col(|ui| {
                                ui.label(format!("{}", token.txn_count_1h));
                            });

                            row.col(|ui| {
                                // Signal detection
                                if token.age_hours < 1.0 && token.liquidity_usd > 10000.0 {
                                    ui.colored_label(egui::Color32::LIGHT_GREEN, "🆕 FRESH");
                                } else if token.price_change_1h > 20.0 && token.volume_24h > 50000.0 {
                                    ui.colored_label(egui::Color32::YELLOW, "🔥 TRENDING");
                                } else if token.volume_24h > 1_000_000.0 && token.liquidity_usd > 100_000.0 {
                                    ui.colored_label(egui::Color32::GREEN, "💎 RELIABLE");
                                } else if token.liquidity_usd < 5000.0 {
                                    ui.colored_label(egui::Color32::RED, "⚠️ LOW LIQ");
                                } else if token.buy_sell_ratio_1h > 2.0 {
                                    ui.colored_label(egui::Color32::GREEN, "📈 BUYING");
                                } else if token.buy_sell_ratio_1h < 0.5 && token.buy_sell_ratio_1h > 0.0 {
                                    ui.colored_label(egui::Color32::RED, "📉 SELLING");
                                } else {
                                    ui.label("-");
                                }
                            });
                        });
                    }
                });
        }
    }
}
