// Data structures for OHLCV candles and market data
use serde::{Deserialize, Serialize};

/// Open-High-Low-Close-Volume candle data structure
#[derive(Debug, Clone, Serialize, Deserialize, Copy)]
pub struct Candle {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub timestamp: u64, // Unix timestamp
}

impl Candle {
    pub fn new(open: f64, high: f64, low: f64, close: f64, volume: f64, timestamp: u64) -> Self {
        Candle {
            open,
            high,
            low,
            close,
            volume,
            timestamp,
        }
    }

    /// Typical price = (high + low + close) / 3
    pub fn typical_price(&self) -> f64 {
        (self.high + self.low + self.close) / 3.0
    }

    /// HL2 = (high + low) / 2
    pub fn hl2(&self) -> f64 {
        (self.high + self.low) / 2.0
    }
}

/// Market data for a specific coin at a point in time
#[derive(Debug, Clone)]
pub struct MarketData {
    pub symbol: String,
    pub price: f64,
    pub change_24h: f64,
    pub volume_24h: f64,
}

/// Historical candle series for technical analysis
#[derive(Debug, Clone)]
pub struct CandleSeries {
    pub symbol: String,
    pub candles: Vec<Candle>,
}

impl CandleSeries {
    pub fn new(symbol: String) -> Self {
        CandleSeries {
            symbol,
            candles: Vec::new(),
        }
    }

    pub fn push(&mut self, candle: Candle) {
        self.candles.push(candle);
    }

    pub fn get_closes(&self) -> Vec<f64> {
        self.candles.iter().map(|c| c.close).collect()
    }

    pub fn get_highs(&self) -> Vec<f64> {
        self.candles.iter().map(|c| c.high).collect()
    }

    pub fn get_lows(&self) -> Vec<f64> {
        self.candles.iter().map(|c| c.low).collect()
    }

    pub fn get_volumes(&self) -> Vec<f64> {
        self.candles.iter().map(|c| c.volume).collect()
    }

    pub fn len(&self) -> usize {
        self.candles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.candles.is_empty()
    }

    pub fn last(&self) -> Option<Candle> {
        self.candles.last().copied()
    }
}
