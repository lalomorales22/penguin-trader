// Technical indicators: RSI, MACD, ADX, Stochastic, Volume Ratio
use crate::data::{Candle, CandleSeries};

/// RSI (Relative Strength Index) - 14 period default
/// Measures momentum on a scale of 0-100
/// > 80 = overbought, < 30 = oversold
pub fn calculate_rsi(closes: &[f64], period: usize) -> Option<f64> {
    if closes.len() < period + 1 {
        return None;
    }

    let mut gains = 0.0;
    let mut losses = 0.0;

    // Calculate average gains and losses over first period
    for i in 1..=period {
        let change = closes[i] - closes[i - 1];
        if change > 0.0 {
            gains += change;
        } else {
            losses += -change;
        }
    }

    let mut avg_gain = gains / period as f64;
    let mut avg_loss = losses / period as f64;

    // Calculate for remaining candles using smoothed averages
    for i in (period + 1)..closes.len() {
        let change = closes[i] - closes[i - 1];
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { -change } else { 0.0 };

        avg_gain = (avg_gain * (period - 1) as f64 + gain) / period as f64;
        avg_loss = (avg_loss * (period - 1) as f64 + loss) / period as f64;
    }

    if avg_loss == 0.0 {
        if avg_gain == 0.0 {
            return Some(50.0);
        }
        return Some(100.0);
    }

    let rs = avg_gain / avg_loss;
    let rsi = 100.0 - (100.0 / (1.0 + rs));
    Some(rsi)
}

/// MACD (Moving Average Convergence Divergence)
/// Returns (macd_line, signal_line, histogram)
/// Detects momentum and trend changes
pub fn calculate_macd(closes: &[f64], fast: usize, slow: usize, signal: usize) -> Option<(f64, f64, f64)> {
    if closes.len() < slow {
        return None;
    }

    let fast_ema = calculate_ema(closes, fast)?;
    let slow_ema = calculate_ema(closes, slow)?;
    let macd_line = fast_ema - slow_ema;

    // Calculate signal line (EMA of MACD line)
    // For simplicity, use last 9 values for signal line calculation
    if closes.len() < slow + signal {
        let signal_line = macd_line; // Default to MACD line if not enough data
        let histogram = 0.0;
        return Some((macd_line, signal_line, histogram));
    }

    // Create array of MACD values for the last 'signal' candles
    let mut macd_values = Vec::new();
    for i in (closes.len() - signal)..closes.len() {
        let f_ema = calculate_ema(&closes[0..=i], fast)?;
        let s_ema = calculate_ema(&closes[0..=i], slow)?;
        macd_values.push(f_ema - s_ema);
    }

    let signal_line = calculate_ema(&macd_values, signal)?;
    let histogram = macd_line - signal_line;

    Some((macd_line, signal_line, histogram))
}

/// EMA (Exponential Moving Average) - helper function
fn calculate_ema(values: &[f64], period: usize) -> Option<f64> {
    if values.len() < period {
        return None;
    }

    let multiplier = 2.0 / (period as f64 + 1.0);
    let mut ema = values[0..period].iter().sum::<f64>() / period as f64;

    for value in &values[period..] {
        ema = (value - ema) * multiplier + ema;
    }

    Some(ema)
}

/// ADX (Average Directional Index) - 14 period default
/// Measures trend strength (0-100)
/// < 30 = weak trend, 30-50 = moderate, > 50 = strong
pub fn calculate_adx(candles: &[Candle], period: usize) -> Option<f64> {
    if candles.len() < period + 1 {
        return None;
    }

    let mut plus_dm = Vec::new();
    let mut minus_dm = Vec::new();
    let mut true_range = Vec::new();

    // Calculate DM and TR for each candle
    for i in 1..candles.len() {
        let prev = candles[i - 1];
        let curr = candles[i];

        let high_diff = curr.high - prev.high;
        let low_diff = prev.low - curr.low;

        let pdm = if high_diff > 0.0 && high_diff > low_diff {
            high_diff
        } else {
            0.0
        };

        let mdm = if low_diff > 0.0 && low_diff > high_diff {
            low_diff
        } else {
            0.0
        };

        plus_dm.push(pdm);
        minus_dm.push(mdm);

        let tr = (curr.high - curr.low)
            .max((curr.high - prev.close).abs())
            .max((curr.low - prev.close).abs());
        true_range.push(tr);
    }

    if plus_dm.len() < period {
        return None;
    }

    // Calculate smoothed DM and TR
    let mut smoothed_plus_dm = plus_dm[0..period].iter().sum::<f64>();
    let mut smoothed_minus_dm = minus_dm[0..period].iter().sum::<f64>();
    let mut smoothed_tr = true_range[0..period].iter().sum::<f64>();

    let mut di_plus = (smoothed_plus_dm / smoothed_tr) * 100.0;
    let mut di_minus = (smoothed_minus_dm / smoothed_tr) * 100.0;
    let mut dx = ((di_plus - di_minus).abs() / (di_plus + di_minus)) * 100.0;

    let mut adx = dx;
    let mut count = 1.0;

    // Calculate ADX
    for i in period..plus_dm.len() {
        smoothed_plus_dm = smoothed_plus_dm - smoothed_plus_dm / period as f64 + plus_dm[i];
        smoothed_minus_dm = smoothed_minus_dm - smoothed_minus_dm / period as f64 + minus_dm[i];
        smoothed_tr = smoothed_tr - smoothed_tr / period as f64 + true_range[i];

        if smoothed_tr == 0.0 {
            break;
        }

        di_plus = (smoothed_plus_dm / smoothed_tr) * 100.0;
        di_minus = (smoothed_minus_dm / smoothed_tr) * 100.0;
        dx = ((di_plus - di_minus).abs() / (di_plus + di_minus)) * 100.0;

        adx = (adx * count + dx) / (count + 1.0);
        count += 1.0;
    }

    Some(adx)
}

/// Stochastic Oscillator (14,3,3 default)
/// Measures momentum and overbought/oversold levels
/// > 80 = overbought, < 20 = oversold
pub fn calculate_stochastic(candles: &[Candle], period: usize, _k_period: usize, _d_period: usize) -> Option<(f64, f64)> {
    if candles.len() < period {
        return None;
    }

    let lookback = &candles[candles.len() - period..];

    let highest = lookback.iter().map(|c| c.high).fold(f64::NEG_INFINITY, f64::max);
    let lowest = lookback.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
    let close = candles.last().unwrap().close;

    let high_low_range = highest - lowest;
    if high_low_range == 0.0 {
        return Some((50.0, 50.0));
    }

    let k_percent = ((close - lowest) / high_low_range) * 100.0;

    // For D line, we'd need multiple K values, but simplified here
    let d_percent = k_percent; // Simplified - in real implementation calculate EMA of K

    Some((k_percent, d_percent))
}

/// Volume Ratio - current volume vs 20-period average
/// Returns ratio > 1.0 = above average, < 1.0 = below average
pub fn calculate_volume_ratio(candles: &[Candle], period: usize) -> Option<f64> {
    if candles.len() < period {
        return None;
    }

    let lookback = &candles[candles.len() - period..];
    let avg_volume: f64 = lookback.iter().map(|c| c.volume).sum::<f64>() / period as f64;

    if avg_volume == 0.0 {
        return Some(1.0);
    }

    let current_volume = candles.last().unwrap().volume;
    Some(current_volume / avg_volume)
}

/// Complete indicator snapshot for a coin
#[derive(Debug, Clone)]
pub struct IndicatorSnapshot {
    pub symbol: String,
    pub rsi: Option<f64>,
    pub macd_line: Option<f64>,
    pub macd_signal: Option<f64>,
    pub macd_histogram: Option<f64>,
    pub adx: Option<f64>,
    pub stochastic_k: Option<f64>,
    pub stochastic_d: Option<f64>,
    pub volume_ratio: Option<f64>,
}

impl IndicatorSnapshot {
    pub fn new(symbol: String) -> Self {
        IndicatorSnapshot {
            symbol,
            rsi: None,
            macd_line: None,
            macd_signal: None,
            macd_histogram: None,
            adx: None,
            stochastic_k: None,
            stochastic_d: None,
            volume_ratio: None,
        }
    }

    /// Calculate all indicators from a candle series
    pub fn from_candles(candles: &CandleSeries) -> Self {
        let mut snapshot = IndicatorSnapshot::new(candles.symbol.clone());

        if candles.len() < 14 {
            return snapshot; // Not enough data
        }

        let closes = candles.get_closes();

        // RSI (14-period)
        snapshot.rsi = calculate_rsi(&closes, 14);

        // MACD (12,26,9)
        if let Some((macd, signal, hist)) = calculate_macd(&closes, 12, 26, 9) {
            snapshot.macd_line = Some(macd);
            snapshot.macd_signal = Some(signal);
            snapshot.macd_histogram = Some(hist);
        }

        // ADX (14-period)
        snapshot.adx = calculate_adx(&candles.candles, 14);

        // Stochastic (14,3,3)
        if let Some((k, d)) = calculate_stochastic(&candles.candles, 14, 3, 3) {
            snapshot.stochastic_k = Some(k);
            snapshot.stochastic_d = Some(d);
        }

        // Volume Ratio (20-period average)
        snapshot.volume_ratio = calculate_volume_ratio(&candles.candles, 20);

        snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rsi_overbought() {
        // Test with overbought conditions
        let closes = vec![100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 108.0, 109.0, 110.0, 111.0, 112.0, 113.0, 114.0];
        let rsi = calculate_rsi(&closes, 14);
        assert!(rsi.is_some());
        let rsi_val = rsi.unwrap();
        assert!(rsi_val > 70.0, "Uptrend should have RSI > 70");
    }

    #[test]
    fn test_volume_ratio() {
        let mut candles = Vec::new();
        for i in 0..25 {
            candles.push(Candle::new(100.0, 101.0, 99.0, 100.5, 1000.0, i as u64));
        }
        // Last candle with 5x volume
        candles.push(Candle::new(100.0, 101.0, 99.0, 100.5, 5000.0, 25));

        let series = CandleSeries {
            symbol: "TEST".to_string(),
            candles,
        };

        let ratio = calculate_volume_ratio(&series.candles, 20);
        assert!(ratio.is_some());
        let ratio_val = ratio.unwrap();
        assert!(ratio_val > 4.0, "High volume spike should show high ratio");
    }
}
