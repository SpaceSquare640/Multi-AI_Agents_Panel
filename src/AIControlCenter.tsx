import { useEffect, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  CLOUD_PROVIDERS,
  type CuratedModel,
  type OllamaModel,
  type OpenRouterModel,
  type ProviderKeyView,
  type UsageSummary,
} from "./types";
import "./AIControlCenter.css";

type BatchEntry = {
  provider: string;
  secret: string;
  label?: string;
  modelHint?: string;
};

function parseBatchInput(text: string): BatchEntry[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith("#"))
    .map((line) => {
      const [provider, secret, label, modelHint] = line.split(",").map((s) => s.trim());
      return { provider, secret, label: label || undefined, modelHint: modelHint || undefined };
    })
    .filter((entry) => entry.provider && entry.secret);
}

function formatBytes(bytes: number | null): string {
  if (bytes === null) return "—";
  const gb = bytes / 1_000_000_000;
  return gb >= 1 ? `${gb.toFixed(1)} GB` : `${(bytes / 1_000_000).toFixed(0)} MB`;
}

export default function AIControlCenter() {
  const [keys, setKeys] = useState<ProviderKeyView[]>([]);
  const [usage, setUsage] = useState<UsageSummary[]>([]);
  const [modelProvider, setModelProvider] = useState<string>("openrouter");
  const [curatedModels, setCuratedModels] = useState<CuratedModel[]>([]);
  const [ollamaRunning, setOllamaRunning] = useState<boolean | null>(null);
  const [ollamaInstalled, setOllamaInstalled] = useState<OllamaModel[]>([]);
  const [ollamaCurated, setOllamaCurated] = useState<CuratedModel[]>([]);
  const [pullingModel, setPullingModel] = useState<string | null>(null);
  const [ollamaModelsEnvHint, setOllamaModelsEnvHint] = useState<string | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);

  // Live OpenRouter catalog (search + real USD pricing) — only relevant
  // when modelProvider === "openrouter"; other providers stay on the
  // static curated list. See openrouter_catalog.rs for the 24h-cache +
  // fallback-to-static-on-failure policy this mirrors.
  const [openRouterModels, setOpenRouterModels] = useState<OpenRouterModel[]>([]);
  const [openRouterLive, setOpenRouterLive] = useState(true);
  const [openRouterQuery, setOpenRouterQuery] = useState("");
  const [openRouterLoading, setOpenRouterLoading] = useState(false);

  // Single-add form state.
  const [singleProvider, setSingleProvider] = useState<string>("openrouter");
  const [singleSecret, setSingleSecret] = useState("");
  const [singleLabel, setSingleLabel] = useState("");
  const [singleModelHint, setSingleModelHint] = useState("");

  // Batch-add form state.
  const [batchText, setBatchText] = useState("");

  // Import-from-files form state.
  const [fileImportProvider, setFileImportProvider] = useState<string>("openrouter");
  const [fileImportBusy, setFileImportBusy] = useState(false);

  async function refreshKeys() {
    setKeys(await invoke<ProviderKeyView[]>("list_provider_keys"));
  }

  async function refreshUsage() {
    setUsage(await invoke<UsageSummary[]>("get_usage_summary"));
  }

  async function refreshCuratedModels(provider: string) {
    setCuratedModels(await invoke<CuratedModel[]>("list_curated_models", { provider }));
  }

  async function refreshOpenRouterModels(forceRefresh: boolean) {
    setOpenRouterLoading(true);
    try {
      const result = await invoke<{ models: OpenRouterModel[]; live: boolean }>("list_openrouter_models_live", {
        forceRefresh,
      });
      setOpenRouterModels(result.models);
      setOpenRouterLive(result.live);
    } finally {
      setOpenRouterLoading(false);
    }
  }

  async function refreshOllama() {
    const running = await invoke<boolean>("ollama_is_running");
    setOllamaRunning(running);
    if (running) {
      setOllamaInstalled(await invoke<OllamaModel[]>("list_ollama_installed_models"));
    } else {
      setOllamaInstalled([]);
    }
  }

  useEffect(() => {
    refreshKeys().catch((e) => setError(String(e)));
    refreshUsage().catch((e) => setError(String(e)));
    refreshOllama().catch((e) => setError(String(e)));
    invoke<CuratedModel[]>("list_curated_models", { provider: "ollama" })
      .then(setOllamaCurated)
      .catch((e) => setError(String(e)));
    invoke<string | null>("ollama_models_env_hint")
      .then(setOllamaModelsEnvHint)
      .catch((e) => setError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    refreshCuratedModels(modelProvider).catch((e) => setError(String(e)));
    if (modelProvider === "openrouter") {
      refreshOpenRouterModels(false).catch((e) => setError(String(e)));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [modelProvider]);

  async function handleAddSingle(e: FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      await invoke("add_provider_key", {
        provider: singleProvider,
        secret: singleSecret,
        label: singleLabel || null,
        modelHint: singleModelHint || null,
      });
      setSingleSecret("");
      setSingleLabel("");
      setSingleModelHint("");
      await refreshKeys();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleBatchAdd(e: FormEvent) {
    e.preventDefault();
    setError(null);
    const entries = parseBatchInput(batchText);
    if (entries.length === 0) {
      setError("No valid lines found. Format: provider,secret[,label[,modelHint]] — one per line.");
      return;
    }
    try {
      await invoke("batch_add_provider_keys", { entries });
      setBatchText("");
      await refreshKeys();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleImportFromFiles() {
    setError(null);
    const paths = await open({ multiple: true, title: "Select API key files" });
    if (!paths) return;
    const list = Array.isArray(paths) ? paths : [paths];
    if (list.length === 0) return;
    setFileImportBusy(true);
    try {
      await invoke("import_provider_keys_from_files", { provider: fileImportProvider, paths: list });
      await refreshKeys();
    } catch (err) {
      setError(String(err));
    } finally {
      setFileImportBusy(false);
    }
  }

  async function handleDeleteKey(id: string) {
    setError(null);
    try {
      await invoke("delete_provider_key", { id });
      await refreshKeys();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handlePullModel(name: string) {
    setError(null);
    setPullingModel(name);
    try {
      await invoke("pull_ollama_model", { name });
      await refreshOllama();
    } catch (err) {
      setError(String(err));
    } finally {
      setPullingModel(null);
    }
  }

  async function handleDeleteOllamaModel(name: string) {
    setError(null);
    try {
      await invoke("delete_ollama_model", { name });
      await refreshOllama();
    } catch (err) {
      setError(String(err));
    }
  }

  const notYetInstalled = ollamaCurated.filter(
    (m) => !ollamaInstalled.some((installed) => installed.name === m.id),
  );

  return (
    <div className="ai-control-center">
      <h1>AI Control Center</h1>
      {error && (
        <div className="acc-error" role="alert">
          {error}
          <button onClick={() => setError(null)}>×</button>
        </div>
      )}

      <section className="acc-section">
        <h2>API Keys</h2>

        <form className="acc-form" onSubmit={handleAddSingle}>
          <h3>Add one</h3>
          <div className="acc-form-row">
            <select value={singleProvider} onChange={(e) => setSingleProvider(e.target.value)}>
              {CLOUD_PROVIDERS.map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
            <input
              type="password"
              placeholder="API key"
              value={singleSecret}
              onChange={(e) => setSingleSecret(e.target.value)}
              required
            />
            <input
              type="text"
              placeholder="Label (optional)"
              value={singleLabel}
              onChange={(e) => setSingleLabel(e.target.value)}
            />
            <input
              type="text"
              placeholder="Model hint (optional)"
              value={singleModelHint}
              onChange={(e) => setSingleModelHint(e.target.value)}
            />
            <button type="submit">Add</button>
          </div>
        </form>

        <form className="acc-form" onSubmit={handleBatchAdd}>
          <h3>Add in bulk</h3>
          <p className="acc-hint">
            One key per line: <code>provider,secret,label,modelHint</code> — label and modelHint
            are optional.
          </p>
          <textarea
            rows={5}
            placeholder={"openrouter,sk-or-v1-...,Ling-3.0-flash (free),inclusionai/ling-3.0-flash:free\nopenrouter,sk-or-v1-...,Poolside S (free)"}
            value={batchText}
            onChange={(e) => setBatchText(e.target.value)}
          />
          <button type="submit">Import all</button>
        </form>

        <div className="acc-form">
          <h3>Import from files</h3>
          <p className="acc-hint">
            Pick multiple files: each file's name becomes the label, its full content (trimmed)
            becomes the key. All files use the same provider.
          </p>
          <div className="acc-form-row">
            <select value={fileImportProvider} onChange={(e) => setFileImportProvider(e.target.value)}>
              {CLOUD_PROVIDERS.map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
            <button type="button" disabled={fileImportBusy} onClick={() => handleImportFromFiles()}>
              {fileImportBusy ? "Importing…" : "Choose files…"}
            </button>
          </div>
        </div>

        <table className="acc-table">
          <thead>
            <tr>
              <th>Provider</th>
              <th>Label</th>
              <th>Model hint</th>
              <th>Key</th>
              <th>Last used</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {keys.length === 0 && (
              <tr>
                <td colSpan={6} className="acc-empty">
                  No API keys yet.
                </td>
              </tr>
            )}
            {keys.map((k) => (
              <tr key={k.id}>
                <td>{k.provider}</td>
                <td>{k.label ?? "—"}</td>
                <td>{k.modelHint ?? "—"}</td>
                <td className="acc-mono">{k.maskedSecret}</td>
                <td>{k.lastUsedAt ?? "never"}</td>
                <td>
                  <button onClick={() => handleDeleteKey(k.id)}>Delete</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section className="acc-section">
        <h2>Cloud AI Models</h2>
        <select value={modelProvider} onChange={(e) => setModelProvider(e.target.value)}>
          {CLOUD_PROVIDERS.map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
        </select>
        {modelProvider === "openrouter" ? (
          <>
            <div className="acc-form-row">
              <input
                type="text"
                placeholder="Search all OpenRouter models…"
                value={openRouterQuery}
                onChange={(e) => setOpenRouterQuery(e.target.value)}
              />
              <button disabled={openRouterLoading} onClick={() => refreshOpenRouterModels(true).catch((e) => setError(String(e)))}>
                {openRouterLoading ? "Refreshing…" : "Refresh from OpenRouter"}
              </button>
            </div>
            {!openRouterLive && (
              <p className="acc-hint">
                ⚠ Couldn't reach OpenRouter's live catalog — showing the built-in default list instead. Prices and
                availability shown here may not be current; check your network connection.
              </p>
            )}
            <ul className="acc-model-list">
              {openRouterModels
                .filter((m) => {
                  const q = openRouterQuery.trim().toLowerCase();
                  return !q || m.id.toLowerCase().includes(q) || m.name.toLowerCase().includes(q);
                })
                .map((m) => (
                  <li key={m.id}>
                    <span className="acc-mono">{m.id}</span> — {m.name}
                    {(m.promptPricePerMillion !== null || m.completionPricePerMillion !== null) && (
                      <span className="acc-hint">
                        {" "}
                        (${m.promptPricePerMillion?.toFixed(2) ?? "?"} in / $
                        {m.completionPricePerMillion?.toFixed(2) ?? "?"} out per 1M tokens)
                      </span>
                    )}
                  </li>
                ))}
            </ul>
          </>
        ) : (
          <ul className="acc-model-list">
            {curatedModels.map((m) => (
              <li key={m.id}>
                <span className="acc-mono">{m.id}</span> — {m.label}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="acc-section">
        <h2>Local AI Models (Ollama)</h2>
        <p>
          Status:{" "}
          {ollamaRunning === null ? "checking…" : ollamaRunning ? "running" : "not running"}{" "}
          <button onClick={() => refreshOllama().catch((e) => setError(String(e)))}>Refresh</button>
        </p>

        <details className="acc-ollama-storage-hint">
          <summary>Where are Ollama's models stored?</summary>
          <p className="acc-hint">
            Ollama is a separate program this app doesn't control or install — it's only called over
            {" "}
            <code>localhost:11434</code>. This app does not move or manage Ollama's model files itself.
          </p>
          {ollamaModelsEnvHint === undefined ? null : ollamaModelsEnvHint ? (
            <p className="acc-hint">
              This app's own process sees <code>OLLAMA_MODELS={ollamaModelsEnvHint}</code>. This may or may not
              match what the actual Ollama service uses, if it was started from a different environment.
            </p>
          ) : (
            <p className="acc-hint">
              No <code>OLLAMA_MODELS</code> override is visible to this app (Ollama is likely using its own
              default location). To change where Ollama stores models, set the <code>OLLAMA_MODELS</code>{" "}
              environment variable yourself and restart the Ollama service:
            </p>
          )}
          <ul className="acc-hint">
            <li>
              Windows (PowerShell, as your user):{" "}
              <code>setx OLLAMA_MODELS "C:\path\to\folder"</code>, then restart Ollama
            </li>
            <li>
              macOS/Linux: add <code>export OLLAMA_MODELS=/path/to/folder</code> to your shell profile, then
              restart Ollama
            </li>
          </ul>
        </details>

        {ollamaRunning && (
          <>
            <h3>Installed</h3>
            <table className="acc-table">
              <thead>
                <tr>
                  <th>Model</th>
                  <th>Size</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {ollamaInstalled.length === 0 && (
                  <tr>
                    <td colSpan={3} className="acc-empty">
                      No models installed yet.
                    </td>
                  </tr>
                )}
                {ollamaInstalled.map((m) => (
                  <tr key={m.name}>
                    <td className="acc-mono">{m.name}</td>
                    <td>{formatBytes(m.size)}</td>
                    <td>
                      <button onClick={() => handleDeleteOllamaModel(m.name)}>Remove</button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>

            <h3>Available to install</h3>
            <ul className="acc-model-list">
              {notYetInstalled.map((m) => (
                <li key={m.id}>
                  <span className="acc-mono">{m.id}</span> — {m.label}{" "}
                  <button disabled={pullingModel !== null} onClick={() => handlePullModel(m.id)}>
                    {pullingModel === m.id ? "Installing…" : "Install"}
                  </button>
                </li>
              ))}
            </ul>
          </>
        )}
      </section>

      <section className="acc-section">
        <h2>Usage</h2>
        <table className="acc-table">
          <thead>
            <tr>
              <th>Provider</th>
              <th>Label</th>
              <th>Success</th>
              <th>Failure</th>
              <th>Last used</th>
            </tr>
          </thead>
          <tbody>
            {usage.length === 0 && (
              <tr>
                <td colSpan={5} className="acc-empty">
                  No usage recorded yet.
                </td>
              </tr>
            )}
            {usage.map((u) => (
              <tr key={u.providerKeyId}>
                <td>{u.provider}</td>
                <td>{u.label ?? "—"}</td>
                <td>{u.successCount}</td>
                <td>{u.failureCount}</td>
                <td>{u.lastUsedAt ?? "never"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </div>
  );
}
