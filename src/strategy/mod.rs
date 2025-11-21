// Trading strategy scoring system
// Combines Burry overbought shorts with Penguin divergence longs

pub mod burry;
pub mod penguin;
pub mod scorer;

use serde::{Deserialize, Serialize};

/// Trading action recommendation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Action {
    LONG,
    SHORT,
    HOLD,
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::LONG => write!(f, "LONG"),
            Action::SHORT => write!(f, "SHORT"),
            Action::HOLD => write!(f, "HOLD"),
        }
    }
}

/// Conviction level of a trading signal
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Conviction {
    No,       // 0-1 signals
    Watch,    // 2 signals
    Solid,    // 3 signals
    Textbook, // 4-5 signals
}

impl std::fmt::Display for Conviction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Conviction::No => write!(f, "no"),
            Conviction::Watch => write!(f, "watch"),
            Conviction::Solid => write!(f, "solid"),
            Conviction::Textbook => write!(f, "textbook"),
        }
    }
}

/// Complete market signal for a coin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSignal {
    pub symbol: String,
    pub current_price: f64,
    pub btc_price: f64,
    pub btc_change_24h: f64,

    // Burry SHORT signals
    pub burry_score: u8,           // 0-5
    pub burry_signals: Vec<String>, // Detailed signal breakdown

    // Penguin LONG signals
    pub penguin_score: u8,         // 0-5
    pub penguin_signals: Vec<String>, // Detailed signal breakdown

    // Technical indicators (for reference)
    pub rsi: Option<f64>,
    pub macd_histogram: Option<f64>,
    pub adx: Option<f64>,
    pub stochastic: Option<f64>,
    pub volume_ratio: Option<f64>,

    // Overall recommendation
    pub action: Action,
    pub conviction: Conviction,
    pub reasoning: String,

    // Risk assessment
    pub risk_level: String, // "low", "medium", "high", "extreme"
}

impl MarketSignal {
    pub fn new(symbol: String) -> Self {
        MarketSignal {
            symbol,
            current_price: 0.0,
            btc_price: 0.0,
            btc_change_24h: 0.0,
            burry_score: 0,
            burry_signals: Vec::new(),
            penguin_score: 0,
            penguin_signals: Vec::new(),
            rsi: None,
            macd_histogram: None,
            adx: None,
            stochastic: None,
            volume_ratio: None,
            action: Action::HOLD,
            conviction: Conviction::No,
            reasoning: String::new(),
            risk_level: "unknown".to_string(),
        }
    }
}

pub use burry::score_burry_setup;
pub use penguin::score_penguin_setup;
pub use scorer::Scorer;
