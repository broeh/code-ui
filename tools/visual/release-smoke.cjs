// Synthetic browser checks. This never starts a real agent CLI or touches native app data.
const { chromium } = require("playwright");
const assert = require("node:assert/strict");
const { spawn } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "../..");
const url = process.argv[2] ?? "http://127.0.0.1:1423";
const output = path.resolve(process.argv[3] ?? path.join(root, ".cache/ui-checks"));
fs.mkdirSync(output, { recursive: true });

async function waitForServer(server) {
  for (let attempt = 0; attempt < 100; attempt++) {
    if (server?.exitCode != null) throw Error("Preview server exited");
    try { if ((await fetch(url)).ok) return; } catch {}
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw Error("Preview server did not start");
}

(async () => {
  const server = process.argv[2] ? null : spawn(process.execPath, [path.join(root, "node_modules/vite/bin/vite.js"), "--host", "127.0.0.1", "--port", "1423"], { cwd: root, stdio: "ignore" });
  let browser;
  try {
    await waitForServer(server);
    browser = await chromium.launch({ headless: true, args: ["--no-sandbox"] });
    const errors = [];
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const page = await context.newPage();
    page.setDefaultTimeout(10000);
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(url);
    await page.getByRole("heading", { name: "Set up CODE_UI" }).waitFor();
    assert.equal(await page.locator(".tab-bar .tab").count(), 0, "setup must finish before opening a draft");
    await page.screenshot({ path: path.join(output, "setup-projects.png") });
    await page.getByLabel("Projects folder", { exact: true }).fill("/home/user/My Projects");
    await page.getByRole("checkbox", { name: "Create this folder if it does not exist" }).check();
    await page.getByRole("button", { name: "Next", exact: true }).click();
    await page.getByRole("checkbox", { name: "Claude Code", exact: true }).uncheck();
    await page.getByRole("button", { name: "Check again", exact: true }).click();
    await page.getByRole("button", { name: "Check again", exact: true }).waitFor();
    await page.screenshot({ path: path.join(output, "setup-agents.png") });
    await page.getByRole("button", { name: "Next", exact: true }).click();
    assert.equal(await page.getByLabel("Default agent", { exact: true }).inputValue(), "codex");
    assert.equal(await page.getByRole("radio").first().isChecked(), true);
    assert.equal(await page.getByRole("checkbox", { name: "Discover Codex history" }).isChecked(), false);
    await page.screenshot({ path: path.join(output, "setup-preferences.png") });
    await page.getByRole("button", { name: "Open workspace", exact: true }).click();
    await page.getByRole("heading", { name: "Set up CODE_UI" }).waitFor({ state: "hidden" });
    const composer = page.getByRole("textbox", { name: "Message Codex", exact: true });
    await composer.fill("Reply with a short greeting");
    await composer.press("Shift+Enter");
    await page.getByRole("heading", { name: "Trust this project?" }).waitFor();
    assert.equal(await page.locator(".message.assistant").count(), 0);
    await page.screenshot({ path: path.join(output, "project-trust.png") });
    await page.getByRole("button", { name: "Cancel", exact: true }).click();
    assert.equal(await composer.inputValue(), "Reply with a short greeting");
    await composer.press("Shift+Enter");
    await page.getByRole("button", { name: "Trust folder and continue", exact: true }).click();
    await page.getByRole("button", { name: "Allow", exact: true }).waitFor();
    await page.screenshot({ path: path.join(output, "approval.png") });
    await page.getByRole("button", { name: "Allow", exact: true }).click();
    await page.locator(".message.assistant").first().waitFor();
    await page.screenshot({ path: path.join(output, "workspace.png") });
    await page.reload();
    await page.locator(".message.assistant").first().waitFor();
    assert.equal(await page.getByRole("heading", { name: "Set up CODE_UI" }).count(), 0);
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByRole("button", { name: "2. Coding agents", exact: true }).click();
    assert.equal(await page.getByRole("checkbox", { name: "Claude Code", exact: true }).isChecked(), false);
    assert.equal(await page.getByRole("checkbox", { name: "Codex", exact: true }).isChecked(), true);
    await page.getByRole("button", { name: "Cancel", exact: true }).click();
    await context.close();

    const empty = await browser.newContext({ viewport: { width: 1000, height: 650 } });
    await empty.addInitScript(() => localStorage.setItem("code-ui:mock_agents", JSON.stringify({ claude: false, codex: false, agy: false, muse: false })));
    const zero = await empty.newPage();
    zero.setDefaultTimeout(10000);
    zero.on("pageerror", (error) => errors.push(error.message));
    await zero.goto(url);
    await zero.getByRole("heading", { name: "Set up CODE_UI" }).waitFor();
    await zero.getByRole("button", { name: "Next", exact: true }).click();
    await zero.screenshot({ path: path.join(output, "setup-small-window.png") });
    const fits = await zero.evaluate(() => {
      const dialog = document.querySelector("dialog").getBoundingClientRect();
      return dialog.left >= 0 && dialog.right <= innerWidth && dialog.top >= 0 && dialog.bottom <= innerHeight && document.documentElement.scrollWidth <= innerWidth && [...document.querySelectorAll("dialog footer button")].every((button) => button.getBoundingClientRect().bottom <= dialog.bottom - 12);
    });
    assert.equal(fits, true, "setup should fit the minimum supported window");
    await zero.getByRole("button", { name: "Next", exact: true }).click();
    await zero.getByRole("button", { name: "Open workspace", exact: true }).click();
    await zero.locator(".composer textarea").waitFor();
    assert.equal(await zero.getByRole("button", { name: "Send message", exact: true }).isDisabled(), true);
    assert.match(await zero.locator(".agents-pill").innerText(), /0 CLIs detected/);
    await empty.close();
    assert.deepEqual(errors, []);
    console.log("Passed: setup, Codex-only defaults, project trust/cancel, persistence, zero CLIs, minimum window, and console checks.");
    console.log(`Screenshots: ${output}`);
  } finally {
    await browser?.close();
    if (server) { server.kill("SIGTERM"); await new Promise((resolve) => server.once("exit", resolve)); }
  }
})().catch((error) => { console.error(error); process.exitCode = 1; });
