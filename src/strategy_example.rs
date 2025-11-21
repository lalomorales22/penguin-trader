// Strategy Scorer Demo
// Shows Burry SHORT and Penguin LONG strategies in action

use penguin_trader::{Candle, CandleSeries, IndicatorSnapshot, Scorer};

const SEP: &str = "══════════════════════════════════════════════════════════════════════════";

fn main() {
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║     🐧 PENGUIN x 🕵️ BURRY HYBRID STRATEGY SCORER             ║");
    println!("║         Phase 2: Dual Strategy Analysis Demo                  ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    // Scenario 1: TEXTBOOK BURRY SHORT (Overbought Reversal)
    println!("\n{}", SEP);
    println!("SCENARIO 1: TEXTBOOK BURRY SHORT - Overbought Reversal");
    println!("{}", SEP);
    demonstrate_burry_short();

    // Scenario 2: TEXTBOOK PENGUIN LONG (BTC Dump + Alt Pump)
    println!("\n{}", SEP);
    println!("SCENARIO 2: TEXTBOOK PENGUIN LONG - Divergence Setup");
    println!("{}", SEP);
    demonstrate_penguin_long();

    // Scenario 3: BOTH SIGNALS (Conflict - what wins?)
    println!("\n{}", SEP);
    println!("SCENARIO 3: CONFLICTING SIGNALS (No Clear Setup)");
    println!("{}", SEP);
    demonstrate_no_signal();

    // Scenario 4: WATCH SIGNALS (Early Signs)
    println!("\n{}", SEP);
    println!("SCENARIO 4: EARLY SIGNALS (Watch But Not Trade)");
    println!("{}", SEP);
    demonstrate_watch_signals();

    println!("\n\n{}", SEP);
    println!("STRATEGY DECISION TREE");
    println!("{}", SEP);
    println!(
        r#"
                    ┌─── Burry Score >= 4 ──> TEXTBOOK SHORT (extreme risk)
                    │
    Market Signal ──┤─── Penguin Score >= 4 ──> TEXTBOOK LONG (medium risk)
                    │
                    ├─── Burry Score == 3 ──> SOLID SHORT (high risk)
                    │
                    ├─── Penguin Score == 3 ──> SOLID LONG (medium risk)
                    │
                    ├─── Burry Score == 2 ──> WATCH SHORT (medium risk)
                    │
                    ├─── Penguin Score == 2 ──> WATCH LONG (low risk)
                    │
                    └─── No Strong Signal ──> HOLD (low risk)

KEY DIFFERENCES:
🔴 Burry SHORT: Looks for exhaustion in overbought conditions (RSI>80, MACD fade, weak ADX)
🟢 Penguin LONG: Looks for fear rotation during BTC dumps (altcoins decouple upward)

TRADING RULES:
✓ Only take Textbook or Solid conviction setups
✓ Avoid conflicting signals
✓ Use risk level to size position
✓ Textbook = max size, Watch = min size
"#
    );

    println!("\n{}", SEP);
    println!("INDICATOR THRESHOLDS");
    println!("{}", SEP);
    println!(
        r#"
Indicator        | Burry SHORT | Penguin LONG
─────────────────┼─────────────┼──────────────────
RSI (14)         | > 80        | 70-85
MACD Histogram   | < 0 (fade)  | > 0 (momentum)
ADX (14)         | < 30 (weak) | > 30 (trend)
Stochastic %K    | > 90        | (supporting)
Volume Ratio     | > 2.0       | 2-3x average
BTC Context      | flat/up     | -3% to -8% dump
─────────────────┴─────────────┴──────────────────

Score Thresholds:
5 signals = TEXTBOOK (trade with full size)
4 signals = TEXTBOOK (trade with full size)
3 signals = SOLID   (trade with reduced size)
2 signals = WATCH   (monitor, small size only)
0-1 signals = NO   (skip the trade)
"#
    );
}

fn demonstrate_burry_short() {
    // Create parabolic move with volume fade
    let mut series = CandleSeries::new("SOL".to_string());
    for i in 0..25 {
        let close = 100.0 + (i as f64 * 1.5);
        let volume = if i < 15 { 5000.0 - (i as f64 * 200.0) } else { 2000.0 };
        series.push(Candle::new(
            close - 0.5,
            close + 0.8,
            close - 0.6,
            close,
            volume,
            i as u64,
        ));
    }

    let snapshot = IndicatorSnapshot::from_candles(&series);
    let signal = Scorer::score_market(&snapshot, 142.0, 30.0, 91234.0, -1.7);

    println!(
        "Setup: SOL parabolic pump (+30% in 24h) while BTC flat (-1.7%)"
    );
    println!("Volume fading on the move (exhaustion pattern)\n");
    println!("{}", Scorer::format_signal(&signal));

    println!("\n📈 BREAKDOWN:");
    println!("   1. RSI {:.1} → Extreme overbought", snapshot.rsi.unwrap_or(0.0));
    println!(
        "   2. MACD Histogram {:.6} → Negative (momentum fade)",
        snapshot.macd_histogram.unwrap_or(0.0)
    );
    println!(
        "   3. ADX {:.1} → Weak trend (no support)",
        snapshot.adx.unwrap_or(0.0)
    );
    println!(
        "   4. Stochastic {:.1} → Overbought confirmation",
        snapshot.stochastic_k.unwrap_or(0.0)
    );
    println!(
        "   5. Volume {:.2}x → Blow-off top (conviction)",
        snapshot.volume_ratio.unwrap_or(0.0)
    );

    println!("\n⚖️ DECISION: This is a TEXTBOOK Burry SHORT setup");
    println!("   - All 5 signals present");
    println!("   - High risk/high reward");
    println!("   - Deploy 70-80% of account");
    println!("   - Target: 20-25% profit before TP");
    println!("   - Stop loss: 15% above entry (at resistance)");
}

fn demonstrate_penguin_long() {
    // Create ETH pump during BTC dump
    let mut series = CandleSeries::new("ETH".to_string());
    for i in 0..25 {
        let close = 100.0 + (i as f64 * 0.8);
        series.push(Candle::new(
            close - 0.3,
            close + 0.5,
            close - 0.4,
            close,
            1500.0 + (i as f64 * 50.0),
            i as u64,
        ));
    }

    let snapshot = IndicatorSnapshot::from_candles(&series);
    let signal = Scorer::score_market(&snapshot, 120.0, 18.0, 91234.0, -5.5);

    println!(
        "Setup: ETH pumping (+18% in 24h) while BTC dumping (-5.5%)"
    );
    println!("Classic fear rotation - money rotating from BTC to alts\n");
    println!("{}", Scorer::format_signal(&signal));

    println!("\n📈 BREAKDOWN:");
    println!("   1. BTC -5.5% → Perfect dump range (-3% to -8%)");
    println!("   2. ETH +18% → Fear rotation into altcoins");
    println!(
        "   3. RSI {:.1} → Strong momentum",
        snapshot.rsi.unwrap_or(0.0)
    );
    println!(
        "   4. ADX {:.1} → Trend support",
        snapshot.adx.unwrap_or(0.0)
    );
    println!(
        "   5. Volume {:.2}x → Conviction",
        snapshot.volume_ratio.unwrap_or(0.0)
    );

    println!("\n⚖️ DECISION: This is a TEXTBOOK Penguin LONG setup");
    println!("   - All 5 signals present");
    println!("   - Medium risk/high reward");
    println!("   - Deploy 50-70% of account");
    println!("   - Target: 25-35% profit before TP");
    println!("   - Stop loss: 8% below entry (break previous low)");
    println!("   - This is the PENGUIN EDGE - divergence plays");
}

fn demonstrate_no_signal() {
    // Create ranging market
    let mut series = CandleSeries::new("BTC".to_string());
    for i in 0..25 {
        let close = 100.0 + (if i % 2 == 0 { 0.5 } else { -0.5 });
        series.push(Candle::new(
            close - 0.2,
            close + 0.3,
            close - 0.3,
            close,
            1000.0,
            i as u64,
        ));
    }

    let snapshot = IndicatorSnapshot::from_candles(&series);
    let signal = Scorer::score_market(&snapshot, 100.5, 0.5, 91234.0, 0.2);

    println!("Setup: Ranging market, both BTC and alts sideways");
    println!("No clear direction or momentum\n");
    println!("{}", Scorer::format_signal(&signal));

    println!("\n📈 BREAKDOWN:");
    println!(
        "   - RSI {:.1} → Neutral zone (50)",
        snapshot.rsi.unwrap_or(0.0)
    );
    println!(
        "   - ADX {:.1} → Very weak trend",
        snapshot.adx.unwrap_or(0.0)
    );
    println!(
        "   - Volume {:.2}x → Normal",
        snapshot.volume_ratio.unwrap_or(0.0)
    );
    println!("   - BTC +0.2% → No clear direction");

    println!("\n⚖️ DECISION: SKIP THIS SETUP");
    println!("   - No strong signals (0/5 for both)");
    println!("   - Probability of success is low");
    println!("   - Better to wait for setup with 3+ signals");
}

fn demonstrate_watch_signals() {
    // Create early momentum
    let mut series = CandleSeries::new("LINK".to_string());
    for i in 0..25 {
        let close = 100.0 + (i as f64 * 0.3);
        series.push(Candle::new(
            close - 0.15,
            close + 0.25,
            close - 0.2,
            close,
            1200.0,
            i as u64,
        ));
    }

    let snapshot = IndicatorSnapshot::from_candles(&series);
    let signal = Scorer::score_market(&snapshot, 107.5, 7.5, 91234.0, -2.0);

    println!("Setup: Early divergence forming, not yet textbook");
    println!("BTC starting to dump, alts starting to rotate\n");
    println!("{}", Scorer::format_signal(&signal));

    println!("\n📈 BREAKDOWN:");
    println!(
        "   - BTC -2.0% → Within divergence range but early"
    );
    println!("   - LINK +7.5% → Not yet 10-25% pump range");
    println!(
        "   - RSI {:.1} → Good but not extreme",
        snapshot.rsi.unwrap_or(0.0)
    );
    println!("   - Early signals forming");

    println!("\n⚖️ DECISION: WATCH AND WAIT");
    println!("   - Score 2/5 = WATCH (not SOLID)");
    println!("   - If it becomes 3+ signals, enter with small size");
    println!("   - Otherwise, skip if price moves against us");
}
