#!/bin/bash

# Penguin AI Trader - Build Script

set -e

echo "=================================="
echo "🐧 Penguin AI Trader - Setup"
echo "=================================="
echo ""

# Check Rust installation
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust not found"
    echo ""
    echo "Install Rust from: https://rustup.rs/"
    echo "Then run this script again"
    exit 1
fi

echo "✓ Rust found ($(rustc --version))"
echo ""

# Check for API key
if [ -z "$ANTHROPIC_API_KEY" ]; then
    echo "⚠️  ANTHROPIC_API_KEY not set"
    echo ""
    echo "AI features will be disabled."
    echo "To enable AI:"
    echo "  1. Get API key from https://console.anthropic.com/"
    echo "  2. Run: export ANTHROPIC_API_KEY='sk-ant-api03-...'"
    echo "  3. Or add to ~/.bashrc for persistence"
    echo ""
    read -p "Continue without AI? (y/n): " -n 1 -r
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 0
    fi
else
    echo "✓ ANTHROPIC_API_KEY configured"
    echo ""
fi

# Install Python dependencies
echo "=================================="
echo "Installing Python Dependencies..."
echo "=================================="
echo ""
pip install requests cryptography bit colorama web3 solders base58 --break-system-packages || echo "⚠️  Python dependency install failed (check pip)"
echo ""

# Build
echo "=================================="
echo "Building Penguin Trader..."
echo "=================================="
echo ""

cargo build --release

if [ $? -eq 0 ]; then
    echo ""
    echo "=================================="
    echo "✓ Build successful!"
    echo "=================================="
    echo ""
    echo "Binary location: target/release/penguin"
    echo ""
    echo "To run:"
    echo "  ./target/release/penguin"
    echo ""
    echo "Or:"
    echo "  cargo run --release"
    echo ""
    
    # Offer to run
    read -p "Run now? (y/n): " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo ""
        ./target/release/penguin
    fi
else
    echo ""
    echo "❌ Build failed"
    echo "Check error messages above"
    exit 1
fi
