#!/usr/bin/env bash
# Build the self-contained web page: tools/stryktipset/reduce.html
#
# Requires:
#   - rustup with the wasm32-unknown-unknown target
#   - wasm-bindgen-cli matching the version locked in Cargo.lock
#   - python3 (for inlining)
set -euo pipefail

root="$(cd "$(dirname "$0")" && pwd)"
cd "$root"

fail() { echo "error: $*" >&2; exit 1; }

# --- toolchain checks -------------------------------------------------------
command -v cargo >/dev/null || fail "cargo not found. Install Rust: https://rustup.rs"

if ! rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown; then
  fail "wasm32-unknown-unknown target missing. Run: rustup target add wasm32-unknown-unknown"
fi

command -v wasm-bindgen >/dev/null || {
  lockver="$(grep -A1 'name = "wasm-bindgen"' Cargo.lock | grep version | head -1 | cut -d'"' -f2)"
  fail "wasm-bindgen CLI not found. Run: cargo install wasm-bindgen-cli --version ${lockver:-0.2.130}"
}

# The wasm-bindgen CLI version must exactly match the crate version in
# Cargo.lock; a silent mismatch produces confusing runtime errors.
lockver="$(grep -A1 'name = "wasm-bindgen"' Cargo.lock | grep version | head -1 | cut -d'"' -f2)"
cliver="$(wasm-bindgen --version | awk '{print $2}')"
if [ "$lockver" != "$cliver" ]; then
  fail "wasm-bindgen CLI ($cliver) does not match Cargo.lock ($lockver). Run: cargo install wasm-bindgen-cli --version $lockver"
fi

# --- build ------------------------------------------------------------------
echo ">> building reduce-web (wasm32)..."
RUSTFLAGS="-C panic=abort" \
  cargo build --release --target wasm32-unknown-unknown -p reduce-web

echo ">> generating JS bindings (no-modules)..."
rm -rf build
wasm-bindgen --target no-modules --out-dir build \
  target/wasm32-unknown-unknown/release/reduce_web.wasm

if command -v wasm-opt >/dev/null 2>&1; then
  echo ">> optimizing with wasm-opt..."
  wasm-opt -Oz build/reduce_web_bg.wasm -o build/reduce_web_bg.opt.wasm
  mv build/reduce_web_bg.opt.wasm build/reduce_web_bg.wasm
else
  echo ">> (wasm-opt not found, skipping optimization)"
fi

echo ">> inlining into reduce.html..."
python3 inline.py
echo ">> done: $(dirname "$root")/reduce.html"
