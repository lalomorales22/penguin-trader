#!/usr/bin/env python3
"""
PENGUIN 2026 PAPER TRADING ENGINE
Multi-coin margin trading simulator with long/short positions, leverage, and full PnL tracking
Trade like Penguin from 2026: Textbook tops, high conviction, patient holds
"""

import json
import os
import re
import sqlite3
import sys
import textwrap
import time
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional, Tuple

import requests
from cryptography.fernet import Fernet
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC
import base64
import getpass

# Check for required libraries
try:
    from bit import Key
    from bit.format import bytes_to_wif
except ImportError:
    print("\n❌ Missing required library: bit")
    print("Install with: pip install bit requests colorama web3 solders base58 --break-system-packages\n")
    sys.exit(1)

try:
    from colorama import Fore, Style, init
    init(autoreset=True)
except ImportError:
    print("\n⚠️  colorama not found - using plain text")
    class Fore:
        GREEN = RED = YELLOW = CYAN = MAGENTA = BLUE = WHITE = ""
    class Style:
        BRIGHT = RESET_ALL = ""

# Optional wallet libraries
try:
    from web3 import Web3
    from eth_account import Account
    ETH_AVAILABLE = True
except ImportError:
    ETH_AVAILABLE = False

try:
    from solders.keypair import Keypair
    import base58
    SOL_AVAILABLE = True
except ImportError:
    SOL_AVAILABLE = False

try:
    from xrpl.wallet import Wallet as XRPLWallet
    XRP_AVAILABLE = True
except ImportError:
    XRP_AVAILABLE = False

DATA_FILE = "penguin_trader_data.json"
DATABASE_FILE = "penguin_trader.db"
AI_COMMAND_PREFIX = "COMMAND:"


def make_default_state() -> Dict:
    """Return a new default state dictionary for persistence layers."""
    now = datetime.now(timezone.utc).isoformat()
    return {
        "wallet": None,
        "private_key_shown": False,
        "wallets": {},
        "portfolio": {
            "balance": 10000.00,
            "realizedPnL": 0.0,
            "createdAt": now
        },
        "openPositions": [],
        "closedTrades": [],
        "tradeJournal": [],
        "spotTransactions": []
    }

# Supported coins (picked for BTC correlation and alt season patterns)
SUPPORTED_COINS = {
    "BTC": "bitcoin",
    "ETH": "ethereum",
    "SOL": "solana",
    "BNB": "binancecoin",
    "XRP": "ripple",
    "ADA": "cardano",
    "LINK": "chainlink"
}

# Session time windows (UTC)
SESSION_WINDOWS = {
    "ASIAN": (0, 8),
    "LONDON": (8, 13),
    "US": (13, 21),
    "US_CLOSE": (21, 24)
}


class EncryptionManager:
    def __init__(self, password: str, salt: bytes = None):
        self.salt = salt or os.urandom(16)
        self.key = self._derive_key(password, self.salt)
        self.fernet = Fernet(self.key)

    def _derive_key(self, password: str, salt: bytes) -> bytes:
        kdf = PBKDF2HMAC(
            algorithm=hashes.SHA256(),
            length=32,
            salt=salt,
            iterations=480000,
        )
        return base64.urlsafe_b64encode(kdf.derive(password.encode()))

    def encrypt(self, data: str) -> str:
        if not data: return ""
        return self.fernet.encrypt(data.encode()).decode()

    def decrypt(self, token: str) -> str:
        if not token: return ""
        try:
            return self.fernet.decrypt(token.encode()).decode()
        except Exception:
            return ""


class PenguinDatabase:
    """SQLite persistence for Penguin Trader state and AI chat."""

    def __init__(self, db_path: str = DATABASE_FILE, encryption_manager: Optional[EncryptionManager] = None):
        self.db_path = db_path
        self.encryption_manager = encryption_manager
        self.conn = sqlite3.connect(self.db_path)
        self.conn.row_factory = sqlite3.Row
        self.conn.execute("PRAGMA journal_mode=WAL;")
        self.conn.execute("PRAGMA synchronous=NORMAL;")
        self.seeded_from_default = False
        self._ensure_schema()

    def _ensure_schema(self):
        cur = self.conn.cursor()
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS portfolio (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                balance REAL NOT NULL,
                realized_pnl REAL NOT NULL,
                created_at TEXT NOT NULL
            )
            """
        )
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS open_positions (
                position_id TEXT PRIMARY KEY,
                symbol TEXT NOT NULL,
                position_type TEXT NOT NULL,
                entry_price REAL NOT NULL,
                size REAL NOT NULL,
                margin_used REAL NOT NULL,
                leverage INTEGER NOT NULL,
                stop_loss REAL NOT NULL,
                take_profit REAL NOT NULL,
                liquidation_price REAL NOT NULL,
                opened_at TEXT NOT NULL,
                signals TEXT,
                reasoning TEXT,
                unrealized_pnl REAL DEFAULT 0,
                unrealized_pct REAL DEFAULT 0
            )
            """
        )
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS closed_trades (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                position_id TEXT,
                symbol TEXT,
                position_type TEXT,
                entry_price REAL,
                exit_price REAL,
                size REAL,
                margin_used REAL,
                leverage INTEGER,
                realized_pnl REAL,
                realized_pct REAL,
                opened_at TEXT,
                closed_at TEXT,
                close_reason TEXT,
                signals TEXT,
                reasoning TEXT
            )
            """
        )
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS trade_journal (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                action TEXT NOT NULL,
                symbol TEXT,
                position_type TEXT,
                entry_price REAL,
                exit_price REAL,
                size REAL,
                leverage INTEGER,
                margin_used REAL,
                stop_loss REAL,
                take_profit REAL,
                realized_pnl REAL,
                realized_pct REAL,
                signals TEXT,
                reasoning TEXT,
                outcome TEXT,
                close_reason TEXT
            )
            """
        )
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS wallets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol TEXT NOT NULL UNIQUE,
                address TEXT,
                private_key TEXT,
                label TEXT,
                spot_balance REAL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            """
        )
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS spot_transactions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol TEXT NOT NULL,
                side TEXT NOT NULL,
                amount REAL NOT NULL,
                usd_value REAL NOT NULL,
                source TEXT,
                created_at TEXT NOT NULL
            )
            """
        )
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS app_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
            """
        )
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS ai_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL
            )
            """
        )
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS encryption_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                salt BLOB NOT NULL,
                verification_hash TEXT NOT NULL
            )
            """
        )
        self._ensure_column("spot_transactions", "source", "TEXT")
        self.conn.commit()

    def _ensure_column(self, table: str, column: str, column_type: str):
        cur = self.conn.cursor()
        existing = cur.execute(f"PRAGMA table_info({table})").fetchall()
        if not any(row["name"] == column for row in existing):
            cur.execute(f"ALTER TABLE {table} ADD COLUMN {column} {column_type}")
            self.conn.commit()

    def load_state(self) -> Dict:
        """Load latest state snapshot from SQLite."""
        state = make_default_state()
        cur = self.conn.cursor()
        portfolio = cur.execute("SELECT balance, realized_pnl, created_at FROM portfolio WHERE id = 1").fetchone()
        if portfolio:
            state["portfolio"] = {
                "balance": portfolio["balance"],
                "realizedPnL": portfolio["realized_pnl"],
                "createdAt": portfolio["created_at"]
            }
        else:
            # First launch: seed defaults into DB
            self.seeded_from_default = True
            self.save_state(state)
            return state

        wallets = cur.execute("SELECT symbol, address, private_key, label, spot_balance, created_at, updated_at FROM wallets").fetchall()
        for row in wallets:
            state["wallets"][row["symbol"]] = {
                "address": row["address"],
                "privateKey": self.encryption_manager.decrypt(row["private_key"]) if self.encryption_manager else row["private_key"],
                "label": row["label"],
                "spotBalance": row["spot_balance"],
                "createdAt": row["created_at"],
                "updatedAt": row["updated_at"]
            }

        open_positions = cur.execute(
            """
            SELECT position_id, symbol, position_type, entry_price, size, margin_used, leverage,
                   stop_loss, take_profit, liquidation_price, opened_at, signals, reasoning,
                   unrealized_pnl, unrealized_pct
            FROM open_positions
            """
        ).fetchall()
        for row in open_positions:
            state["openPositions"].append({
                "positionId": row["position_id"],
                "symbol": row["symbol"],
                "type": row["position_type"],
                "entryPrice": row["entry_price"],
                "size": row["size"],
                "marginUsed": row["margin_used"],
                "leverage": row["leverage"],
                "stopLoss": row["stop_loss"],
                "takeProfit": row["take_profit"],
                "liquidationPrice": row["liquidation_price"],
                "openedAt": row["opened_at"],
                "signals": row["signals"],
                "reasoning": row["reasoning"],
                "unrealizedPnL": row["unrealized_pnl"],
                "unrealizedPnLPercent": row["unrealized_pct"]
            })

        closed_trades = cur.execute(
            """
            SELECT position_id, symbol, position_type, entry_price, exit_price, size, margin_used, leverage,
                   realized_pnl, realized_pct, opened_at, closed_at, close_reason, signals, reasoning
            FROM closed_trades ORDER BY id ASC
            """
        ).fetchall()
        for row in closed_trades:
            state["closedTrades"].append({
                "positionId": row["position_id"],
                "symbol": row["symbol"],
                "type": row["position_type"],
                "entryPrice": row["entry_price"],
                "exitPrice": row["exit_price"],
                "size": row["size"],
                "marginUsed": row["margin_used"],
                "leverage": row["leverage"],
                "realizedPnL": row["realized_pnl"],
                "realizedPnLPercent": row["realized_pct"],
                "openedAt": row["opened_at"],
                "closedAt": row["closed_at"],
                "closeReason": row["close_reason"],
                "signals": row["signals"],
                "reasoning": row["reasoning"]
            })

        journal_rows = cur.execute(
            """
            SELECT timestamp, action, symbol, position_type, entry_price, exit_price, size, leverage,
                   margin_used, stop_loss, take_profit, realized_pnl, realized_pct, signals, reasoning,
                   outcome, close_reason
            FROM trade_journal ORDER BY id ASC
            """
        ).fetchall()
        for row in journal_rows:
            state["tradeJournal"].append({
                "timestamp": row["timestamp"],
                "action": row["action"],
                "symbol": row["symbol"],
                "type": row["position_type"],
                "entryPrice": row["entry_price"],
                "exitPrice": row["exit_price"],
                "size": row["size"],
                "leverage": row["leverage"],
                "marginUsed": row["margin_used"],
                "stopLoss": row["stop_loss"],
                "takeProfit": row["take_profit"],
                "realizedPnL": row["realized_pnl"],
                "realizedPnLPercent": row["realized_pct"],
                "signals": row["signals"],
                "reasoning": row["reasoning"],
                "outcome": row["outcome"],
                "closeReason": row["close_reason"]
            })

        spot_rows = cur.execute("SELECT symbol, side, amount, usd_value, source, created_at FROM spot_transactions ORDER BY id ASC").fetchall()
        for row in spot_rows:
            state["spotTransactions"].append({
                "symbol": row["symbol"],
                "side": row["side"],
                "amount": row["amount"],
                "usdValue": row["usd_value"],
                "source": row["source"],
                "createdAt": row["created_at"]
            })

        meta_rows = cur.execute("SELECT key, value FROM app_meta").fetchall()
        for row in meta_rows:
            if row["key"] == "private_key_shown":
                state["private_key_shown"] = row["value"] == "1"
            elif row["key"] == "btc_wallet":
                try:
                    state["wallet"] = json.loads(row["value"])
                except json.JSONDecodeError:
                    state["wallet"] = None

        return state

    def save_state(self, data: Dict):
        """Persist current snapshot to SQLite."""
        cur = self.conn.cursor()
        portfolio = data["portfolio"]
        cur.execute(
            """
            INSERT INTO portfolio (id, balance, realized_pnl, created_at)
            VALUES (1, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                balance = excluded.balance,
                realized_pnl = excluded.realized_pnl,
                created_at = excluded.created_at
            """,
            (portfolio["balance"], portfolio["realizedPnL"], portfolio["createdAt"])
        )

        cur.execute("DELETE FROM open_positions")
        cur.executemany(
            """
            INSERT INTO open_positions (
                position_id, symbol, position_type, entry_price, size, margin_used, leverage,
                stop_loss, take_profit, liquidation_price, opened_at, signals, reasoning,
                unrealized_pnl, unrealized_pct
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            [
                (
                    pos["positionId"],
                    pos["symbol"],
                    pos["type"],
                    pos["entryPrice"],
                    pos["size"],
                    pos["marginUsed"],
                    pos["leverage"],
                    pos["stopLoss"],
                    pos["takeProfit"],
                    pos["liquidationPrice"],
                    pos["openedAt"],
                    pos.get("signals"),
                    pos.get("reasoning"),
                    pos.get("unrealizedPnL", 0),
                    pos.get("unrealizedPnLPercent", 0)
                )
                for pos in data["openPositions"]
            ]
        )

        cur.execute("DELETE FROM closed_trades")
        cur.executemany(
            """
            INSERT INTO closed_trades (
                position_id, symbol, position_type, entry_price, exit_price, size, margin_used,
                leverage, realized_pnl, realized_pct, opened_at, closed_at, close_reason,
                signals, reasoning
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            [
                (
                    trade.get("positionId"),
                    trade.get("symbol"),
                    trade.get("type"),
                    trade.get("entryPrice"),
                    trade.get("exitPrice"),
                    trade.get("size"),
                    trade.get("marginUsed"),
                    trade.get("leverage"),
                    trade.get("realizedPnL"),
                    trade.get("realizedPnLPercent"),
                    trade.get("openedAt"),
                    trade.get("closedAt"),
                    trade.get("closeReason"),
                    trade.get("signals"),
                    trade.get("reasoning")
                )
                for trade in data["closedTrades"]
            ]
        )

        cur.execute("DELETE FROM trade_journal")
        cur.executemany(
            """
            INSERT INTO trade_journal (
                timestamp, action, symbol, position_type, entry_price, exit_price, size, leverage,
                margin_used, stop_loss, take_profit, realized_pnl, realized_pct, signals, reasoning,
                outcome, close_reason
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            [
                (
                    entry.get("timestamp"),
                    entry.get("action"),
                    entry.get("symbol"),
                    entry.get("type"),
                    entry.get("entryPrice"),
                    entry.get("exitPrice"),
                    entry.get("size"),
                    entry.get("leverage"),
                    entry.get("marginUsed"),
                    entry.get("stopLoss"),
                    entry.get("takeProfit"),
                    entry.get("realizedPnL"),
                    entry.get("realizedPnLPercent"),
                    entry.get("signals"),
                    entry.get("reasoning"),
                    entry.get("outcome"),
                    entry.get("closeReason")
                )
                for entry in data["tradeJournal"]
            ]
        )

        cur.execute("DELETE FROM wallets")
        cur.executemany(
            """
            INSERT INTO wallets (symbol, address, private_key, label, spot_balance, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            [
                (
                    symbol,
                    wallet.get("address"),
                    self.encryption_manager.encrypt(wallet.get("privateKey")) if self.encryption_manager else wallet.get("privateKey"),
                    wallet.get("label"),
                    wallet.get("spotBalance", 0.0),
                    wallet.get("createdAt", datetime.now(timezone.utc).isoformat()),
                    wallet.get("updatedAt", datetime.now(timezone.utc).isoformat())
                )
                for symbol, wallet in data["wallets"].items()
            ]
        )

        cur.execute("DELETE FROM spot_transactions")
        cur.executemany(
            """
            INSERT INTO spot_transactions (symbol, side, amount, usd_value, source, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            """,
            [
                (
                    tx.get("symbol"),
                    tx.get("side"),
                    tx.get("amount"),
                    tx.get("usdValue"),
                    tx.get("source"),
                    tx.get("createdAt")
                )
                for tx in data.get("spotTransactions", [])
            ]
        )

        meta_entries = {
            "private_key_shown": "1" if data.get("private_key_shown") else "0",
            "btc_wallet": json.dumps(data.get("wallet")) if data.get("wallet") else ""
        }
        cur.execute("DELETE FROM app_meta")
        cur.executemany(
            "INSERT INTO app_meta (key, value) VALUES (?, ?)",
            list(meta_entries.items())
        )

        self.conn.commit()

    def append_ai_message(self, role: str, content: str):
        cur = self.conn.cursor()
        cur.execute(
            "INSERT INTO ai_messages (role, content, created_at) VALUES (?, ?, ?)",
            (role, content, datetime.now(timezone.utc).isoformat())
        )
        self.conn.commit()
        self._prune_ai_history()

    def get_ai_messages(self, limit: int = 20) -> List[Dict[str, str]]:
        cur = self.conn.cursor()
        rows = cur.execute(
            "SELECT role, content FROM ai_messages ORDER BY id DESC LIMIT ?",
            (limit,)
        ).fetchall()
        return [{"role": row["role"], "content": row["content"]} for row in reversed(rows)]

    def _prune_ai_history(self, max_messages: int = 200):
        cur = self.conn.cursor()
        row = cur.execute("SELECT COUNT(*) AS cnt FROM ai_messages").fetchone()
        if row and row["cnt"] > max_messages:
            to_delete = row["cnt"] - max_messages
            cur.execute("DELETE FROM ai_messages WHERE id IN (SELECT id FROM ai_messages ORDER BY id ASC LIMIT ?)", (to_delete,))
            self.conn.commit()

    def clear_ai_messages(self):
        cur = self.conn.cursor()
        cur.execute("DELETE FROM ai_messages")
        self.conn.commit()

    def close(self):  # pragma: no cover - convenience cleanup
        try:
            self.conn.close()
        except Exception:
            pass

    def __del__(self):  # pragma: no cover - ensure connection closes
        self.close()


class AITradingConsole:
    """Simple chat UI that lets Ollama issue structured trade commands."""

    def __init__(self, trader: "PenguinTrader"):
        self.trader = trader
        self.db = trader.db
        host = os.environ.get("OLLAMA_HOST", "http://localhost:11434").rstrip("/")
        self.base_url = host if host.startswith("http") else f"http://{host}"
        self.model = os.environ.get("OLLAMA_MODEL", "gemma3:1b")

    def start(self):
        print(f"\n{Fore.CYAN}{'='*70}")
        print(f"{Fore.YELLOW}{Style.BRIGHT}AI TRADING CONSOLE (Ollama: {self.model})")
        print(f"{Fore.CYAN}{'='*70}")
        print(f"{Fore.WHITE}Type trading instructions in natural language. Commands:\n"
              f"  'exit' to return | '/clear' to reset AI memory.")

        history = self.db.get_ai_messages(limit=10)
        if history:
            print(f"\n{Fore.WHITE}Last AI chat snippets:")
            for msg in history:
                prefix = "You" if msg["role"] == "user" else "AI"
                print(f"  {Fore.CYAN}{prefix}{Fore.WHITE}: {msg['content']}")

        while True:
            user_input = input(f"\n{Fore.GREEN}You › {Fore.WHITE}").strip()
            if not user_input:
                continue
            lower = user_input.lower()
            if lower in {"exit", "back", "quit"}:
                return
            if lower == "/clear":
                self.db.clear_ai_messages()
                print(f"{Fore.YELLOW}✓ Cleared AI chat history")
                continue

            response = self._query_model(user_input)
            if response is None:
                continue

            print(f"{Fore.MAGENTA}\nAI › {Fore.WHITE}{response}\n")
            self.db.append_ai_message("user", user_input)
            self.db.append_ai_message("assistant", response)

            commands = self._extract_commands(response)
            if commands:
                print(f"{Fore.YELLOW}{'='*70}")
                print(f"{Fore.YELLOW}{Style.BRIGHT}🤖 AI GENERATED {len(commands)} COMMAND(S) - AUTHORIZATION REQUIRED")
                print(f"{Fore.YELLOW}{'='*70}")
            
            executed_count = 0
            cancelled_count = 0
            
            for cmd in commands:
                was_executed = self._handle_command(cmd)
                if was_executed:
                    executed_count += 1
                else:
                    cancelled_count += 1
            
            if commands:
                print(f"\n{Fore.CYAN}{'='*70}")
                if executed_count > 0:
                    print(f"{Fore.GREEN}{Style.BRIGHT}✓ {executed_count} command(s) executed successfully")
                if cancelled_count > 0:
                    print(f"{Fore.RED}✗ {cancelled_count} command(s) cancelled or failed")
                print(f"{Fore.CYAN}{'='*70}\n")

    def _query_model(self, user_input: str) -> Optional[str]:
        messages = [{"role": "system", "content": self._system_prompt()}]
        messages.extend(self.db.get_ai_messages(limit=20))
        messages.append({"role": "user", "content": user_input})
        payload = {"model": self.model, "messages": messages, "stream": False}

        try:
            response = requests.post(f"{self.base_url}/api/chat", json=payload, timeout=120)
            response.raise_for_status()
            data = response.json()
        except requests.RequestException as exc:
            print(f"{Fore.RED}AI request failed: {exc}")
            print(f"{Fore.YELLOW}Ensure Ollama is running: ollama run {self.model}")
            return None

        message = data.get("message", {})
        content = message.get("content")
        if isinstance(content, list):
            content = "".join(part.get("text", "") if isinstance(part, dict) else str(part) for part in content)
        if not isinstance(content, str):
            print(f"{Fore.RED}Unexpected AI response payload")
            return None
        return content.strip()

    def _system_prompt(self) -> str:
        data = self.trader.data
        balance = data["portfolio"]["balance"]
        realized = data["portfolio"]["realizedPnL"]
        open_positions = data["openPositions"]
        wallets = data.get("wallets", {})

        open_lines = [
            f"{pos['type']} {pos['symbol']} @ ${pos['entryPrice']:.2f} | {pos['leverage']}x | stop {pos['stopLoss']:.2f} | tp {pos['takeProfit']:.2f}"
            for pos in open_positions
        ]
        wallet_lines = [
            f"{symbol}: {wallet.get('spotBalance', 0):.6f} units"
            for symbol, wallet in wallets.items()
            if wallet.get("spotBalance")
        ]
        open_summary = "; ".join(open_lines) if open_lines else "none"
        wallet_summary = "; ".join(wallet_lines) if wallet_lines else "empty"

        instruction = textwrap.dedent(
            f"""
            You are an expert cryptocurrency trading assistant for Penguin Trader. You communicate naturally and conversationally while having the ability to execute trades.
            
            CURRENT PORTFOLIO STATUS:
            - Available Balance: ${balance:,.2f}
            - Realized PnL: ${realized:,.2f}
            - Open Positions: {open_summary}
            - Spot Wallet Balances: {wallet_summary}
            
            COMMUNICATION STYLE:
            - Respond in natural, conversational language
            - Be helpful, concise, and professional
            - Explain your trading suggestions clearly
            - When executing trades, describe what you're doing in plain English
            
            TRADING CAPABILITIES:
            You have the following tools available to execute trades. Each command requires manual authorization from the user.
            
            1. Open Position (Long/Short with leverage):
               COMMAND: open_position {{"action":"open_position","symbol":"BTC","side":"LONG","risk_pct":5,"leverage":10,"stop_loss":61000,"take_profit":68000,"signals":"AI","reasoning":"text"}}
            
            2. Close Position:
               COMMAND: close_position {{"action":"close_position","position_id":"abcd1234","percent":100}}
               OR
               COMMAND: close_position {{"action":"close_position","symbol":"ETH","percent":50}}
            
            3. Buy Spot (convert USD to crypto):
               COMMAND: buy_spot {{"action":"buy_spot","symbol":"ETH","usd_amount":500}}
            
            4. Sell Spot (convert crypto to USD):
               COMMAND: sell_spot {{"action":"sell_spot","symbol":"SOL","percent":50}}
            
            IMPORTANT INSTRUCTIONS:
            - First, respond to the user in natural language explaining what you understand and what you plan to do
            - If the user wants to execute a trade, include the COMMAND line AFTER your natural language explanation
            - NEVER output just the COMMAND without explanation
            - After you include a COMMAND, the system will pause and ask the user to authorize it
            - The user must type 'yes' in the terminal (not in chat) to authorize the command
            - Example good response: "I'll open a long position on ETH with 10x leverage. Based on current market conditions at $3,000, this looks like a good entry point. COMMAND: open_position {{"action":"open_position","symbol":"ETH","side":"LONG","risk_pct":5,"leverage":10,"stop_loss":2800,"take_profit":3200,"signals":"AI","reasoning":"Good entry at support level"}}"
            - Example bad response: "COMMAND: open_position {{"action":"open_position",...}}"
            
            When users ask to trade, always:
            1. Acknowledge their request in natural language
            2. Explain what you're doing and why (entry point, risk level, strategy)
            3. Then include the COMMAND to execute
            4. Remember: commands need terminal authorization, not chat confirmation
            
            You can execute multiple commands in one response by including multiple COMMAND lines.
            """
        ).strip()
        return instruction

    def _extract_commands(self, text: str) -> List[Dict]:
        commands = []
        start = 0
        while True:
            idx = text.find(AI_COMMAND_PREFIX, start)
            if idx == -1:
                break
            idx += len(AI_COMMAND_PREFIX)
            json_blob, end_idx = self._extract_json_block(text, idx)
            start = end_idx
            if not json_blob:
                continue
            try:
                commands.append(json.loads(json_blob))
            except json.JSONDecodeError:
                print(f"{Fore.YELLOW}AI produced malformed command: {json_blob}")
        return commands

    @staticmethod
    def _extract_json_block(text: str, start_idx: int) -> Tuple[Optional[str], int]:
        while start_idx < len(text) and text[start_idx].isspace():
            start_idx += 1
        if start_idx >= len(text) or text[start_idx] != '{':
            return None, start_idx
        depth = 0
        for idx in range(start_idx, len(text)):
            char = text[idx]
            if char == '{':
                depth += 1
            elif char == '}':
                depth -= 1
                if depth == 0:
                    return text[start_idx:idx + 1], idx + 1
        return None, len(text)

    def _handle_command(self, command: Dict) -> bool:
        """Handle a command and return True if executed, False if cancelled/failed"""
        action = (command.get("action") or "").lower()
        if not action:
            return False
        if action == "open_position":
            return self._handle_open_position(command)
        elif action == "close_position":
            return self._handle_close_position(command)
        elif action == "buy_spot":
            return self._handle_buy_spot(command)
        elif action == "sell_spot":
            return self._handle_sell_spot(command)
        else:
            print(f"{Fore.YELLOW}AI requested unknown action: {action}")
            return False

    def _handle_open_position(self, command: Dict) -> bool:
        symbol = (command.get("symbol") or "").upper()
        side = (command.get("side") or "LONG").upper()
        if symbol not in SUPPORTED_COINS:
            print(f"{Fore.RED}AI attempted unsupported symbol: {symbol}")
            return False
        details = (
            f"Open {side} {symbol} | risk {command.get('risk_pct', 'auto')}% | "
            f"lev {command.get('leverage', 'auto')}x"
        )
        if not self._confirm(details):
            return False
        success, message = self.trader.open_position_from_ai(
            symbol=symbol,
            position_type=side,
            leverage=command.get("leverage"),
            risk_pct=command.get("risk_pct"),
            stop_loss=command.get("stop_loss"),
            take_profit=command.get("take_profit"),
            reasoning=command.get("reasoning", "AI directive"),
            signals=command.get("signals", "AI")
        )
        color = Fore.GREEN if success else Fore.RED
        print(f"\n{color}{'='*70}")
        print(f"{color}{Style.BRIGHT}{'✓ TRADE EXECUTED' if success else '✗ TRADE FAILED'}")
        print(f"{color}{message}")
        print(f"{color}{'='*70}\n")
        return success

    def _handle_close_position(self, command: Dict) -> bool:
        position_id = command.get("position_id")
        symbol = (command.get("symbol") or "").upper()
        percent = float(command.get("percent", 100))
        if percent <= 0 or percent > 100:
            print(f"{Fore.RED}Invalid close percent from AI: {percent}")
            return False
        if not position_id and not symbol:
            print(f"{Fore.YELLOW}AI close command missing position identifier")
            return False
        summary = f"Close {percent}% of {position_id or symbol}"
        if not self._confirm(summary):
            return False
        success, message = self.trader.close_position_via_ai(position_id=position_id, symbol=symbol, close_pct=percent)
        color = Fore.GREEN if success else Fore.RED
        print(f"\n{color}{'='*70}")
        print(f"{color}{Style.BRIGHT}{'✓ POSITION CLOSED' if success else '✗ CLOSE FAILED'}")
        print(f"{color}{message}")
        print(f"{color}{'='*70}\n")
        return success

    def _handle_buy_spot(self, command: Dict) -> bool:
        symbol = (command.get("symbol") or "").upper()
        if symbol not in SUPPORTED_COINS:
            print(f"{Fore.RED}AI attempted unsupported asset {symbol}")
            return False
        usd_amount = command.get("usd_amount") or command.get("usd") or command.get("amount_usd")
        if not usd_amount:
            print(f"{Fore.YELLOW}AI spot buy missing usd_amount")
            return False
        try:
            usd_amount = float(usd_amount)
        except (TypeError, ValueError):
            print(f"{Fore.RED}Invalid usd_amount {usd_amount}")
            return False
        if usd_amount <= 0:
            print(f"{Fore.RED}usd_amount must be positive")
            return False
        if not self._confirm(f"Buy ${usd_amount:.2f} of {symbol}"):
            return False
        success, message = self.trader.buy_spot_position(symbol=symbol, usd_amount=usd_amount, source="AI")
        color = Fore.GREEN if success else Fore.RED
        print(f"\n{color}{'='*70}")
        print(f"{color}{Style.BRIGHT}{'✓ SPOT PURCHASE COMPLETE' if success else '✗ PURCHASE FAILED'}")
        print(f"{color}{message}")
        print(f"{color}{'='*70}\n")
        return success

    def _handle_sell_spot(self, command: Dict) -> bool:
        symbol = (command.get("symbol") or "").upper()
        if symbol not in SUPPORTED_COINS:
            print(f"{Fore.RED}AI attempted unsupported asset {symbol}")
            return False
        percent = command.get("percent")
        amount = command.get("amount")
        if percent is None and amount is None:
            print(f"{Fore.YELLOW}AI spot sell missing percent/amount")
            return False
        qty = None
        pct = None
        if percent is not None:
            try:
                pct = float(percent)
            except (TypeError, ValueError):
                print(f"{Fore.RED}Invalid percent {percent}")
                return False
            if pct <= 0 or pct > 100:
                print(f"{Fore.RED}Percent must be between 0 and 100")
                return False
        if amount is not None:
            try:
                qty = float(amount)
            except (TypeError, ValueError):
                print(f"{Fore.RED}Invalid amount {amount}")
                return False
        summary = f"Sell {pct or qty} of {symbol}"
        if not self._confirm(summary):
            return False
        success, message = self.trader.sell_spot_position(symbol=symbol, percent=pct, amount=qty, source="AI")
        color = Fore.GREEN if success else Fore.RED
        print(f"\n{color}{'='*70}")
        print(f"{color}{Style.BRIGHT}{'✓ SPOT SALE COMPLETE' if success else '✗ SALE FAILED'}")
        print(f"{color}{message}")
        print(f"{color}{'='*70}\n")
        return success

    @staticmethod
    def _confirm(prompt: str) -> bool:
        print(f"\n{Fore.CYAN}{'─'*70}")
        print(f"{Fore.YELLOW}{Style.BRIGHT}⚠️  AUTHORIZATION REQUIRED")
        print(f"{Fore.WHITE}Action: {Fore.CYAN}{prompt}")
        print(f"{Fore.CYAN}{'─'*70}")
        answer = input(f"{Fore.GREEN}Type 'yes' to authorize, 'no' to cancel: {Fore.WHITE}").strip().lower()
        authorized = answer in {"y", "yes"}
        if authorized:
            print(f"{Fore.GREEN}✓ Authorized - Executing...")
        else:
            print(f"{Fore.RED}✗ Cancelled by user")
        return authorized


class PenguinTrader:
    """Main trading engine - Penguin from 2026 style"""
    
    def __init__(self, encryption_manager: Optional[EncryptionManager] = None):
        self.db = PenguinDatabase(encryption_manager=encryption_manager)
        self._init_encryption_meta()
        self.prices = {}
        self.price_cache_time = None
        self.data = self.load_data()
        self.ai_console = AITradingConsole(self)

    def _init_encryption_meta(self):
        """Ensure encryption metadata exists if manager is present."""
        if not self.db.encryption_manager:
            return
        
        cur = self.db.conn.cursor()
        row = cur.execute("SELECT 1 FROM encryption_meta WHERE id = 1").fetchone()
        if not row:
            # New encryption setup - save salt and verifier
            verifier = self.db.encryption_manager.encrypt("penguin_verifier")
            cur.execute(
                "INSERT INTO encryption_meta (id, salt, verification_hash) VALUES (1, ?, ?)",
                (self.db.encryption_manager.salt, verifier)
            )
            self.db.conn.commit()
            print(f"{Fore.GREEN}✓ Encryption initialized")
        
    def load_data(self) -> Dict:
        """Load persisted state from SQLite and legacy JSON as fallback."""
        state = self.db.load_state()

        if self.db.seeded_from_default and os.path.exists(DATA_FILE):
            try:
                with open(DATA_FILE, 'r') as f:
                    legacy_state = json.load(f)
                state = self._merge_legacy_state(state, legacy_state)
            except (json.JSONDecodeError, OSError):
                print(f"{Fore.YELLOW}⚠️  Legacy data file was unreadable, continuing with database snapshot.")

        state = self._ensure_state_defaults(state)
        return state
    
    def save_data(self):
        """Save state to SQLite and mirrored JSON file"""
        self._sync_btc_wallet_alias(self.data)
        self.db.save_state(self.data)
        with open(DATA_FILE, 'w') as f:
            json.dump(self.data, f, indent=2)

    def _merge_legacy_state(self, current: Dict, legacy: Dict) -> Dict:
        """Blend older JSON data into the DB snapshot."""
        if not isinstance(legacy, dict):
            return current
        for key in ["wallet", "private_key_shown", "portfolio", "openPositions", "closedTrades", "tradeJournal", "spotTransactions", "wallets"]:
            if legacy.get(key):
                current[key] = legacy[key]
        return current

    def _ensure_state_defaults(self, state: Dict) -> Dict:
        """Guarantee required keys exist and sync wallet aliases."""
        defaults = make_default_state()
        state.setdefault("wallet", defaults["wallet"])
        state.setdefault("private_key_shown", False)
        state.setdefault("portfolio", defaults["portfolio"])
        state.setdefault("openPositions", [])
        state.setdefault("closedTrades", [])
        state.setdefault("tradeJournal", [])
        state.setdefault("wallets", {})
        state.setdefault("spotTransactions", [])
        self._sync_btc_wallet_alias(state)
        return state

    def _sync_btc_wallet_alias(self, state: Dict):
        """Keep legacy single wallet and multi-wallet entries consistent."""
        wallets = state.setdefault("wallets", {})
        btc_wallet_entry = wallets.get("BTC")
        legacy_wallet = state.get("wallet")

        now = datetime.now(timezone.utc).isoformat()
        if legacy_wallet and not btc_wallet_entry:
            wallets["BTC"] = {
                "address": legacy_wallet.get("address"),
                "privateKey": legacy_wallet.get("private_key"),
                "label": "Bitcoin Wallet",
                "spotBalance": wallets.get("BTC", {}).get("spotBalance", 0.0),
                "createdAt": legacy_wallet.get("created_at", now),
                "updatedAt": now
            }
        elif btc_wallet_entry and not legacy_wallet:
            state["wallet"] = {
                "address": btc_wallet_entry.get("address"),
                "private_key": btc_wallet_entry.get("privateKey"),
                "created_at": btc_wallet_entry.get("createdAt", now)
            }

    # ------------------------------------------------------------------
    # Wallet desk + spot functionality
    # ------------------------------------------------------------------
    def wallet_desk_menu(self):
        while True:
            print(f"\n{Fore.CYAN}{'='*70}")
            print(f"{Fore.YELLOW}{Style.BRIGHT}WALLET DESK & SPOT TRADES")
            print(f"{Fore.CYAN}{'='*70}")
            print(f"{Fore.WHITE}1. View wallets & balances")
            print(f"{Fore.WHITE}2. Generate new wallet")
            print(f"{Fore.WHITE}3. Import wallet from private key")
            print(f"{Fore.WHITE}4. Buy spot crypto (USD → coin)")
            print(f"{Fore.WHITE}5. Sell spot crypto (coin → USD)")
            print(f"{Fore.WHITE}6. Send crypto to external wallet")
            print(f"{Fore.WHITE}0. Back to main menu")

            choice = input(f"\n{Fore.CYAN}Select option: ").strip()
            if choice == "1":
                self._show_wallet_overview()
            elif choice == "2":
                self._prompt_generate_wallet()
            elif choice == "3":
                self._prompt_import_wallet()
            elif choice == "4":
                self._prompt_spot_buy()
            elif choice == "5":
                self._prompt_spot_sell()
            elif choice == "6":
                self._prompt_send_crypto()
            elif choice == "0":
                break
            else:
                print(f"{Fore.RED}Invalid option")

    def _show_wallet_overview(self):
        prices = self.get_prices()
        print(f"\n{Fore.WHITE}Configured wallets:")
        if not self.data["wallets"]:
            print(f"  {Fore.YELLOW}No wallets set yet. Add one via option 2.")
        else:
            # Offer to refresh balances
            refresh = input(f"{Fore.CYAN}Refresh blockchain balances? (y/n): ").strip().lower()
            if refresh == 'y':
                self._refresh_all_wallet_balances()
        
        for symbol, wallet in self.data["wallets"].items():
            address = wallet.get("address") or "(not set)"
            balance = wallet.get("spotBalance", 0.0)
            price = prices.get(symbol, {}).get("price") if prices else None
            usd_value = balance * price if price else 0
            print(f"  {Fore.CYAN}{symbol}{Fore.WHITE} → {address}")
            print(f"     Paper Balance: {balance:.6f} {symbol} ({Fore.GREEN}${usd_value:,.2f}{Fore.WHITE})")
        input(f"\n{Fore.CYAN}Press Enter to continue...")

    def _prompt_generate_wallet(self):
        """Prompt user to select chain and generate new wallet"""
        print(f"\n{Fore.CYAN}{'='*70}")
        print(f"{Fore.YELLOW}{Style.BRIGHT}GENERATE NEW WALLET")
        print(f"{Fore.CYAN}{'='*70}")
        print(f"\n{Fore.WHITE}Supported chains:")
        print(f"{Fore.CYAN}BTC {Fore.WHITE}• {Fore.CYAN}ETH {Fore.WHITE}• {Fore.CYAN}SOL {Fore.WHITE}• {Fore.CYAN}BNB {Fore.WHITE}• {Fore.CYAN}XRP {Fore.WHITE}• {Fore.CYAN}ADA")

        symbol = input(f"\n{Fore.CYAN}Select chain to generate wallet for: ").strip().upper()
        if symbol not in SUPPORTED_COINS:
            print(f"{Fore.RED}Unsupported symbol")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return

        self._generate_new_wallet(symbol)

    def _prompt_import_wallet(self):
        """Prompt user to select chain and import wallet from private key"""
        print(f"\n{Fore.CYAN}{'='*70}")
        print(f"{Fore.YELLOW}{Style.BRIGHT}IMPORT WALLET FROM PRIVATE KEY")
        print(f"{Fore.CYAN}{'='*70}")
        print(f"\n{Fore.WHITE}Supported chains:")
        print(f"{Fore.CYAN}BTC {Fore.WHITE}• {Fore.CYAN}ETH {Fore.WHITE}• {Fore.CYAN}SOL {Fore.WHITE}• {Fore.CYAN}BNB {Fore.WHITE}• {Fore.CYAN}XRP")

        symbol = input(f"\n{Fore.CYAN}Select chain to import wallet for: ").strip().upper()
        if symbol not in ["BTC", "ETH", "SOL", "BNB", "XRP"]:
            print(f"{Fore.RED}Unsupported symbol for import")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return

        self._import_wallet_from_private_key(symbol)

    def _prompt_spot_buy(self):
        symbol = input(f"\n{Fore.CYAN}Symbol to buy: ").strip().upper()
        amount = input(f"{Fore.CYAN}USD amount to deploy: $").strip()
        try:
            usd_amount = float(amount)
        except ValueError:
            print(f"{Fore.RED}Invalid USD amount")
            return
        success, message = self.buy_spot_position(symbol, usd_amount)
        color = Fore.GREEN if success else Fore.RED
        print(f"{color}{message}")
        input(f"\n{Fore.CYAN}Press Enter to continue...")

    def _prompt_spot_sell(self):
        symbol = input(f"\n{Fore.CYAN}Symbol to sell: ").strip().upper()
        wallet_entry = self.data["wallets"].get(symbol)
        if not wallet_entry or wallet_entry.get("spotBalance", 0) <= 0:
            print(f"{Fore.YELLOW}No holdings for {symbol}")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        mode = input(f"{Fore.CYAN}Sell by percent or amount? (p/a): ").strip().lower()
        percent = amount = None
        if mode == "p":
            try:
                percent = float(input(f"{Fore.CYAN}Percent to sell (0-100): ").strip())
            except ValueError:
                print(f"{Fore.RED}Invalid percent")
                return
        else:
            try:
                amount = float(input(f"{Fore.CYAN}Coin amount to sell: ").strip())
            except ValueError:
                print(f"{Fore.RED}Invalid amount")
                return
        success, message = self.sell_spot_position(symbol, percent=percent, amount=amount)
        color = Fore.GREEN if success else Fore.RED
        print(f"{color}{message}")
        input(f"\n{Fore.CYAN}Press Enter to continue...")
    
    def _prompt_send_crypto(self):
        """Prompt user to send crypto to external wallet"""
        print(f"\n{Fore.CYAN}{'='*70}")
        print(f"{Fore.YELLOW}{Style.BRIGHT}SEND CRYPTO TO EXTERNAL WALLET")
        print(f"{Fore.CYAN}{'='*70}")
        
        # Show available wallets with balances
        available_wallets = {sym: wallet for sym, wallet in self.data["wallets"].items() 
                            if wallet.get("spotBalance", 0) > 0 and wallet.get("privateKey")}
        
        if not available_wallets:
            print(f"\n{Fore.YELLOW}No wallets with balance and private key available")
            print(f"{Fore.WHITE}You need both a balance and private key to send crypto.")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        print(f"\n{Fore.WHITE}Available wallets to send from:")
        for symbol, wallet in available_wallets.items():
            balance = wallet.get("spotBalance", 0)
            print(f"  {Fore.CYAN}{symbol}: {Fore.WHITE}{balance:.6f} {symbol}")
        
        symbol = input(f"\n{Fore.CYAN}Symbol to send: ").strip().upper()
        if symbol not in available_wallets:
            print(f"{Fore.RED}Invalid symbol or insufficient balance/no private key")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        wallet = available_wallets[symbol]
        balance = wallet.get("spotBalance", 0)
        
        to_address = input(f"{Fore.CYAN}Recipient address: ").strip()
        if not to_address:
            print(f"{Fore.RED}Address required")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        amount_str = input(f"{Fore.CYAN}Amount to send (max {balance:.6f} {symbol}): ").strip()
        try:
            amount = float(amount_str)
        except ValueError:
            print(f"{Fore.RED}Invalid amount")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        if amount <= 0 or amount > balance:
            print(f"{Fore.RED}Amount must be between 0 and {balance:.6f}")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        # Confirmation
        print(f"\n{Fore.YELLOW}{'='*70}")
        print(f"{Fore.YELLOW}{Style.BRIGHT}⚠️  TRANSACTION CONFIRMATION")
        print(f"{Fore.YELLOW}{'='*70}")
        print(f"{Fore.WHITE}Send: {Fore.CYAN}{amount:.6f} {symbol}")
        print(f"{Fore.WHITE}To: {Fore.CYAN}{to_address}")
        print(f"{Fore.WHITE}From: {Fore.CYAN}{wallet.get('address')}")
        print(f"{Fore.RED}This will broadcast a real blockchain transaction!")
        
        confirm = input(f"\n{Fore.CYAN}Type 'CONFIRM' to send: ").strip()
        if confirm != "CONFIRM":
            print(f"{Fore.YELLOW}Transaction cancelled")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        # Execute the send
        success, message = self._send_crypto(symbol, to_address, amount, wallet)
        color = Fore.GREEN if success else Fore.RED
        print(f"\n{color}{'='*70}")
        print(f"{color}{Style.BRIGHT}{'✓ TRANSACTION SENT' if success else '✗ TRANSACTION FAILED'}")
        print(f"{color}{message}")
        print(f"{color}{'='*70}")
        input(f"\n{Fore.CYAN}Press Enter to continue...")

    def buy_spot_position(self, symbol: str, usd_amount: float, source: str = "Manual") -> Tuple[bool, str]:
        symbol = symbol.upper()
        if symbol not in SUPPORTED_COINS:
            return False, f"Unsupported asset {symbol}"
        if usd_amount <= 0:
            return False, "USD amount must be positive"
        if self.data["portfolio"]["balance"] < usd_amount:
            return False, "Insufficient USD balance"
        wallet_entry = self.data["wallets"].get(symbol)
        if not wallet_entry or not wallet_entry.get("address"):
            return False, f"Add a {symbol} wallet before buying"
        price = self.get_price(symbol)
        if not price:
            return False, "Price data unavailable"
        coin_amount = usd_amount / price
        wallet_entry["spotBalance"] = wallet_entry.get("spotBalance", 0.0) + coin_amount
        wallet_entry["updatedAt"] = datetime.now(timezone.utc).isoformat()
        self.data["portfolio"]["balance"] -= usd_amount
        tx = {
            "symbol": symbol,
            "side": "BUY",
            "amount": coin_amount,
            "usdValue": usd_amount,
            "createdAt": datetime.now(timezone.utc).isoformat(),
            "source": source
        }
        self.data["spotTransactions"].append(tx)
        journal_entry = {
            "timestamp": tx["createdAt"],
            "action": "SPOT_BUY",
            "symbol": symbol,
            "entryPrice": price,
            "size": coin_amount,
            "marginUsed": usd_amount,
            "reasoning": f"{source} spot buy"
        }
        self.data["tradeJournal"].append(journal_entry)
        self.save_data()
        return True, f"Bought {coin_amount:.6f} {symbol} for ${usd_amount:,.2f}"

    def sell_spot_position(self, symbol: str, percent: Optional[float] = None, amount: Optional[float] = None, source: str = "Manual") -> Tuple[bool, str]:
        symbol = symbol.upper()
        wallet_entry = self.data["wallets"].get(symbol)
        if not wallet_entry:
            return False, f"No wallet configured for {symbol}"
        balance = wallet_entry.get("spotBalance", 0.0)
        if balance <= 0:
            return False, f"No {symbol} holdings to sell"
        if amount is None and percent is None:
            return False, "Provide percent or amount to sell"
        if percent is not None:
            if percent <= 0 or percent > 100:
                return False, "Percent must be between 0 and 100"
            amount_to_sell = balance * (percent / 100)
        else:
            amount_to_sell = min(balance, amount)
        if amount_to_sell <= 0:
            return False, "Sell amount must be positive"
        price = self.get_price(symbol)
        if not price:
            return False, "Price data unavailable"
        usd_value = amount_to_sell * price
        wallet_entry["spotBalance"] = balance - amount_to_sell
        wallet_entry["updatedAt"] = datetime.now(timezone.utc).isoformat()
        self.data["portfolio"]["balance"] += usd_value
        tx = {
            "symbol": symbol,
            "side": "SELL",
            "amount": amount_to_sell,
            "usdValue": usd_value,
            "createdAt": datetime.now(timezone.utc).isoformat(),
            "source": source
        }
        self.data["spotTransactions"].append(tx)
        journal_entry = {
            "timestamp": tx["createdAt"],
            "action": "SPOT_SELL",
            "symbol": symbol,
            "exitPrice": price,
            "size": amount_to_sell,
            "realizedPnL": usd_value,
            "reasoning": f"{source} spot sell"
        }
        self.data["tradeJournal"].append(journal_entry)
        self.save_data()
        return True, f"Sold {amount_to_sell:.6f} {symbol} for ${usd_value:,.2f}"
    
    def get_prices(self, force_refresh=False) -> Dict[str, float]:
        """Get current prices for all supported coins from CoinGecko"""
        # Cache prices for 15 seconds to avoid hammering the API
        if not force_refresh and self.price_cache_time and (time.time() - self.price_cache_time < 15):
            return self.prices
        
        try:
            coin_ids = ",".join(SUPPORTED_COINS.values())
            response = requests.get(
                f"https://api.coingecko.com/api/v3/simple/price?ids={coin_ids}&vs_currencies=usd&include_24h_change=true",
                timeout=10
            )
            if response.status_code == 200:
                data = response.json()
                self.prices = {}
                for symbol, coin_id in SUPPORTED_COINS.items():
                    if coin_id in data:
                        self.prices[symbol] = {
                            "price": data[coin_id]["usd"],
                            "change24h": data[coin_id].get("usd_24h_change", 0)
                        }
                self.price_cache_time = time.time()
                return self.prices
            else:
                print(f"{Fore.RED}API returned status code: {response.status_code}")
                return self.prices if self.prices else {}
        except Exception as e:
            print(f"{Fore.RED}Error fetching prices: {e}")
            return self.prices if self.prices else {}
    
    def get_price(self, symbol: str) -> Optional[float]:
        """Get price for a single coin"""
        prices = self.get_prices()
        if symbol in prices:
            return prices[symbol]["price"]
        return None
    
    def calculate_position_size(self, balance: float, risk_pct: float, entry_price: float, 
                               stop_loss: float, leverage: int) -> Tuple[float, float]:
        """Calculate position size based on risk percentage and stop distance"""
        # Account for 3% entry fee
        raw_size = balance * (risk_pct / 100)
        size_after_fees = raw_size / 1.03
        
        # Minimum $50
        if size_after_fees < 50:
            return 0, 0
        
        # Maximum 80% of balance
        max_size = balance * 0.8
        if size_after_fees > max_size:
            size_after_fees = max_size
        
        # Calculate coin amount based on margin and leverage
        margin_used = size_after_fees
        position_value = margin_used * leverage
        coin_amount = position_value / entry_price
        
        return margin_used, coin_amount
    
    def calculate_liquidation_price(self, entry_price: float, leverage: int, position_type: str) -> float:
        """Calculate liquidation price"""
        if position_type == "SHORT":
            return entry_price * (1 + (1 / leverage))
        else:  # LONG
            return entry_price * (1 - (1 / leverage))
    
    def calculate_unrealized_pnl(self, position: Dict, current_price: float) -> Tuple[float, float]:
        """Calculate unrealized PnL for a position"""
        entry_price = position["entryPrice"]
        margin_used = position["marginUsed"]
        leverage = position["leverage"]
        position_type = position["type"]
        
        # Position value calculation
        position_value = margin_used * leverage
        
        if position_type == "SHORT":
            # For shorts: profit when price goes down
            price_diff = entry_price - current_price
            unrealized_pnl = (price_diff / entry_price) * position_value
        else:  # LONG
            # For longs: profit when price goes up
            price_diff = current_price - entry_price
            unrealized_pnl = (price_diff / entry_price) * position_value
        
        # Calculate percentage
        unrealized_pnl_pct = (unrealized_pnl / margin_used) * 100
        
        return unrealized_pnl, unrealized_pnl_pct
    
    def suggest_leverage(self, num_signals: int) -> int:
        """Suggest leverage based on signal strength"""
        if num_signals >= 4:
            return 50  # Textbook top - go maximum
        elif num_signals == 3:
            return 30  # Solid setup
        elif num_signals == 2:
            return 20  # Conservative
        else:
            return 15  # Minimal conviction
    
    def suggest_risk_pct(self, num_signals: int) -> float:
        """Suggest risk percentage based on signal strength"""
        if num_signals >= 4:
            return 75  # Textbook top - 70-80%
        elif num_signals == 3:
            return 45  # Solid - 40-50%
        elif num_signals == 2:
            return 20  # Conservative - 15-25%
        else:
            return 10  # Minimal
    
    def get_session_bias(self) -> Tuple[str, float]:
        """Detect current trading session and suggest sizing"""
        hour = datetime.now(timezone.utc).hour
        
        if SESSION_WINDOWS["ASIAN"][0] <= hour < SESSION_WINDOWS["ASIAN"][1]:
            return "ASIAN", 8.5
        elif SESSION_WINDOWS["LONDON"][0] <= hour < SESSION_WINDOWS["LONDON"][1]:
            return "LONDON", 11.5
        elif SESSION_WINDOWS["US"][0] <= hour < SESSION_WINDOWS["US"][1]:
            return "US", 13.5
        else:
            return "US_CLOSE", 9.0
    
    def check_stop_hit(self, position: Dict, current_price: float) -> bool:
        """Check if stop loss is hit"""
        if position["type"] == "SHORT":
            return current_price >= position["stopLoss"]
        else:  # LONG
            return current_price <= position["stopLoss"]
    
    def check_tp_hit(self, position: Dict, current_price: float) -> bool:
        """Check if take profit is hit"""
        if position["type"] == "SHORT":
            return current_price <= position["takeProfit"]
        else:  # LONG
            return current_price >= position["takeProfit"]
    
    def check_liquidation(self, position: Dict, current_price: float) -> bool:
        """Check if position is liquidated"""
        if position["type"] == "SHORT":
            return current_price >= position["liquidationPrice"]
        else:  # LONG
            return current_price <= position["liquidationPrice"]
    
    def auto_close_positions(self):
        """Auto-close positions that hit stops, TPs, or liquidations"""
        prices = self.get_prices(force_refresh=True)
        positions_to_close = []
        
        for i, pos in enumerate(self.data["openPositions"]):
            symbol = pos["symbol"]
            if symbol not in prices:
                continue
            
            current_price = prices[symbol]["price"]
            
            # Check liquidation first
            if self.check_liquidation(pos, current_price):
                positions_to_close.append((i, "LIQUIDATED", current_price))
            # Check stop loss
            elif self.check_stop_hit(pos, current_price):
                positions_to_close.append((i, "STOP_LOSS", current_price))
            # Check take profit
            elif self.check_tp_hit(pos, current_price):
                positions_to_close.append((i, "TAKE_PROFIT", current_price))
        
        # Close positions in reverse order to maintain indices
        for i, reason, price in reversed(positions_to_close):
            self.close_position_internal(i, 100, price, reason)
    
    def close_position_internal(self, pos_index: int, close_pct: float, current_price: float, reason: str):
        """Internal method to close a position"""
        position = self.data["openPositions"][pos_index]
        
        # Calculate PnL
        pnl, pnl_pct = self.calculate_unrealized_pnl(position, current_price)
        
        # Account for 3% exit fee
        exit_fee = position["marginUsed"] * (close_pct / 100) * 0.03
        realized_pnl = (pnl * (close_pct / 100)) - exit_fee
        
        # Update balance
        margin_returned = position["marginUsed"] * (close_pct / 100)
        self.data["portfolio"]["balance"] += margin_returned + realized_pnl
        self.data["portfolio"]["realizedPnL"] += realized_pnl
        
        # Record closed trade
        closed_trade = {
            "positionId": position["positionId"],
            "symbol": position["symbol"],
            "type": position["type"],
            "entryPrice": position["entryPrice"],
            "exitPrice": current_price,
            "size": position["size"] * (close_pct / 100),
            "marginUsed": margin_returned,
            "leverage": position["leverage"],
            "realizedPnL": realized_pnl,
            "realizedPnLPercent": (realized_pnl / margin_returned) * 100,
            "openedAt": position["openedAt"],
            "closedAt": datetime.now(timezone.utc).isoformat(),
            "closeReason": reason,
            "signals": position.get("signals", ""),
            "reasoning": position.get("reasoning", "")
        }
        self.data["closedTrades"].append(closed_trade)
        
        # Journal entry
        journal_entry = {
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "action": "CLOSE",
            "symbol": position["symbol"],
            "type": position["type"],
            "entryPrice": position["entryPrice"],
            "exitPrice": current_price,
            "size": closed_trade["size"],
            "leverage": position["leverage"],
            "marginUsed": margin_returned,
            "realizedPnL": realized_pnl,
            "realizedPnLPercent": closed_trade["realizedPnLPercent"],
            "signals": position.get("signals", ""),
            "reasoning": position.get("reasoning", ""),
            "outcome": "WIN" if realized_pnl > 0 else "LOSS",
            "closeReason": reason
        }
        self.data["tradeJournal"].append(journal_entry)
        
        # Remove or reduce position
        if close_pct >= 100:
            del self.data["openPositions"][pos_index]
        else:
            # Partial close
            self.data["openPositions"][pos_index]["size"] *= (1 - close_pct / 100)
            self.data["openPositions"][pos_index]["marginUsed"] *= (1 - close_pct / 100)
        
        self.save_data()
    
    def open_position(self, position_type: str):
        """Open a new SHORT or LONG position"""
        prices = self.get_prices(force_refresh=True)
        if not prices:
            print(f"\n{Fore.RED}Unable to fetch prices. Cannot open position.")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        print(f"\n{Fore.CYAN}{'='*70}")
        print(f"{Fore.YELLOW}{Style.BRIGHT}OPEN {position_type} POSITION - Penguin Style")
        print(f"{Fore.CYAN}{'='*70}")
        
        # Show available coins with prices
        print(f"\n{Fore.WHITE}Available Coins:")
        for symbol in SUPPORTED_COINS.keys():
            if symbol in prices:
                price = prices[symbol]["price"]
                change = prices[symbol]["change24h"]
                color = Fore.GREEN if change >= 0 else Fore.RED
                sign = "+" if change >= 0 else ""
                print(f"  {Fore.CYAN}{symbol}: {Fore.WHITE}${price:,.2f} {color}({sign}{change:.2f}%)")
        
        # Get symbol
        symbol = input(f"\n{Fore.CYAN}Enter symbol (BTC/ETH/SOL/XRP/ADA/LINK): ").strip().upper()
        if symbol not in SUPPORTED_COINS:
            print(f"{Fore.RED}Invalid symbol!")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        if symbol not in prices:
            print(f"{Fore.RED}Price not available for {symbol}")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        entry_price = prices[symbol]["price"]
        
        print(f"\n{Fore.WHITE}Current {symbol} Price: {Fore.GREEN}${entry_price:,.2f}")
        print(f"{Fore.WHITE}Available Balance: {Fore.CYAN}${self.data['portfolio']['balance']:,.2f}")
        
        # Get signals
        print(f"\n{Fore.YELLOW}Enter your signals (comma-separated)")
        print(f"{Fore.WHITE}Examples: RSI 81, MACD cross, Volume fade, Textbook top, Rejection wick")
        signals_input = input(f"{Fore.CYAN}Signals: ").strip()
        
        num_signals = len([s for s in signals_input.split(",") if s.strip()]) if signals_input else 1
        
        # Session bias
        session, session_size = self.get_session_bias()
        print(f"\n{Fore.MAGENTA}Current Session: {session} (suggested base size: {session_size}%)")
        
        # Suggest leverage and risk
        suggested_lev = self.suggest_leverage(num_signals)
        suggested_risk = self.suggest_risk_pct(num_signals)
        
        print(f"\n{Fore.YELLOW}Signal Strength: {num_signals} signals detected")
        print(f"{Fore.WHITE}Suggested Leverage: {Fore.GREEN}{suggested_lev}x")
        print(f"{Fore.WHITE}Suggested Risk %: {Fore.GREEN}{suggested_risk}%")
        
        # Get leverage
        try:
            leverage_input = input(f"\n{Fore.CYAN}Leverage (1-50x, default {suggested_lev}x): ").strip()
            leverage = int(leverage_input) if leverage_input else suggested_lev
            if leverage < 1 or leverage > 50:
                print(f"{Fore.RED}Leverage must be between 1 and 50")
                input(f"\n{Fore.CYAN}Press Enter to continue...")
                return
        except ValueError:
            print(f"{Fore.RED}Invalid leverage!")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        # Get risk percentage
        try:
            risk_input = input(f"{Fore.CYAN}Risk % of balance (default {suggested_risk}%): ").strip()
            risk_pct = float(risk_input) if risk_input else suggested_risk
            if risk_pct <= 0 or risk_pct > 80:
                print(f"{Fore.RED}Risk % must be between 0 and 80")
                input(f"\n{Fore.CYAN}Press Enter to continue...")
                return
        except ValueError:
            print(f"{Fore.RED}Invalid risk percentage!")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        # Get stop loss
        try:
            if position_type == "SHORT":
                print(f"\n{Fore.YELLOW}Stop Loss (price above entry where you exit):")
            else:
                print(f"\n{Fore.YELLOW}Stop Loss (price below entry where you exit):")
            
            stop_loss = float(input(f"{Fore.CYAN}Stop Loss Price: $").strip())
            
            if position_type == "SHORT" and stop_loss <= entry_price:
                print(f"{Fore.RED}Stop loss must be above entry price for shorts!")
                input(f"\n{Fore.CYAN}Press Enter to continue...")
                return
            elif position_type == "LONG" and stop_loss >= entry_price:
                print(f"{Fore.RED}Stop loss must be below entry price for longs!")
                input(f"\n{Fore.CYAN}Press Enter to continue...")
                return
        except ValueError:
            print(f"{Fore.RED}Invalid stop loss price!")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        # Get take profit
        try:
            if position_type == "SHORT":
                print(f"\n{Fore.YELLOW}Take Profit (price below entry where you take profit):")
            else:
                print(f"\n{Fore.YELLOW}Take Profit (price above entry where you take profit):")
            
            take_profit = float(input(f"{Fore.CYAN}Take Profit Price: $").strip())
            
            if position_type == "SHORT" and take_profit >= entry_price:
                print(f"{Fore.RED}Take profit must be below entry price for shorts!")
                input(f"\n{Fore.CYAN}Press Enter to continue...")
                return
            elif position_type == "LONG" and take_profit <= entry_price:
                print(f"{Fore.RED}Take profit must be above entry price for longs!")
                input(f"\n{Fore.CYAN}Press Enter to continue...")
                return
        except ValueError:
            print(f"{Fore.RED}Invalid take profit price!")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        # Get reasoning
        print(f"\n{Fore.YELLOW}Why this trade? (Penguin reasoning)")
        reasoning = input(f"{Fore.CYAN}Reasoning: ").strip()
        
        # Calculate position size
        margin_used, coin_amount = self.calculate_position_size(
            self.data["portfolio"]["balance"],
            risk_pct,
            entry_price,
            stop_loss,
            leverage
        )
        
        if margin_used == 0:
            print(f"{Fore.RED}Position size too small (minimum $50)")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        if margin_used > self.data["portfolio"]["balance"]:
            print(f"{Fore.RED}Insufficient balance!")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        # Calculate liquidation price
        liq_price = self.calculate_liquidation_price(entry_price, leverage, position_type)
        
        # Calculate R:R ratio
        if position_type == "SHORT":
            risk_amount = abs(stop_loss - entry_price)
            reward_amount = abs(entry_price - take_profit)
        else:
            risk_amount = abs(entry_price - stop_loss)
            reward_amount = abs(take_profit - entry_price)
        
        rr_ratio = reward_amount / risk_amount if risk_amount > 0 else 0
        
        # Show confirmation
        print(f"\n{Fore.CYAN}{'='*70}")
        print(f"{Fore.YELLOW}{Style.BRIGHT}POSITION CONFIRMATION")
        print(f"{Fore.CYAN}{'='*70}")
        print(f"\n{Fore.WHITE}Symbol: {Fore.CYAN}{symbol}USDT")
        print(f"{Fore.WHITE}Type: {Fore.MAGENTA}{position_type}")
        print(f"{Fore.WHITE}Entry Price: {Fore.GREEN}${entry_price:,.2f}")
        print(f"{Fore.WHITE}Leverage: {Fore.YELLOW}{leverage}x")
        print(f"{Fore.WHITE}Margin Used: {Fore.CYAN}${margin_used:,.2f} ({risk_pct:.1f}% of balance)")
        print(f"{Fore.WHITE}Position Size: {Fore.CYAN}{coin_amount:.6f} {symbol}")
        print(f"{Fore.WHITE}Position Value: {Fore.CYAN}${margin_used * leverage:,.2f}")
        print(f"\n{Fore.WHITE}Stop Loss: {Fore.RED}${stop_loss:,.2f}")
        print(f"{Fore.WHITE}Take Profit: {Fore.GREEN}${take_profit:,.2f}")
        print(f"{Fore.WHITE}Liquidation: {Fore.RED}${liq_price:,.2f}")
        print(f"\n{Fore.WHITE}R:R Ratio: {Fore.GREEN}{rr_ratio:.2f}:1")
        
        # Entry fee calculation
        entry_fee = margin_used * 0.03
        print(f"\n{Fore.YELLOW}Entry Fee (3%): ${entry_fee:,.2f}")
        print(f"{Fore.YELLOW}Signals: {Fore.WHITE}{signals_input}")
        print(f"{Fore.YELLOW}Reasoning: {Fore.WHITE}{reasoning}")
        
        # Portfolio heat check
        total_margin = sum(p["marginUsed"] for p in self.data["openPositions"])
        new_heat = ((total_margin + margin_used) / self.data["portfolio"]["balance"]) * 100
        print(f"\n{Fore.WHITE}Current Portfolio Heat: {Fore.CYAN}{(total_margin / self.data['portfolio']['balance'] * 100):.1f}%")
        print(f"{Fore.WHITE}New Portfolio Heat: {Fore.CYAN}{new_heat:.1f}%")
        
        if new_heat > 60:
            print(f"{Fore.RED}{Style.BRIGHT}⚠️  WARNING: Portfolio heat exceeds 60%! High risk!")
        elif new_heat > 40:
            print(f"{Fore.YELLOW}⚠️  Portfolio heat above 40% - be cautious")
        
        confirm = input(f"\n{Fore.CYAN}Execute trade? (yes/no): ").strip().lower()
        
        if confirm == "yes":
            # Create position
            position = {
                "positionId": str(uuid.uuid4())[:8],
                "symbol": symbol,
                "type": position_type,
                "entryPrice": entry_price,
                "size": coin_amount,
                "marginUsed": margin_used,
                "leverage": leverage,
                "stopLoss": stop_loss,
                "takeProfit": take_profit,
                "liquidationPrice": liq_price,
                "unrealizedPnL": 0,
                "unrealizedPnLPercent": 0,
                "openedAt": datetime.now(timezone.utc).isoformat(),
                "signals": signals_input,
                "reasoning": reasoning
            }
            
            self.data["openPositions"].append(position)
            self.data["portfolio"]["balance"] -= margin_used
            
            # Journal entry
            journal_entry = {
                "timestamp": datetime.now(timezone.utc).isoformat(),
                "action": "OPEN",
                "symbol": symbol,
                "type": position_type,
                "entryPrice": entry_price,
                "size": coin_amount,
                "leverage": leverage,
                "marginUsed": margin_used,
                "stopLoss": stop_loss,
                "takeProfit": take_profit,
                "signals": signals_input,
                "reasoning": reasoning
            }
            self.data["tradeJournal"].append(journal_entry)
            
            self.save_data()
            
            print(f"\n{Fore.GREEN}✓ Position opened successfully!")
            print(f"{Fore.WHITE}Position ID: {Fore.CYAN}{position['positionId']}")
        else:
            print(f"\n{Fore.YELLOW}Trade cancelled")
        
        input(f"\n{Fore.CYAN}Press Enter to continue...")

    def open_position_from_ai(
        self,
        symbol: str,
        position_type: str,
        leverage: Optional[float] = None,
        risk_pct: Optional[float] = None,
        stop_loss: Optional[float] = None,
        take_profit: Optional[float] = None,
        reasoning: str = "AI directive",
        signals: str = "AI"
    ) -> Tuple[bool, str]:
        prices = self.get_prices(force_refresh=True)
        symbol = symbol.upper()
        position_type = position_type.upper()
        if symbol not in prices:
            return False, f"Price not available for {symbol}"
        entry_price = prices[symbol]["price"]
        leverage = int(leverage) if leverage else self.suggest_leverage(3)
        risk_pct = float(risk_pct) if risk_pct else self.suggest_risk_pct(3)
        leverage = max(1, min(50, leverage))
        risk_pct = max(1, min(80, risk_pct))
        if position_type not in {"SHORT", "LONG"}:
            return False, "Type must be SHORT or LONG"

        if stop_loss is not None:
            try:
                stop_loss = float(stop_loss)
            except (TypeError, ValueError):
                stop_loss = None
        if take_profit is not None:
            try:
                take_profit = float(take_profit)
            except (TypeError, ValueError):
                take_profit = None

        if stop_loss is None:
            stop_loss = entry_price * (0.98 if position_type == "LONG" else 1.02)
        if take_profit is None:
            take_profit = entry_price * (1.04 if position_type == "LONG" else 0.96)

        if position_type == "LONG" and stop_loss >= entry_price:
            stop_loss = entry_price * 0.98
        if position_type == "SHORT" and stop_loss <= entry_price:
            stop_loss = entry_price * 1.02
        if position_type == "LONG" and take_profit <= entry_price:
            take_profit = entry_price * 1.04
        if position_type == "SHORT" and take_profit >= entry_price:
            take_profit = entry_price * 0.96

        margin_used, coin_amount = self.calculate_position_size(
            self.data["portfolio"]["balance"],
            risk_pct,
            entry_price,
            stop_loss,
            leverage
        )
        if margin_used == 0:
            return False, "Position size too small"
        if margin_used > self.data["portfolio"]["balance"]:
            return False, "Insufficient balance"

        liq_price = self.calculate_liquidation_price(entry_price, leverage, position_type)
        position = {
            "positionId": str(uuid.uuid4())[:8],
            "symbol": symbol,
            "type": position_type,
            "entryPrice": entry_price,
            "size": coin_amount,
            "marginUsed": margin_used,
            "leverage": leverage,
            "stopLoss": stop_loss,
            "takeProfit": take_profit,
            "liquidationPrice": liq_price,
            "unrealizedPnL": 0,
            "unrealizedPnLPercent": 0,
            "openedAt": datetime.now(timezone.utc).isoformat(),
            "signals": signals,
            "reasoning": reasoning
        }
        self.data["openPositions"].append(position)
        self.data["portfolio"]["balance"] -= margin_used
        journal_entry = {
            "timestamp": position["openedAt"],
            "action": "OPEN",
            "symbol": symbol,
            "type": position_type,
            "entryPrice": entry_price,
            "size": coin_amount,
            "leverage": leverage,
            "marginUsed": margin_used,
            "stopLoss": stop_loss,
            "takeProfit": take_profit,
            "signals": signals,
            "reasoning": reasoning + " (AI)"
        }
        self.data["tradeJournal"].append(journal_entry)
        self.save_data()
        return True, f"AI opened {position_type} {symbol} @ ${entry_price:,.2f}"

    def close_position_via_ai(self, position_id: Optional[str], symbol: str, close_pct: float) -> Tuple[bool, str]:
        if close_pct <= 0 or close_pct > 100:
            return False, "Close % must be between 0 and 100"
        prices = self.get_prices(force_refresh=True)
        for idx, pos in enumerate(self.data["openPositions"]):
            if position_id and pos["positionId"] != position_id:
                continue
            if symbol and pos["symbol"] != symbol.upper():
                continue
            price = prices.get(pos["symbol"], {}).get("price")
            if not price:
                return False, f"Price unavailable for {pos['symbol']}"
            self.close_position_internal(idx, close_pct, price, "AI_COMMAND")
            return True, f"Closed {close_pct}% of {pos['symbol']}"
        return False, "No matching position"
    
    def view_positions(self):
        """View all open positions with live PnL"""
        # Auto-close any positions that hit stops/TPs
        self.auto_close_positions()
        
        prices = self.get_prices(force_refresh=True)
        
        print(f"\n{Fore.CYAN}{'='*70}")
        print(f"{Fore.YELLOW}{Style.BRIGHT}OPEN POSITIONS")
        print(f"{Fore.CYAN}{'='*70}")
        
        if not self.data["openPositions"]:
            print(f"\n{Fore.YELLOW}No open positions")
        else:
            total_unrealized = 0
            total_margin = 0
            
            for i, pos in enumerate(self.data["openPositions"], 1):
                symbol = pos["symbol"]
                
                if symbol not in prices:
                    print(f"\n{Fore.RED}Price not available for {symbol}")
                    continue
                
                current_price = prices[symbol]["price"]
                pnl, pnl_pct = self.calculate_unrealized_pnl(pos, current_price)
                
                # Update position with current PnL
                pos["unrealizedPnL"] = pnl
                pos["unrealizedPnLPercent"] = pnl_pct
                
                total_unrealized += pnl
                total_margin += pos["marginUsed"]
                
                # Color coding
                pnl_color = Fore.GREEN if pnl >= 0 else Fore.RED
                type_color = Fore.MAGENTA if pos["type"] == "SHORT" else Fore.CYAN
                sign = "+" if pnl >= 0 else ""
                
                # Distance to stops
                if pos["type"] == "SHORT":
                    stop_dist = ((pos["stopLoss"] - current_price) / current_price) * 100
                    tp_dist = ((current_price - pos["takeProfit"]) / current_price) * 100
                    liq_dist = ((pos["liquidationPrice"] - current_price) / current_price) * 100
                else:
                    stop_dist = ((current_price - pos["stopLoss"]) / current_price) * 100
                    tp_dist = ((pos["takeProfit"] - current_price) / current_price) * 100
                    liq_dist = ((current_price - pos["liquidationPrice"]) / current_price) * 100
                
                print(f"\n{Fore.WHITE}POSITION {i}: {type_color}{pos['type']} {symbol}USDT {Fore.WHITE}(ID: {pos['positionId']})")
                print(f"  Entry: ${pos['entryPrice']:,.2f} @ {pos['leverage']}x leverage")
                print(f"  Size: {pos['size']:.6f} {symbol} (${pos['marginUsed']:,.2f} margin)")
                print(f"  Current Price: ${current_price:,.2f}")
                print(f"  Unrealized PnL: {pnl_color}{sign}${pnl:,.2f} ({sign}{pnl_pct:.2f}%)")
                print(f"  Stop Loss: ${pos['stopLoss']:,.2f} ({stop_dist:.2f}% away)")
                print(f"  Take Profit: ${pos['takeProfit']:,.2f} ({tp_dist:.2f}% away)")
                print(f"  Liquidation: ${pos['liquidationPrice']:,.2f} ({liq_dist:.2f}% away)")
                
                status = f"{pnl_color}{'✅ PROFITABLE' if pnl >= 0 else '❌ LOSING'}"
                heat = (pos['marginUsed'] / self.data['portfolio']['balance']) * 100
                print(f"  Status: {status} {Fore.WHITE}| Heat: {Fore.CYAN}{heat:.1f}%")
                
                if pos.get("signals"):
                    print(f"  Signals: {Fore.YELLOW}{pos['signals']}")
                if pos.get("reasoning"):
                    print(f"  Reasoning: {Fore.WHITE}{pos['reasoning']}")
            
            # Summary
            portfolio_heat = (total_margin / (self.data['portfolio']['balance'] + total_margin)) * 100
            
            print(f"\n{Fore.CYAN}{'='*70}")
            total_color = Fore.GREEN if total_unrealized >= 0 else Fore.RED
            sign = "+" if total_unrealized >= 0 else ""
            print(f"{Fore.WHITE}Total Unrealized PnL: {total_color}{Style.BRIGHT}{sign}${total_unrealized:,.2f}")
            print(f"{Fore.WHITE}Total Margin Deployed: {Fore.CYAN}${total_margin:,.2f}")
            print(f"{Fore.WHITE}Portfolio Heat: {Fore.CYAN}{portfolio_heat:.1f}%")
        
        self.save_data()
        input(f"\n{Fore.CYAN}Press Enter to continue...")
    
    def manage_position(self):
        """Manage an existing position"""
        if not self.data["openPositions"]:
            print(f"\n{Fore.YELLOW}No open positions to manage")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return
        
        # Show positions
        self.auto_close_positions()
        prices = self.get_prices(force_refresh=True)
        
        print(f"\n{Fore.CYAN}{'='*70}")
        print(f"{Fore.YELLOW}{Style.BRIGHT}MANAGE POSITIONS")
        print(f"{Fore.CYAN}{'='*70}")
        
        for i, pos in enumerate(self.data["openPositions"], 1):
            symbol = pos["symbol"]
            if symbol in prices:
                current_price = prices[symbol]["price"]
                pnl, pnl_pct = self.calculate_unrealized_pnl(pos, current_price)
                pnl_color = Fore.GREEN if pnl >= 0 else Fore.RED
                sign = "+" if pnl >= 0 else ""
                
                print(f"\n{i}. {pos['type']} {symbol} - ${pos['entryPrice']:,.2f} @ {pos['leverage']}x")
                print(f"   PnL: {pnl_color}{sign}${pnl:,.2f} ({sign}{pnl_pct:.2f}%)")
        
        try:
            choice = int(input(f"\n{Fore.CYAN}Select position number (0 to cancel): ").strip())
            if choice == 0 or choice < 1 or choice > len(self.data["openPositions"]):
                return
            
            pos_index = choice - 1
            position = self.data["openPositions"][pos_index]
            symbol = position["symbol"]
            
            if symbol not in prices:
                print(f"{Fore.RED}Price not available for {symbol}")
                input(f"\n{Fore.CYAN}Press Enter to continue...")
                return
            
            current_price = prices[symbol]["price"]
            
            # Management options
            print(f"\n{Fore.YELLOW}Position Management Options:")
            print(f"{Fore.WHITE}1. Close position (full)")
            print(f"{Fore.WHITE}2. Close partial (specify %)")
            print(f"{Fore.WHITE}3. Adjust stop loss")
            print(f"{Fore.WHITE}4. Adjust take profit")
            print(f"{Fore.WHITE}5. Take profit at 50% TP")
            print(f"{Fore.WHITE}0. Cancel")
            
            action = input(f"\n{Fore.CYAN}Select action: ").strip()
            
            if action == "1":
                # Close full
                confirm = input(f"{Fore.CYAN}Close entire position? (yes/no): ").strip().lower()
                if confirm == "yes":
                    self.close_position_internal(pos_index, 100, current_price, "MANUAL_CLOSE")
                    print(f"\n{Fore.GREEN}✓ Position closed")
            
            elif action == "2":
                # Close partial
                pct = float(input(f"{Fore.CYAN}Enter % to close (1-100): ").strip())
                if pct > 0 and pct <= 100:
                    confirm = input(f"{Fore.CYAN}Close {pct}% of position? (yes/no): ").strip().lower()
                    if confirm == "yes":
                        self.close_position_internal(pos_index, pct, current_price, "PARTIAL_CLOSE")
                        print(f"\n{Fore.GREEN}✓ {pct}% of position closed")
                else:
                    print(f"{Fore.RED}Invalid percentage")
            
            elif action == "3":
                # Adjust stop loss
                new_stop = float(input(f"{Fore.CYAN}Enter new stop loss price: $").strip())
                if position["type"] == "SHORT" and new_stop > position["entryPrice"]:
                    self.data["openPositions"][pos_index]["stopLoss"] = new_stop
                    self.save_data()
                    print(f"\n{Fore.GREEN}✓ Stop loss updated to ${new_stop:,.2f}")
                elif position["type"] == "LONG" and new_stop < position["entryPrice"]:
                    self.data["openPositions"][pos_index]["stopLoss"] = new_stop
                    self.save_data()
                    print(f"\n{Fore.GREEN}✓ Stop loss updated to ${new_stop:,.2f}")
                else:
                    print(f"{Fore.RED}Invalid stop loss price")
            
            elif action == "4":
                # Adjust take profit
                new_tp = float(input(f"{Fore.CYAN}Enter new take profit price: $").strip())
                if position["type"] == "SHORT" and new_tp < position["entryPrice"]:
                    self.data["openPositions"][pos_index]["takeProfit"] = new_tp
                    self.save_data()
                    print(f"\n{Fore.GREEN}✓ Take profit updated to ${new_tp:,.2f}")
                elif position["type"] == "LONG" and new_tp > position["entryPrice"]:
                    self.data["openPositions"][pos_index]["takeProfit"] = new_tp
                    self.save_data()
                    print(f"\n{Fore.GREEN}✓ Take profit updated to ${new_tp:,.2f}")
                else:
                    print(f"{Fore.RED}Invalid take profit price")
            
            elif action == "5":
                # Take profit at 50% TP
                if position["type"] == "SHORT":
                    partial_tp = position["entryPrice"] - ((position["entryPrice"] - position["takeProfit"]) * 0.5)
                else:
                    partial_tp = position["entryPrice"] + ((position["takeProfit"] - position["entryPrice"]) * 0.5)
                
                confirm = input(f"{Fore.CYAN}Close 50% at ${partial_tp:,.2f}? (yes/no): ").strip().lower()
                if confirm == "yes":
                    self.close_position_internal(pos_index, 50, current_price, "PARTIAL_TP")
                    print(f"\n{Fore.GREEN}✓ 50% closed at partial TP")
        
        except (ValueError, IndexError):
            print(f"{Fore.RED}Invalid input")
        
        input(f"\n{Fore.CYAN}Press Enter to continue...")
    
    def view_trade_journal(self):
        """View complete trade journal"""
        print(f"\n{Fore.CYAN}{'='*70}")
        print(f"{Fore.YELLOW}{Style.BRIGHT}TRADE JOURNAL")
        print(f"{Fore.CYAN}{'='*70}")
        
        if not self.data["tradeJournal"]:
            print(f"\n{Fore.YELLOW}No trades logged yet")
        else:
            # Show recent trades first
            for entry in reversed(self.data["tradeJournal"][-20:]):  # Last 20 trades
                action = entry["action"]
                timestamp = entry["timestamp"][:19]
                
                if action == "OPEN":
                    print(f"\n{Fore.GREEN}[{timestamp}] OPENED {entry['type']} {entry['symbol']}")
                    print(f"  Entry: ${entry['entryPrice']:,.2f} @ {entry['leverage']}x")
                    print(f"  Size: {entry['size']:.6f} {entry['symbol']} (${entry['marginUsed']:,.2f})")
                    print(f"  Stop: ${entry['stopLoss']:,.2f} | TP: ${entry['takeProfit']:,.2f}")
                    if entry.get("signals"):
                        print(f"  Signals: {entry['signals']}")
                    if entry.get("reasoning"):
                        print(f"  Reasoning: {entry['reasoning']}")
                
                elif action == "CLOSE":
                    outcome_color = Fore.GREEN if entry['outcome'] == "WIN" else Fore.RED
                    sign = "+" if entry['realizedPnL'] >= 0 else ""
                    
                    print(f"\n{outcome_color}[{timestamp}] CLOSED {entry['type']} {entry['symbol']} - {entry['outcome']}")
                    print(f"  Entry: ${entry['entryPrice']:,.2f} → Exit: ${entry['exitPrice']:,.2f}")
                    print(f"  Size: {entry['size']:.6f} {entry['symbol']} @ {entry['leverage']}x")
                    print(f"  PnL: {sign}${entry['realizedPnL']:,.2f} ({sign}{entry['realizedPnLPercent']:.2f}%)")
                    print(f"  Reason: {entry['closeReason']}")
                elif action in {"SPOT_BUY", "SPOT_SELL"}:
                    side_color = Fore.GREEN if action == "SPOT_BUY" else Fore.RED
                    price_label = "Price" if action == "SPOT_BUY" else "Exit"
                    price_value = entry.get("entryPrice") or entry.get("exitPrice") or 0
                    usd = entry.get("marginUsed") or entry.get("realizedPnL") or 0
                    print(f"\n{side_color}[{timestamp}] {action.replace('_', ' ')} {entry.get('symbol')}")
                    print(f"  {price_label}: ${price_value:,.2f} | Size: {entry.get('size', 0):.6f}")
                    print(f"  USD Value: ${usd:,.2f} | Note: {entry.get('reasoning', 'N/A')}")
        
        input(f"\n{Fore.CYAN}Press Enter to continue...")
    
    def view_portfolio_stats(self):
        """View comprehensive portfolio statistics"""
        print(f"\n{Fore.CYAN}{'='*70}")
        print(f"{Fore.YELLOW}{Style.BRIGHT}PORTFOLIO STATISTICS")
        print(f"{Fore.CYAN}{'='*70}")
        
        balance = self.data["portfolio"]["balance"]
        realized_pnl = self.data["portfolio"]["realizedPnL"]
        
        # Calculate metrics from closed trades
        closed_trades = self.data["closedTrades"]
        wins = [t for t in closed_trades if t["realizedPnL"] > 0]
        losses = [t for t in closed_trades if t["realizedPnL"] <= 0]
        
        win_rate = (len(wins) / len(closed_trades) * 100) if closed_trades else 0
        avg_win = sum(t["realizedPnL"] for t in wins) / len(wins) if wins else 0
        avg_loss = sum(t["realizedPnL"] for t in losses) / len(losses) if losses else 0
        
        # Open positions metrics
        total_margin = sum(p["marginUsed"] for p in self.data["openPositions"])
        available_margin = balance
        portfolio_heat = (total_margin / (balance + total_margin) * 100) if (balance + total_margin) > 0 else 0
        
        # Calculate total unrealized PnL and spot value
        prices = self.get_prices()
        total_unrealized = 0
        for pos in self.data["openPositions"]:
            if pos["symbol"] in prices:
                pnl, _ = self.calculate_unrealized_pnl(pos, prices[pos["symbol"]]["price"])
                total_unrealized += pnl

        spot_value = 0
        for symbol, wallet in self.data.get("wallets", {}).items():
            bal = wallet.get("spotBalance", 0)
            if bal > 0 and symbol in prices:
                spot_value += bal * prices[symbol]["price"]
        
        # Display stats
        print(f"\n{Fore.WHITE}💰 ACCOUNT BALANCE")
        print(f"  Available: {Fore.CYAN}${available_margin:,.2f}")
        print(f"  Margin Deployed: {Fore.YELLOW}${total_margin:,.2f}")
        print(f"  Spot Holdings: {Fore.MAGENTA}${spot_value:,.2f}")
        print(f"  Total Value: {Fore.GREEN}${(available_margin + total_margin + total_unrealized + spot_value):,.2f}")
        
        pnl_color = Fore.GREEN if realized_pnl >= 0 else Fore.RED
        sign = "+" if realized_pnl >= 0 else ""
        print(f"\n{Fore.WHITE}📊 PnL TRACKING")
        print(f"  Realized PnL: {pnl_color}{sign}${realized_pnl:,.2f}")
        
        unrealized_color = Fore.GREEN if total_unrealized >= 0 else Fore.RED
        u_sign = "+" if total_unrealized >= 0 else ""
        print(f"  Unrealized PnL: {unrealized_color}{u_sign}${total_unrealized:,.2f}")
        
        total_pnl = realized_pnl + total_unrealized
        total_color = Fore.GREEN if total_pnl >= 0 else Fore.RED
        t_sign = "+" if total_pnl >= 0 else ""
        print(f"  Total PnL: {total_color}{Style.BRIGHT}{t_sign}${total_pnl:,.2f}")
        
        print(f"\n{Fore.WHITE}🎯 TRADING PERFORMANCE")
        print(f"  Total Trades: {Fore.CYAN}{len(closed_trades)}")
        print(f"  Wins: {Fore.GREEN}{len(wins)} {Fore.WHITE}| Losses: {Fore.RED}{len(losses)}")
        print(f"  Win Rate: {Fore.CYAN}{win_rate:.1f}%")
        
        if wins:
            print(f"  Avg Win: {Fore.GREEN}+${avg_win:,.2f}")
        if losses:
            print(f"  Avg Loss: {Fore.RED}${avg_loss:,.2f}")
        
        print(f"\n{Fore.WHITE}🔥 PORTFOLIO HEAT")
        heat_color = Fore.RED if portfolio_heat > 60 else (Fore.YELLOW if portfolio_heat > 40 else Fore.GREEN)
        print(f"  Current Heat: {heat_color}{portfolio_heat:.1f}%")
        print(f"  Open Positions: {Fore.CYAN}{len(self.data['openPositions'])}")
        
        if portfolio_heat > 60:
            print(f"  {Fore.RED}⚠️  HIGH RISK - Over 60% deployed!")
        elif portfolio_heat > 40:
            print(f"  {Fore.YELLOW}⚠️  MODERATE RISK - Over 40% deployed")
        else:
            print(f"  {Fore.GREEN}✓ Safe exposure level")
        
        input(f"\n{Fore.CYAN}Press Enter to continue...")
    
    def generate_wallet(self):
        """Generate wallets for BTC, ETH, SOL, BNB, and XRP"""
        print(f"\n{Fore.CYAN}{'='*70}")
        print(f"{Fore.YELLOW}{Style.BRIGHT}GENERATE CRYPTO WALLET")
        print(f"{Fore.CYAN}{'='*70}")
        print(f"\n{Fore.WHITE}Available blockchains:")
        print(f"{Fore.WHITE}1. Bitcoin (BTC)")
        print(f"{Fore.WHITE}2. Ethereum (ETH)" + ("" if ETH_AVAILABLE else f" {Fore.RED}[NOT INSTALLED]"))
        print(f"{Fore.WHITE}3. Solana (SOL)" + ("" if SOL_AVAILABLE else f" {Fore.RED}[NOT INSTALLED]"))
        print(f"{Fore.WHITE}4. BNB Smart Chain (BNB)" + ("" if ETH_AVAILABLE else f" {Fore.RED}[NOT INSTALLED]"))
        print(f"{Fore.WHITE}5. XRP Ledger (XRP)" + ("" if XRP_AVAILABLE else f" {Fore.RED}[NOT INSTALLED]"))
        print(f"{Fore.WHITE}0. Cancel")
        
        choice = input(f"\n{Fore.CYAN}Select blockchain: ").strip()
        
        if choice == "1":
            self._generate_new_wallet("BTC")
        elif choice == "2":
            if not ETH_AVAILABLE:
                print(f"{Fore.RED}Ethereum support not installed!")
                print(f"{Fore.YELLOW}Install: pip install web3 eth-account")
            else:
                self._generate_new_wallet("ETH")
        elif choice == "3":
            if not SOL_AVAILABLE:
                print(f"{Fore.RED}Solana support not installed!")
                print(f"{Fore.YELLOW}Install: pip install solders base58")
            else:
                self._generate_new_wallet("SOL")
        elif choice == "4":
            if not ETH_AVAILABLE:
                print(f"{Fore.RED}BNB support not installed (uses Ethereum libraries)!")
                print(f"{Fore.YELLOW}Install: pip install web3 eth-account")
            else:
                self._generate_new_wallet("BNB")
        elif choice == "5":
            if not XRP_AVAILABLE:
                print(f"{Fore.RED}XRP support not installed!")
                print(f"{Fore.YELLOW}Install: pip install xrpl-py")
            else:
                self._generate_new_wallet("XRP")
        
        if choice != "0":
            input(f"\n{Fore.CYAN}Press Enter to continue...")
    
    def settings_menu(self):
        """Settings menu"""
        while True:
            print(f"\n{Fore.CYAN}{'='*70}")
            print(f"{Fore.YELLOW}{Style.BRIGHT}SETTINGS")
            print(f"{Fore.CYAN}{'='*70}")
            print(f"\n{Fore.WHITE}1. Set Initial Balance")
            print(f"{Fore.WHITE}2. Reset Trading Data (Keep Wallet)")
            print(f"{Fore.WHITE}3. View Closed Trades")
            print(f"{Fore.WHITE}4. Export Trade Journal")
            print(f"{Fore.WHITE}5. Back to Main Menu")
            
            choice = input(f"\n{Fore.CYAN}Select option: ").strip()
            
            if choice == "1":
                try:
                    new_balance = float(input(f"\n{Fore.CYAN}Enter new balance: $").strip())
                    if new_balance > 0:
                        self.data["portfolio"]["balance"] = new_balance
                        self.save_data()
                        print(f"{Fore.GREEN}✓ Balance set to ${new_balance:,.2f}")
                except ValueError:
                    print(f"{Fore.RED}Invalid amount!")
                input(f"\n{Fore.CYAN}Press Enter to continue...")
            
            elif choice == "2":
                confirm = input(f"\n{Fore.RED}Reset all trading data? (yes/no): ").strip().lower()
                if confirm == "yes":
                    wallet = self.data["wallet"]
                    key_shown = self.data["private_key_shown"]
                    self.data = self.load_data()
                    self.data["wallet"] = wallet
                    self.data["private_key_shown"] = key_shown
                    self.save_data()
                    print(f"{Fore.GREEN}✓ Trading data reset!")
                input(f"\n{Fore.CYAN}Press Enter to continue...")
            
            elif choice == "3":
                self.view_closed_trades()
            
            elif choice == "4":
                self.export_journal()
            
            elif choice == "5":
                break
    
    def view_closed_trades(self):
        """View all closed trades"""
        print(f"\n{Fore.CYAN}{'='*70}")
        print(f"{Fore.YELLOW}{Style.BRIGHT}CLOSED TRADES")
        print(f"{Fore.CYAN}{'='*70}")
        
        if not self.data["closedTrades"]:
            print(f"\n{Fore.YELLOW}No closed trades yet")
        else:
            for trade in reversed(self.data["closedTrades"][-15:]):
                outcome_color = Fore.GREEN if trade["realizedPnL"] > 0 else Fore.RED
                sign = "+" if trade["realizedPnL"] >= 0 else ""
                
                print(f"\n{outcome_color}{trade['type']} {trade['symbol']} - {trade['closeReason']}")
                print(f"  Entry: ${trade['entryPrice']:,.2f} → Exit: ${trade['exitPrice']:,.2f}")
                print(f"  Leverage: {trade['leverage']}x | Size: {trade['size']:.6f}")
                print(f"  PnL: {sign}${trade['realizedPnL']:,.2f} ({sign}{trade['realizedPnLPercent']:.2f}%)")
                print(f"  Closed: {trade['closedAt'][:19]}")
        
        input(f"\n{Fore.CYAN}Press Enter to continue...")
    
    def export_journal(self):
        """Export trade journal to text file"""
        filename = f"trade_journal_{datetime.now().strftime('%Y%m%d_%H%M%S')}.txt"
        
        with open(filename, 'w') as f:
            f.write("PENGUIN 2026 TRADE JOURNAL\n")
            f.write("="*70 + "\n\n")
            
            for entry in self.data["tradeJournal"]:
                entry_type = entry.get('type', '') or ''
                symbol = entry.get('symbol', '') or ''
                f.write(f"[{entry['timestamp']}] {entry['action']} {entry_type} {symbol}\n")
                
                if entry['action'] == "OPEN":
                    f.write(f"Entry: ${entry['entryPrice']:,.2f} @ {entry['leverage']}x\n")
                    f.write(f"Size: {entry['size']:.6f} (${entry['marginUsed']:,.2f})\n")
                    f.write(f"Signals: {entry.get('signals', 'N/A')}\n")
                    f.write(f"Reasoning: {entry.get('reasoning', 'N/A')}\n")
                elif entry['action'] == "CLOSE":
                    f.write(f"Entry: ${entry['entryPrice']:,.2f} → Exit: ${entry['exitPrice']:,.2f}\n")
                    f.write(f"PnL: ${entry['realizedPnL']:,.2f} ({entry['realizedPnLPercent']:.2f}%)\n")
                    f.write(f"Outcome: {entry['outcome']} - {entry['closeReason']}\n")
                elif entry['action'] in {"SPOT_BUY", "SPOT_SELL"}:
                    price = entry.get("entryPrice") or entry.get("exitPrice") or 0
                    usd_value = entry.get("marginUsed") or entry.get("realizedPnL") or 0
                    f.write(f"Spot price: ${price:,.2f} | USD value: ${usd_value:,.2f}\n")
                    f.write(f"Reasoning: {entry.get('reasoning', 'N/A')}\n")
                
                f.write("\n")
        
        print(f"\n{Fore.GREEN}✓ Journal exported to {filename}")
        input(f"\n{Fore.CYAN}Press Enter to continue...")
    
    def _validate_private_key(self, symbol: str, private_key: str) -> tuple[bool, Optional[str]]:
        """Validate and derive address from private key for given blockchain
        Returns: (is_valid, derived_address or None)
        """
        try:
            if symbol == "BTC":
                return self._validate_btc_private_key(private_key)
            elif symbol == "ETH":
                return self._validate_eth_private_key(private_key)
            elif symbol == "SOL":
                return self._validate_sol_private_key(private_key)
            elif symbol == "BNB":
                return self._validate_bnb_private_key(private_key)
            elif symbol == "XRP":
                return self._validate_xrp_private_key(private_key)
            else:
                return False, None
        except Exception as e:
            print(f"{Fore.YELLOW}Validation error: {e}")
            return False, None

    def _validate_btc_private_key(self, private_key: str) -> tuple[bool, Optional[str]]:
        """Validate Bitcoin private key (WIF format)"""
        try:
            key = Key(private_key)
            return True, key.address
        except:
            return False, None

    def _validate_eth_private_key(self, private_key: str) -> tuple[bool, Optional[str]]:
        """Validate Ethereum private key (hex format)"""
        try:
            if not private_key.startswith('0x'):
                private_key = '0x' + private_key
            account = Account.from_key(private_key)
            return True, account.address
        except:
            return False, None

    def _validate_sol_private_key(self, private_key: str) -> tuple[bool, Optional[str]]:
        """Validate Solana private key (base58 format)"""
        try:
            key_bytes = base58.b58decode(private_key)
            keypair = Keypair.from_bytes(key_bytes)
            return True, str(keypair.pubkey())
        except:
            return False, None

    def _validate_bnb_private_key(self, private_key: str) -> tuple[bool, Optional[str]]:
        """Validate BNB private key (same as Ethereum)"""
        return self._validate_eth_private_key(private_key)

    def _validate_xrp_private_key(self, private_key: str) -> tuple[bool, Optional[str]]:
        """Validate XRP seed/private key"""
        try:
            wallet = XRPLWallet.from_seed(private_key)
            return True, wallet.address
        except:
            return False, None

    def _import_wallet_from_private_key(self, symbol: str):
        """Import wallet from private key with validation"""
        print(f"\n{Fore.CYAN}{'='*70}")
        print(f"{Fore.YELLOW}{Style.BRIGHT}IMPORT {symbol} WALLET FROM PRIVATE KEY")
        print(f"{Fore.CYAN}{'='*70}")

        # Show format hints
        format_hints = {
            "BTC": "WIF format (starts with 5, K, or L)",
            "ETH": "Hex format (64 characters, with or without 0x prefix)",
            "SOL": "Base58 encoded private key",
            "BNB": "Hex format (same as Ethereum)",
            "XRP": "Seed format (starts with 's')"
        }

        print(f"{Fore.WHITE}Expected format: {Fore.CYAN}{format_hints.get(symbol, 'Check documentation')}")
        private_key = input(f"\n{Fore.CYAN}Enter private key: ").strip()

        if not private_key:
            print(f"{Fore.RED}Private key cannot be empty")
            return

        print(f"\n{Fore.YELLOW}Validating private key...")
        is_valid, derived_address = self._validate_private_key(symbol, private_key)

        if not is_valid:
            print(f"{Fore.RED}✗ Invalid private key format for {symbol}")
            print(f"{Fore.YELLOW}Please check the format and try again.")
            input(f"\n{Fore.CYAN}Press Enter to continue...")
            return

        print(f"{Fore.GREEN}✓ Valid private key!")
        print(f"{Fore.WHITE}Derived address: {Fore.GREEN}{derived_address}")

        # Optional label
        label = input(f"\n{Fore.CYAN}Label (optional, press Enter for default): ").strip()
        if not label:
            label = f"Imported {symbol} Wallet"

        # Fetch blockchain balance
        print(f"\n{Fore.YELLOW}Fetching balance from blockchain...")
        real_balance = self._fetch_blockchain_balance(symbol, derived_address)

        if real_balance is not None:
            print(f"{Fore.GREEN}✓ Balance: {real_balance:.6f} {symbol}")
        else:
            print(f"{Fore.YELLOW}Could not fetch balance, starting with 0")
            real_balance = 0.0

        # Save wallet
        now = datetime.now(timezone.utc).isoformat()
        self.data["wallets"][symbol] = {
            "address": derived_address,
            "privateKey": private_key,
            "label": label,
            "spotBalance": real_balance,
            "createdAt": now,
            "updatedAt": now
        }

        # Special handling for BTC (legacy support)
        if symbol == "BTC":
            self.data["wallet"] = {
                "address": derived_address,
                "private_key": private_key,
                "created_at": now
            }

        self.save_data()

        print(f"\n{Fore.GREEN}{'='*70}")
        print(f"{Fore.GREEN}{Style.BRIGHT}✓ WALLET IMPORTED SUCCESSFULLY!")
        print(f"{Fore.GREEN}{'='*70}")
        print(f"{Fore.WHITE}Symbol: {Fore.CYAN}{symbol}")
        print(f"{Fore.WHITE}Address: {Fore.CYAN}{derived_address}")
        print(f"{Fore.WHITE}Label: {Fore.CYAN}{label}")
        print(f"{Fore.WHITE}Balance: {Fore.GREEN}{real_balance:.6f} {symbol}")
        input(f"\n{Fore.CYAN}Press Enter to continue...")

    def _generate_new_wallet(self, symbol: str):
        """Generate a new wallet for the specified blockchain"""
        if symbol == "BTC":
            self._generate_btc_wallet()
        elif symbol == "ETH":
            self._generate_eth_wallet()
        elif symbol == "SOL":
            self._generate_sol_wallet()
        elif symbol == "BNB":
            self._generate_bnb_wallet()
        elif symbol == "XRP":
            self._generate_xrp_wallet()
        else:
            print(f"{Fore.RED}Unsupported blockchain: {symbol}")
    
    def _generate_btc_wallet(self):
        """Generate Bitcoin wallet"""
        btc_entry = self.data.setdefault("wallets", {}).get("BTC")
        if btc_entry and btc_entry.get("address"):
            overwrite = input(f"\n{Fore.YELLOW}BTC wallet exists! Overwrite? (yes/no): ").strip().lower()
            if overwrite != "yes":
                return
        
        print(f"\n{Fore.YELLOW}Generating Bitcoin wallet...")
        key = Key()
        address = key.address
        private_key_wif = key.to_wif()
        
        now = datetime.now(timezone.utc).isoformat()
        self.data["wallet"] = {
            "address": address,
            "private_key": private_key_wif,
            "created_at": now
        }
        self.data["wallets"]["BTC"] = {
            "address": address,
            "privateKey": private_key_wif,
            "label": "Bitcoin Wallet",
            "spotBalance": 0.0,
            "createdAt": now,
            "updatedAt": now
        }
        self.save_data()
        
        print(f"\n{Fore.GREEN}✓ Bitcoin Wallet Generated!")
        print(f"{Fore.WHITE}Address: {Fore.GREEN}{Style.BRIGHT}{address}")
        print(f"\n{Fore.RED}{Style.BRIGHT}⚠️  CRITICAL: Save your private key securely!")
        
        show = input(f"\n{Fore.CYAN}Show private key? (yes/no): ").strip().lower()
        if show == "yes":
            print(f"\n{Fore.RED}{'='*70}")
            print(f"{Fore.RED}{Style.BRIGHT}BITCOIN PRIVATE KEY (WIF):")
            print(f"{Fore.GREEN}{Style.BRIGHT}{private_key_wif}")
            print(f"{Fore.RED}{'='*70}")
            print(f"{Fore.YELLOW}Write this down! You will not be able to see it again.")
            input(f"\n{Fore.CYAN}Press Enter after securing your key...")
    
    def _generate_eth_wallet(self):
        """Generate Ethereum wallet"""
        eth_entry = self.data.setdefault("wallets", {}).get("ETH")
        if eth_entry and eth_entry.get("address"):
            overwrite = input(f"\n{Fore.YELLOW}ETH wallet exists! Overwrite? (yes/no): ").strip().lower()
            if overwrite != "yes":
                return
        
        print(f"\n{Fore.YELLOW}Generating Ethereum wallet...")
        account = Account.create()
        address = account.address
        private_key = account.key.hex()
        
        now = datetime.now(timezone.utc).isoformat()
        self.data["wallets"]["ETH"] = {
            "address": address,
            "privateKey": private_key,
            "label": "Ethereum Wallet",
            "spotBalance": 0.0,
            "createdAt": now,
            "updatedAt": now
        }
        self.save_data()
        
        print(f"\n{Fore.GREEN}✓ Ethereum Wallet Generated!")
        print(f"{Fore.WHITE}Address: {Fore.GREEN}{Style.BRIGHT}{address}")
        print(f"\n{Fore.RED}{Style.BRIGHT}⚠️  CRITICAL: Save your private key securely!")
        
        show = input(f"\n{Fore.CYAN}Show private key? (yes/no): ").strip().lower()
        if show == "yes":
            print(f"\n{Fore.RED}{'='*70}")
            print(f"{Fore.RED}{Style.BRIGHT}ETHEREUM PRIVATE KEY (HEX):")
            print(f"{Fore.GREEN}{Style.BRIGHT}{private_key}")
            print(f"{Fore.RED}{'='*70}")
            print(f"{Fore.YELLOW}Write this down! You will not be able to see it again.")
            input(f"\n{Fore.CYAN}Press Enter after securing your key...")
    
    def _generate_sol_wallet(self):
        """Generate Solana wallet"""
        sol_entry = self.data.setdefault("wallets", {}).get("SOL")
        if sol_entry and sol_entry.get("address"):
            overwrite = input(f"\n{Fore.YELLOW}SOL wallet exists! Overwrite? (yes/no): ").strip().lower()
            if overwrite != "yes":
                return
        
        print(f"\n{Fore.YELLOW}Generating Solana wallet...")
        keypair = Keypair()
        address = str(keypair.pubkey())
        private_key = base58.b58encode(bytes(keypair)).decode('ascii')
        
        now = datetime.now(timezone.utc).isoformat()
        self.data["wallets"]["SOL"] = {
            "address": address,
            "privateKey": private_key,
            "label": "Solana Wallet",
            "spotBalance": 0.0,
            "createdAt": now,
            "updatedAt": now
        }
        self.save_data()
        
        print(f"\n{Fore.GREEN}✓ Solana Wallet Generated!")
        print(f"{Fore.WHITE}Address: {Fore.GREEN}{Style.BRIGHT}{address}")
        print(f"\n{Fore.RED}{Style.BRIGHT}⚠️  CRITICAL: Save your private key securely!")
        
        show = input(f"\n{Fore.CYAN}Show private key? (yes/no): ").strip().lower()
        if show == "yes":
            print(f"\n{Fore.RED}{'='*70}")
            print(f"{Fore.RED}{Style.BRIGHT}SOLANA PRIVATE KEY (Base58):")
            print(f"{Fore.GREEN}{Style.BRIGHT}{private_key}")
            print(f"{Fore.RED}{'='*70}")
            print(f"{Fore.YELLOW}Write this down! You will not be able to see it again.")
            input(f"\n{Fore.CYAN}Press Enter after securing your key...")
    
    def _generate_bnb_wallet(self):
        """Generate BNB Smart Chain wallet (uses Ethereum format)"""
        bnb_entry = self.data.setdefault("wallets", {}).get("BNB")
        if bnb_entry and bnb_entry.get("address"):
            overwrite = input(f"\n{Fore.YELLOW}BNB wallet exists! Overwrite? (yes/no): ").strip().lower()
            if overwrite != "yes":
                return
        
        print(f"\n{Fore.YELLOW}Generating BNB Smart Chain wallet...")
        account = Account.create()
        address = account.address
        private_key = account.key.hex()
        
        now = datetime.now(timezone.utc).isoformat()
        self.data["wallets"]["BNB"] = {
            "address": address,
            "privateKey": private_key,
            "label": "BNB Smart Chain Wallet",
            "spotBalance": 0.0,
            "createdAt": now,
            "updatedAt": now
        }
        self.save_data()
        
        print(f"\n{Fore.GREEN}✓ BNB Smart Chain Wallet Generated!")
        print(f"{Fore.WHITE}Address: {Fore.GREEN}{Style.BRIGHT}{address}")
        print(f"{Fore.CYAN}(Compatible with Ethereum wallets - same address format)")
        print(f"\n{Fore.RED}{Style.BRIGHT}⚠️  CRITICAL: Save your private key securely!")
        
        show = input(f"\n{Fore.CYAN}Show private key? (yes/no): ").strip().lower()
        if show == "yes":
            print(f"\n{Fore.RED}{'='*70}")
            print(f"{Fore.RED}{Style.BRIGHT}BNB PRIVATE KEY (HEX):")
            print(f"{Fore.GREEN}{Style.BRIGHT}{private_key}")
            print(f"{Fore.RED}{'='*70}")
            print(f"{Fore.YELLOW}Write this down! You will not be able to see it again.")
            input(f"\n{Fore.CYAN}Press Enter after securing your key...")
    
    def _generate_xrp_wallet(self):
        """Generate XRP Ledger wallet"""
        xrp_entry = self.data.setdefault("wallets", {}).get("XRP")
        if xrp_entry and xrp_entry.get("address"):
            overwrite = input(f"\n{Fore.YELLOW}XRP wallet exists! Overwrite? (yes/no): ").strip().lower()
            if overwrite != "yes":
                return
        
        print(f"\n{Fore.YELLOW}Generating XRP Ledger wallet...")
        wallet = XRPLWallet.create()
        address = wallet.address
        private_key = wallet.seed
        
        # Fetch real balance from blockchain
        print(f"{Fore.YELLOW}Fetching balance from XRP Ledger...", end=" ", flush=True)
        real_balance = self._fetch_xrp_balance(address)
        if real_balance is not None:
            print(f"{Fore.GREEN}✓ {real_balance:.6f} XRP")
        else:
            print(f"{Fore.YELLOW}(will use paper balance)")
            real_balance = 0.0
        
        now = datetime.now(timezone.utc).isoformat()
        self.data["wallets"]["XRP"] = {
            "address": address,
            "privateKey": private_key,
            "label": "XRP Ledger Wallet",
            "spotBalance": real_balance,
            "createdAt": now,
            "updatedAt": now
        }
        self.save_data()
        
        print(f"\n{Fore.GREEN}✓ XRP Ledger Wallet Generated!")
        print(f"{Fore.WHITE}Address: {Fore.GREEN}{Style.BRIGHT}{address}")
        print(f"{Fore.WHITE}Balance: {Fore.GREEN}{real_balance:.6f} XRP")
        print(f"\n{Fore.RED}{Style.BRIGHT}⚠️  CRITICAL: Save your private key securely!")
        
        show = input(f"\n{Fore.CYAN}Show private key? (yes/no): ").strip().lower()
        if show == "yes":
            print(f"\n{Fore.RED}{'='*70}")
            print(f"{Fore.RED}{Style.BRIGHT}XRP SEED (Secret Key):")
            print(f"{Fore.GREEN}{Style.BRIGHT}{private_key}")
            print(f"{Fore.RED}{'='*70}")
            print(f"{Fore.YELLOW}Write this down! You will not be able to see it again.")
            input(f"\n{Fore.CYAN}Press Enter after securing your key...")
    
    def _fetch_blockchain_balance(self, symbol: str, address: str) -> Optional[float]:
        """Fetch real balance from blockchain"""
        try:
            if symbol == "BTC":
                return self._fetch_btc_balance(address)
            elif symbol == "ETH":
                return self._fetch_eth_balance(address)
            elif symbol == "SOL":
                return self._fetch_sol_balance(address)
            elif symbol == "BNB":
                return self._fetch_bnb_balance(address)
            elif symbol == "XRP":
                return self._fetch_xrp_balance(address)
        except Exception as e:
            print(f"{Fore.YELLOW}Warning: Could not fetch {symbol} balance: {e}")
        return None
    
    def _fetch_btc_balance(self, address: str) -> Optional[float]:
        """Fetch Bitcoin balance from blockchain.info API"""
        try:
            response = requests.get(f"https://blockchain.info/q/addressbalance/{address}", timeout=10)
            if response.status_code == 200:
                satoshis = int(response.text.strip())
                return satoshis / 100000000  # Convert satoshis to BTC
        except Exception as e:
            print(f"{Fore.YELLOW}BTC balance fetch failed: {e}")
        return None
    
    def _fetch_eth_balance(self, address: str) -> Optional[float]:
        """Fetch Ethereum balance using public RPC endpoints"""
        if not ETH_AVAILABLE:
            return None
        
        # Try multiple public RPC endpoints
        rpc_endpoints = [
            'https://eth.llamarpc.com',
            'https://rpc.ankr.com/eth',
            'https://ethereum.publicnode.com',
        ]
        
        for rpc_url in rpc_endpoints:
            try:
                w3 = Web3(Web3.HTTPProvider(rpc_url, request_kwargs={'timeout': 10}))
                if w3.is_connected():
                    # Convert address to checksum format
                    checksum_address = Web3.to_checksum_address(address)
                    balance_wei = w3.eth.get_balance(checksum_address)
                    return float(w3.from_wei(balance_wei, 'ether'))
            except Exception as e:
                continue  # Try next endpoint
        
        print(f"{Fore.YELLOW}ETH balance fetch failed: All RPC endpoints unavailable")
        return None
    
    def _fetch_sol_balance(self, address: str) -> Optional[float]:
        """Fetch Solana balance using public RPC"""
        try:
            # Using public Solana RPC
            response = requests.post(
                "https://api.mainnet-beta.solana.com",
                json={
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "getBalance",
                    "params": [address]
                },
                timeout=10
            )
            if response.status_code == 200:
                data = response.json()
                if "result" in data and "value" in data["result"]:
                    lamports = data["result"]["value"]
                    return lamports / 1000000000  # Convert lamports to SOL
        except Exception as e:
            print(f"{Fore.YELLOW}SOL balance fetch failed: {e}")
        return None
    
    def _fetch_bnb_balance(self, address: str) -> Optional[float]:
        """Fetch BNB balance using BSC public RPC"""
        if not ETH_AVAILABLE:
            return None
        
        # BNB Smart Chain RPC endpoints
        rpc_endpoints = [
            'https://bsc-dataseed.binance.org',
            'https://bsc-dataseed1.defibit.io',
            'https://bsc-dataseed1.ninicoin.io',
        ]
        
        for rpc_url in rpc_endpoints:
            try:
                w3 = Web3(Web3.HTTPProvider(rpc_url, request_kwargs={'timeout': 10}))
                if w3.is_connected():
                    checksum_address = Web3.to_checksum_address(address)
                    balance_wei = w3.eth.get_balance(checksum_address)
                    return float(w3.from_wei(balance_wei, 'ether'))
            except Exception:
                continue
        
        print(f"{Fore.YELLOW}BNB balance fetch failed: All RPC endpoints unavailable")
        return None
    
    def _fetch_xrp_balance(self, address: str) -> Optional[float]:
        """Fetch XRP balance using public RPC"""
        try:
            response = requests.post(
                "https://s1.ripple.com:51234/",
                json={
                    "method": "account_info",
                    "params": [{
                        "account": address,
                        "ledger_index": "validated"
                    }]
                },
                timeout=10
            )
            if response.status_code == 200:
                data = response.json()
                
                # Check if account exists on ledger
                if "result" in data:
                    result = data["result"]
                    
                    # Account exists and has balance
                    if "account_data" in result:
                        drops = int(result["account_data"]["Balance"])
                        return drops / 1000000  # Convert drops to XRP
                    
                    # Account not found - unfunded/inactive (needs 10 XRP minimum to activate)
                    if result.get("error") == "actNotFound" or result.get("error_code") == 19:
                        return 0.0
                    
                    # Account malformed or other errors
                    if "error" in result:
                        print(f"{Fore.YELLOW}XRP API error: {result.get('error_message', result.get('error'))}")
                        return 0.0
                        
        except Exception as e:
            print(f"{Fore.YELLOW}XRP balance fetch failed: {e}")
        return None
    
    def _refresh_all_wallet_balances(self):
        """Refresh balances for all configured wallets"""
        print(f"\n{Fore.YELLOW}Refreshing blockchain balances...")
        for symbol, wallet in self.data["wallets"].items():
            address = wallet.get("address")
            if address:
                print(f"  Checking {symbol}...", end=" ", flush=True)
                balance = self._fetch_blockchain_balance(symbol, address)
                if balance is not None:
                    wallet["spotBalance"] = balance
                    wallet["updatedAt"] = datetime.now(timezone.utc).isoformat()
                    print(f"{Fore.GREEN}✓ {balance:.6f} {symbol}")
                else:
                    print(f"{Fore.YELLOW}(kept paper balance)")
        self.save_data()
        print(f"{Fore.GREEN}✓ Refresh complete!")
    
    def _send_crypto(self, symbol: str, to_address: str, amount: float, wallet: Dict) -> Tuple[bool, str]:
        """Send crypto to external wallet - calls appropriate blockchain send function"""
        try:
            if symbol == "BTC":
                return self._send_btc(to_address, amount, wallet)
            elif symbol == "ETH":
                return self._send_eth(to_address, amount, wallet)
            elif symbol == "SOL":
                return self._send_sol(to_address, amount, wallet)
            elif symbol == "BNB":
                return self._send_bnb(to_address, amount, wallet)
            elif symbol == "XRP":
                return self._send_xrp(to_address, amount, wallet)
            else:
                return False, f"Sending {symbol} not yet implemented"
        except Exception as e:
            return False, f"Transaction failed: {str(e)}"
    
    def _send_btc(self, to_address: str, amount: float, wallet: Dict) -> Tuple[bool, str]:
        """Send Bitcoin transaction"""
        try:
            from bit import PrivateKeyTestnet
            private_key_wif = wallet.get("privateKey")
            if not private_key_wif:
                return False, "Private key not available"
            
            # Create key from WIF
            key = Key(private_key_wif)
            
            # Convert BTC to satoshis
            satoshis = int(amount * 100000000)
            
            # Create and broadcast transaction
            # Note: This uses recommended fee rate
            tx_hash = key.send([(to_address, satoshis, 'satoshi')])
            
            # Update local balance
            wallet["spotBalance"] -= amount
            wallet["updatedAt"] = datetime.now(timezone.utc).isoformat()
            self.save_data()
            
            return True, f"Sent {amount} BTC\\nTransaction: {tx_hash}"
        except Exception as e:
            return False, f"Bitcoin send failed: {str(e)}"
    
    def _send_eth(self, to_address: str, amount: float, wallet: Dict) -> Tuple[bool, str]:
        """Send Ethereum transaction"""
        if not ETH_AVAILABLE:
            return False, "Ethereum libraries not installed"
        
        try:
            private_key_hex = wallet.get("privateKey")
            if not private_key_hex:
                return False, "Private key not available"
            
            # Connect to Ethereum network
            w3 = Web3(Web3.HTTPProvider('https://eth.llamarpc.com'))
            if not w3.is_connected():
                return False, "Could not connect to Ethereum network"
            
            # Load account
            account = Account.from_key(private_key_hex)
            from_address = account.address
            
            # Get nonce
            nonce = w3.eth.get_transaction_count(from_address)
            
            # Build transaction
            tx = {
                'nonce': nonce,
                'to': Web3.to_checksum_address(to_address),
                'value': w3.to_wei(amount, 'ether'),
                'gas': 21000,
                'gasPrice': w3.eth.gas_price,
                'chainId': 1  # Mainnet
            }
            
            # Sign transaction
            signed_tx = account.sign_transaction(tx)
            
            # Send transaction
            tx_hash = w3.eth.send_raw_transaction(signed_tx.rawTransaction)
            
            # Update local balance
            wallet["spotBalance"] -= amount
            wallet["updatedAt"] = datetime.now(timezone.utc).isoformat()
            self.save_data()
            
            return True, f"Sent {amount} ETH\\nTransaction: {tx_hash.hex()}"
        except Exception as e:
            return False, f"Ethereum send failed: {str(e)}"
    
    def _send_sol(self, to_address: str, amount: float, wallet: Dict) -> Tuple[bool, str]:
        """Send Solana transaction"""
        if not SOL_AVAILABLE:
            return False, "Solana libraries not installed"
        
        try:
            from solders.pubkey import Pubkey
            from solders.system_program import transfer, TransferParams
            from solders.transaction import Transaction
            from solders.message import Message
            import requests
            
            private_key_b58 = wallet.get("privateKey")
            if not private_key_b58:
                return False, "Private key not available"
            
            # Decode private key
            keypair = Keypair.from_bytes(base58.b58decode(private_key_b58))
            
            # Convert amount to lamports
            lamports = int(amount * 1000000000)
            
            # Get recent blockhash
            response = requests.post(
                "https://api.mainnet-beta.solana.com",
                json={
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "getLatestBlockhash"
                },
                timeout=10
            )
            
            if response.status_code != 200:
                return False, "Could not get recent blockhash"
            
            blockhash_data = response.json()
            if "result" not in blockhash_data or "value" not in blockhash_data["result"]:
                return False, "Invalid blockhash response"
            
            # Note: Full Solana transaction requires more complex setup
            # This is a simplified example - production would need proper transaction construction
            
            return False, "Solana sending requires additional setup (use Phantom wallet for now)"
        except Exception as e:
            return False, f"Solana send failed: {str(e)}"
    
    def _send_bnb(self, to_address: str, amount: float, wallet: Dict) -> Tuple[bool, str]:
        """Send BNB transaction (same as ETH but different network)"""
        if not ETH_AVAILABLE:
            return False, "BNB libraries not installed"
        
        try:
            private_key_hex = wallet.get("privateKey")
            if not private_key_hex:
                return False, "Private key not available"
            
            # Connect to BSC network
            w3 = Web3(Web3.HTTPProvider('https://bsc-dataseed.binance.org'))
            if not w3.is_connected():
                return False, "Could not connect to BNB Smart Chain"
            
            # Load account
            account = Account.from_key(private_key_hex)
            from_address = account.address
            
            # Get nonce
            nonce = w3.eth.get_transaction_count(from_address)
            
            # Build transaction
            tx = {
                'nonce': nonce,
                'to': Web3.to_checksum_address(to_address),
                'value': w3.to_wei(amount, 'ether'),
                'gas': 21000,
                'gasPrice': w3.eth.gas_price,
                'chainId': 56  # BSC Mainnet
            }
            
            # Sign transaction
            signed_tx = account.sign_transaction(tx)
            
            # Send transaction
            tx_hash = w3.eth.send_raw_transaction(signed_tx.rawTransaction)
            
            # Update local balance
            wallet["spotBalance"] -= amount
            wallet["updatedAt"] = datetime.now(timezone.utc).isoformat()
            self.save_data()
            
            return True, f"Sent {amount} BNB\\nTransaction: {tx_hash.hex()}"
        except Exception as e:
            return False, f"BNB send failed: {str(e)}"
    
    def _send_xrp(self, to_address: str, amount: float, wallet: Dict) -> Tuple[bool, str]:
        """Send XRP transaction"""
        if not XRP_AVAILABLE:
            return False, "XRP libraries not installed"
        
        try:
            from xrpl.clients import JsonRpcClient
            from xrpl.models.transactions import Payment
            from xrpl.transaction import submit_and_wait
            from xrpl.utils import xrp_to_drops
            
            seed = wallet.get("privateKey")
            if not seed:
                return False, "Private key not available"
            
            # Create wallet from seed
            xrp_wallet = XRPLWallet.from_seed(seed)
            
            # Connect to XRP Ledger
            client = JsonRpcClient("https://s1.ripple.com:51234/")
            
            # Create payment transaction
            payment = Payment(
                account=xrp_wallet.address,
                amount=xrp_to_drops(amount),
                destination=to_address
            )
            
            # Sign and submit
            response = submit_and_wait(payment, client, xrp_wallet)
            
            # Update local balance
            wallet["spotBalance"] -= amount
            wallet["updatedAt"] = datetime.now(timezone.utc).isoformat()
            self.save_data()
            
            return True, f"Sent {amount} XRP\\nTransaction: {response.result.get('hash', 'N/A')}"
        except Exception as e:
            return False, f"XRP send failed: {str(e)}"
    
    def main_menu(self):
        """Main menu"""
        while True:
            os.system('clear' if os.name == 'posix' else 'cls')
            
            print(f"{Fore.CYAN}{'='*70}")
            print(f"{Fore.GREEN}{Style.BRIGHT}    🐧 PENGUIN 2026 TRADING ENGINE")
            print(f"{Fore.CYAN}{'='*70}")
            
            # Quick stats
            balance = self.data["portfolio"]["balance"]
            total_margin = sum(p["marginUsed"] for p in self.data["openPositions"])
            
            print(f"\n{Fore.WHITE}Available Balance: {Fore.CYAN}${balance:,.2f}")
            print(f"{Fore.WHITE}Margin Deployed: {Fore.YELLOW}${total_margin:,.2f}")
            print(f"{Fore.WHITE}Open Positions: {Fore.CYAN}{len(self.data['openPositions'])}")
            print(f"{Fore.WHITE}Realized PnL: {Fore.GREEN}${self.data['portfolio']['realizedPnL']:,.2f}")
            
            # Show current prices ticker
            prices = self.get_prices()
            if prices:
                print(f"\n{Fore.WHITE}Market Prices:")
                ticker = " | ".join([
                    f"{sym}: ${prices[sym]['price']:,.2f}" 
                    for sym in ["BTC", "ETH", "SOL"] if sym in prices
                ])
                print(f"  {Fore.CYAN}{ticker}")
            
            print(f"\n{Fore.YELLOW}{Style.BRIGHT}TRADING MENU")
            print(f"{Fore.WHITE}1. Open SHORT")
            print(f"{Fore.WHITE}2. Open LONG")
            print(f"{Fore.WHITE}3. View Open Positions")
            print(f"{Fore.WHITE}4. Manage Position")
            print(f"{Fore.WHITE}5. View Trade Journal")
            print(f"{Fore.WHITE}6. View Portfolio Stats")
            
            print(f"\n{Fore.YELLOW}{Style.BRIGHT}AUTOMATION & SETTINGS")
            print(f"{Fore.WHITE}7. AI Trading Console (Ollama)")
            print(f"{Fore.WHITE}8. Wallet Desk & Spot Trades")
            print(f"{Fore.WHITE}9. Settings")
            print(f"{Fore.WHITE}10. Generate Crypto Wallet (BTC/ETH/SOL/BNB/XRP)")
            
            print(f"\n{Fore.WHITE}0. Exit")
            
            choice = input(f"\n{Fore.CYAN}Select option: ").strip()
            
            if choice == "1":
                self.open_position("SHORT")
            elif choice == "2":
                self.open_position("LONG")
            elif choice == "3":
                self.view_positions()
            elif choice == "4":
                self.manage_position()
            elif choice == "5":
                self.view_trade_journal()
            elif choice == "6":
                self.view_portfolio_stats()
            elif choice == "7":
                self.ai_console.start()
            elif choice == "8":
                self.wallet_desk_menu()
            elif choice == "9":
                self.settings_menu()
            elif choice == "10":
                self.generate_wallet()
            elif choice == "0":
                print(f"\n{Fore.GREEN}Trade like Penguin. Patient, precise, profitable. 🐧")
                break
            else:
                print(f"\n{Fore.RED}Invalid option!")
                time.sleep(1)


def main():
    """Entry point"""
    try:
        # Encryption setup
        db_path = DATABASE_FILE
        encryption_manager = None
        
        print(f"{Fore.CYAN}{'='*70}")
        print(f"{Fore.GREEN}{Style.BRIGHT}    🐧 PENGUIN 2026 TRADING ENGINE - SECURE BOOT")
        print(f"{Fore.CYAN}{'='*70}")

        if os.path.exists(db_path):
            conn = sqlite3.connect(db_path)
            try:
                cur = conn.cursor()
                # Check if table exists first
                try:
                    row = cur.execute("SELECT salt, verification_hash FROM encryption_meta WHERE id = 1").fetchone()
                except sqlite3.OperationalError:
                    row = None
                
                if row:
                    salt = row[0]
                    stored_hash = row[1]
                    while True:
                        password = getpass.getpass(f"{Fore.CYAN}Enter wallet password: {Fore.WHITE}")
                        if not password: continue
                        manager = EncryptionManager(password, salt)
                        if manager.decrypt(stored_hash) == "penguin_verifier":
                            encryption_manager = manager
                            print(f"{Fore.GREEN}✓ Database unlocked")
                            break
                        else:
                            print(f"{Fore.RED}Incorrect password")
                else:
                    # Migration case
                    print(f"{Fore.YELLOW}⚠️  Unencrypted database detected.")
                    print(f"{Fore.WHITE}You must set a password to encrypt your keys.")
                    while True:
                        pw1 = getpass.getpass(f"{Fore.CYAN}Set new password: {Fore.WHITE}")
                        pw2 = getpass.getpass(f"{Fore.CYAN}Confirm password: {Fore.WHITE}")
                        if pw1 == pw2 and pw1:
                            encryption_manager = EncryptionManager(pw1)
                            break
                        print(f"{Fore.RED}Passwords do not match or empty")
            except Exception as e:
                print(f"{Fore.RED}Database error: {e}")
            finally:
                conn.close()
        else:
            # New DB
            print(f"{Fore.GREEN}Welcome to Penguin Trader!")
            print(f"{Fore.WHITE}Please set a secure password for your wallet database.")
            while True:
                pw1 = getpass.getpass(f"{Fore.CYAN}Set password: {Fore.WHITE}")
                pw2 = getpass.getpass(f"{Fore.CYAN}Confirm password: {Fore.WHITE}")
                if pw1 == pw2 and pw1:
                    encryption_manager = EncryptionManager(pw1)
                    break
                print(f"{Fore.RED}Passwords do not match or empty")

        app = PenguinTrader(encryption_manager)
        app.main_menu()
    except KeyboardInterrupt:
        print(f"\n\n{Fore.YELLOW}Session ended. Your positions are saved.")
        sys.exit(0)


if __name__ == "__main__":
    main()
