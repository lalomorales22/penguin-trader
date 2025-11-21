// Penguin AI Trading Engine library
pub mod data;
pub mod indicators;
pub mod strategy;
pub mod wallet;
pub mod ui;
pub mod trading;

pub use data::{Candle, CandleSeries, MarketData};
pub use indicators::IndicatorSnapshot;
pub use strategy::{Action, Conviction, MarketSignal, Scorer};
pub use trading::{
    PaperEngine,
    PaperPosition,
    ClosedPaperTrade,
    SharedDataStore,
    SharedAppData,
    PositionRecord,
    PositionSide,
};
pub use wallet::{Network, Wallet, WalletStorage, ChartPoint, ChartSnapshot, ChartView};
