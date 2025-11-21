// Michael Burry's Overbought SHORT Strategy
// Detects exhaustion reversal patterns

use crate::indicators::IndicatorSnapshot;

/// Score a coin for Burry overbought SHORT setup
/// Returns (score, signals_list)
pub fn score_burry_setup(
    snapshot: &IndicatorSnapshot,
    btc_change_24h: f64,
) -> (u8, Vec<String>) {
    let mut score = 0u8;
    let mut signals = Vec::new();

    // Signal 1: RSI > 80 (extreme overbought)
    if let Some(rsi) = snapshot.rsi {
        if rsi > 80.0 {
            score += 1;
            signals.push(format!("RSI {:.1} (extreme overbought ≥80)", rsi));
        } else if rsi > 75.0 {
            signals.push(format!("RSI {:.1} (near overbought)", rsi));
        }
    }

    // Signal 2: MACD histogram turning negative (momentum fade)
    if let Some(hist) = snapshot.macd_histogram {
        if hist < 0.0 {
            score += 1;
            signals.push(format!("MACD histogram {:.6} (negative = momentum fade)", hist));
        } else if hist > -0.001 && hist < 0.001 {
            signals.push(format!("MACD near zero (divergence forming)"));
        }
    }

    // Signal 3: ADX < 30 (weak trend = no uptrend support)
    if let Some(adx) = snapshot.adx {
        if adx < 30.0 {
            score += 1;
            signals.push(format!("ADX {:.1} (weak trend <30, no uptrend support)", adx));
        }
    }

    // Signal 4: Stochastic > 90 (overbought confirmation)
    if let Some(k) = snapshot.stochastic_k {
        if k > 90.0 {
            score += 1;
            signals.push(format!("Stochastic %K {:.1} (extreme overbought ≥90)", k));
        } else if k > 80.0 {
            signals.push(format!("Stochastic %K {:.1} (overbought ≥80)", k));
        }
    }

    // Signal 5: Volume ratio > 2.0 (blow-off top)
    if let Some(vol_ratio) = snapshot.volume_ratio {
        if vol_ratio > 2.0 {
            score += 1;
            signals.push(format!("Volume {:.2}x average (blow-off top ≥2.0)", vol_ratio));
        } else if vol_ratio > 1.5 {
            signals.push(format!("Volume {:.2}x average (elevated)", vol_ratio));
        }
    }

    // Bonus Signal: BTC context (BTC flat <1% or down)
    if btc_change_24h < 1.0 && btc_change_24h > -8.0 {
        // BTC not dumping, good for alt shorts
        signals.push(format!("BTC {:.1}% (relatively flat/down, no support from BTC)", btc_change_24h));
    } else if btc_change_24h >= 1.0 {
        signals.push(format!(
            "BTC +{:.1}% (pumping - weak SHORT signal)",
            btc_change_24h
        ));
    }

    (score.min(5), signals)
}

/// Interpret Burry SHORT score as risk level
pub fn burry_risk_level(score: u8) -> String {
    match score {
        5 => "extreme".to_string(),
        4 => "high".to_string(),
        3 => "medium".to_string(),
        2 => "low".to_string(),
        _ => "none".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_burry_overbought_extreme() {
        let mut snapshot = IndicatorSnapshot::new("TEST".to_string());
        snapshot.rsi = Some(95.0);           // Extreme overbought
        snapshot.macd_histogram = Some(-0.5); // Negative
        snapshot.adx = Some(15.0);           // Weak trend
        snapshot.stochastic_k = Some(96.0); // Extreme overbought
        snapshot.volume_ratio = Some(3.5);  // Blow-off top

        let (score, signals) = score_burry_setup(&snapshot, 0.5);
        assert_eq!(score, 5, "Perfect Burry setup should score 5");
        assert!(signals.len() >= 5, "Should have at least 5 signals");
        println!("Signals: {:?}", signals);
    }

    #[test]
    fn test_burry_weak_signal() {
        let mut snapshot = IndicatorSnapshot::new("TEST".to_string());
        snapshot.rsi = Some(45.0);            // Not overbought
        snapshot.macd_histogram = Some(0.1);  // Positive
        snapshot.adx = Some(35.0);            // Moderate trend
        snapshot.stochastic_k = Some(50.0);  // Neutral
        snapshot.volume_ratio = Some(1.0);   // Normal

        let (score, _signals) = score_burry_setup(&snapshot, 5.0);
        assert!(score < 3, "Weak setup should score < 3");
    }
}
