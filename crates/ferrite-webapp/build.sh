#!/usr/bin/env bash
# Fallback build script for when `trunk build` fails (e.g. on Windows before trunk is fixed).
# Usage (from crates/ferrite-webapp/):
#   bash build.sh          # dev build
#   bash build.sh release  # release build
#
# Prerequisites:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version $(grep -A1 'name = "wasm-bindgen"' ../../Cargo.lock | grep version | cut -d'"' -f2)

set -e
cd "$(dirname "$0")"
ROOT="../.."

PROFILE="${1:-debug}"
CARGO_FLAG=""
if [ "$PROFILE" = "release" ]; then
  CARGO_FLAG="--release"
fi

echo "==> Building wasm ($PROFILE)..."
cargo build --target wasm32-unknown-unknown -p ferrite-webapp $CARGO_FLAG

echo "==> Running wasm-bindgen..."
mkdir -p dist
wasm-bindgen --no-typescript --target web \
  --out-dir dist \
  "$ROOT/target/wasm32-unknown-unknown/$PROFILE/ferrite_webapp.wasm"

echo "==> Generating dist/index.html..."
cat > dist/index.html << 'HTML'
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <title>Ferrite DevTools</title>
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <style>
    html, body { margin: 0; padding: 0; overflow: hidden; width: 100%; height: 100%; }
    canvas { width: 100%; height: 100%; }
  </style>
</head>
<body>
  <canvas id="the_canvas_id"></canvas>
  <script type="module">
    import init from './ferrite_webapp.js';
    init();
  </script>
</body>
</html>
HTML

echo "==> Done. Serve with: npx serve dist -l 8080"
echo "    or: python -m http.server 8080 --directory dist"
