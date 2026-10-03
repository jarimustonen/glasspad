import assert from 'node:assert/strict';
import { chromium } from 'playwright';
const url = process.argv[2];
assert.ok(url, 'Mermaid artifact URL required');
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 400, height: 800 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(url);
  const frame = page.frameLocator('iframe');
  await frame.locator('.gp-mermaid-ready svg').first().waitFor({ timeout: 20000 });
  assert.equal(await frame.locator('.gp-mermaid-ready svg').count(), 2);
  assert.ok((await frame.locator('.gp-mermaid-ready').first().innerText()).includes('Start'));
  assert.ok((await frame.locator('.gp-mermaid-ready').last().innerText()).includes('Alice'));
  assert.ok((await frame.locator('.gp-mermaid:not(.gp-mermaid-ready)').innerText()).includes('not a diagram'));
  const before = await frame.locator('.gp-mermaid-ready svg').first().getAttribute('id');
  const toggle = page.getByRole('button', { name: /theme/i });
  await toggle.click(); // auto -> light
  await toggle.click(); // light -> dark
  await frame.locator('html[data-theme="dark"]').waitFor();
  await frame.locator('.gp-mermaid-ready svg').first().waitFor();
  await frame.locator('.gp-mermaid-ready svg').first().evaluate((svg, old) => new Promise((resolve, reject) => {
    const started = Date.now();
    const tick = () => svg.parentElement.querySelector('svg')?.id !== old ? resolve() :
      Date.now() - started > 20000 ? reject(new Error('Mermaid did not rerender on theme change')) : setTimeout(tick, 100);
    tick();
  }), before);
  assert.equal(await frame.locator('.gp-mermaid-ready svg').count(), 2);
  const width = await frame.locator('body').evaluate(() => ({ page: document.documentElement.scrollWidth, viewport: innerWidth }));
  assert.ok(width.page <= width.viewport + 1, `diagram overflow: ${JSON.stringify(width)}`);
  assert.deepEqual(errors, []);
  console.log('PASS Mermaid flowchart + sequence render, theme change, and mobile width');
  await page.close();
  const nojs = await browser.newContext({ javaScriptEnabled: false });
  const fallback = await nojs.newPage();
  await fallback.goto(url.replace('/myspace/mermaid', '/myspace/_c/mermaid'));
  assert.ok((await fallback.locator('.gp-mermaid').first().innerText()).includes('flowchart LR'));
  console.log('PASS Mermaid source fallback without JavaScript');
  await nojs.close();
} finally { await browser.close(); }
