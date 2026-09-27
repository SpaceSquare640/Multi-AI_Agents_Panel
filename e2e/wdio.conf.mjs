/**
 * WebdriverIO drives the real desktop app through tauri-driver, which
 * forwards to Microsoft Edge WebDriver (the app's webview is WebView2).
 *
 * Needs, before `npm test`:
 *   - the app built without bundling:  npm run tauri build -- --debug --no-bundle
 *   - tauri-driver:                    cargo install tauri-driver --locked
 *   - msedgedriver.exe on PATH, matching the installed WebView2 version
 *
 * APP_PATH overrides which build is launched. CI uses it to point the
 * current tests at an older commit's build, to prove the freeze test
 * fails where the freeze still exists.
 */

import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { startFakeOllama } from "./fake-ollama.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const application =
  process.env.APP_PATH ?? path.resolve(here, "../src-tauri/target/debug/multi-ai-agents-panel.exe");
const artifacts = path.join(here, "artifacts");

let tauriDriver;
let fakeOllama;

export const config = {
  runner: "local",
  hostname: "127.0.0.1",
  port: 4444,
  // Sorted by name and run one at a time: 01 walks a brand-new user's first
  // launch, so it has to meet the app before anything else creates agents.
  specs: ["./specs/*.e2e.mjs"],
  maxInstances: 1,
  capabilities: [{ maxInstances: 1, "tauri:options": { application } }],
  logLevel: "warn",
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: { ui: "bdd", timeout: 120_000 },
  waitforTimeout: 20_000,

  // The launcher process outlives every spec's worker, so the fake lives
  // here and the specs steer it over HTTP.
  onPrepare: async () => {
    mkdirSync(artifacts, { recursive: true });
    fakeOllama = await startFakeOllama();
  },
  onComplete: async () => {
    await fakeOllama?.close();
  },

  beforeSession: () => {
    tauriDriver = spawn(path.resolve(os.homedir(), ".cargo", "bin", "tauri-driver"), [], {
      stdio: [null, process.stdout, process.stderr],
    });
  },
  afterSession: () => {
    tauriDriver?.kill();
  },

  afterTest: async (test, _context, { passed }) => {
    if (passed) return;
    const name = `${test.parent} -- ${test.title}`.replace(/[^\w.-]+/g, "_").slice(0, 150);
    await browser.saveScreenshot(path.join(artifacts, `${name}.png`)).catch(() => {});
  },
};
