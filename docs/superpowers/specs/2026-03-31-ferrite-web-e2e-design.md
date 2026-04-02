# Ferrite Web — E2E Test Suite Design

**Date:** 2026-03-31
**Scope:** Playwright E2E tests for `crates/ferrite-web` (Dioxus/wasm site served via `dx serve`)

---

## Context

The ferrite-web crate is a Dioxus 0.7 single-page app compiled to wasm and served by `dx serve`.
It has five routes:

| Route | Component |
|-------|-----------|
| `/ferrite/` | Home (hero, pet canvas, feature cards, CTA) |
| `/ferrite/guides` | Guide index (4 links) |
| `/ferrite/guides/:slug` | Guide page (rendered markdown) |
| `/*` | NotFound (404) |

Assets served from `public/`: `esheep.json`, `esheep.png`, `default.petstate`, `assets/tailwind.css`.

---

## Goals

- Verify all routes render correctly and routing works
- Confirm the animated canvas draws pixels (wasm loaded and running)
- Assert no console errors during normal page load
- Run fully automatically including server start/stop
- Use a random port each run to avoid conflicts in CI and parallel runs

---

## Non-goals

- Visual regression / screenshot diffing
- Testing wasm internals (animation logic is covered by Rust unit tests)
- Cross-browser matrix (Chromium only for now)

---

## Architecture

### Directory structure

```
crates/ferrite-web/
  e2e/
    nav.spec.ts
    home.spec.ts
    canvas.spec.ts
    guides.spec.ts
    not-found.spec.ts
  scripts/
    test.js              # free-port finder; spawns playwright
  playwright.config.ts
  package.json           # adds @playwright/test devDep, "test:e2e" script
```

### Port strategy

Tests must use a random free port to avoid clashes. A Node.js launcher script
(`scripts/test.js`) binds a TCP socket to port 0 to let the OS assign a free port,
closes it, then re-exports that port as `TEST_PORT` before spawning `playwright test`.
The Playwright config reads `process.env.TEST_PORT` and threads it into:

- `use.baseURL` — so all `page.goto('/')` calls resolve correctly
- `webServer.command` — `dx serve --port <TEST_PORT>`

This is cross-platform (no bash required on Windows).

### `scripts/test.js`

```js
const net = require('net');
const { spawn } = require('child_process');
const server = net.createServer();
server.listen(0, '127.0.0.1', () => {
  const port = server.address().port;
  server.close(() => {
    const proc = spawn(
      'npx', ['playwright', 'test', ...process.argv.slice(2)],
      { env: { ...process.env, TEST_PORT: String(port) }, stdio: 'inherit', shell: true }
    );
    proc.on('close', code => process.exit(code ?? 0));
  });
});
```

### `playwright.config.ts`

```ts
import { defineConfig } from '@playwright/test';

const port = parseInt(process.env.TEST_PORT ?? '8080');

export default defineConfig({
  testDir: './e2e',
  use: {
    baseURL: `http://localhost:${port}/ferrite`,
  },
  webServer: {
    command: `dx serve --port ${port}`,
    url: `http://localhost:${port}/ferrite`,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
  projects: [{ name: 'chromium', use: { browserName: 'chromium' } }],
});
```

### `package.json` addition

```json
"scripts": {
  "test:e2e": "node scripts/test.js"
},
"devDependencies": {
  "@playwright/test": "^1.50"
}
```

---

## Test Specs

### `nav.spec.ts` — Navigation bar

Present on every page; tests run from Home, Guides, and a Guide detail page.

| Test | Assertion |
|------|-----------|
| Nav renders on home | `nav` element visible |
| "Home" link → home | `page.url()` ends with `/ferrite/` |
| "Guides" link → guides | `page.url()` ends with `/ferrite/guides` |
| GitHub link has correct href | `href` contains `github.com`, `target="_blank"` |
| GitHub link does not navigate | clicking opens new tab, current page unchanged |

### `home.spec.ts` — Home page content

| Test | Assertion |
|------|-----------|
| Hero heading | `h1` text "Ferrite" visible |
| Hero tagline | contains "Windows desktop" |
| Download button visible | text "Download for Windows" |
| Download button href | contains `github.com` and `releases` |
| 3 feature cards | headings "Animated", "Custom Sprites", "Scriptable" all visible |
| "Read the Guides" link | navigates to `/ferrite/guides` |

### `canvas.spec.ts` — Animated pet canvas

Wasm loads asynchronously; tests must wait for non-empty canvas pixels.

| Test | Assertion |
|------|-----------|
| Canvas element present | `#pet-canvas` is in DOM |
| Canvas draws pixels | `getImageData` has non-zero alpha channel after up to 10 s |
| No console errors | `page.on('console')` captures no `error`-level messages |

Canvas pixel check via `page.evaluate`:
```ts
await expect.poll(async () => {
  return page.evaluate(() => {
    const c = document.getElementById('pet-canvas') as HTMLCanvasElement;
    const d = c.getContext('2d')!.getImageData(0, 0, c.width, c.height).data;
    return Array.from(d).some(v => v > 0);
  });
}, { timeout: 10_000 }).toBe(true);
```

### `guides.spec.ts` — Guide index and detail pages

| Test | Assertion |
|------|-----------|
| Guides index heading | text "Guides" visible |
| All 4 guide links present | "Getting Started", "Custom Sprites", "State Machines", "Configuration" |
| Getting Started navigates | URL ends with `/guides/getting-started` |
| Custom Sprites navigates | URL ends with `/guides/custom-sprites` |
| State Machines navigates | URL ends with `/guides/state-machines` |
| Configuration navigates | URL ends with `/guides/configuration` |
| Getting Started has h1 | `<h1>` with content visible on page |
| Each guide page has content | page body text length > 100 chars |

### `not-found.spec.ts` — 404 handling

| Test | Assertion |
|------|-----------|
| Unknown route shows 404 | navigate to `/ferrite/does-not-exist`, text "404" visible |
| Not-found message | text "Page not found" visible |

---

## Running the tests

```bash
cd crates/ferrite-web
npm install
npx playwright install chromium
npm run test:e2e
```

For CI (no reuse of existing server):
```bash
CI=true npm run test:e2e
```

To run a single spec:
```bash
npm run test:e2e -- e2e/canvas.spec.ts
```

---

## Error handling notes

- `dx serve` takes up to ~120 s on first cold build; `webServer.timeout` is set accordingly
- `reuseExistingServer: !process.env.CI` lets developers run `dx serve` themselves during development to skip rebuild
- Canvas test uses `expect.poll` with a 10 s timeout to tolerate variable wasm init time
- Console error listener is installed before navigation so no early errors are missed
