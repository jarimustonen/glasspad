// Real-host table template regression: serve a space named tablespace with the
// gallery's samples/table.md as index.md and `template: table` in glasspad.yaml.
// GLASSPAD_PORT=<port> node tests/security/table.mjs
import assert from 'node:assert/strict';
import { chromium } from 'playwright';
import { execFileSync } from 'node:child_process';

const port = process.env.GLASSPAD_PORT;
assert.ok(port, 'GLASSPAD_PORT required');
const browser = await chromium.launch({ headless: true });
try {
  for (const width of [360, 1280]) for (const theme of ['light', 'dark']) for (const js of [true, false]) {
    const context = await browser.newContext({ viewport: { width, height: 900 }, javaScriptEnabled: js });
    const page = await context.newPage();
    await page.goto(`http://127.0.0.1:${port}/tablespace/${js ? 'index' : '_c/index'}${!js && theme === 'dark' ? '?gp_theme=dark' : ''}`);
    const frame = js ? page.frameLocator('iframe') : page;
    await frame.locator('.gp-table table').waitFor();
    if (js && theme === 'dark') {
      const toggle = page.getByRole('button', { name: /theme/i });
      await toggle.click(); await toggle.click();
      await frame.locator('html[data-theme="dark"]').waitFor();
    }
    assert.equal(await frame.locator('table thead th').count(), 10);
    assert.equal(await frame.locator('table tbody tr').count(), 12);
    assert.equal(await frame.locator('.gp-table-scroll').count(), js ? 1 : 0);
    const metrics = await frame.locator('.gp-table').evaluate(root => {
      const t = root.querySelector('table'), wrap = root.querySelector('.gp-table-scroll');
      return { bodyWidth: document.documentElement.scrollWidth, viewport: innerWidth,
        tableWidth: t.getBoundingClientRect().width, regionWidth: wrap?.clientWidth,
        scrollWidth: wrap?.scrollWidth, regionLabel: wrap?.getAttribute('aria-label'),
        sticky: getComputedStyle(t.querySelector('thead th')).position,
        first: getComputedStyle(t.querySelector('tbody tr td')).position,
        fg: getComputedStyle(t.querySelector('tbody td')).color };
    });
    assert.ok(metrics.bodyWidth <= metrics.viewport + 1, `body overflow ${width}/${theme}/${js}: ${JSON.stringify(metrics)}`);
    assert.ok(metrics.fg !== 'rgba(0, 0, 0, 0)', 'visible text');
    if (js) {
      assert.match(metrics.regionLabel, /Equipment inventory \(scrollable\)/);
      assert.equal(metrics.sticky, 'sticky'); assert.equal(metrics.first, 'sticky');
      const region = frame.locator('.gp-table-scroll');
      const overflows = metrics.scrollWidth > metrics.regionWidth + 1;
      assert.equal(await region.getAttribute('tabindex'), overflows ? '0' : '-1');
      if (overflows) {
        await region.focus();
        assert.equal(await region.evaluate(el => document.activeElement === el), true);
      }
      if (width === 360) {
        assert.ok(metrics.scrollWidth > metrics.regionWidth, 'wide table scrolls locally');
        await page.keyboard.press('End');
        await page.keyboard.press('ArrowRight');
        await page.waitForTimeout(150);
        assert.ok(await region.evaluate(el => el.scrollLeft > 0), 'keyboard horizontal scroll');
      }
    } else {
      assert.ok(metrics.tableWidth <= metrics.viewport + 1, 'no-script table wraps');
    }
    const link = frame.getByRole('link', { name: 'Supply guide' });
    await link.focus();
    assert.equal(await link.evaluate(el => document.activeElement === el), true);
    assert.equal(await link.getAttribute('href'), './index.md');
    // Direct content document can print even if the shell has no JS.
    if (!js && width === 1280 && theme === 'light') {
      await page.emulateMedia({ media: 'print' });
      const pdf = await page.pdf({ printBackground: true, preferCSSPageSize: true, format: 'Letter' });
      assert.ok(pdf.length > 1000);
      if (process.platform === 'linux') {
        const text = execFileSync('pdftotext', ['-layout', '-', '-'], { input: pdf }).toString();
        for (const heading of ['ASSET', 'ITEM', 'CATEGO', 'SITE', 'CUSTODIA', 'PURCHAS', 'REPLACEMENT', 'UNIT', 'STATUS', 'NOTES']) {
          assert.ok(text.includes(heading), `missing print column ${heading}`);
        }
        assert.ok(text.includes('1518') && text.includes('Color reference'), 'last row printed');
      }
    }
    console.log(`PASS table ${width}px ${theme} script=${js}`);
    await context.close();
  }
  // Wide first-column labels must not pin over the remaining cells. This fixture
  // also exercises print's script-added wrap class, not just the no-script PDF.
  const page = await browser.newPage({ viewport: { width: 360, height: 900 } });
  await page.goto(`http://127.0.0.1:${port}/tablespace/wide`);
  const frame = page.frameLocator('iframe');
  await frame.locator('.gp-table-scroll').waitFor();
  const wide = await frame.locator('.gp-table-scroll').evaluate(el => {
    const t = el.querySelector('table');
    return { width: el.clientWidth, firstWidth: t.rows[0].cells[0].offsetWidth,
      firstPosition: getComputedStyle(t.rows[0].cells[0]).position,
      wraps: t.querySelectorAll('.gp-col-wrap').length,
      otherColumn: t.rows[1].cells[1].textContent };
  });
  assert.ok(wide.firstWidth > wide.width * 0.4, `fixture must trigger sticky opt-out: ${JSON.stringify(wide)}`);
  assert.equal(wide.firstPosition, 'static', 'wide first column must not obscure other columns');
  assert.ok(wide.wraps >= 2);
  await frame.locator('.gp-table-scroll').focus();
  await page.keyboard.press('End'); await page.keyboard.press('ArrowRight');
  await page.waitForTimeout(250); // Chromium's keyboard scroll animation is asynchronous.
  assert.ok(await frame.locator('.gp-table-scroll').evaluate(el => el.scrollLeft > 0));
  const print = await browser.newPage();
  await print.goto(`http://127.0.0.1:${port}/tablespace/_c/wide`);
  await print.locator('.gp-col-wrap').first().waitFor();
  await print.emulateMedia({ media: 'print' });
  assert.equal(await print.locator('.gp-col-wrap').first().evaluate(el => getComputedStyle(el).minWidth), '0px');
  assert.ok((await print.pdf({ printBackground: true, preferCSSPageSize: true })).length > 1000);
  console.log('PASS table wide labels, keyboard and enhanced print');
} finally { await browser.close(); }
