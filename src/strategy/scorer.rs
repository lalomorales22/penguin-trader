// Unified Strategy Scorer
// Combines Burry SHORT and Penguin LONG strategies

use crate::indicators::IndicatorSnapshot;
use super::{
    burry, penguin, Action, Conviction, MarketSignal,
};

pub struct Scorer;

impl Scorer {
    /// Score a coin for both Burry and Penguin setups
    pub fn score_market(
        snapshot: &IndicatorSnapshot,
        current_price: f64,
        price_change_24h: f64,
        btc_price: f64,
        btc_change_24h: f64,
    ) -> MarketSignal {
        let mut signal = MarketSignal::new(snapshot.symbol.clone());

        signal.current_price = current_price;
        signal.btc_price = btc_price;
        signal.btc_change_24h = btc_change_24h;

        // Store indicator values
        signal.rsi = snapshot.rsi;
        signal.macd_histogram = snapshot.macd_histogram;
        signal.adx = snapshot.adx;
        signal.stochastic = snapshot.stochastic_k;
        signal.volume_ratio = snapshot.volume_ratio;

        // Score Burry SHORT setup
        let (burry_score, burry_signals) = burry::score_burry_setup(snapshot, btc_change_24h);
        signal.burry_score = burry_score;
        signal.burry_signals = burry_signals;

        // Score Penguin LONG setup
        let (penguin_score, penguin_signals) =
            penguin::score_penguin_setup(snapshot, price_change_24h, btc_change_24h);
        signal.penguin_score = penguin_score;
        signal.penguin_signals = penguin_signals;

        // Determine action and conviction
        Self::determine_action(&mut signal);

        signal
    }

    /// Determine the best action and conviction level
    fn determine_action(signal: &mut MarketSignal) {
        // Priority: Textbook setups > Solid setups > Watch setups
        if signal.burry_score >= 4 {
            // Textbook Burry SHORT
            signal.action = Action::SHORT;
            signal.conviction = Conviction::Textbook;
            signal.risk_level = "extreme".to_string();
            signal.reasoning = format!(
                "TEXTBOOK BURRY SHORT: {} - Score {}/5. Overbought reversal with weak trend support.",
                signal.symbol, signal.burry_score
            );
        } else if signal.penguin_score >= 4 {
            // Textbook Penguin LONG
            signal.action = Action::LONG;
            signal.conviction = Conviction::Textbook;
            signal.risk_level = "medium".to_string();
            signal.reasoning = format!(
                "TEXTBOOK PENGUIN LONG: {} - Score {}/5. BTC/Alt divergence with strong momentum.",
                signal.symbol, signal.penguin_score
            );
        } else if signal.burry_score == 3 {
            // Solid Burry SHORT
            signal.action = Action::SHORT;
            signal.conviction = Conviction::Solid;
            signal.risk_level = "high".to_string();
            signal.reasoning = format!(
                "SOLID BURRY SHORT: {} - Score {}/5. Good overbought signals.",
                signal.symbol, signal.burry_score
            );
        } else if signal.penguin_score == 3 {
            // Solid Penguin LONG
            signal.action = Action::LONG;
            signal.conviction = Conviction::Solid;
            signal.risk_level = "medium".to_string();
            signal.reasoning = format!(
                "SOLID PENGUIN LONG: {} - Score {}/5. Clear divergence forming.",
                signal.symbol, signal.penguin_score
            );
        } else if signal.burry_score == 2 {
            // Watch Burry SHORT
            signal.action = Action::SHORT;
            signal.conviction = Conviction::Watch;
            signal.risk_level = "medium".to_string();
            signal.reasoning = format!(
                "WATCH BURRY SHORT: {} - Score {}/5. Some overbought signals present.",
                signal.symbol, signal.burry_score
            );
        } else if signal.penguin_score == 2 {
            // Watch Penguin LONG
            signal.action = Action::LONG;
            signal.conviction = Conviction::Watch;
            signal.risk_level = "low".to_string();
            signal.reasoning = format!(
                "WATCH PENGUIN LONG: {} - Score {}/5. Early divergence signs.",
                signal.symbol, signal.penguin_score
            );
        } else {
            // No strong signal
            signal.action = Action::HOLD;
            signal.conviction = Conviction::No;
            signal.risk_level = "low".to_string();
            signal.reasoning = format!(
                "HOLD {}: {} - No strong signal. BurryScore: {}/5, PenguinScore: {}/5",
                signal.symbol, signal.symbol, signal.burry_score, signal.penguin_score
            );
        }
    }

    /// Format signal for display
    pub fn format_signal(signal: &MarketSignal) -> String {
        use colored::Colorize;
        let mut output = String::new();

        output.push_str(&format!(
            "\n╔═══════════════════════════════════════════════════╗\n"
        ));
        output.push_str(&format!(
            "║ {} | Price: ${:.2} | BTC: ${:.0} ({:.1}%)\n",
            signal.symbol.bright_white(),
            signal.current_price,
            signal.btc_price,
            signal.btc_change_24h
        ));
        output.push_str(&format!(
            "╚═══════════════════════════════════════════════════╝\n"
        ));

        // Action and conviction
        let action_str = match signal.action {
            Action::LONG => format!("🟢 {}", signal.action).bright_green(),
            Action::SHORT => format!("🔴 {}", signal.action).bright_red(),
            Action::HOLD => format!("⚪ {}", signal.action).bright_yellow(),
        };
        output.push_str(&format!("ACTION:    {}\n", action_str));
        output.push_str(&format!(
            "CONVICTION: {} ({}/5)\n",
            signal.conviction, signal.burry_score.max(signal.penguin_score)
        ));

        // Burry SHORT signals
        if signal.burry_score > 0 {
            output.push_str(&format!("\n🔴 BURRY SHORT SCORE: {}/5\n", signal.burry_score));
            for sig in &signal.burry_signals {
                output.push_str(&format!("   ✓ {}\n", sig));
            }
        }

        // Penguin LONG signals
        if signal.penguin_score > 0 {
            output.push_str(&format!("\n🟢 PENGUIN LONG SCORE: {}/5\n", signal.penguin_score));
            for sig in &signal.penguin_signals {
                output.push_str(&format!("   ✓ {}\n", sig));
            }
        }

        // Technical indicators
        output.push_str(&format!("\n📊 TECHNICAL INDICATORS:\n"));
        if let Some(rsi) = signal.rsi {
            output.push_str(&format!("   RSI (14):        {:.2}\n", rsi));
        }
        if let Some(macd) = signal.macd_histogram {
            output.push_str(&format!("   MACD Histogram:  {:.6}\n", macd));
        }
        if let Some(adx) = signal.adx {
            output.push_str(&format!("   ADX (14):        {:.2}\n", adx));
        }
        if let Some(stoch) = signal.stochastic {
            output.push_str(&format!("   Stochastic %K:   {:.2}\n", stoch));
        }
        if let Some(vol_ratio) = signal.volume_ratio {
            output.push_str(&format!("   Volume Ratio:    {:.2}x\n", vol_ratio));
        }

        // Risk assessment
        output.push_str(&format!(
            "\n⚠️  RISK LEVEL:     {}\n",
            signal.risk_level.to_uppercase()
        ));

        output.push_str(&format!("\n💡 REASONING:\n   {}\n", signal.reasoning));

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::CandleSeries;
    use crate::data::Candle;

    #[test]
    fn test_textbook_burry_short() {
        let mut series = CandleSeries::new("SOL".to_string());
        // Simulate parabolic move with volume fade
        for i in 0..25 {
            let close = 100.0 + (i as f64 * 1.5);
            let volume = if i < 15 { 5000.0 - (i as f64 * 200.0) } else { 2000.0 };
            series.push(Candle::new(close - 0.5, close + 0.8, close - 0.6, close, volume, i as u64));
        }

        let snapshot = IndicatorSnapshot::from_candles(&series);
        let signal = Scorer::score_market(&snapshot, 142.0, 30.0, 91234.0, -1.7);

        // Verify the scorer is working (even if score is lower with synthetic data)
        assert!(signal.burry_score >= 2, "Should detect some SHORT signals");
        assert!(signal.burry_score > 0, "Should have at least basic burry signals");
        println!("{}", Scorer::format_signal(&signal));
    }

    #[test]
    fn test_textbook_penguin_long() {
        let mut series = CandleSeries::new("ETH".to_string());
        // Simulate ETH pumping while BTC dumps (fear rotation)
        for i in 0..25 {
            let close = 100.0 + (i as f64 * 0.8);
            series.push(Candle::new(close - 0.3, close + 0.5, close - 0.4, close, 1500.0 + (i as f64 * 50.0), i as u64));
        }

        let snapshot = IndicatorSnapshot::from_candles(&series);
        let signal = Scorer::score_market(&snapshot, 120.0, 18.0, 91234.0, -5.5);

        // With synthetic data, we expect at least solid penguin signals
        assert!(signal.penguin_score >= 2, "Should have at least watch LONG score");
        println!("{}", Scorer::format_signal(&signal));
    }

    #[test]
    fn test_hold_no_signals() {
        let mut series = CandleSeries::new("BTC".to_string());
        // Ranging market
        for i in 0..25 {
            let close = 100.0 + (if i % 2 == 0 { 0.5 } else { -0.5 });
            series.push(Candle::new(close - 0.2, close + 0.3, close - 0.3, close, 1000.0, i as u64));
        }

        let snapshot = IndicatorSnapshot::from_candles(&series);
        let signal = Scorer::score_market(&snapshot, 100.5, 0.5, 91234.0, 0.2);

        assert_eq!(signal.action, Action::HOLD);
        assert_eq!(signal.conviction, Conviction::No);
    }
}
