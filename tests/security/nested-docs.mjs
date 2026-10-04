// Optional local integration probe against the real native-agent-host/docs tree.
// Start `glasspad loopback serve ../native-agent-host/docs --port 19431` first;
// GLASSPAD_PORT=19431 node tests/security/nested-docs.mjs
import { chromium } from "playwright";

const port = process.env.GLASSPAD_PORT;
if (!port) throw new Error("GLASSPAD_PORT is required");
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${port}/docs/`);
  async function clickAndExpect(from, linkText, to) {
    await page.waitForFunction((slug) => document.querySelector("iframe")?.getAttribute("src")?.includes(`/_c/${slug}`), from);
    // The iframe src changes before the new document finishes loading. A
    // frameLocator waits for the new link instead of racing page.frame(url).
    await page.frameLocator("iframe").getByRole("link", { name: linkText, exact: true }).first().click();
    await page.waitForFunction((slug) => document.querySelector("iframe")?.getAttribute("src")?.includes(`/_c/${slug}`), to);
    if (page.url() !== `http://127.0.0.1:${port}/docs/`) throw new Error("trusted shell navigated away");
  }
  await clickAndExpect("index", "Arkkitehtuuri", "architecture/index");
  await clickAndExpect("architecture/index", "Components and authorities", "architecture/components");
  // The trusted sidebar returns to the authored home page without leaving the shell.
  await page.locator('a[data-slug="index"]').first().click();
  await clickAndExpect("index", "Päätökset", "decisions/index");
  await clickAndExpect("decisions/index", "ADR 0001", "decisions/adr-0001-unix-native-agent-host");
  console.log("nested docs browser navigation passed");
} finally {
  await browser.close();
}
