/**
 * A stand-in for Ollama on its usual port, so the real app can be driven
 * end to end without a model, a GPU or an API key.
 *
 * The app talks to Ollama at a fixed `http://localhost:11434` and uses two
 * endpoints: `GET /api/tags` (is it running, which models) and
 * `POST /api/chat` (non-streaming). This answers both, and adds three
 * endpoints the tests use to steer it:
 *
 *   POST /__control  { delayMs?, reply? }  how the next chats behave
 *   GET  /__log                             every /api/chat body received
 *   POST /__reset                           clear the log and the controls
 *
 * The delay is the point. A reply that takes seconds is what froze the
 * window, and it is what a responsive window has to survive.
 *
 * Run on its own with `node fake-ollama.mjs` to poke at the app by hand.
 */

import http from "node:http";
import { pathToFileURL } from "node:url";

export const FAKE_MODEL = "fake-model:latest";
export const FAKE_PORT = 11434;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function readBody(req) {
  return new Promise((resolve, reject) => {
    let data = "";
    req.setEncoding("utf8");
    req.on("data", (chunk) => (data += chunk));
    req.on("end", () => resolve(data));
    req.on("error", reject);
  });
}

export async function startFakeOllama({ port = FAKE_PORT } = {}) {
  const fresh = () => ({ delayMs: 0, reply: null, log: [] });
  let state = fresh();
  let replies = 0;

  const server = http.createServer(async (req, res) => {
    const send = (status, body) => {
      res.writeHead(status, { "content-type": "application/json" });
      res.end(JSON.stringify(body));
    };
    try {
      const raw = await readBody(req);

      if (req.method === "GET" && req.url === "/api/tags") {
        return send(200, { models: [{ name: FAKE_MODEL, size: 1, modified_at: new Date().toISOString() }] });
      }

      if (req.method === "POST" && req.url === "/api/chat") {
        const request = JSON.parse(raw || "{}");
        state.log.push(request);
        replies += 1;
        const reply = state.reply ?? `fake reply ${replies}`;
        await sleep(state.delayMs);
        return send(200, {
          model: request.model,
          created_at: new Date().toISOString(),
          message: { role: "assistant", content: reply },
          done: true,
        });
      }

      if (req.method === "POST" && req.url === "/__control") {
        const { delayMs, reply } = JSON.parse(raw || "{}");
        if (delayMs !== undefined) state.delayMs = delayMs;
        if (reply !== undefined) state.reply = reply;
        return send(200, { ok: true });
      }

      if (req.method === "GET" && req.url === "/__log") {
        return send(200, state.log);
      }

      if (req.method === "POST" && req.url === "/__reset") {
        state = fresh();
        return send(200, { ok: true });
      }

      return send(404, { error: `fake Ollama has no ${req.method} ${req.url}` });
    } catch (err) {
      return send(500, { error: String(err) });
    }
  });

  // Listen on both IPv4 and IPv6: the app asks for "localhost", which
  // Windows may resolve to ::1 first.
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen({ host: "::", port, ipv6Only: false }, resolve);
  });

  return {
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}

// Paths with spaces are percent-encoded in import.meta.url, so compare
// against a properly built file URL rather than a hand-assembled one.
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await startFakeOllama();
  console.log(`fake Ollama listening on port ${FAKE_PORT} with model ${FAKE_MODEL}`);
}
