/**
 * The window must stay responsive while a model is answering.
 *
 * Deliberately uses commands only, no UI: the same file is run against a
 * build from before the fix, where it has to fail — a test that cannot
 * fail proves nothing — and that build's screens are different.
 *
 * The measurement: while a send waits on an 8-second reply, a second,
 * trivial command is issued. When sends ran on the IPC thread, that second
 * command queued behind the send and took the full ~8 seconds; with sends
 * moved off it, the second command answers at once.
 */

import { FAKE_MODEL } from "../fake-ollama.mjs";
import { fake, invoke, waitForApp } from "../lib/app.mjs";

const SLOW_REPLY_MS = 8_000;
const RESPONSIVE_MS = 1_500;

describe("while a model is answering", () => {
  let sessionId;

  before(async () => {
    await waitForApp();
    const agent = await invoke("create_agent", {
      name: "Freeze probe",
      roleTemplate: null,
      systemPrompt: null,
      providerKind: "local",
      providerName: "ollama",
      model: FAKE_MODEL,
    });
    const session = await invoke("create_independent_session", { title: "Freeze probe", agentId: agent.id });
    sessionId = session.id;
  });

  afterEach(async () => {
    await fake("/__control", { delayMs: 0, reply: null });
  });

  it("another command still answers at once", async () => {
    await fake("/__control", { delayMs: SLOW_REPLY_MS, reply: "slow reply" });

    const result = await browser.execute(async (id) => {
      const call = window.__TAURI_INTERNALS__.invoke;
      const send = call("send_chat_message", { sessionId: id, content: "take your time" }).then(
        () => "completed",
        (err) => String(err),
      );
      // Let the send reach the backend before measuring.
      await new Promise((resolve) => setTimeout(resolve, 500));
      const started = performance.now();
      await call("list_sessions");
      const otherCommandMs = performance.now() - started;
      return { otherCommandMs, send: await send };
    }, sessionId);

    expect(result.send).toBe("completed");
    expect(result.otherCommandMs).toBeLessThan(RESPONSIVE_MS);
  });

  it("Stop cancels the reply and nothing is saved", async () => {
    await fake("/__control", { delayMs: 5_000, reply: "this must never be saved" });

    const result = await browser.execute(async (id) => {
      const call = window.__TAURI_INTERNALS__.invoke;
      const send = call("send_chat_message", { sessionId: id, content: "never mind" }).then(
        () => "completed",
        (err) => String(err),
      );
      await new Promise((resolve) => setTimeout(resolve, 1_000));
      const accepted = await call("cancel_send", { sessionId: id });
      const outcome = await send;
      const messages = await call("list_messages", { sessionId: id });
      return { accepted, outcome, saved: messages.some((m) => m.content === "this must never be saved") };
    }, sessionId);

    // Before the fix, cancel_send could not even be dispatched until the
    // send it was meant to stop had finished, so it found nothing to stop.
    expect(result.accepted).toBe(true);
    expect(result.outcome).toBe("cancelled");
    expect(result.saved).toBe(false);
  });
});
