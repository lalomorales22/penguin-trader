// Penguin 2026 BTC/Altcoin Divergence LONG Strategy
// Exploits fear rotation patterns

use crate::indicators::IndicatorSnapshot;

/// Score a coin for Penguin divergence LONG setup
/// Returns (score, signals_list)
pub fn score_penguin_setup(
    snapshot: &IndicatorSnapshot,
    alt_change_24h: f64,
    btc_change_24h: f64,
) -> (u8, Vec<String>) {
    let mut score = 0u8;
    let mut signals = Vec::new();

    // Signal 1: BTC down -3% to -8% (fear conditions)
    if btc_change_24h < -3.0 && btc_change_24h > -8.0 {
        score += 1;
        signals.push(format!(
            "BTC {:.1}% (dumping -3% to -8%, fear rotation triggered)",
            btc_change_24h
        ));
    } else if btc_change_24h < -8.0 {
        signals.push(format!(
            "BTC {:.1}% (heavy dump <-8%, panic selling)",
            btc_change_24h
        ));
    } else if btc_change_24h >= -3.0 {
        signals.push(format!("BTC {:.1}% (not dumping, no fear rotation)", btc_change_24h));
    }

    // Signal 2: Alt up +10-25% (fear rotation into alts)
    if alt_change_24h > 10.0 && alt_change_24h < 25.0 {
        score += 1;
        signals.push(format!(
            "Alt +{:.1}% (10-25% pump, fear rotation into alts)",
            alt_change_24h
        ));
    } else if alt_change_24h >= 25.0 {
        signals.push(format!(
            "Alt +{:.1}% (extreme pump >25%, momentum exhaustion risk)",
            alt_change_24h
        ));
    } else if alt_change_24h <= 10.0 {
        signals.push(format!(
            "Alt {:.1}% (insufficient pump <10%, weak rotation)",
            alt_change_24h
        ));
    }

    // Signal 3: RSI 70-85 (momentum, not exhaustion)
    if let Some(rsi) = snapshot.rsi {
        if rsi >= 70.0 && rsi <= 85.0 {
            score += 1;
            signals.push(format!(
                "RSI {:.1} (70-85 range, momentum without exhaustion)",
                rsi
            ));
        } else if rsi > 85.0 {
            signals.push(format!(
                "RSI {:.1} (>85 exhaustion risk, may reverse soon)",
                rsi
            ));
        } else if rsi < 70.0 {
            signals.push(format!("RSI {:.1} (<70, not enough momentum)", rsi));
        }
    }

    // Signal 4: Volume 2-3x average (conviction in the move)
    if let Some(vol_ratio) = snapshot.volume_ratio {
        if vol_ratio >= 2.0 && vol_ratio <= 3.0 {
            score += 1;
            signals.push(format!(
                "Volume {:.2}x average (2-3x, strong conviction)",
                vol_ratio
            ));
        } else if vol_ratio > 3.0 {
            signals.push(format!(
                "Volume {:.2}x average (>3x, blow-off top risk)",
                vol_ratio
            ));
        } else if vol_ratio < 2.0 {
            signals.push(format!(
                "Volume {:.2}x average (<2x, weak conviction)",
                vol_ratio
            ));
        }
    }

    // Signal 5: ADX trend strength (support structure)
    if let Some(adx) = snapshot.adx {
        if adx > 30.0 && adx < 50.0 {
            score += 1;
            signals.push(format!(
                "ADX {:.1} (30-50, established trend with support)",
                adx
            ));
        } else if adx >= 50.0 {
            signals.push(format!("ADX {:.1} (>50, very strong trend)", adx));
        } else if adx <= 30.0 {
            signals.push(format!(
                "ADX {:.1} (≤30, weak/no trend, risky momentum play)",
                adx
            ));
        }
    }

    (score.min(5), signals)
}

/// Interpret Penguin LONG score as conviction level
pub fn penguin_conviction(score: u8) -> String {
    match score {
        5 => "textbook".to_string(),
        4 => "textbook".to_string(),
        3 => "solid".to_string(),
        2 => "watch".to_string(),
        _ => "no".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_penguin_textbook_divergence() {
        let mut snapshot = IndicatorSnapshot::new("TEST".to_string());
        snapshot.rsi = Some(78.0);           // Strong momentum
        snapshot.adx = Some(40.0);           // Established trend
        snapshot.volume_ratio = Some(2.5);  // Strong volume

        let (score, signals) = score_penguin_setup(&snapshot, 18.0, -5.0);
        assert_eq!(score, 5, "Textbook divergence should score 5");
        assert!(signals.len() >= 4);
        println!("Signals: {:?}", signals);
    }

    #[test]
    fn test_penguin_weak_divergence() {
        let mut snapshot = IndicatorSnapshot::new("TEST".to_string());
        snapshot.rsi = Some(55.0);          // Weak momentum
        snapshot.adx = Some(20.0);          // Very weak trend
        snapshot.volume_ratio = Some(1.0); // No volume

        let (score, _signals) = score_penguin_setup(&snapshot, 5.0, 2.0);
        assert!(score < 2, "Weak divergence should score < 2");
    }
}
