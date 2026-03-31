# Ferrite Web E2E Test Suite — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Playwright E2E test suite to `crates/ferrite-web` that auto-starts `dx serve` on a random port and verifies all routes, content, canvas rendering, and navigation.

**Architecture:** A Node.js launcher script finds a free OS port, sets `TEST_PORT`, then spawns `playwright test`. `playwright.config.ts` reads `TEST_PORT` and threads it into `webServer.command` (`dx serve --port <port>`) and `baseURL`. Five spec files cover navigation, home content, animated canvas, guides, and 404.

**Tech Stack:** `@playwright/test` ^1.50, Chromium, Node.js (free-port script), existing `dx serve` + Dioxus 0.7 app.

---

## File Map

| Action | Path | Responsibility |
|--------|------|----------------|
| Modify | `crates/ferrite-web/package.json` | Add `@playwright/test` devDep, `test:e2e` and `css` scripts |
| Create | `crates/ferrite-web/playwright.config.ts` | Playwright config; reads `TEST_PORT`; wires `webServer` |
| Create | `crates/ferrite-web/scripts/test.js` | Finds free port, sets `TEST_PORT`, spawns `playwright test` |
| Create | `crates/ferrite-web/e2e/nav.spec.ts` | Nav bar tests (all pages) |
| Create | `crates/ferrite-web/e2e/home.spec.ts` | Home page content tests |
| Create | `crates/ferrite-web/e2e/canvas.spec.ts` | Wasm canvas pixel + console error tests |
| Create | `crates/ferrite-web/e2e/guides.spec.ts` | Guide index + detail page tests |
| Create | `crates/ferrite-web/e2e/not-found.spec.ts` | 404 route test |
| Modify | `crates/ferrite-web/.gitignore` (create if absent) | Ignore playwright-report, test-results |

---

## Task 1: Project scaffold — package.json, .gitignore, install

**Files:**
- Modify: `crates/ferrite-web/package.json`
- Create: `crates/ferrite-web/.gitignore`

- [ ] **Step 1: Update `package.json`**

Replace `crates/ferrite-web/package.json` entirely with:

```json
{
  "scripts": {
    "css": "npx tailwindcss -i ./input.css -o ./public/assets/tailwind.css",
    "test:e2e": "node scripts/test.js"
  },
  "dependencies": {
    "@tailwindcss/cli": "^4.2.2",
    "tailwindcss": "^3.4.19"
  },
  "devDependencies": {
    "@playwright/test": "^1.50"
  }
}
```

- [ ] **Step 2: Create `crates/ferrite-web/.gitignore`**

```
node_modules/
playwright-report/
test-results/
```

- [ ] **Step 3: Install dependencies and Playwright browser**

```bash
cd crates/ferrite-web
npm install
npx playwright install chromium
```

Expected: `node_modules/@playwright/test` present, Chromium downloaded.

- [ ] **Step 4: Commit**

```bash
cd crates/ferrite-web
git add package.json .gitignore package-lock.json
git commit -m "chore(web): add @playwright/test, gitignore playwright artifacts"
```

---

## Task 2: Playwright config and port-finder script

**Files:**
- Create: `crates/ferrite-web/playwright.config.ts`
- Create: `crates/ferrite-web/scripts/test.js`

- [ ] **Step 1: Create `scripts/test.js`**

```js
// scripts/test.js
// Finds a free OS port, sets TEST_PORT, spawns `playwright test`.
// Cross-platform — no bash required.
const net = require('net');
const { spawn } = require('child_process');

const server = net.createServer();
server.listen(0, '127.0.0.1', () => {
  const port = server.address().port;
  server.close(() => {
    const proc = spawn(
      'npx',
      ['playwright', 'test', ...process.argv.slice(2)],
      {
        env: { ...process.env, TEST_PORT: String(port) },
        stdio: 'inherit',
        shell: true,
      }
    );
    proc.on('close', code => process.exit(code ?? 0));
  });
});
```

- [ ] **Step 2: Create `playwright.config.ts`**

```ts
import { defineConfig } from '@playwright/test';

const port = parseInt(process.env.TEST_PORT ?? '8080');

export default defineConfig({
  testDir: './e2e',
  // Each test file gets a fresh browser context
  use: {
    baseURL: `http://localhost:${port}`,
  },
  webServer: {
    // Rebuild CSS then start dev server on the chosen port
    command: `npm run css && dx serve --port ${port}`,
    url: `http://localhost:${port}/ferrite/`,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
  projects: [
    { name: 'chromium', use: { browserName: 'chromium' } },
  ],
});
```

- [ ] **Step 3: Verify config is valid**

```bash
cd crates/ferrite-web
npx playwright --version
```

Expected: prints version like `Version 1.5x.x` without errors.

- [ ] **Step 4: Commit**

```bash
git add crates/ferrite-web/playwright.config.ts crates/ferrite-web/scripts/test.js
git commit -m "test(web): add Playwright config and random-port launcher"
```

---

## Task 3: `nav.spec.ts` — Navigation bar

**Files:**
- Create: `crates/ferrite-web/e2e/nav.spec.ts`

- [ ] **Step 1: Create `e2e/nav.spec.ts`**

```ts
import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/ferrite/');
});

test('nav bar is visible on home', async ({ page }) => {
  await expect(page.locator('nav')).toBeVisible();
});

test('Home link navigates to home page', async ({ page }) => {
  // Go to guides first so the click is meaningful
  await page.goto('/ferrite/guides');
  await page.locator('nav').getByText('Home').click();
  await expect(page).toHaveURL(/\/ferrite\/?$/);
});

test('Guides link navigates to guides index', async ({ page }) => {
  await page.locator('nav').getByText('Guides').click();
  await expect(page).toHaveURL(/\/ferrite\/guides$/);
});

test('GitHub link has correct href and target', async ({ page }) => {
  const github = page.locator('nav a[href*="github.com"]');
  await expect(github).toBeVisible();
  await expect(github).toHaveAttribute('target', '_blank');
  const href = await github.getAttribute('href');
  expect(href).toMatch(/github\.com/);
});

test('GitHub link opens new tab and does not navigate current page', async ({ page, context }) => {
  const [newPage] = await Promise.all([
    context.waitForEvent('page'),
    page.locator('nav a[href*="github.com"]').click(),
  ]);
  await expect(page).toHaveURL(/\/ferrite\/?$/);
  await newPage.close();
});
```

- [ ] **Step 2: Run nav tests**

```bash
cd crates/ferrite-web
npm run test:e2e -- e2e/nav.spec.ts
```

Expected: 5 tests pass. If `dx serve` is already running on a different port, it still works because a new port is chosen.

- [ ] **Step 3: Commit**

```bash
git add crates/ferrite-web/e2e/nav.spec.ts
git commit -m "test(web): add nav bar E2E tests"
```

---

## Task 4: `home.spec.ts` — Home page content

**Files:**
- Create: `crates/ferrite-web/e2e/home.spec.ts`

- [ ] **Step 1: Create `e2e/home.spec.ts`**

```ts
import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/ferrite/');
});

test('hero heading shows Ferrite', async ({ page }) => {
  await expect(page.locator('h1').first()).toContainText('Ferrite');
});

test('hero tagline mentions Windows desktop', async ({ page }) => {
  await expect(page.getByText(/Windows desktop/)).toBeVisible();
});

test('Download for Windows button is visible', async ({ page }) => {
  await expect(page.getByText('Download for Windows').first()).toBeVisible();
});

test('Download button links to GitHub releases', async ({ page }) => {
  const btn = page.getByText('Download for Windows').first();
  const href = await btn.getAttribute('href');
  expect(href).toMatch(/github\.com/);
  expect(href).toMatch(/releases/);
});

test('three feature cards are visible', async ({ page }) => {
  await expect(page.getByText('Animated')).toBeVisible();
  await expect(page.getByText('Custom Sprites')).toBeVisible();
  await expect(page.getByText('Scriptable')).toBeVisible();
});

test('Read the Guides link navigates to guides index', async ({ page }) => {
  await page.getByText('Read the Guides').click();
  await expect(page).toHaveURL(/\/ferrite\/guides$/);
});
```

- [ ] **Step 2: Run home tests**

```bash
cd crates/ferrite-web
npm run test:e2e -- e2e/home.spec.ts
```

Expected: 6 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/ferrite-web/e2e/home.spec.ts
git commit -m "test(web): add home page content E2E tests"
```

---

## Task 5: `canvas.spec.ts` — Animated pet canvas

**Files:**
- Create: `crates/ferrite-web/e2e/canvas.spec.ts`

Note: wasm initialisation is asynchronous. The pixel poll uses `expect.poll` with a 10 s timeout and 500 ms check interval to accommodate variable build+load times. The console error check installs the listener **before** `goto` so no messages are missed.

- [ ] **Step 1: Create `e2e/canvas.spec.ts`**

```ts
import { test, expect } from '@playwright/test';

test('pet canvas element is present in DOM', async ({ page }) => {
  await page.goto('/ferrite/');
  await expect(page.locator('#pet-canvas')).toBeVisible();
});

test('canvas draws non-zero pixels after wasm loads', async ({ page }) => {
  await page.goto('/ferrite/');

  await expect.poll(
    () =>
      page.evaluate(() => {
        const c = document.getElementById('pet-canvas') as HTMLCanvasElement | null;
        if (!c) return false;
        const ctx = c.getContext('2d');
        if (!ctx) return false;
        const data = ctx.getImageData(0, 0, c.width, c.height).data;
        return Array.from(data).some(v => v > 0);
      }),
    { timeout: 10_000, intervals: [500] }
  ).toBe(true);
});

test('no console errors during home page load', async ({ page }) => {
  const errors: string[] = [];
  page.on('console', msg => {
    if (msg.type() === 'error') errors.push(msg.text());
  });

  await page.goto('/ferrite/');

  // Allow time for wasm to fully initialise
  await page.waitForTimeout(3_000);

  expect(errors, `unexpected console errors: ${errors.join(', ')}`).toHaveLength(0);
});
```

- [ ] **Step 2: Run canvas tests**

```bash
cd crates/ferrite-web
npm run test:e2e -- e2e/canvas.spec.ts
```

Expected: 3 tests pass. The pixel poll test may take a few seconds.

- [ ] **Step 3: Commit**

```bash
git add crates/ferrite-web/e2e/canvas.spec.ts
git commit -m "test(web): add animated canvas E2E tests"
```

---

## Task 6: `guides.spec.ts` — Guide index and detail pages

**Files:**
- Create: `crates/ferrite-web/e2e/guides.spec.ts`

- [ ] **Step 1: Create `e2e/guides.spec.ts`**

```ts
import { test, expect } from '@playwright/test';

test.describe('Guides index', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/ferrite/guides');
  });

  test('heading is visible', async ({ page }) => {
    await expect(page.getByRole('heading', { name: 'Guides' })).toBeVisible();
  });

  test('all four guide links are present', async ({ page }) => {
    await expect(page.getByText('Getting Started')).toBeVisible();
    await expect(page.getByText('Custom Sprites')).toBeVisible();
    await expect(page.getByText('State Machines')).toBeVisible();
    await expect(page.getByText('Configuration')).toBeVisible();
  });

  test('Getting Started link goes to correct slug', async ({ page }) => {
    await page.getByText('Getting Started').click();
    await expect(page).toHaveURL(/\/ferrite\/guides\/getting-started$/);
  });

  test('Custom Sprites link goes to correct slug', async ({ page }) => {
    await page.getByText('Custom Sprites').click();
    await expect(page).toHaveURL(/\/ferrite\/guides\/custom-sprites$/);
  });

  test('State Machines link goes to correct slug', async ({ page }) => {
    await page.getByText('State Machines').click();
    await expect(page).toHaveURL(/\/ferrite\/guides\/state-machines$/);
  });

  test('Configuration link goes to correct slug', async ({ page }) => {
    await page.getByText('Configuration').click();
    await expect(page).toHaveURL(/\/ferrite\/guides\/configuration$/);
  });
});

test.describe('Guide detail pages', () => {
  test('Getting Started has an h1 heading', async ({ page }) => {
    await page.goto('/ferrite/guides/getting-started');
    await expect(page.locator('h1').first()).toBeVisible();
  });

  for (const slug of [
    'getting-started',
    'custom-sprites',
    'state-machines',
    'configuration',
  ]) {
    test(`${slug} page has substantial content`, async ({ page }) => {
      await page.goto(`/ferrite/guides/${slug}`);
      const text = await page.locator('body').innerText();
      expect(text.length, `guide "${slug}" body should have > 100 chars`).toBeGreaterThan(100);
    });
  }
});
```

- [ ] **Step 2: Run guides tests**

```bash
cd crates/ferrite-web
npm run test:e2e -- e2e/guides.spec.ts
```

Expected: 11 tests pass (6 index + 1 heading + 4 content).

- [ ] **Step 3: Commit**

```bash
git add crates/ferrite-web/e2e/guides.spec.ts
git commit -m "test(web): add guides index and detail page E2E tests"
```

---

## Task 7: `not-found.spec.ts` — 404 route

**Files:**
- Create: `crates/ferrite-web/e2e/not-found.spec.ts`

- [ ] **Step 1: Create `e2e/not-found.spec.ts`**

```ts
import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/ferrite/does-not-exist');
});

test('unknown route shows 404 heading', async ({ page }) => {
  await expect(page.getByRole('heading', { name: '404' })).toBeVisible();
});

test('404 page shows Page not found message', async ({ page }) => {
  await expect(page.getByText(/Page not found/)).toBeVisible();
});
```

- [ ] **Step 2: Run 404 tests**

```bash
cd crates/ferrite-web
npm run test:e2e -- e2e/not-found.spec.ts
```

Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/ferrite-web/e2e/not-found.spec.ts
git commit -m "test(web): add 404 not-found route E2E tests"
```

---

## Task 8: Full suite run and final commit

- [ ] **Step 1: Run the full suite**

```bash
cd crates/ferrite-web
npm run test:e2e
```

Expected output (all passing):

```
Running 27 tests using 1 worker

  ✓ nav.spec.ts:8:1 › nav bar is visible on home
  ✓ nav.spec.ts:14:1 › Home link navigates to home page
  ✓ nav.spec.ts:21:1 › Guides link navigates to guides index
  ✓ nav.spec.ts:27:1 › GitHub link has correct href and target
  ✓ nav.spec.ts:35:1 › GitHub link opens new tab and does not navigate current page
  ✓ home.spec.ts:8:1 › hero heading shows Ferrite
  ✓ home.spec.ts:12:1 › hero tagline mentions Windows desktop
  ✓ home.spec.ts:16:1 › Download for Windows button is visible
  ✓ home.spec.ts:20:1 › Download button links to GitHub releases
  ✓ home.spec.ts:26:1 › three feature cards are visible
  ✓ home.spec.ts:32:1 › Read the Guides link navigates to guides index
  ✓ canvas.spec.ts:5:1 › pet canvas element is present in DOM
  ✓ canvas.spec.ts:10:1 › canvas draws non-zero pixels after wasm loads
  ✓ canvas.spec.ts:24:1 › no console errors during home page load
  ✓ guides.spec.ts:8:1 › Guides index › heading is visible
  ✓ guides.spec.ts:12:1 › Guides index › all four guide links are present
  ✓ guides.spec.ts:19:1 › Guides index › Getting Started link goes to correct slug
  ✓ guides.spec.ts:24:1 › Guides index › Custom Sprites link goes to correct slug
  ✓ guides.spec.ts:29:1 › Guides index › State Machines link goes to correct slug
  ✓ guides.spec.ts:34:1 › Guides index › Configuration link goes to correct slug
  ✓ guides.spec.ts:41:1 › Guide detail pages › Getting Started has an h1 heading
  ✓ guides.spec.ts:48:1 › Guide detail pages › getting-started page has substantial content
  ✓ guides.spec.ts:48:1 › Guide detail pages › custom-sprites page has substantial content
  ✓ guides.spec.ts:48:1 › Guide detail pages › state-machines page has substantial content
  ✓ guides.spec.ts:48:1 › Guide detail pages › configuration page has substantial content
  ✓ not-found.spec.ts:6:1 › unknown route shows 404 heading
  ✓ not-found.spec.ts:10:1 › 404 page shows Page not found message

  27 passed (Xm Xs)
```

If any test fails, check the HTML report:
```bash
npx playwright show-report
```

- [ ] **Step 2: Commit any minor fixes, then final commit**

```bash
git add -A
git commit -m "test(web): complete Playwright E2E suite — 27 tests passing"
```
