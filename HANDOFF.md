# 🐧 Penguin AI Trading Engine - Handoff (Phase 8 in progress)

**Status**: Phases 1-7 delivered; Phase 8 (Chart Deck) underway; Phase 9 planned.  
**Tests**: `cargo test --lib` → 51 passing.  
**Shared storage**: `wallets.db`, `penguin_data.json`, `anthropic_key.json` (all local, no cloud).

## Key Outcomes This Session
- Chart Deck scaffolding shipped: new GUI tab (Paper/Live aware) saves, lists/filters, and deletes chart snapshots with overlay tags and line/bar selection; GUI opens wider (~1280px).
- Chart storage layer added: `chart_snapshots` table in `wallets.db` plus `ChartSnapshot`/`ChartPoint` structs and `WalletStorage` helpers so charts persist across sessions.
- “Sync Shared DB” now also refreshes charts; Paper/Live toggle reloads charts for the active mode.

## Current Feature Snapshot
- Indicators + hybrid scorer (Burry SHORT + Penguin LONG).
- Wallet generation, encryption (Argon2id + AES-GCM), SQLite storage, multi-chain balance/send stubs.
- Paper engine for positions; live/paper shared persistence via `SharedDataStore`.
- Egui dashboard (scanner/positions/wallets/chart deck) with Go Live toggle and shared DB sync.

## Pending / Next Phases
- **Phase 8: GUI Chart Deck** — wire real price/volume history into snapshots, improve overlay selection UI, and render stored points on-canvas.
- **Phase 9: GUI Layout Polish** — widen/lock layout per design and ensure all panels bind to shared DB data.

## Important Files
- `penguin_ai_trader.rs`: CLI app, wallet desk, secure master/AI key handling, shared persistence.
- `src/ui/dashboard.rs`: GUI app with Go Live toggle, chart deck tab, shared DB sync, wallet send/save flows.
- `src/ui/chart_deck.rs`: Chart Deck tab UI (save/list/filter/delete snapshots).
- `src/wallet/storage.rs`: Wallet DB schema; now includes `chart_snapshots` table and helpers.
- `src/trading/persistence.rs`: SharedDataStore, shared app data structures.
- `README.md`, `TASKS.md`: updated with chart deck progress and current phase.

## How to Run
```bash
cargo test --lib
cargo run --release                # CLI
cargo run --release --bin penguin-gui  # GUI (Chart Deck tab)
# Configure Claude key: Settings -> Configure AI (encrypts to anthropic_key.json)
```

## Open Questions / Caveats
- ETH balance fetch still uses public Infura project ID; may rate-limit. Consider env override + fallback RPC.
- Send flows are framework-only; real signing/broadcast not wired.
- Chart deck currently persists snapshots with sample datapoints; real price/volume feeds still need to populate chart data.
