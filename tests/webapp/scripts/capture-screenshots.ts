/**
 * Playwright screenshot capture for ferrite-web guide images.
 *
 * Run after building the webapp dist:
 *   cd tests/webapp
 *   npx serve -l 8080 ../../crates/ferrite-webapp/dist &
 *   npx tsx scripts/capture-screenshots.ts
 *
 * Output: crates/ferrite-web/public/assets/guides/{slug}.png
 */

import { chromium } from '@playwright/test';
import * as path from 'path';
import * as fs from 'fs';

const BASE_URL = 'http://localhost:8080';
const OUT_DIR = path.resolve(__dirname, '../../../crates/ferrite-web/public/assets/guides');

async function waitForApp(page: any): Promise<void> {
  await page.waitForFunction(
    '() => typeof window.__ferrite !== "undefined"',
    { timeout: 15000 }
  );
  // Extra wait for WASM to fully render
  await page.waitForTimeout(2500);
}

// Tab bar pixel positions at 1280×720. Adjust if tabs shift.
const TAB_POSITIONS: Record<string, { x: number; y: number }> = {
  config:    { x: 45,  y: 20 },
  sprites:   { x: 130, y: 20 },
  sm:        { x: 250, y: 20 },
  simulation:{ x: 400, y: 20 },
};

(async () => {
  fs.mkdirSync(OUT_DIR, { recursive: true });

  const browser = await chromium.launch();
  const page = await browser.newPage();
  await page.setViewportSize({ width: 1280, height: 720 });

  console.log(`Navigating to ${BASE_URL}…`);
  await page.goto(BASE_URL);
  await waitForApp(page);

  // ── getting-started: Config tab (always has content) ──────────────────
  {
    const slug = 'getting-started';
    const pos = TAB_POSITIONS['config'];
    console.log(`Clicking config tab for ${slug}…`);
    await page.mouse.click(pos.x, pos.y);
    await page.waitForTimeout(1000);
    const outPath = path.join(OUT_DIR, `${slug}.png`);
    await page.screenshot({ path: outPath });
    console.log(`  → saved ${outPath}`);
  }

  // ── custom-sprites: Sprites tab, then click eSheep to open editor ──────
  {
    const slug = 'custom-sprites';
    const pos = TAB_POSITIONS['sprites'];
    console.log(`Clicking sprites tab for ${slug}…`);
    await page.mouse.click(pos.x, pos.y);
    await page.waitForTimeout(1200);
    // Click the first sprite in the list (eSheep, typically at ~x=45, y=93)
    await page.mouse.click(45, 93);
    await page.waitForTimeout(1500);
    const outPath = path.join(OUT_DIR, `${slug}.png`);
    await page.screenshot({ path: outPath });
    console.log(`  → saved ${outPath}`);
  }

  // ── state-machines: SM tab ─────────────────────────────────────────────
  {
    const slug = 'state-machines';
    const pos = TAB_POSITIONS['sm'];
    console.log(`Clicking sm tab for ${slug}…`);
    await page.mouse.click(pos.x, pos.y);
    await page.waitForTimeout(1500);
    const outPath = path.join(OUT_DIR, `${slug}.png`);
    await page.screenshot({ path: outPath });
    console.log(`  → saved ${outPath}`);
  }

  // ── configuration: Config tab ──────────────────────────────────────────
  {
    const slug = 'configuration';
    const pos = TAB_POSITIONS['config'];
    console.log(`Clicking config tab for ${slug}…`);
    await page.mouse.click(pos.x, pos.y);
    await page.waitForTimeout(1000);
    const outPath = path.join(OUT_DIR, `${slug}.png`);
    await page.screenshot({ path: outPath });
    console.log(`  → saved ${outPath}`);
  }

  await browser.close();
  console.log('Done.');
})();
