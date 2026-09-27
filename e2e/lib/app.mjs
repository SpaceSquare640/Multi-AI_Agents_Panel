/** Helpers shared by the specs. */

import { readFileSync } from "node:fs";
import { FAKE_PORT } from "../fake-ollama.mjs";

/** The English strings, read from the app's own translation file so a
 *  wording change does not silently break a selector. The app starts in
 *  English unless a language was saved, and a fresh test machine has none. */
export const en = JSON.parse(readFileSync(new URL("../../src/locales/en/translation.json", import.meta.url), "utf8"));

/** Fills `{{name}}`-style placeholders the way i18next does. */
export const fill = (template, values) =>
  template.replace(/\{\{(\w+)\}\}/g, (_, key) => String(values[key]));

/** Steers the fake Ollama; see fake-ollama.mjs. */
export async function fake(path, body) {
  const response = await fetch(`http://127.0.0.1:${FAKE_PORT}${path}`, {
    method: body === undefined ? "GET" : "POST",
    headers: { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  return response.json();
}

/** Calls a Tauri command from inside the app's webview, exactly as the
 *  frontend does. */
export function invoke(command, args = {}) {
  return browser.execute((c, a) => window.__TAURI_INTERNALS__.invoke(c, a), command, args);
}

/** Waits until the webview has loaded far enough to talk to the backend. */
export async function waitForApp() {
  await browser.waitUntil(() => browser.execute(() => Boolean(window.__TAURI_INTERNALS__)), {
    timeout: 60_000,
    timeoutMsg: "the app never exposed its Tauri bridge",
  });
}
