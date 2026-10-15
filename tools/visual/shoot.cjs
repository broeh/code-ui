// Visual check for the browser preview (npm run dev, mock data).
// Captures the main UI states at 2400x1350,
// and reports console errors, text below 12px and horizontal overflow.
// Usage: node tools/visual/shoot.cjs [outDir] [url]
const { chromium } = require("playwright");
const path = require("path");
const fs = require("fs");

const outDir = path.resolve(process.argv[2] ?? "/tmp/code-ui-shots");
const url = process.argv[3] ?? "http://localhost:1420";
fs.mkdirSync(outDir, { recursive: true });

async function send(page, text) {
  const box = page.locator(".composer textarea");
  await box.fill(text);
  await box.press("Shift+Enter");
  const trust = page.getByRole("button", { name: "Trust folder and continue", exact: true });
  await trust.waitFor({ timeout: 1000 }).then(() => trust.click()).catch(() => {});
  await page.waitForTimeout(2500);
}

async function audit(page, name) {
  const result = await page.evaluate(() => {
    const small = [];
    for (const el of document.querySelectorAll("body *")) {
      const own = [...el.childNodes].some(
        (n) => n.nodeType === 3 && n.textContent.trim(),
      );
      if (!own || !el.getClientRects().length) continue;
      const size = parseFloat(getComputedStyle(el).fontSize);
      if (size < 12)
        small.push(`${size}px ${el.tagName.toLowerCase()}.${el.className} "${el.textContent.trim().slice(0, 40)}"`);
    }
    return {
      small: small.slice(0, 15),
      smallCount: small.length,
      overflow: document.documentElement.scrollWidth > innerWidth,
    };
  });
  console.log(`${name}: text<12px=${result.smallCount} overflow=${result.overflow}`);
  result.small.forEach((line) => console.log(`  ${line}`));
}

(async () => {
  const browser = await chromium.launch({ headless: true, args: ["--no-sandbox"] });
  const page = await browser.newPage({ viewport: { width: 2400, height: 1350 }, deviceScaleFactor: 1 });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto(url, { waitUntil: "networkidle" });
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(500);
  await page.getByRole("button", { name: "3. Preferences", exact: true }).click();
  // Full access is selected only for synthetic demo screenshots.
  await page.getByRole("radio").nth(1).check();
  await page.getByRole("button", { name: "Open workspace", exact: true }).click();

  const shots = [
    ["01-welcome", async () => {}],
    ["02-chat", () => send(page, "Review the README and suggest a cleaner project structure")],
    ["03-question", () => send(page, "Show me a question card")],
    ["04-approval", async () => {
      await page.keyboard.press("Control+t");
      await page.waitForTimeout(500);
      await page.click('button[aria-label="Agent settings"]');
      await page.locator(".agent-popover button", { hasText: "Ask before actions" }).click();
      await page.keyboard.press("Escape");
      await send(page, "Print the project folder");
    }],
    ["05-picker", () => page.click('button[aria-label="Agent settings"]')],
    ["06-editor", async () => {
      await page.keyboard.press("Escape");
      await page.locator(".file-row", { hasText: "README.md" }).first().click();
      await page.waitForTimeout(1000);
    }],
  ];
  for (const [name, step] of shots) {
    await step();
    await page.waitForTimeout(300);
    await page.screenshot({ path: path.join(outDir, `${name}.png`) });
    await audit(page, name);
  }
  console.log(`console errors: ${errors.length}`);
  errors.forEach((e) => console.log(`  ${e}`));
  console.log(`screenshots in ${outDir}`);
  await browser.close();
})();
