#!/usr/bin/env bash
set -eo pipefail

ARGS=("$@")

echo "Launching Penguin GUI (release)..."
if [ "${#ARGS[@]}" -gt 0 ]; then
  cargo run --release --bin penguin-gui "${ARGS[@]}" &
else
  cargo run --release --bin penguin-gui &
fi
GUI_PID=$!

echo "Launching Penguin CLI (release)... (press Ctrl+C to stop both)"
trap 'echo "Stopping apps..."; kill $GUI_PID 2>/dev/null || true; exit' INT TERM

if [ "${#ARGS[@]}" -gt 0 ]; then
  cargo run --release --bin penguin "${ARGS[@]}"
else
  cargo run --release --bin penguin
fi

kill $GUI_PID 2>/dev/null || true
