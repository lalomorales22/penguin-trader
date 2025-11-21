// Example: Using the indicators to analyze market data
// This demonstrates how all 5 technical indicators work together

use penguin_trader::{Candle, CandleSeries, IndicatorSnapshot};

fn main() {
    // Example 1: Strong Uptrend with Volume Spike
    println!("=== EXAMPLE 1: Strong Uptrend ===\n");
    let mut uptrend = CandleSeries::new("BTC".to_string());

    // Simulate uptrending candles (slowly increasing)
    for i in 0..25 {
        let close = 100.0 + (i as f64 * 0.5);
        uptrend.push(Candle::new(
            close - 0.2,
            close + 0.3,
            close - 0.4,
            close,
            1000.0 + (i as f64 * 100.0), // Increasing volume
            i as u64,
        ));
    }

    let snapshot = IndicatorSnapshot::from_candles(&uptrend);
    print_indicator_snapshot(&snapshot);
    println!("\nAnalysis: Strong uptrend with increasing volume.");
    println!("RSI should be > 70 (overbought territory)");
    println!("MACD should be positive with positive histogram");
    println!("ADX should be moderate to high (trend strength)\n");

    // Example 2: Overbought Reversal (Perfect Burry SHORT Signal)
    println!("\n=== EXAMPLE 2: Overbought Reversal (Burry SHORT) ===\n");
    let mut overbought = CandleSeries::new("SOL".to_string());

    // Parabolic move with volume fade
    for i in 0..20 {
        let close = 100.0 + (i as f64 * 1.5); // Aggressive moves
        let volume = if i < 15 { 5000.0 - (i as f64 * 200.0) } else { 2000.0 }; // Volume fading

        overbought.push(Candle::new(
            close - 0.5,
            close + 0.8,
            close - 0.6,
            close,
            volume,
            i as u64,
        ));
    }

    let snapshot = IndicatorSnapshot::from_candles(&overbought);
    print_indicator_snapshot(&snapshot);
    println!("\nAnalysis: Parabolic move with volume fade = perfect SHORT setup");
    println!("RSI should be > 80 (extreme overbought)");
    println!("MACD histogram should show negative (momentum fade)");
    println!("Stochastic > 90 (overbought confirmation)");
    println!("Volume ratio < 1.0 (volume fading)");
    println!("ADX should be < 30 (weak trend - no uptrend support)\n");

    // Example 3: Consolidation/Ranging
    println!("\n=== EXAMPLE 3: Consolidation Range ===\n");
    let mut ranging = CandleSeries::new("ETH".to_string());

    // Sideways market
    for i in 0..25 {
        let base = 100.0;
        let close = base + (if i % 2 == 0 { 0.5 } else { -0.5 });
        ranging.push(Candle::new(
            close - 0.2,
            close + 0.3,
            close - 0.3,
            close,
            1000.0,
            i as u64,
        ));
    }

    let snapshot = IndicatorSnapshot::from_candles(&ranging);
    print_indicator_snapshot(&snapshot);
    println!("\nAnalysis: Ranging market with no clear direction");
    println!("RSI should be near 50 (neutral)");
    println!("MACD should be near zero line");
    println!("Volume ratio near 1.0 (normal volume)");
    println!("ADX should be < 30 (weak/no trend)\n");

    // Example 4: Strong Downtrend with High Volume
    println!("\n=== EXAMPLE 4: Strong Downtrend ===\n");
    let mut downtrend = CandleSeries::new("XRP".to_string());

    // Downtrending candles
    for i in 0..25 {
        let close = 100.0 - (i as f64 * 0.5);
        downtrend.push(Candle::new(
            close + 0.2,
            close + 0.4,
            close - 0.3,
            close,
            1500.0 + (i as f64 * 50.0), // Increasing volume in downtrend
            i as u64,
        ));
    }

    let snapshot = IndicatorSnapshot::from_candles(&downtrend);
    print_indicator_snapshot(&snapshot);
    println!("\nAnalysis: Strong downtrend = potential LONG (divergence play)");
    println!("RSI should be < 30 (oversold)");
    println!("MACD should be negative");
    println!("ADX should show trend strength");
    println!("Volume high (conviction)\n");

    // Example 5: Real-world scenario interpretation
    println!("\n=== HOW TO USE THESE INDICATORS FOR TRADING ===\n");
    println!("🟢 BURRY SHORT SETUP (overbought reversal):");
    println!("   ✓ RSI > 80");
    println!("   ✓ MACD histogram turning negative");
    println!("   ✓ ADX < 30 (no uptrend support)");
    println!("   ✓ Stochastic > 90");
    println!("   ✓ Volume ratio > 2.0 (blow-off top)\n");

    println!("🟢 PENGUIN LONG SETUP (divergence):");
    println!("   ✓ BTC down -3% to -8%");
    println!("   ✓ Alt up +10-25% (fear rotation)");
    println!("   ✓ RSI 70-85 (momentum, not exhaustion)");
    println!("   ✓ Volume ratio 2-3x average");
    println!("   ✓ Support level holding\n");

    println!("🔴 HOLD/AVOID:");
    println!("   ✗ RSI 30-70 (neutral zone)");
    println!("   ✗ ADX 30-50 (moderate trend, too risky)");
    println!("   ✗ Low volume (no conviction)");
    println!("   ✗ MACD near crossover (indecisive)\n");
}

fn print_indicator_snapshot(snapshot: &IndicatorSnapshot) {
    println!("Symbol: {}", snapshot.symbol);

    if let Some(rsi) = snapshot.rsi {
        println!("  RSI (14):       {:.2} {}", rsi, interpret_rsi(rsi));
    } else {
        println!("  RSI (14):       N/A (need more data)");
    }

    if let Some(macd) = snapshot.macd_line {
        println!("  MACD Line:      {:.6}", macd);
    }
    if let Some(signal) = snapshot.macd_signal {
        println!("  MACD Signal:    {:.6}", signal);
    }
    if let Some(hist) = snapshot.macd_histogram {
        println!("  MACD Histogram: {:.6} {}", hist, interpret_macd_histogram(hist));
    }

    if let Some(adx) = snapshot.adx {
        println!("  ADX (14):       {:.2} {}", adx, interpret_adx(adx));
    } else {
        println!("  ADX (14):       N/A");
    }

    if let Some(k) = snapshot.stochastic_k {
        println!("  Stochastic %K:  {:.2} {}", k, interpret_stochastic(k));
    }
    if let Some(d) = snapshot.stochastic_d {
        println!("  Stochastic %D:  {:.2}", d);
    }

    if let Some(vol_ratio) = snapshot.volume_ratio {
        println!("  Volume Ratio:   {:.2}x {}", vol_ratio, interpret_volume(vol_ratio));
    }
}

fn interpret_rsi(rsi: f64) -> &'static str {
    match rsi {
        rsi if rsi > 80.0 => "← OVERBOUGHT ⚠️",
        rsi if rsi > 70.0 => "← Strong momentum",
        rsi if rsi > 50.0 => "← Bullish",
        rsi if rsi >= 30.0 => "← Bearish",
        rsi if rsi >= 20.0 => "← Strong weakness",
        _ => "← OVERSOLD ⚠️",
    }
}

fn interpret_adx(adx: f64) -> &'static str {
    match adx {
        adx if adx > 50.0 => "← Very strong trend",
        adx if adx > 40.0 => "← Strong trend",
        adx if adx >= 30.0 => "← Moderate trend",
        _ => "← Weak/no trend (good for Burry shorts)",
    }
}

fn interpret_stochastic(k: f64) -> &'static str {
    match k {
        k if k > 90.0 => "← EXTREME overbought",
        k if k > 80.0 => "← Overbought",
        k if k > 50.0 => "← Bullish",
        k if k >= 20.0 => "← Bearish",
        _ => "← EXTREME oversold",
    }
}

fn interpret_volume(ratio: f64) -> &'static str {
    match ratio {
        ratio if ratio > 3.0 => "← EXTREME volume spike",
        ratio if ratio > 2.0 => "← High conviction",
        ratio if ratio > 1.5 => "← Above average",
        ratio if ratio >= 0.67 => "← Normal",
        _ => "← Low volume (weak signal)",
    }
}

fn interpret_macd_histogram(hist: f64) -> &'static str {
    if hist > 0.0 {
        "← Positive (bullish)"
    } else if hist < 0.0 {
        "← Negative (bearish)"
    } else {
        "← Neutral"
    }
}
