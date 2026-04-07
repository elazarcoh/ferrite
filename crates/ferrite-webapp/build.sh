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
  <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>🐾</text></svg>" />
  <style>
    html, body { margin: 0; padding: 0; overflow: hidden; width: 100%; height: 100%; background: #1a1a2e; }
    canvas { width: 100%; height: 100%; }
    #loading {
      position: fixed; inset: 0;
      display: flex; flex-direction: column; align-items: center; justify-content: center;
      background: #1a1a2e; color: #ccc; font-family: sans-serif; font-size: 1.1rem;
      z-index: 10;
    }
    #loading .spinner {
      width: 40px; height: 40px; margin-bottom: 16px;
      border: 4px solid #444; border-top-color: #7c9aff;
      border-radius: 50%; animation: spin 0.8s linear infinite;
    }
    @keyframes spin { to { transform: rotate(360deg); } }
  </style>
</head>
<body>
  <div id="loading"><div class="spinner"></div>Loading Ferrite DevTools…</div>
  <canvas id="the_canvas_id"></canvas>
  <script type="module">
    import init from './ferrite_webapp.js';
    init();
  </script>
  <script>
    // Hide loading overlay once the __ferrite bridge is ready (set in WebApp::new)
    (function poll() {
      if (window.__ferrite) {
        var el = document.getElementById('loading');
        if (el) el.remove();
      } else {
        setTimeout(poll, 50);
      }
    })();
  </script>
</body>
</html>
HTML

echo "==> Done. Serve with: npx serve dist -l 8080"
echo "    or: python -m http.server 8080 --directory dist"
