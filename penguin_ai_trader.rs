// Penguin 2026 AI-Powered Paper Trading Engine
// Single-file Rust CLI app with Claude AI integration
// Supports multi-coin shorting/longing with AI signal detection

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{self, Write};
use std::process::Command;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use colored::*;
use hex;
use penguin_trader::wallet::{
    decrypt, encrypt, fetch_balance, generate_wallet, send_transaction, estimate_fee, Network, Wallet, WalletStorage, EncryptedData, TransactionStatus, check_transaction_status,
};
use reqwest::blocking::Client;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const ANTHROPIC_KEY_PATH: &str = "anthropic_key.json";

// ============================================================================
// DATA STRUCTURES
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Position {
    position_id: String,
    symbol: String,
    #[serde(rename = "type")]
    position_type: PositionType,
    entry_price: f64,
    size: f64,
    margin_used: f64,
    leverage: u32,
    stop_loss: f64,
    take_profit: f64,
    unrealized_pnl: f64,
    unrealized_pnl_percent: f64,
    liquidation_price: f64,
    opened_at: String,
    signals: String,
    reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum PositionType {
    LONG,
    SHORT,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClosedTrade {
    trade_id: String,
    symbol: String,
    position_type: PositionType,
    entry_price: f64,
    exit_price: f64,
    size: f64,
    leverage: u32,
    realized_pnl: f64,
    realized_pnl_percent: f64,
    hold_time_minutes: i64,
    signals: String,
    reasoning: String,
    outcome: String,
    opened_at: String,
    closed_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Portfolio {
    balance: f64,
    realized_pnl: f64,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum TradingMode {
    Paper,
    Live,
}

#[derive(Debug, Serialize, Deserialize)]
struct AppData {
    portfolio: Portfolio,
    open_positions: Vec<Position>,
    closed_trades: Vec<ClosedTrade>,
    #[serde(default = "default_trading_mode")]
    trading_mode: TradingMode,
}

fn default_trading_mode() -> TradingMode {
    TradingMode::Paper
}

#[derive(Debug, Clone)]
struct DisplayBalance {
    address: String,
    network: Network,
    amount: f64,
    currency: String,
    fetched_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MasterRecord {
    ciphertext: String,
    salt: String,
    nonce: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiKeyRecord {
    ciphertext: String,
    salt: String,
    nonce: String,
}

struct TradingEngine {
    data: AppData,
    client: Client,
    prices: HashMap<String, CoinPrice>,
    anthropic_api_key: Option<String>,
    wallet_storage: Option<WalletStorage>,
    master_password: Option<String>,
    selected_wallet_balance: Option<DisplayBalance>,
}

#[derive(Debug, Clone, Deserialize)]
struct CoinPrice {
    price: f64,
    change_24h: f64,
    volume_24h: f64,
}

#[derive(Debug, Deserialize)]
struct CoinGeckoResponse {
    bitcoin: Option<PriceData>,
    ethereum: Option<PriceData>,
    solana: Option<PriceData>,
    ripple: Option<PriceData>,
    cardano: Option<PriceData>,
    #[serde(rename = "chainlink")]
    link: Option<PriceData>,
}

#[derive(Debug, Deserialize)]
struct PriceData {
    usd: f64,
    usd_24h_change: Option<f64>,
    usd_24h_vol: Option<f64>,
}

#[derive(Debug, Serialize)]
struct AIAnalysisRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AIMessage>,
}

#[derive(Debug, Serialize)]
struct AIMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AIAnalysisResponse {
    content: Vec<AIContent>,
}

#[derive(Debug, Deserialize)]
struct AIContent {
    text: String,
}

#[derive(Debug, Clone)]
struct PriceTick {
    symbol: String,
    price: f64,
    _ts: i64,
}

/// WebSocket price streamer with HTTP fallback for quick CLI updates.
struct RealtimePriceStream;

impl RealtimePriceStream {
    fn spawn(symbols: &[&str]) -> mpsc::Receiver<PriceTick> {
        let symbols_vec: Vec<String> = symbols.iter().map(|s| s.to_uppercase()).collect();
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            if let Ok(rt) = tokio::runtime::Runtime::new() {
                let ws_symbols = symbols_vec.clone();
                if let Err(e) = rt.block_on(Self::stream_ws(&ws_symbols, tx.clone())) {
                    eprintln!("WebSocket feed failed, falling back to HTTP: {}", e);
                } else {
                    // WebSocket ended gracefully; still fall through to HTTP to keep updates flowing.
                }
            }
            Self::http_poll_loop(&symbols_vec, tx);
        });

        rx
    }

    async fn stream_ws(symbols: &[String], tx: Sender<PriceTick>) -> Result<(), String> {
        if symbols.is_empty() {
            return Ok(());
        }
        let streams: Vec<String> = symbols
            .iter()
            .map(|s| format!("{}@miniTicker", s.to_ascii_lowercase()))
            .collect();
        let url = format!("wss://stream.binance.com:9443/stream?streams={}", streams.join("/"));
        let (ws_stream, _) = connect_async(&url)
            .await
            .map_err(|e| format!("connect failed: {}", e))?;
        let (_write, mut read) = ws_stream.split();

        while let Some(msg) = read.next().await {
            let msg = msg.map_err(|e| format!("ws read error: {}", e))?;
            if let Message::Text(text) = msg {
                if let Some((symbol, price)) = Self::parse_ws_message(&text) {
                    let _ = tx.send(PriceTick {
                        symbol,
                        price,
                        _ts: Utc::now().timestamp(),
                    });
                }
            }
        }
        Ok(())
    }

    fn parse_ws_message(raw: &str) -> Option<(String, f64)> {
        let val: serde_json::Value = serde_json::from_str(raw).ok()?;
        let data = val.get("data")?;
        let symbol = data.get("s")?.as_str()?.to_string();
        let price_str = data.get("c")?.as_str()?;
        let price = price_str.parse::<f64>().ok()?;
        Some((symbol, price))
    }

    fn http_poll_loop(symbols: &[String], tx: Sender<PriceTick>) {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());
        let filter = Self::symbol_set(symbols);
        let mut fallback_prices = Self::seed_prices(symbols);

        loop {
            let url = "https://api.coingecko.com/api/v3/simple/price?\
                   ids=bitcoin,ethereum,solana,ripple,cardano,chainlink&\
                   vs_currencies=usd";

            let ticks = match client.get(url).send().and_then(|r| r.json::<CoinGeckoResponse>()) {
                Ok(resp) => {
                    let mapped = Self::map_coingecko_to_ticks(resp);
                    // refresh the fallback cache whenever we have real data
                    for tick in &mapped {
                        fallback_prices.insert(tick.symbol.clone(), tick.price);
                    }
                    mapped
                }
                Err(_) => Self::fallback_ticks(symbols, &mut fallback_prices),
            };

            for tick in ticks {
                if filter.contains(&tick.symbol) {
                    if tx.send(tick).is_err() {
                        return;
                    }
                }
            }

            thread::sleep(Duration::from_secs(3));
        }
    }

    fn map_coingecko_to_ticks(resp: CoinGeckoResponse) -> Vec<PriceTick> {
        let mut ticks = Vec::new();
        let now = Utc::now().timestamp();
        if let Some(btc) = resp.bitcoin {
            ticks.push(PriceTick {
                symbol: "BTCUSDT".to_string(),
                price: btc.usd,
                _ts: now,
            });
        }
        if let Some(eth) = resp.ethereum {
            ticks.push(PriceTick {
                symbol: "ETHUSDT".to_string(),
                price: eth.usd,
                _ts: now,
            });
        }
        if let Some(sol) = resp.solana {
            ticks.push(PriceTick {
                symbol: "SOLUSDT".to_string(),
                price: sol.usd,
                _ts: now,
            });
        }
        if let Some(xrp) = resp.ripple {
            ticks.push(PriceTick {
                symbol: "XRPUSDT".to_string(),
                price: xrp.usd,
                _ts: now,
            });
        }
        if let Some(ada) = resp.cardano {
            ticks.push(PriceTick {
                symbol: "ADAUSDT".to_string(),
                price: ada.usd,
                _ts: now,
            });
        }
        if let Some(link) = resp.link {
            ticks.push(PriceTick {
                symbol: "LINKUSDT".to_string(),
                price: link.usd,
                _ts: now,
            });
        }
        ticks
    }

    fn seed_prices(symbols: &[String]) -> HashMap<String, f64> {
        let mut map = HashMap::new();
        for (idx, sym) in symbols.iter().enumerate() {
            let base = 100.0 + idx as f64 * 25.0;
            map.insert(sym.clone(), base);
        }
        map
    }

    fn fallback_ticks(symbols: &[String], cache: &mut HashMap<String, f64>) -> Vec<PriceTick> {
        let now = Utc::now().timestamp();
        let mut ticks = Vec::new();
        for sym in symbols {
            let entry = cache.entry(sym.clone()).or_insert(100.0);
            let jiggle = ((now % 5) as f64 - 2.0) * 0.15;
            *entry = (*entry * (1.0 + jiggle / 100.0)).max(0.01);
            ticks.push(PriceTick {
                symbol: sym.clone(),
                price: *entry,
                _ts: now,
            });
        }
        ticks
    }

    fn symbol_set(symbols: &[String]) -> HashSet<String> {
        symbols.iter().cloned().collect()
    }
}

fn render_sparkline(points: &VecDeque<f64>) -> String {
    if points.is_empty() {
        return "---".to_string();
    }
    let min = points.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = points.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let levels: Vec<char> = " .:-=+*#%@".chars().collect();
    let range = (max - min).max(1e-9);
    let mut out = String::new();
    for p in points {
        let norm = ((*p - min) / range * (levels.len() as f64 - 1.0)).round() as usize;
        out.push(levels.get(norm).copied().unwrap_or('#'));
    }
    out
}

fn watchlist_symbols() -> Vec<&'static str> {
    vec!["BTCUSDT", "ETHUSDT", "SOLUSDT", "XRPUSDT", "ADAUSDT", "LINKUSDT"]
}

// ============================================================================
// TRADING ENGINE
// ============================================================================



impl TradingEngine {
    fn new() -> Self {
        let data = Self::load_data();
        let client = Client::new();
        
        // Try to load Anthropic API key from environment
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok();

        let wallet_storage = WalletStorage::new("wallets.db").ok();

        TradingEngine {
            data,
            client,
            prices: HashMap::new(),
            anthropic_api_key: api_key,
            wallet_storage,
            master_password: None,
            selected_wallet_balance: None,
        }
    }

    fn load_data() -> AppData {
        if let Ok(contents) = fs::read_to_string("penguin_data.json") {
            serde_json::from_str(&contents).unwrap_or_else(|_| Self::default_data())
        } else {
            Self::default_data()
        }
    }

    fn default_data() -> AppData {
        AppData {
            portfolio: Portfolio {
                balance: 10000.0,
                realized_pnl: 0.0,
                created_at: Utc::now().to_rfc3339(),
            },
            open_positions: Vec::new(),
            closed_trades: Vec::new(),
            trading_mode: TradingMode::Paper,
        }
    }

    fn save_data(&self) {
        let json = serde_json::to_string_pretty(&self.data).unwrap();
        fs::write("penguin_data.json", json).expect("Failed to save data");
    }

    // ========================================================================
    // MARKET DATA
    // ========================================================================

    fn fetch_prices(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let url = "https://api.coingecko.com/api/v3/simple/price?\
                   ids=bitcoin,ethereum,solana,ripple,cardano,chainlink&\
                   vs_currencies=usd&include_24hr_change=true&include_24hr_vol=true";
        
        let response: CoinGeckoResponse = self.client.get(url).send()?.json()?;
        
        // Map CoinGecko data to our coin symbols
        if let Some(btc) = response.bitcoin {
            self.prices.insert("BTCUSDT".to_string(), CoinPrice {
                price: btc.usd,
                change_24h: btc.usd_24h_change.unwrap_or(0.0),
                volume_24h: btc.usd_24h_vol.unwrap_or(0.0),
            });
        }
        
        if let Some(eth) = response.ethereum {
            self.prices.insert("ETHUSDT".to_string(), CoinPrice {
                price: eth.usd,
                change_24h: eth.usd_24h_change.unwrap_or(0.0),
                volume_24h: eth.usd_24h_vol.unwrap_or(0.0),
            });
        }
        
        if let Some(sol) = response.solana {
            self.prices.insert("SOLUSDT".to_string(), CoinPrice {
                price: sol.usd,
                change_24h: sol.usd_24h_change.unwrap_or(0.0),
                volume_24h: sol.usd_24h_vol.unwrap_or(0.0),
            });
        }
        
        if let Some(xrp) = response.ripple {
            self.prices.insert("XRPUSDT".to_string(), CoinPrice {
                price: xrp.usd,
                change_24h: xrp.usd_24h_change.unwrap_or(0.0),
                volume_24h: xrp.usd_24h_vol.unwrap_or(0.0),
            });
        }
        
        if let Some(ada) = response.cardano {
            self.prices.insert("ADAUSDT".to_string(), CoinPrice {
                price: ada.usd,
                change_24h: ada.usd_24h_change.unwrap_or(0.0),
                volume_24h: ada.usd_24h_vol.unwrap_or(0.0),
            });
        }
        
        if let Some(link) = response.link {
            self.prices.insert("LINKUSDT".to_string(), CoinPrice {
                price: link.usd,
                change_24h: link.usd_24h_change.unwrap_or(0.0),
                volume_24h: link.usd_24h_vol.unwrap_or(0.0),
            });
        }
        
        Ok(())
    }

    fn get_price(&self, symbol: &str) -> Option<f64> {
        self.prices.get(symbol).map(|p| p.price)
    }

    // ========================================================================
    // AI INTEGRATION
    // ========================================================================

    fn ensure_api_key(&mut self) -> Result<(), String> {
        if self.anthropic_api_key.is_some() {
            return Ok(());
        }

        if !std::path::Path::new(ANTHROPIC_KEY_PATH).exists() {
            return Err("AI not configured. Set ANTHROPIC_API_KEY env var or configure in Settings.".to_string());
        }

        // Prompt for master password if needed
        let master = self.ensure_master_password()?;
        
        // Load key
        match self.load_anthropic_key_encrypted(&master) {
            Ok(Some(key)) => {
                self.anthropic_api_key = Some(key);
                Ok(())
            }
            Ok(None) => Err("Failed to load API key from file.".to_string()),
            Err(e) => Err(format!("Error loading API key: {}", e)),
        }
    }

    fn ai_scan_market(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        if let Err(e) = self.ensure_api_key() {
            return Ok(format!("❌ {}", e));
        }

        // Build market data summary
        let mut market_summary = String::from("Current Market Data:\n\n");
        
        let btc_change = self.prices.get("BTCUSDT").map(|p| p.change_24h).unwrap_or(0.0);
        
        for (symbol, coin) in &self.prices {
            market_summary.push_str(&format!(
                "{}: ${:.2} (24h: {:.2}%, vol: ${:.0}M)\n",
                symbol,
                coin.price,
                coin.change_24h,
                coin.volume_24h / 1_000_000.0
            ));
        }

        // Build prompt for AI
        let prompt = format!(
            r#"You are Penguin, an aggressive momentum trader who exploits BTC/altcoin divergence.

{}

STRATEGY:
- Hunt for altcoin pumps when BTC dumps (-3% or more)
- Look for RSI 70-85 (overbought momentum)
- Volume 2x+ average = strong signal
- Score each coin 1-5 based on signal strength

BTC 24h change: {:.2}%

TASK: Scan all coins and identify the BEST setup right now for either LONG or SHORT.

For each coin, assess:
1. Is there a divergence play? (BTC down, alt up = LONG opportunity)
2. Is there a top forming? (Parabolic move, volume fade = SHORT opportunity)
3. Signal count (1-5)
4. Conviction level

Output format:
BEST SETUP: [SYMBOL] [LONG/SHORT]
Signals: [list them]
Score: X/5
Reasoning: [why this trade in Penguin's voice]
Suggested size: [%]
Suggested leverage: [Xx]

If no good setup, say "NO SETUP - HOLD" and explain why."#,
            market_summary,
            btc_change
        );

        self.call_claude_api(&prompt)
    }

    fn ai_analyze_position(&mut self, position_id: &str) -> Result<String, Box<dyn std::error::Error>> {
        if let Err(e) = self.ensure_api_key() {
            return Ok(format!("❌ {}", e));
        }

        let position = self.data.open_positions.iter()
            .find(|p| p.position_id == position_id)
            .ok_or("Position not found")?;

        let current_price = self.get_price(&position.symbol)
            .ok_or("Price not available")?;

        let prompt = format!(
            r#"You are Penguin, reviewing an open position.

POSITION:
Symbol: {}
Type: {:?}
Entry: ${:.2}
Current: ${:.2}
Size: {:.4} ({:.0}x leverage)
Stop Loss: ${:.2}
Take Profit: ${:.2}
Unrealized PnL: ${:.2} ({:.2}%)
Liquidation: ${:.2}
Original Reasoning: {}

TASK: Analyze this position and suggest:
1. Should I hold, take partial profit, or close?
2. Should I adjust my stop loss?
3. What's the risk right now?
4. What would you do?

Be direct and tactical."#,
            position.symbol,
            position.position_type,
            position.entry_price,
            current_price,
            position.size,
            position.leverage,
            position.stop_loss,
            position.take_profit,
            position.unrealized_pnl,
            position.unrealized_pnl_percent,
            position.liquidation_price,
            position.reasoning
        );

        self.call_claude_api(&prompt)
    }

    fn ai_suggest_sizing(&mut self, symbol: &str, signals: &str) -> Result<String, Box<dyn std::error::Error>> {
        if let Err(e) = self.ensure_api_key() {
            return Ok(format!("❌ {}", e));
        }

        let available_balance = self.get_available_balance();
        let portfolio_heat = self.calculate_portfolio_heat();

        let prompt = format!(
            r#"You are Penguin, calculating position size.

PORTFOLIO:
Available Balance: ${:.2}
Current Heat: {:.1}%

SETUP:
Symbol: {}
Signals Present: {}

SIZING RULES:
- 1-2 signals: 15-25% size, 15-20x leverage
- 3 signals: 40-50% size, 25-30x leverage
- 4+ signals: 70-80% size, 35-50x leverage
- Account for 3% entry fee (divide by 1.03)

TASK: Calculate optimal position size and leverage.

Output format:
Suggested Size: $X (Y% of balance)
Suggested Leverage: Xx
Risk Assessment: [conservative/solid/textbook]
Reasoning: [why this sizing]"#,
            available_balance,
            portfolio_heat,
            symbol,
            signals
        );

        self.call_claude_api(&prompt)
    }

    fn call_claude_api(&self, prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
        let api_key = self.anthropic_api_key.as_ref()
            .ok_or("Anthropic API key not set")?;

        let request = AIAnalysisRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 1000,
            messages: vec![
                AIMessage {
                    role: "user".to_string(),
                    content: prompt.to_string(),
                }
            ],
        };

        let response = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()?;

        if !response.status().is_success() {
            return Err(format!("API error: {}", response.status()).into());
        }

        let ai_response: AIAnalysisResponse = response.json()?;
        
        Ok(ai_response.content.first()
            .map(|c| c.text.clone())
            .unwrap_or_else(|| "No response from AI".to_string()))
    }

    // ========================================================================
    // POSITION MANAGEMENT
    // ========================================================================

    fn open_position(&mut self, 
                     symbol: String,
                     position_type: PositionType,
                     margin_usd: f64,
                     leverage: u32,
                     stop_loss: f64,
                     take_profit: f64,
                     signals: String,
                     reasoning: String) -> Result<(), String> {
        
        let price = self.get_price(&symbol)
            .ok_or("Unable to fetch price")?;

        // Validate available balance
        let available = self.get_available_balance();
        if margin_usd > available {
            return Err(format!("Insufficient balance. Available: ${:.2}", available));
        }

        // Calculate position details
        let size = (margin_usd * leverage as f64) / price;
        let liquidation_price = match position_type {
            PositionType::SHORT => price * (1.0 + (1.0 / leverage as f64)),
            PositionType::LONG => price * (1.0 - (1.0 / leverage as f64)),
        };

        // Validate stop loss isn't beyond liquidation
        let stop_distance = match position_type {
            PositionType::SHORT => ((stop_loss - price) / price * 100.0).abs(),
            PositionType::LONG => ((price - stop_loss) / price * 100.0).abs(),
        };

        if stop_distance < 1.0 {
            return Err("Stop loss too close to entry (minimum 1%)".to_string());
        }

        let position = Position {
            position_id: format!("pos_{}", self.generate_id()),
            symbol: symbol.clone(),
            position_type,
            entry_price: price,
            size,
            margin_used: margin_usd,
            leverage,
            stop_loss,
            take_profit,
            unrealized_pnl: 0.0,
            unrealized_pnl_percent: 0.0,
            liquidation_price,
            opened_at: Utc::now().to_rfc3339(),
            signals,
            reasoning,
        };

        self.data.open_positions.push(position);
        self.save_data();

        Ok(())
    }

    fn close_position(&mut self, position_id: &str, percentage: f64) -> Result<(), String> {
        let pos_index = self.data.open_positions.iter()
            .position(|p| p.position_id == position_id)
            .ok_or("Position not found")?;

        let mut position = self.data.open_positions[pos_index].clone();
        
        let current_price = self.get_price(&position.symbol)
            .ok_or("Unable to fetch current price")?;

        // Calculate PnL
        let (pnl, _) = Self::calculate_pnl(&position, current_price);
        
        // Account for fees (3% entry + 3% exit = 6% total)
        let fee = position.margin_used * 0.06;
        let realized_pnl = pnl - fee;
        let realized_pnl_percent = (realized_pnl / position.margin_used) * 100.0;

        // Calculate hold time
        let opened: DateTime<Utc> = position.opened_at.parse()
            .map_err(|_| "Invalid timestamp")?;
        let now = Utc::now();
        let hold_time_minutes = (now - opened).num_minutes();

        // Update balance
        self.data.portfolio.balance += position.margin_used + realized_pnl;
        self.data.portfolio.realized_pnl += realized_pnl;

        // Create closed trade record
        let outcome = if realized_pnl > 0.0 { "WIN" } else { "LOSS" };
        
        let closed_trade = ClosedTrade {
            trade_id: format!("trade_{}", self.generate_id()),
            symbol: position.symbol.clone(),
            position_type: position.position_type.clone(),
            entry_price: position.entry_price,
            exit_price: current_price,
            size: position.size,
            leverage: position.leverage,
            realized_pnl,
            realized_pnl_percent,
            hold_time_minutes,
            signals: position.signals.clone(),
            reasoning: position.reasoning.clone(),
            outcome: outcome.to_string(),
            opened_at: position.opened_at.clone(),
            closed_at: Utc::now().to_rfc3339(),
        };

        self.data.closed_trades.push(closed_trade);
        
        // Remove position if 100% closed
        if percentage >= 100.0 {
            self.data.open_positions.remove(pos_index);
        } else {
            // Partial close - reduce position size
            position.size *= (100.0 - percentage) / 100.0;
            position.margin_used *= (100.0 - percentage) / 100.0;
            self.data.open_positions[pos_index] = position;
        }

        self.save_data();
        Ok(())
    }

    fn update_position_pnl(&mut self) {
        if self.data.open_positions.is_empty() {
            return;
        }

        // Clone symbols to avoid holding a mutable borrow while fetching prices
        let symbols: Vec<String> = self.data.open_positions
            .iter()
            .map(|p| p.symbol.clone())
            .collect();

        for (idx, symbol) in symbols.iter().enumerate() {
            if let Some(current_price) = self.get_price(symbol) {
                let (pnl, pnl_percent) = {
                    let position = &self.data.open_positions[idx];
                    Self::calculate_pnl(position, current_price)
                };

                if let Some(position) = self.data.open_positions.get_mut(idx) {
                    position.unrealized_pnl = pnl;
                    position.unrealized_pnl_percent = pnl_percent;

                    // Check stop loss / take profit without borrowing self
                    Self::check_auto_close(position, current_price);
                }
            }
        }
    }

    fn calculate_pnl(position: &Position, current_price: f64) -> (f64, f64) {
        let pnl = match position.position_type {
            PositionType::SHORT => {
                (position.entry_price - current_price) * position.size
            }
            PositionType::LONG => {
                (current_price - position.entry_price) * position.size
            }
        };
        
        let pnl_percent = (pnl / position.margin_used) * 100.0;
        (pnl, pnl_percent)
    }

    fn check_auto_close(position: &Position, current_price: f64) {
        // Check if stop loss or take profit hit
        let should_close = match position.position_type {
            PositionType::SHORT => {
                current_price >= position.stop_loss || current_price <= position.take_profit
            }
            PositionType::LONG => {
                current_price <= position.stop_loss || current_price >= position.take_profit
            }
        };

        if should_close {
            println!("{}", "⚠️  ALERT: Position hit stop/TP!".red().bold());
            println!("Position {} should be closed", position.position_id);
        }
    }

    // ========================================================================
    // PORTFOLIO CALCULATIONS
    // ========================================================================

    fn get_available_balance(&self) -> f64 {
        let total_margin: f64 = self.data.open_positions.iter()
            .map(|p| p.margin_used)
            .sum();
        self.data.portfolio.balance - total_margin
    }

    fn calculate_portfolio_heat(&self) -> f64 {
        let total_margin: f64 = self.data.open_positions.iter()
            .map(|p| p.margin_used)
            .sum();
        (total_margin / self.data.portfolio.balance) * 100.0
    }

    fn get_total_unrealized_pnl(&self) -> f64 {
        self.data.open_positions.iter()
            .map(|p| p.unrealized_pnl)
            .sum()
    }

    fn generate_id(&self) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        format!("{:x}", timestamp)
    }

    // ========================================================================
    // UI DISPLAY FUNCTIONS
    // ========================================================================

    fn display_header(&self) {
        println!("\n{}", "=".repeat(70).cyan());
        println!("{}", "    🐧 PENGUIN 2026 AI-POWERED TRADING ENGINE".green().bold());
        let mode_str = match self.data.trading_mode {
            TradingMode::Paper => "PAPER TRADING".green(),
            TradingMode::Live => "LIVE TRADING".red().bold(),
        };
        println!("    MODE: {}", mode_str);
        println!("{}", "=".repeat(70).cyan());
    }

    fn display_portfolio_summary(&mut self) {
        self.update_position_pnl();
        
        let available = self.get_available_balance();
        let heat = self.calculate_portfolio_heat();
        let unrealized = self.get_total_unrealized_pnl();
        let total_value = self.data.portfolio.balance + unrealized;

        println!("\n{}", "PORTFOLIO SUMMARY".yellow().bold());
        println!("{}", "-".repeat(50).cyan());
        println!("Balance: {}", format!("${:.2}", self.data.portfolio.balance).cyan());
        println!("Available: {}", format!("${:.2}", available).cyan());
        println!("Unrealized PnL: {}", self.format_pnl(unrealized));
        println!("Realized PnL: {}", self.format_pnl(self.data.portfolio.realized_pnl));
        println!("Total Value: {}", format!("${:.2}", total_value).green().bold());
        println!("Portfolio Heat: {}", self.format_heat(heat));
        println!("Open Positions: {}", self.data.open_positions.len());
        if let Some(wallet) = &self.selected_wallet_balance {
            println!(
                "Selected Wallet: {} ({}) → {:.6} {} @ {}",
                Self::truncate_text(&wallet.address, 18),
                wallet.network.name(),
                wallet.amount,
                wallet.currency,
                wallet.fetched_at
            );
        }
    }

    fn display_market_data(&self) {
        println!("\n{}", "MARKET DATA".yellow().bold());
        println!("{}", "-".repeat(50).cyan());
        
        for (symbol, coin) in &self.prices {
            let change_color = if coin.change_24h >= 0.0 { 
                coin.change_24h.to_string().green() 
            } else { 
                coin.change_24h.to_string().red() 
            };
            
            println!("{}: {} (24h: {}%)", 
                     symbol.white().bold(),
                     format!("${:.2}", coin.price).cyan(),
                     change_color);
        }
    }

    /// Live board with websocket → HTTP fallback, price sparklines, and wallet overlay.
    fn display_live_board(&mut self) {
        println!("\n{}", "⚡ REAL-TIME MARKET BOARD".yellow().bold());
        println!("WebSocket (Binance miniTicker) with HTTP fallback. Runs ~25 seconds.");

        let symbols = watchlist_symbols();
        let receiver = RealtimePriceStream::spawn(&symbols);
        let mut history: HashMap<String, VecDeque<f64>> = HashMap::new();
        let wallet_balances = self.snapshot_wallet_balances();
        let start = Instant::now();
        let mut last_render = Instant::now() - Duration::from_secs(1);

        while start.elapsed() < Duration::from_secs(25) {
            while let Ok(tick) = receiver.try_recv() {
                self.prices.insert(
                    tick.symbol.clone(),
                    CoinPrice {
                        price: tick.price,
                        change_24h: 0.0,
                        volume_24h: 0.0,
                    },
                );
                let entry = history
                    .entry(tick.symbol.clone())
                    .or_insert_with(|| VecDeque::with_capacity(64));
                if entry.len() >= 64 {
                    entry.pop_front();
                }
                entry.push_back(tick.price);
            }

            if last_render.elapsed() >= Duration::from_millis(800) {
                self.render_live_board(&history, &wallet_balances);
                last_render = Instant::now();
            }

            thread::sleep(Duration::from_millis(120));
        }

        println!("\n{}", "Live board complete. Returning to menu...".green());
    }

    fn render_live_board(
        &self,
        history: &HashMap<String, VecDeque<f64>>,
        wallets: &[DisplayBalance],
    ) {
        println!("{}", "-".repeat(86).cyan());
        println!(
            "{:<8} {:>12} {:>16} {:>20}",
            "Symbol", "Last", "Sparkline", "Wallet Balance"
        );
        println!("{}", "-".repeat(86).cyan());

        for symbol in watchlist_symbols() {
            let last_price = self
                .get_price(symbol)
                .or_else(|| history.get(symbol).and_then(|h| h.back().copied()))
                .unwrap_or(0.0);
            let spark = history
                .get(symbol)
                .map(render_sparkline)
                .unwrap_or_else(|| "---".to_string());
            let wallet_line = self.wallet_line(wallets, symbol);
            println!(
                "{:<8} ${:>10.2} {:>16} {:>20}",
                symbol, last_price, spark, wallet_line
            );
        }
    }

    fn wallet_line(&self, wallets: &[DisplayBalance], symbol: &str) -> String {
        let prefix = symbol.get(0..3).unwrap_or(symbol).to_ascii_uppercase();
        if let Some(w) = wallets
            .iter()
            .find(|w| w.currency.to_ascii_uppercase() == prefix)
        {
            format!("{:.6} {}", w.amount, w.currency)
        } else {
            "-".to_string()
        }
    }

    /// Try to load wallet balances once so the live board can overlay holdings.
    fn snapshot_wallet_balances(&self) -> Vec<DisplayBalance> {
        let mut balances = Vec::new();
        let Some(storage) = &self.wallet_storage else {
            return balances;
        };
        let runtime = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(_) => return balances,
        };

        let wallets = match storage.load_all_wallets() {
            Ok(w) => w,
            Err(_) => return balances,
        };

        for w in wallets {
            if let Ok(balance) = runtime.block_on(fetch_balance(&w.address, w.network)) {
                balances.push(DisplayBalance {
                    address: w.address.clone(),
                    network: w.network,
                    amount: balance.balance,
                    currency: balance.currency.clone(),
                    fetched_at: Utc::now().to_rfc3339(),
                });
            }
        }

        balances
    }

    fn display_open_positions(&mut self) -> Vec<String> {
        self.update_position_pnl();

        if self.data.open_positions.is_empty() {
            println!("\n{}", "No open positions".yellow());
            return Vec::new();
        }

        println!("\n{}", "OPEN POSITIONS".yellow().bold());
        let header = format!(
            "{:<3} {:<8} {:<6} {:>5} {:>10} {:>10} {:>10} {:>10} {:>18} {:>9} {:>9} {:>9}",
            "#", "Symbol", "Type", "Lev", "Entry", "Current", "Margin", "Size",
            "Unrealized", "Stop", "TP", "Liq"
        );
        println!("{}", header.cyan());
        println!("{}", "-".repeat(140).dimmed());

        let mut ids = Vec::new();

        for (idx, position) in self.data.open_positions.iter().enumerate() {
            ids.push(position.position_id.clone());

            let current_price = self.get_price(&position.symbol).unwrap_or(position.entry_price);
            let type_str = match position.position_type {
                PositionType::LONG => "LONG".green(),
                PositionType::SHORT => "SHORT".red(),
            };
            let pnl_display = self.format_pnl_with_percent(
                position.unrealized_pnl,
                position.unrealized_pnl_percent,
            );

            println!(
                "{:<3} {:<8} {:<6} {:>5} {:>10.2} {:>10.2} {:>10.2} {:>10.4} {:>18} {:>9.2} {:>9.2} {:>9.2}",
                idx + 1,
                position.symbol.white().bold(),
                type_str.bold(),
                position.leverage,
                position.entry_price,
                current_price,
                position.margin_used,
                position.size,
                pnl_display,
                position.stop_loss,
                position.take_profit,
                position.liquidation_price,
            );

            let reason_preview = Self::truncate_text(&position.reasoning, 90);
            println!(
                "    Signals: {} | Reason: {}",
                position.signals.cyan(),
                reason_preview.italic()
            );
            println!("    ID: {}", position.position_id.dimmed());
            println!("{}", "-".repeat(140).dimmed());
        }

        ids
    }

    fn format_pnl(&self, pnl: f64) -> colored::ColoredString {
        let formatted = format!("${:.2}", pnl.abs());
        if pnl >= 0.0 {
            format!("+{}", formatted).green()
        } else {
            format!("-{}", formatted).red()
        }
    }

    fn format_pnl_with_percent(&self, pnl: f64, pnl_percent: f64) -> colored::ColoredString {
        let formatted = format!("${:.2} ({:.2}%)", pnl.abs(), pnl_percent.abs());
        if pnl >= 0.0 {
            format!("+{}", formatted).green().bold()
        } else {
            format!("-{}", formatted).red().bold()
        }
    }

    fn format_heat(&self, heat: f64) -> colored::ColoredString {
        let formatted = format!("{:.1}%", heat);
        if heat > 60.0 {
            formatted.red().bold()
        } else if heat > 40.0 {
            formatted.yellow()
        } else {
            formatted.green()
        }
    }

    fn truncate_text(text: &str, max_len: usize) -> String {
        if text.chars().count() <= max_len {
            return text.to_string();
        }

        let truncated: String = text.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }

    fn resolve_position_input(&self, input: &str, ids: &[String]) -> Option<String> {
        if input.is_empty() {
            return None;
        }

        if let Ok(index) = input.parse::<usize>() {
            if (1..=ids.len()).contains(&index) {
                return Some(ids[index - 1].clone());
            }
        }

        ids.iter()
            .find(|id| id.as_str() == input)
            .cloned()
    }

    // ========================================================================
    // MENU SYSTEM
    // ========================================================================

    fn ensure_wallet_storage(&mut self) -> Result<&WalletStorage, String> {
        if self.wallet_storage.is_none() {
            self.wallet_storage = WalletStorage::new("wallets.db").ok();
        }
        self.wallet_storage
            .as_ref()
            .ok_or_else(|| "Could not open wallet database (wallets.db)".to_string())
    }

    fn ensure_master_password(&mut self) -> Result<String, String> {
        if let Some(pwd) = &self.master_password {
            return Ok(pwd.clone());
        }

        let master_path = "wallet_master.json";
        if !std::path::Path::new(master_path).exists() {
            println!("{}", "\n🔑 Set a master password for wallets (shown once).".yellow());
            let pwd1 = self.read_password_hidden("Enter new password: ");
            let pwd2 = self.read_password_hidden("Confirm password: ");

            if pwd1.is_empty() || pwd1 != pwd2 {
                return Err("Passwords did not match or were empty".to_string());
            }

            let encrypted = encrypt(b"penguin-master-check", &pwd1)
                .map_err(|e| format!("Failed to set master password: {}", e))?;
            let record = MasterRecord {
                ciphertext: hex::encode(encrypted.ciphertext),
                salt: hex::encode(encrypted.salt),
                nonce: hex::encode(encrypted.nonce),
            };
            let json = serde_json::to_string_pretty(&record)
                .map_err(|e| format!("Failed to serialize master record: {}", e))?;
            fs::write(master_path, json)
                .map_err(|e| format!("Failed to save master password file: {}", e))?;

            self.master_password = Some(pwd1.clone());
            return Ok(pwd1);
        }

        // Existing master password: verify
        let contents = fs::read_to_string(master_path)
            .map_err(|e| format!("Failed to read master file: {}", e))?;
        let record: MasterRecord = serde_json::from_str(&contents)
            .map_err(|e| format!("Master file malformed: {}", e))?;

        let ciphertext = hex::decode(&record.ciphertext)
            .map_err(|e| format!("Master file ciphertext decode failed: {}", e))?;
        let salt = hex::decode(&record.salt)
            .map_err(|e| format!("Master file salt decode failed: {}", e))?;
        let nonce = hex::decode(&record.nonce)
            .map_err(|e| format!("Master file nonce decode failed: {}", e))?;

        let encrypted = EncryptedData {
            ciphertext,
            salt,
            nonce,
        };

        for _ in 0..3 {
            let attempt = self.read_password_hidden("Enter master password: ");
            if penguin_trader::wallet::verify_password(&encrypted, &attempt) {
                self.master_password = Some(attempt.clone());
                return Ok(attempt);
            }
            println!("{}", "Incorrect password, try again.".red());
        }

        Err("Failed to verify master password after 3 attempts".to_string())
    }

    fn save_anthropic_key_encrypted(&self, key: &str, master: &str) -> Result<(), String> {
        let encrypted = encrypt(key.as_bytes(), master)
            .map_err(|e| format!("Failed to encrypt API key: {}", e))?;
        let record = ApiKeyRecord {
            ciphertext: hex::encode(encrypted.ciphertext),
            salt: hex::encode(encrypted.salt),
            nonce: hex::encode(encrypted.nonce),
        };
        let json = serde_json::to_string_pretty(&record)
            .map_err(|e| format!("Failed to serialize API key record: {}", e))?;
        fs::write(ANTHROPIC_KEY_PATH, json)
            .map_err(|e| format!("Failed to save API key file: {}", e))
    }

    fn load_anthropic_key_encrypted(&self, master: &str) -> Result<Option<String>, String> {
        let contents = match fs::read_to_string(ANTHROPIC_KEY_PATH) {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };
        let record: ApiKeyRecord =
            serde_json::from_str(&contents).map_err(|e| format!("API key file malformed: {}", e))?;
        let ciphertext = hex::decode(&record.ciphertext)
            .map_err(|e| format!("API key ciphertext decode failed: {}", e))?;
        let salt = hex::decode(&record.salt)
            .map_err(|e| format!("API key salt decode failed: {}", e))?;
        let nonce = hex::decode(&record.nonce)
            .map_err(|e| format!("API key nonce decode failed: {}", e))?;
        let encrypted = EncryptedData {
            ciphertext,
            salt,
            nonce,
        };
        let decrypted = decrypt(&encrypted, master)
            .map_err(|e| format!("Failed to decrypt API key: {}", e))?;
        Ok(Some(String::from_utf8_lossy(&decrypted).to_string()))
    }

    fn mask_key(&self, key: &str) -> String {
        if key.len() <= 8 {
            "***".to_string()
        } else {
            format!("{}...{}", &key[..4], &key[key.len().saturating_sub(4)..])
        }
    }

    fn main_menu(&mut self) {
        loop {
            self.display_header();
            self.display_portfolio_summary();

            println!("\n{}", "MAIN MENU".yellow().bold());
            println!("1. 🤖 AI Scan Market (Find Best Setup)");
            println!("2. 📊 View Market Data");
            println!("3. 📈 Open LONG Position");
            println!("4. 📉 Open SHORT Position");
            println!("5. 💼 View Open Positions");
            println!("6. 🔧 Manage Position");
            println!("7. 📖 View Trade Journal");
            println!("8. 🪙  Wallet Desk");
            println!("9. ⚙️  Settings");
            println!("0. 🚪 Exit");

            print!("\n{}", "Select option: ".cyan());
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();

            match input.trim() {
                "1" => self.menu_ai_scan(),
                "2" => self.menu_view_market(),
                "3" => self.menu_open_long(),
                "4" => self.menu_open_short(),
                "5" => self.menu_view_positions(),
                "6" => self.menu_manage_position(),
                "7" => self.menu_trade_journal(),
                "8" => self.menu_wallets(),
                "9" => self.menu_settings(),
                "0" => {
                    println!("\n{}", "Thanks for trading! 🐧".green().bold());
                    break;
                }
                _ => println!("{}", "Invalid option".red()),
            }

            self.pause();
        }
    }

    fn menu_ai_scan(&mut self) {
        println!("\n{}", "🤖 AI MARKET SCANNER".yellow().bold());
        println!("{}", "=".repeat(70).cyan());

        // Fetch latest prices
        print!("Fetching market data... ");
        io::stdout().flush().unwrap();
        
        if let Err(e) = self.fetch_prices() {
            println!("{}", format!("Error: {}", e).red());
            return;
        }
        println!("{}", "✓".green());

        // Call AI to analyze
        print!("AI analyzing markets... ");
        io::stdout().flush().unwrap();
        
        match self.ai_scan_market() {
            Ok(analysis) => {
                println!("{}", "✓".green());
                println!("\n{}", analysis.cyan());
            }
            Err(e) => {
                println!("{}", format!("Error: {}", e).red());
            }
        }
    }

    fn menu_view_market(&mut self) {
        println!("\n{}", "📊 MARKET DATA".yellow().bold());
        
        if let Err(e) = self.fetch_prices() {
            println!("{}", format!("Error fetching prices: {}", e).red());
            return;
        }

        self.display_market_data();

        println!("\nStart live ticker with WebSocket (Binance) + HTTP fallback for 25s? (y/N): ");
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap_or(0);
        if input.trim().eq_ignore_ascii_case("y") {
            self.display_live_board();
        }
    }

    fn prompt_network(&self) -> Option<Network> {
        println!("\nChoose network:");
    println!("1. Bitcoin  2. Ethereum  3. Solana  4. Ripple  5. Cardano  6. BNB Smart Chain");
    print!("Select (1-6): ");
    io::stdout().flush().unwrap();
    match self.read_line().trim() {
        "1" => Some(Network::Bitcoin),
        "2" => Some(Network::Ethereum),
        "3" => Some(Network::Solana),
        "4" => Some(Network::Ripple),
        "5" => Some(Network::Cardano),
        "6" => Some(Network::Bnb),
        _ => {
            println!("{}", "Invalid selection".red());
            None
        }
    }
    }

    fn menu_wallets(&mut self) {
        println!("\n{}", "🪙 WALLET DESK".yellow().bold());

        if let Err(e) = self.ensure_master_password() {
            println!("{}", e.red());
            return;
        }

        loop {
            println!("\n1. Generate Wallet");
            println!("2. List Wallets");
            println!("3. Delete Wallet");
            println!("4. Show Wallet Balance");
            println!("5. Send Funds");
            println!("0. Back");
            print!("\n{}", "Select option: ".cyan());
            io::stdout().flush().unwrap();

            let choice = self.read_line();
            match choice.trim() {
                "1" => self.menu_generate_wallet(),
                "2" => self.menu_list_wallets(),
                "3" => self.menu_delete_wallet(),
                "4" => self.menu_wallet_balance(),
                "5" => self.menu_send_funds(),
                "0" => break,
                _ => println!("{}", "Invalid option".red()),
            }

            self.pause();
        }
    }

    fn menu_generate_wallet(&mut self) {
        let master = match self.ensure_master_password() {
            Ok(p) => p,
            Err(e) => {
                println!("{}", e.red());
                return;
            }
        };

        let network = match self.prompt_network() {
            Some(n) => n,
            None => return,
        };

        print!("Label (optional): ");
        io::stdout().flush().unwrap();
        let label_input = self.read_line();
        let label = if label_input.trim().is_empty() {
            None
        } else {
            Some(label_input.trim().to_string())
        };

        let wallet_id = format!("wallet_{}", self.generate_id());

        let storage_result = {
            match self.ensure_wallet_storage() {
                Ok(s) => Ok(s),
                Err(e) => Err(e),
            }
        };

        let storage = match storage_result {
            Ok(s) => s,
            Err(e) => {
                println!("{}", e.red());
                return;
            }
        };

        match generate_wallet(network) {
            Ok((address, private_key, mnemonic)) => {
                let encrypted = match encrypt(private_key.as_bytes(), &master) {
                    Ok(e) => e,
                    Err(e) => {
                        println!("{}", format!("Encryption failed: {}", e).red());
                        return;
                    }
                };

                let wallet = Wallet {
                    id: wallet_id,
                    network,
                    address: address.clone(),
                    encrypted_private_key: encrypted.ciphertext,
                    encrypted_mnemonic: None,
                    encryption_salt: encrypted.salt,
                    encryption_nonce: encrypted.nonce,
                    label,
                    created_at: Utc::now().to_rfc3339(),
                    updated_at: Utc::now().to_rfc3339(),
                };

                if let Err(e) = storage.save_wallet(&wallet) {
                    println!("{}", format!("Failed to save wallet: {}", e).red());
                    return;
                }

                println!("{}", "\nWallet generated!".green().bold());
                println!("Network: {}", wallet.network.name());
                println!("Address: {}", address);
                println!("{}", "Mnemonic (save securely, shown once):".yellow());
                println!("{}", mnemonic);
            }
            Err(e) => println!("{}", format!("Failed to generate wallet: {}", e).red()),
        }
    }

    fn menu_list_wallets(&mut self) {
        let storage = match self.ensure_wallet_storage() {
            Ok(s) => s,
            Err(e) => {
                println!("{}", e.red());
                return;
            }
        };

        match storage.load_all_wallets() {
            Ok(wallets) if !wallets.is_empty() => {
                println!("\n{}", "Wallets".yellow().bold());
                for (idx, w) in wallets.iter().enumerate() {
                    println!(
                        "{}. {} | {} | {}",
                        idx + 1,
                        w.network.name(),
                        Self::truncate_text(&w.address, 24),
                        w.label.clone().unwrap_or_else(|| "-".to_string())
                    );
                }
            }
            Ok(_) => println!("{}", "No wallets found.".yellow()),
            Err(e) => println!("{}", format!("Failed to load wallets: {}", e).red()),
        }
    }

    fn menu_delete_wallet(&mut self) {
        let wallets = {
            let storage = match self.ensure_wallet_storage() {
                Ok(s) => s,
                Err(e) => {
                    println!("{}", e.red());
                    return;
                }
            };

            match storage.load_all_wallets() {
                Ok(list) => list,
                Err(e) => {
                    println!("{}", format!("Failed to load wallets: {}", e).red());
                    return;
                }
            }
        };

        if wallets.is_empty() {
            println!("{}", "No wallets to delete.".yellow());
            return;
        }

        println!("{}", "Select wallet to delete:".yellow());
        for (idx, w) in wallets.iter().enumerate() {
            println!(
                "{}. {} | {}",
                idx + 1,
                w.network.name(),
                Self::truncate_text(&w.address, 24)
            );
        }
        print!("Enter number: ");
        io::stdout().flush().unwrap();
        let input = self.read_line();
        let idx = match input.parse::<usize>() {
            Ok(n) if n > 0 && n <= wallets.len() => n - 1,
            _ => {
                println!("{}", "Invalid selection".red());
                return;
            }
        };

        let addr = wallets[idx].address.clone();

        let storage = match self.ensure_wallet_storage() {
            Ok(s) => s,
            Err(e) => {
                println!("{}", e.red());
                return;
            }
        };

        match storage.delete_wallet(&addr) {
            Ok(true) => println!("{}", "Wallet deleted.".green()),
            Ok(false) => println!("{}", "Wallet not found.".yellow()),
            Err(e) => println!("{}", format!("Delete failed: {}", e).red()),
        }
    }

    fn menu_send_funds(&mut self) {
        println!("\n{}", "💸 SEND FUNDS".yellow().bold());

        // 1. Select Wallet
        let wallets = {
            let storage = match self.ensure_wallet_storage() {
                Ok(s) => s,
                Err(e) => {
                    println!("{}", e.red());
                    return;
                }
            };
            match storage.load_all_wallets() {
                Ok(list) => list,
                Err(e) => {
                    println!("{}", format!("Failed to load wallets: {}", e).red());
                    return;
                }
            }
        };

        if wallets.is_empty() {
            println!("{}", "No wallets found.".yellow());
            return;
        }

        println!("{}", "Select sender wallet:".yellow());
        for (idx, w) in wallets.iter().enumerate() {
            println!(
                "{}. {} | {}",
                idx + 1,
                w.network.name(),
                Self::truncate_text(&w.address, 24)
            );
        }
        print!("Enter number: ");
        io::stdout().flush().unwrap();
        let input = self.read_line();
        let idx = match input.parse::<usize>() {
            Ok(n) if n > 0 && n <= wallets.len() => n - 1,
            _ => {
                println!("{}", "Invalid selection".red());
                return;
            }
        };

        let wallet = &wallets[idx];

        // 2. Recipient
        print!("Enter recipient address: ");
        io::stdout().flush().unwrap();
        let to_address = self.read_line().trim().to_string();
        if to_address.is_empty() {
            println!("{}", "Address cannot be empty".red());
            return;
        }

        // 3. Amount
        print!("Enter amount to send: ");
        io::stdout().flush().unwrap();
        let amount = match self.read_line().trim().parse::<f64>() {
            Ok(a) if a > 0.0 => a,
            _ => {
                println!("{}", "Invalid amount".red());
                return;
            }
        };

        // 4. Fee Estimation
        println!("{}", "Estimating fee...".italic());
        let fee = match tokio::runtime::Runtime::new().unwrap().block_on(estimate_fee(wallet.network, amount)) {
            Ok(f) => f,
            Err(e) => {
                println!("{}", format!("Fee estimation failed: {:?}", e).red());
                // Fallback or ask user? For now, abort.
                return;
            }
        };
        println!("Estimated fee: {:.6} {}", fee, wallet.network.symbol());

        // 5. Confirm
        println!("\n{}", "CONFIRM TRANSACTION".red().bold());
        println!("From:   {}", wallet.address);
        println!("To:     {}", to_address);
        println!("Amount: {} {}", amount, wallet.network.symbol());
        println!("Fee:    {} {}", fee, wallet.network.symbol());
        print!("Type 'CONFIRM' to proceed: ");
        io::stdout().flush().unwrap();
        
        if self.read_line().trim() != "CONFIRM" {
            println!("{}", "Transaction cancelled.".yellow());
            return;
        }

        // 6. Decrypt Private Key
        let master = match self.ensure_master_password() {
            Ok(p) => p,
            Err(e) => {
                println!("{}", e.red());
                return;
            }
        };

        let encrypted_pk = EncryptedData {
            ciphertext: wallet.encrypted_private_key.clone(),
            salt: wallet.encryption_salt.clone(),
            nonce: wallet.encryption_nonce.clone(),
        };

        let private_key_bytes = match decrypt(&encrypted_pk, &master) {
            Ok(pk) => pk,
            Err(e) => {
                println!("{}", format!("Failed to decrypt private key: {}", e).red());
                return;
            }
        };
        let private_key_hex = hex::encode(private_key_bytes);

        // 7. Send
    println!("{}", "Sending transaction...".yellow());
    match tokio::runtime::Runtime::new().unwrap().block_on(send_transaction(
        wallet.network,
        &wallet.address,
        &to_address,
        amount,
        &private_key_hex,
        fee,
    )) {
        Ok(tx) => {
            println!("{}", "\nTransaction Broadcasted!".green().bold());
            println!("Hash: {}", tx.tx_hash);
            println!("Status: {}", tx.status);

            if tx.status == TransactionStatus::Pending {
                println!("\n{}", "Waiting for confirmation... (Press Ctrl+C to exit polling)".yellow().italic());
                let rt = tokio::runtime::Runtime::new().unwrap();
                
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    print!(".");
                    io::stdout().flush().unwrap();

                    match rt.block_on(check_transaction_status(wallet.network, &tx.tx_hash)) {
                        Ok(status) => {
                            if status == TransactionStatus::Confirmed {
                                println!("\n{}", "✓ Transaction Confirmed!".green().bold());
                                break;
                            } else if status == TransactionStatus::Failed {
                                println!("\n{}", "✗ Transaction Failed!".red().bold());
                                break;
                            }
                        }
                        Err(e) => {
                            println!("\nError checking status: {}", e);
                            // Don't break, retry
                        }
                    }
                }
            }
        }
        Err(e) => println!("{}", format!("Transaction failed: {}", e).red()),
    }    }

    fn menu_wallet_balance(&mut self) {
        let wallets = {
            let storage = match self.ensure_wallet_storage() {
                Ok(s) => s,
                Err(e) => {
                    println!("{}", e.red());
                    return;
                }
            };

            match storage.load_all_wallets() {
                Ok(list) => list,
                Err(e) => {
                    println!("{}", format!("Failed to load wallets: {}", e).red());
                    return;
                }
            }
        };

        if wallets.is_empty() {
            println!("{}", "No wallets found.".yellow());
            return;
        }

        println!("{}", "Select wallet to fetch balance:".yellow());
        for (idx, w) in wallets.iter().enumerate() {
            println!(
                "{}. {} | {}",
                idx + 1,
                w.network.name(),
                Self::truncate_text(&w.address, 24)
            );
        }
        print!("Enter number: ");
        io::stdout().flush().unwrap();
        let input = self.read_line();
        let idx = match input.parse::<usize>() {
            Ok(n) if n > 0 && n <= wallets.len() => n - 1,
            _ => {
                println!("{}", "Invalid selection".red());
                return;
            }
        };

        let wallet = &wallets[idx];
        let rt = match tokio::runtime::Runtime::new() {
            Ok(r) => r,
            Err(e) => {
                println!("{}", format!("Runtime error: {}", e).red());
                return;
            }
        };
        match rt.block_on(fetch_balance(&wallet.address, wallet.network)) {
            Ok(bal) => {
                println!(
                    "{} balance: {:.6} {}",
                    wallet.network.name(),
                    bal.balance,
                    bal.currency
                );
                self.selected_wallet_balance = Some(DisplayBalance {
                    address: wallet.address.clone(),
                    network: wallet.network,
                    amount: bal.balance,
                    currency: bal.currency,
                    fetched_at: Utc::now().to_rfc3339(),
                });
            }
            Err(e) => println!("{}", format!("Balance check failed: {:?}", e).red()),
        }
    }

    fn menu_open_long(&mut self) {
        self.menu_open_position(PositionType::LONG);
    }

    fn menu_open_short(&mut self) {
        self.menu_open_position(PositionType::SHORT);
    }

    fn menu_open_position(&mut self, position_type: PositionType) {
        let type_str = match position_type {
            PositionType::LONG => "LONG",
            PositionType::SHORT => "SHORT",
        };

        println!("\n{}", format!("📈 OPEN {} POSITION", type_str).yellow().bold());
        println!("{}", "=".repeat(70).cyan());

        // Select symbol
        println!("\nAvailable coins:");
        println!("1. BTCUSDT  2. ETHUSDT  3. SOLUSDT");
        println!("4. XRPUSDT  5. ADAUSDT  6. LINKUSDT");
        
        print!("\nSelect coin (1-6): ");
        io::stdout().flush().unwrap();
        
        let symbol = match self.read_line().trim() {
            "1" => "BTCUSDT",
            "2" => "ETHUSDT",
            "3" => "SOLUSDT",
            "4" => "XRPUSDT",
            "5" => "ADAUSDT",
            "6" => "LINKUSDT",
            _ => {
                println!("{}", "Invalid selection".red());
                return;
            }
        }.to_string();

        // Fetch current price
        if let Err(_) = self.fetch_prices() {
            println!("{}", "Error fetching price".red());
            return;
        }

        let price = match self.get_price(&symbol) {
            Some(p) => p,
            None => {
                println!("{}", "Price not available".red());
                return;
            }
        };

        println!("\nCurrent {} price: {}", symbol, format!("${:.2}", price).green());

        // Get signals
        print!("\nSignals present (e.g., 'RSI 78, MACD cross, volume 3x'): ");
        io::stdout().flush().unwrap();
        let signals = self.read_line();

        // AI sizing suggestion
        if self.anthropic_api_key.is_some() {
            println!("\n{}", "Getting AI sizing recommendation...".cyan());
            if let Ok(suggestion) = self.ai_suggest_sizing(&symbol, &signals) {
                println!("\n{}", suggestion.yellow());
            }
        }

        // Get position parameters
        print!("\nMargin to use (USD): $");
        io::stdout().flush().unwrap();
        let margin: f64 = match self.read_line().trim().parse() {
            Ok(m) => m,
            Err(_) => {
                println!("{}", "Invalid amount".red());
                return;
            }
        };

        print!("Leverage (1-50x): ");
        io::stdout().flush().unwrap();
        let leverage: u32 = match self.read_line().trim().parse() {
            Ok(l) if l >= 1 && l <= 50 => l,
            _ => {
                println!("{}", "Invalid leverage (must be 1-50)".red());
                return;
            }
        };

        print!("Stop Loss price: $");
        io::stdout().flush().unwrap();
        let stop_loss: f64 = match self.read_line().trim().parse() {
            Ok(sl) => sl,
            Err(_) => {
                println!("{}", "Invalid price".red());
                return;
            }
        };

        print!("Take Profit price: $");
        io::stdout().flush().unwrap();
        let take_profit: f64 = match self.read_line().trim().parse() {
            Ok(tp) => tp,
            Err(_) => {
                println!("{}", "Invalid price".red());
                return;
            }
        };

        print!("\nReasoning for this trade: ");
        io::stdout().flush().unwrap();
        let reasoning = self.read_line();

        // Confirm trade
        println!("\n{}", "TRADE SUMMARY".yellow().bold());
        println!("Symbol: {}", symbol);
        println!("Type: {:?}", position_type);
        println!("Entry: ${:.2}", price);
        println!("Margin: ${:.2}", margin);
        println!("Leverage: {}x", leverage);
        println!("Size: {:.6} coins", (margin * leverage as f64) / price);
        println!("Stop Loss: ${:.2}", stop_loss);
        println!("Take Profit: ${:.2}", take_profit);

        print!("\nConfirm trade? (yes/no): ");
        io::stdout().flush().unwrap();
        
        if self.read_line().trim().to_lowercase() == "yes" {
            match self.open_position(
                symbol,
                position_type,
                margin,
                leverage,
                stop_loss,
                take_profit,
                signals,
                reasoning,
            ) {
                Ok(_) => println!("\n{}", "✓ Position opened successfully!".green().bold()),
                Err(e) => println!("\n{}", format!("Error: {}", e).red()),
            }
        } else {
            println!("{}", "Trade cancelled".yellow());
        }
    }

    fn menu_view_positions(&mut self) {
        let position_ids = self.display_open_positions();
        if position_ids.is_empty() {
            return;
        }

        if self.anthropic_api_key.is_some() {
            print!(
                "\n{}",
                "Analyze a position with AI? Enter # or ID (press Enter to skip): ".cyan()
            );
            io::stdout().flush().unwrap();

            let selection = self.read_line();
            if let Some(pos_id) = self.resolve_position_input(selection.trim(), &position_ids) {
                println!("\n{}", "AI analyzing position...".cyan());
                match self.ai_analyze_position(&pos_id) {
                    Ok(analysis) => println!("\n{}", analysis.yellow()),
                    Err(e) => println!("{}", format!("Error: {}", e).red()),
                }
            } else if !selection.trim().is_empty() {
                println!("{}", "Invalid selection".red());
            }
        }
    }

    fn menu_manage_position(&mut self) {
        let position_ids = self.display_open_positions();
        if position_ids.is_empty() {
            return;
        }

        print!("\n{}", "Enter position # or ID to manage: ".cyan());
        io::stdout().flush().unwrap();
        let selection = self.read_line();
        let Some(pos_id) = self.resolve_position_input(selection.trim(), &position_ids) else {
            println!("{}", "Invalid selection".red());
            return;
        };

        println!("\n{}", "MANAGE POSITION".yellow().bold());
        println!("1. Close Position (100%)");
        println!("2. Close Position (50%)");
        println!("3. Close Position (custom %)");
        println!("0. Cancel");

        print!("\nSelect option: ");
        io::stdout().flush().unwrap();

        match self.read_line().trim() {
            "1" => {
                if let Err(e) = self.close_position(&pos_id, 100.0) {
                    println!("{}", format!("Error: {}", e).red());
                } else {
                    println!("{}", "✓ Position closed".green());
                }
            }
            "2" => {
                if let Err(e) = self.close_position(&pos_id, 50.0) {
                    println!("{}", format!("Error: {}", e).red());
                } else {
                    println!("{}", "✓ 50% of position closed".green());
                }
            }
            "3" => {
                print!("Enter percentage to close (1-100): ");
                io::stdout().flush().unwrap();
                let pct: f64 = match self.read_line().trim().parse() {
                    Ok(p) if p > 0.0 && p <= 100.0 => p,
                    _ => {
                        println!("{}", "Invalid percentage".red());
                        return;
                    }
                };
                
                if let Err(e) = self.close_position(&pos_id, pct) {
                    println!("{}", format!("Error: {}", e).red());
                } else {
                    println!("{}", format!("✓ {}% of position closed", pct).green());
                }
            }
            "0" => println!("{}", "Cancelled".yellow()),
            _ => println!("{}", "Invalid option".red()),
        }
    }

    fn menu_trade_journal(&self) {
        println!("\n{}", "📖 TRADE JOURNAL".yellow().bold());
        println!("{}", "=".repeat(70).cyan());

        if self.data.closed_trades.is_empty() {
            println!("\n{}", "No closed trades yet".yellow());
            return;
        }

        // Show stats
        let total_trades = self.data.closed_trades.len();
        let wins = self.data.closed_trades.iter()
            .filter(|t| t.outcome == "WIN")
            .count();
        let win_rate = (wins as f64 / total_trades as f64) * 100.0;
        
        println!("\n{}", "PERFORMANCE STATS".yellow().bold());
        println!("Total Trades: {}", total_trades);
        println!("Wins: {} | Losses: {}", wins, total_trades - wins);
        println!("Win Rate: {:.1}%", win_rate);
        println!("Total Realized PnL: {}", self.format_pnl(self.data.portfolio.realized_pnl));

        // Show recent trades
        println!("\n{}", "RECENT TRADES (Last 10)".yellow().bold());
        for trade in self.data.closed_trades.iter().rev().take(10) {
            let outcome_color = if trade.outcome == "WIN" {
                trade.outcome.as_str().green()
            } else {
                trade.outcome.as_str().red()
            };

            println!("\n{} {:?} {} @ {}x",
                     outcome_color.bold(),
                     trade.position_type,
                     trade.symbol.white().bold(),
                     trade.leverage.to_string().yellow());
            println!("  Entry: ${:.2} | Exit: ${:.2}", 
                     trade.entry_price, trade.exit_price);
            println!("  PnL: {}", self.format_pnl_with_percent(
                trade.realized_pnl, trade.realized_pnl_percent));
            println!("  Hold: {} min | Signals: {}", 
                     trade.hold_time_minutes, trade.signals.cyan());
            println!("  Reason: {}", trade.reasoning.italic());
        }
    }

    fn menu_settings(&mut self) {
        println!("\n{}", "⚙️  SETTINGS".yellow().bold());
        println!("1. Set Balance");
        println!("2. Configure AI (Anthropic API Key)");
        println!("3. Reset Trading Data");
        let mode_str = match self.data.trading_mode {
            TradingMode::Paper => "Paper".green(),
            TradingMode::Live => "LIVE".red().bold(),
        };
        println!("4. Toggle Trading Mode (Current: {})", mode_str);
        println!("0. Back");

        print!("\nSelect option: ");
        io::stdout().flush().unwrap();

        match self.read_line().trim() {
            "1" => {
                print!("Enter new balance: $");
                io::stdout().flush().unwrap();
                if let Ok(balance) = self.read_line().trim().parse::<f64>() {
                    self.data.portfolio.balance = balance;
                    self.save_data();
                    println!("{}", "✓ Balance updated".green());
                } else {
                    println!("{}", "Invalid amount".red());
                }
            }
            "2" => {
                println!("\n{}", "AI CONFIGURATION".yellow());
                if let Some(key) = &self.anthropic_api_key {
                    println!(
                        "{}",
                        format!("Current key (in memory): {}", self.mask_key(key)).green()
                    );
                } else {
                    println!("{}", "No key loaded in memory.".red());
                }

                // Ensure master password so we can store securely.
                let master = match self.ensure_master_password() {
                    Ok(p) => p,
                    Err(e) => {
                        println!("{}", e.red());
                        return;
                    }
                };

                // Try to load a stored key if we don't have one in memory.
                if self.anthropic_api_key.is_none() {
                    match self.load_anthropic_key_encrypted(&master) {
                        Ok(Some(stored)) => {
                            let masked = self.mask_key(&stored);
                            self.anthropic_api_key = Some(stored);
                            println!("{}", format!("Loaded stored key: {}", masked).green());
                        }
                        Ok(None) => {
                            println!("{}", "No stored key found.".yellow());
                        }
                        Err(e) => println!("{}", format!("Could not read stored key: {}", e).red()),
                    }
                }

                println!("\n{}", "Enter a new Anthropic API key (leave blank to keep current):".cyan());
                let new_key = self.read_password_hidden("API key: ");
                if !new_key.trim().is_empty() {
                    let trimmed = new_key.trim();
                    match self.save_anthropic_key_encrypted(trimmed, &master) {
                        Ok(_) => {
                            self.anthropic_api_key = Some(trimmed.to_string());
                            println!(
                                "{}",
                                format!(
                                    "Saved Anthropic key securely to {} ({}).",
                                    ANTHROPIC_KEY_PATH,
                                    self.mask_key(trimmed)
                                )
                                .green()
                            );
                        }
                        Err(e) => println!("{}", format!("Failed to save key: {}", e).red()),
                    }
                } else {
                    println!("{}", "Keeping existing key.".yellow());
                }
            }
            "3" => {
                print!("\n{}", "⚠️  Reset all trading data? (yes/no): ".red().bold());
                io::stdout().flush().unwrap();
                
                if self.read_line().trim().to_lowercase() == "yes" {
                    self.data = Self::default_data();
                    self.save_data();
                    println!("{}", "✓ Data reset".green());
                }
            }
            "4" => {
                match self.data.trading_mode {
                    TradingMode::Paper => {
                        self.data.trading_mode = TradingMode::Live;
                        println!("{}", "✓ Switched to LIVE TRADING mode".red().bold());
                    }
                    TradingMode::Live => {
                        self.data.trading_mode = TradingMode::Paper;
                        println!("{}", "✓ Switched to PAPER TRADING mode".green().bold());
                    }
                }
                self.save_data();
            }
            "0" => {}
            _ => println!("{}", "Invalid option".red()),
        }
    }

    fn read_line(&self) -> String {
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        input.trim().to_string()
    }

    /// Read input without echoing characters (best effort on Unix terminals).
    fn read_password_hidden(&self, prompt: &str) -> String {
        print!("{}", prompt);
        io::stdout().flush().unwrap();

        // Attempt to disable echo; ignore failures so input still works.
        let _ = Command::new("stty").arg("-echo").status();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap_or(0);

        // Re-enable echo and ensure we move to the next line.
        let _ = Command::new("stty").arg("echo").status();
        println!();

        input.trim().to_string()
    }

    fn pause(&self) {
        print!("\n{}", "Press Enter to continue...".cyan());
        io::stdout().flush().unwrap();
        self.read_line();
    }
}

// ============================================================================
// MAIN
// ============================================================================

fn main() {
    // Allow users to disable colors if their terminal theme makes text hard to read.
    let disable_colors = std::env::var("PENGUIN_NO_COLOR").is_ok() || std::env::var("NO_COLOR").is_ok();
    colored::control::set_override(!disable_colors);

    let mut engine = TradingEngine::new();
    
    println!("{}", "Initializing Penguin Trading Engine...".cyan());
    
    // Fetch initial prices
    if let Err(e) = engine.fetch_prices() {
        println!("{}", format!("Warning: Could not fetch prices: {}", e).yellow());
    }

    engine.main_menu();
}
