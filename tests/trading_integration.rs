use penguin_trader::{
    Action, Conviction, IndicatorSnapshot, PaperEngine, Scorer,
};

/// Simulates a scan -> open -> close paper trade loop using the scoring engine and paper engine.
#[test]
fn paper_trade_flow_long_win() {
    // Build a textbook Penguin LONG snapshot
    let mut snapshot = IndicatorSnapshot::new("SOLUSDT".to_string());
    snapshot.rsi = Some(78.0);
    snapshot.adx = Some(40.0);
    snapshot.volume_ratio = Some(2.5);

    let signal = Scorer::score_market(
        &snapshot,
        142.0,   // current price
        18.0,    // alt 24h change
        60_000.0, // BTC price
        -5.0,    // BTC 24h change
    );

    assert_eq!(signal.action, Action::LONG);
    assert!(signal.conviction >= Conviction::Textbook);

    // Open and close using the paper engine
    let mut engine = PaperEngine::new(10_000.0);
    engine.set_price(&signal.symbol, signal.current_price);

    let margin = 7_000.0; // 70% per textbook sizing guidance
    let pos_id = engine
        .open_position(&signal.symbol, Action::LONG, margin, 35, 130.0, 170.0)
        .expect("failed to open position");

    // Move price in our favor and close
    engine.set_price(&signal.symbol, 155.0);
    let closed = engine
        .close_position(&pos_id, None)
        .expect("failed to close position");

    assert!(closed.realized_pnl > 0.0, "PnL should be positive after move");
    assert!(engine.realized_pnl > 0.0);
    assert!(engine.portfolio_heat() <= 0.01, "No heat after closing all positions");
}
