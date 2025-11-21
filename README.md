# 🐧 Penguin 2026 AI-Powered Paper Trading Engine

**Multi-coin margin trading CLI with AI signal detection, position analysis, and sizing recommendations**

Built in Rust for speed, safety, and reliability. Integrates Claude AI for market analysis and trade decision support.

---

## 🚀 Features

### Core Trading
- **Multi-Coin Support**: BTC, ETH, SOL, XRP, ADA, LINK
- **Long & Short Positions**: Open both simultaneously with up to 50x leverage
- **Stop Loss & Take Profit**: Mandatory on all positions
- **Liquidation Tracking**: Real-time liquidation price calculations
- **Position Sizing**: Risk-based sizing with fee accounting
- **Unrealized PnL**: Live profit/loss tracking on all positions
- **Portfolio Heat**: Know your exposure at all times
- **Trade Journal**: Complete history with signals and reasoning

### AI Integration (Powered by Claude)
- **🤖 AI Market Scanner**: Scans all 6 coins and identifies the best setup
- **📊 Signal Scoring**: Rates each setup 1-5 based on Penguin strategy
- **💡 Position Sizing AI**: Calculates optimal size and leverage based on signal strength
- **🔍 Position Analysis**: Reviews open positions and suggests adjustments
- **📈 Pattern Recognition**: Identifies divergence plays, tops, and momentum shifts
- **🧠 Trade Review**: Analyzes your trading patterns and suggests improvements

### Wallet Desk & On-Chain Ops
- **Generate/Import Wallets** for BTC, ETH, SOL, BNB (EVM), and XRP with live addresses
- **Refresh On-Chain Balances** via public RPC endpoints (mainnet)
- **Send Funds** directly from the CLI (BTC/ETH/BNB/XRP live broadcast; SOL send guarded)
- **Spot Desk**: Paper USD ↔︎ coin conversions logged alongside your trades
- **Safety First**: Seeds/passphrases are shown once and never stored; private keys stay local

### Unified Live / Paper Toggle
- **Toggle Mode**: Switch between **Paper Trading** (simulated) and **Live Trading** (real funds) in Settings.
- **Visual Indicator**: Header clearly shows "MODE: PAPER TRADING" (Green) or "MODE: LIVE TRADING" (Red).
- **Shared Storage**: Both modes use the same `wallets.db` but track positions separately.
- **Master Password Unlock**: Sending funds requires your master password.
- **Secure Claude Key Storage**: Set your Anthropic API key in Settings → Configure AI; it’s encrypted on disk in `anthropic_key.json`
- **Test Mode**: `PENGUIN_TEST_MODE=1` keeps paper mode deterministic (mocked balances/fees)

### GUI + Chart Deck
- **Chart Deck Tab**: Save chart snapshots for any coin/wallet pair with line or bar views
- **Shared Storage**: Charts live in the same `wallets.db` as wallets and respect Paper/Live mode
- **Overlay Lists**: Tag charts with overlays (RSI, volume, heat) and quick-filter the saved list
- **Roomy Layout**: GUI opens wider by default (~1280px) to give charts breathing room

---

## 📦 Installation

### Prerequisites
- Rust 1.70+ ([Install Rust](https://rustup.rs/))
- Anthropic API key (for AI features) - [Get key](https://console.anthropic.com/)

### Build from Source

```bash
# Clone the repository
git clone <your-repo-url>
cd penguin-trader

# Build release version
cargo build --release

# Run the CLI app
cargo run --release

# Run the GUI dashboard
cargo run --release --bin penguin-gui
```

### Set Up AI Integration

```bash
# Set your Anthropic API key
export ANTHROPIC_API_KEY="sk-ant-api03-..."

# On Windows (PowerShell)
$env:ANTHROPIC_API_KEY="sk-ant-api03-..."

# Or add to your shell profile (.bashrc, .zshrc, etc.)
echo 'export ANTHROPIC_API_KEY="sk-ant-api03-..."' >> ~/.bashrc
```

**Note:** The app works without an API key, but AI features will be disabled. You'll still have full paper trading functionality.

---

## 🎯 How AI Integration Works

### 1. AI Market Scanner
**What it does:**
- Fetches live prices for all 6 coins
- Analyzes BTC/altcoin divergence patterns
- Calculates signal strength (RSI momentum, volume, trend)
- Scores each potential trade 1-5
- Explains reasoning in Penguin's voice

**Example AI Output:**
```
BEST SETUP: ETHUSDT SHORT
Signals: RSI 82 (overbought), volume 3.2x average, BTC -4.2%
Score: 4/5
Reasoning: ETH pumping hard while BTC dumps - textbook divergence. 
Volume spike confirms momentum exhaustion. This is the top.
Suggested size: 70%
Suggested leverage: 45x
```

### 2. Position Sizing AI
**What it does:**
- Analyzes your available balance and current portfolio heat
- Applies Penguin sizing rules based on signal count
- Accounts for 3% entry fees
- Suggests optimal leverage for the setup

### 3. Position Analysis AI
**What it does:**
- Reviews your open position in detail
- Checks current price vs entry, stop, TP
- Evaluates risk/reward from current level
- Suggests: hold, partial profit, close, or adjust stops

---

## 🎮 Usage

### Quick Start

1. **Run the app**
   ```bash
   cargo run --release
   ```

2. **Use AI to scan for setups**
   - Select option 1: "🤖 AI Scan Market"
   - AI will analyze all coins and suggest best trade

3. **Open a position**
   - Select option 3 (LONG) or 4 (SHORT)
   - Choose coin and enter signals
   - AI will suggest optimal sizing (if configured)
   - Set your stop loss and take profit
   - Confirm trade

4. **Monitor positions**
   - Option 5: View all open positions with live PnL
   - Option 6: Manage/close positions
   - AI can analyze any position for you

5. **Review performance**
   - Option 7: View trade journal with stats

### Menu Options

```
=== PENGUIN 2026 AI-POWERED TRADING ENGINE ===

1. 🤖 AI Scan Market (Find Best Setup)
2. 📊 View Market Data
3. 📈 Open LONG Position
4. 📉 Open SHORT Position
5. 💼 View Open Positions
6. 🔧 Manage Position
7. 📖 View Trade Journal
8. ⚙️  Settings
0. 🚪 Exit
```

**Wallet Desk Menu** (Access via Settings -> Wallet Desk):
```
1. Generate Wallet (BTC, ETH, BNB, SOL, XRP)
2. List Wallets
3. Delete Wallet
4. Show Wallet Balance
5. Send Funds
0. Back
```

---

## 🧮 Technical Details

### Position Calculations

**Liquidation Price:**
- SHORT: `entry_price × (1 + (1 / leverage))`
- LONG: `entry_price × (1 - (1 / leverage))`

**Unrealized PnL:**
- SHORT: `(entry_price - current_price) × size`
- LONG: `(current_price - entry_price) × size`

**Fee Impact:**
- Entry fee: 3%
- Exit fee: 3%
- Total round-trip: 6% drag on profits

### Risk Management

**Portfolio Heat:**
- Calculation: `(total_margin_used / balance) × 100`
- Warning at 40%
- Max recommended: 60%

**Position Limits:**
- Max leverage: 50x
- Min margin: 
- Max single position: 80% of balance (on textbook setups)

---

## 🔐 Security Notes

**Data Storage:**
- All data stored locally in `penguin_data.json`
- No cloud sync
- **Private Keys**: Encrypted at rest using AES-128 (Fernet) with PBKDF2HMAC key derivation.
- **API Keys**: Encrypted on disk (`anthropic_key.json`) using AES-256-GCM.
- **Memory**: Sensitive data is cleared from memory when no longer needed (Rust) or handled via secure libraries (Python).
- **Verification**: On startup, you will be prompted for a password to decrypt your wallet database.

**Paper Trading:**
- No real money involved
- No real crypto wallets
- No actual exchanges connected
- 100% simulation using real prices

---

## 🗺️ Roadmap

See `TASKS.md` for the detailed backlog. High-level tracks:

- Technical indicators engine
- Hybrid strategy scorer (Burry SHORT + Penguin LONG)
- Wallet generation, encryption, and storage (multi-chain)
- Wallet operations (balance fetching, send flows)
- GUI dashboard (egui)
- Polish and future features (backtesting, CSV export, alerts, etc.)
- GUI chart deck (saved charts/overlays per wallet/asset; mini DEX screener)

### Quick Command Reference
```bash
# Run the main trading app
cargo run --release

# Launch the GUI dashboard
cargo run --release --bin penguin-gui

# See technical indicators in action
cargo run --release --bin indicators-example

# See strategy scoring live
cargo run --release --bin strategy-example

# Run all tests
cargo test --lib

# Build release binaries
cargo build --release

# Launch CLI and GUI together
./start.sh
```

---

## 📄 License

Do whatever you want with this code. No warranty. Trade at your own risk.

---

## 🙏 Credits

- Built for learning and practicing crypto trading strategies
- AI powered by Anthropic's Claude
- Market data from CoinGecko (free tier)
- Strategy based on Penguin from 2026 trading persona

---

**Happy Trading! 🐧📈**
