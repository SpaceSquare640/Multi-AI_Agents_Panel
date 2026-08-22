import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
  const { t } = useTranslation();
  const [keys, setKeys] = useState<ProviderKeyView[]>([]);
  const [usage, setUsage] = useState<UsageSummary[]>([]);
  const [modelProvider, setModelProvider] = useState<string>("openrouter");
  const [curatedModels, setCuratedModels] = useState<CuratedModel[]>([]);
  const [ollamaRunning, setOllamaRunning] = useState<boolean | null>(null);
  const [ollamaInstalled, setOllamaInstalled] = useState<OllamaModel[]>([]);
  const [ollamaCurated, setOllamaCurated] = useState<CuratedModel[]>([]);
  const [pullingModel, setPullingModel] = useState<string | null>(null);
  const [pullProgress, setPullProgress] = useState<{ status: string; percent: number | null } | null>(null);
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

  // Game-Playing Agent (Track A — see "Game-Playing Agent Design.md" in
  // the vault): a persistent screenshot -> local vision model -> real
  // mouse/keyboard automation loop. Off by default, only ever starts on
  // an explicit click here — see game_agent module docs.
  const [gameAgentRunning, setGameAgentRunning] = useState(false);
  const [gameAgentModel, setGameAgentModel] = useState("llava");
  const [gameAgentPrompt, setGameAgentPrompt] = useState(
    "You are playing a game. Look at the screenshot and decide the single best next action. " +
      'Reply with ONLY a JSON object: {"action":"click","x":<int>,"y":<int>} or ' +
      '{"action":"key","key":"<name>"} or {"action":"wait"}.',
  );
  const [gameAgentBusy, setGameAgentBusy] = useState(false);

  // Track B (Deep RL) — "record" pipeline stage only (see
  // Game-Playing Agent Design.md §4): starts game_agent_rl's Python CLI
  // as a background subprocess to capture a human demonstration
  // session. label/train-bc/train-rl/play don't exist yet.
  const [recording, setRecording] = useState(false);
  const [recordingSession, setRecordingSession] = useState("session-1");
  const [recordingOutputDir, setRecordingOutputDir] = useState("");
  const [recordingBusy, setRecordingBusy] = useState(false);

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
    invoke<boolean>("game_agent_status").then(setGameAgentRunning).catch((e) => setError(String(e)));
    invoke<boolean>("recording_status").then(setRecording).catch((e) => setError(String(e)));
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
      setError(t("acc.apiKeys.bulkNoValidLines"));
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
    setPullProgress(null);
    // Real streaming progress from the Rust side (see
    // ollama::pull_model_with_progress) — one event per NDJSON line
    // Ollama reports, not a static "loading" indicator.
    const unlisten = await listen<{ name: string; status: string; percent: number | null }>(
      "ollama-pull-progress",
      (event) => {
        if (event.payload.name === name) {
          setPullProgress({ status: event.payload.status, percent: event.payload.percent });
        }
      },
    );
    try {
      await invoke("pull_ollama_model", { name });
      await refreshOllama();
    } catch (err) {
      setError(String(err));
    } finally {
      unlisten();
      setPullingModel(null);
      setPullProgress(null);
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

  async function handleStartGameAgent() {
    setError(null);
    setGameAgentBusy(true);
    try {
      await invoke("start_game_agent", { model: gameAgentModel, prompt: gameAgentPrompt });
      setGameAgentRunning(true);
    } catch (err) {
      setError(String(err));
    } finally {
      setGameAgentBusy(false);
    }
  }

  async function handleStopGameAgent() {
    setError(null);
    try {
      await invoke("stop_game_agent");
      setGameAgentRunning(false);
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleStartRecording() {
    setError(null);
    setRecordingBusy(true);
    try {
      await invoke("start_recording_session", {
        session: recordingSession,
        outputDir: recordingOutputDir,
      });
      setRecording(true);
    } catch (err) {
      setError(String(err));
    } finally {
      setRecordingBusy(false);
    }
  }

  async function handleStopRecording() {
    setError(null);
    try {
      await invoke("stop_recording_session");
      setRecording(false);
    } catch (err) {
      setError(String(err));
    }
  }

  const notYetInstalled = ollamaCurated.filter(
    (m) => !ollamaInstalled.some((installed) => installed.name === m.id),
  );

  return (
    <div className="ai-control-center">
      <h1>{t("acc.title")}</h1>
      {error && (
        <div className="acc-error" role="alert">
          {error}
          <button onClick={() => setError(null)} aria-label={t("acc.dismissError")}>×</button>
        </div>
      )}

      <section className="acc-section">
        <h2>{t("acc.apiKeys.heading")}</h2>

        <form className="acc-form" onSubmit={handleAddSingle}>
          <h3>{t("acc.apiKeys.addOne")}</h3>
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
              placeholder={t("acc.apiKeys.apiKeyPlaceholder")}
              value={singleSecret}
              onChange={(e) => setSingleSecret(e.target.value)}
              required
            />
            <input
              type="text"
              placeholder={t("acc.apiKeys.labelOptionalPlaceholder")}
              value={singleLabel}
              onChange={(e) => setSingleLabel(e.target.value)}
            />
            <input
              type="text"
              placeholder={t("acc.apiKeys.modelHintOptionalPlaceholder")}
              value={singleModelHint}
              onChange={(e) => setSingleModelHint(e.target.value)}
            />
            <button type="submit">{t("acc.apiKeys.add")}</button>
          </div>
        </form>

        <form className="acc-form" onSubmit={handleBatchAdd}>
          <h3>{t("acc.apiKeys.addInBulk")}</h3>
          <p className="acc-hint">
            {t("acc.apiKeys.bulkHintBeforeCode")}
            <code>provider,secret,label,modelHint</code>
            {t("acc.apiKeys.bulkHintAfterCode")}
          </p>
          <textarea
            rows={5}
            placeholder={"openrouter,sk-or-v1-...,Ling-3.0-flash (free),inclusionai/ling-3.0-flash:free\nopenrouter,sk-or-v1-...,Poolside S (free)"}
            value={batchText}
            onChange={(e) => setBatchText(e.target.value)}
          />
          <button type="submit">{t("acc.apiKeys.importAll")}</button>
        </form>

        <div className="acc-form">
          <h3>{t("acc.apiKeys.importFromFiles")}</h3>
          <p className="acc-hint">{t("acc.apiKeys.importFromFilesHint")}</p>
          <div className="acc-form-row">
            <select value={fileImportProvider} onChange={(e) => setFileImportProvider(e.target.value)}>
              {CLOUD_PROVIDERS.map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
            <button type="button" disabled={fileImportBusy} onClick={() => handleImportFromFiles()}>
              {fileImportBusy ? t("acc.apiKeys.importing") : t("acc.apiKeys.chooseFiles")}
            </button>
          </div>
        </div>

        <table className="acc-table">
          <thead>
            <tr>
              <th>{t("acc.apiKeys.tableProvider")}</th>
              <th>{t("acc.apiKeys.tableLabel")}</th>
              <th>{t("acc.apiKeys.tableModelHint")}</th>
              <th>{t("acc.apiKeys.tableKey")}</th>
              <th>{t("acc.apiKeys.tableLastUsed")}</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {keys.length === 0 && (
              <tr>
                <td colSpan={6} className="acc-empty">
                  {t("acc.apiKeys.noKeysYet")}
                </td>
              </tr>
            )}
            {keys.map((k) => (
              <tr key={k.id}>
                <td>{k.provider}</td>
                <td>{k.label ?? "—"}</td>
                <td>{k.modelHint ?? "—"}</td>
                <td className="acc-mono">{k.maskedSecret}</td>
                <td>{k.lastUsedAt ?? t("acc.apiKeys.never")}</td>
                <td>
                  <button onClick={() => handleDeleteKey(k.id)}>{t("acc.apiKeys.delete")}</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section className="acc-section">
        <h2>{t("acc.cloudModels.heading")}</h2>
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
                placeholder={t("acc.cloudModels.searchPlaceholder")}
                value={openRouterQuery}
                onChange={(e) => setOpenRouterQuery(e.target.value)}
              />
              <button disabled={openRouterLoading} onClick={() => refreshOpenRouterModels(true).catch((e) => setError(String(e)))}>
                {openRouterLoading ? t("acc.cloudModels.refreshing") : t("acc.cloudModels.refreshFromOpenRouter")}
              </button>
            </div>
            {!openRouterLive && <p className="acc-hint">{t("acc.cloudModels.liveCatalogUnavailable")}</p>}
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
                        {t("acc.cloudModels.pricing", {
                          promptPrice: m.promptPricePerMillion?.toFixed(2) ?? "?",
                          completionPrice: m.completionPricePerMillion?.toFixed(2) ?? "?",
                        })}
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
        <h2>{t("acc.localModels.heading")}</h2>
        <p>
          {t("acc.localModels.statusLabel")}{" "}
          {ollamaRunning === null
            ? t("acc.localModels.statusChecking")
            : ollamaRunning
              ? t("acc.localModels.statusRunning")
              : t("acc.localModels.statusNotRunning")}{" "}
          <button onClick={() => refreshOllama().catch((e) => setError(String(e)))}>
            {t("acc.localModels.refresh")}
          </button>
        </p>

        <details className="acc-ollama-storage-hint">
          <summary>{t("acc.localModels.storageHintSummary")}</summary>
          <p className="acc-hint">
            {t("acc.localModels.storageHintIntro", { endpoint: "localhost:11434" })}
          </p>
          {ollamaModelsEnvHint === undefined ? null : ollamaModelsEnvHint ? (
            <p className="acc-hint">
              {t("acc.localModels.storageHintEnvSet", { path: ollamaModelsEnvHint })}
            </p>
          ) : (
            <p className="acc-hint">{t("acc.localModels.storageHintEnvUnset")}</p>
          )}
          <ul className="acc-hint">
            <li>{t("acc.localModels.storageHintWindows", { command: 'setx OLLAMA_MODELS "C:\\path\\to\\folder"' })}</li>
            <li>{t("acc.localModels.storageHintUnix", { command: "export OLLAMA_MODELS=/path/to/folder" })}</li>
          </ul>
        </details>

        {ollamaRunning && (
          <>
            <h3>{t("acc.localModels.installedHeading")}</h3>
            <table className="acc-table">
              <thead>
                <tr>
                  <th>{t("acc.localModels.tableModel")}</th>
                  <th>{t("acc.localModels.tableSize")}</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {ollamaInstalled.length === 0 && (
                  <tr>
                    <td colSpan={3} className="acc-empty">
                      {t("acc.localModels.noModelsInstalled")}
                    </td>
                  </tr>
                )}
                {ollamaInstalled.map((m) => (
                  <tr key={m.name}>
                    <td className="acc-mono">{m.name}</td>
                    <td>{formatBytes(m.size)}</td>
                    <td>
                      <button onClick={() => handleDeleteOllamaModel(m.name)}>{t("acc.localModels.remove")}</button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>

            <h3>{t("acc.localModels.availableHeading")}</h3>
            <ul className="acc-model-list">
              {notYetInstalled.map((m) => (
                <li key={m.id}>
                  <span className="acc-mono">{m.id}</span> — {m.label}{" "}
                  <button disabled={pullingModel !== null} onClick={() => handlePullModel(m.id)}>
                    {pullingModel === m.id
                      ? pullProgress?.percent !== null && pullProgress?.percent !== undefined
                        ? `${pullProgress.percent.toFixed(0)}%`
                        : pullProgress?.status ?? t("acc.localModels.installing")
                      : t("acc.localModels.install")}
                  </button>
                </li>
              ))}
            </ul>
          </>
        )}
      </section>

      <section className="acc-section">
        <h2>{t("acc.usage.heading")}</h2>
        <table className="acc-table">
          <thead>
            <tr>
              <th>{t("acc.usage.tableProvider")}</th>
              <th>{t("acc.usage.tableLabel")}</th>
              <th>{t("acc.usage.tableSuccess")}</th>
              <th>{t("acc.usage.tableFailure")}</th>
              <th>{t("acc.usage.tableLastUsed")}</th>
            </tr>
          </thead>
          <tbody>
            {usage.length === 0 && (
              <tr>
                <td colSpan={5} className="acc-empty">
                  {t("acc.usage.noUsageYet")}
                </td>
              </tr>
            )}
            {usage.map((u) => (
              <tr key={u.providerKeyId}>
                <td>{u.provider}</td>
                <td>{u.label ?? "—"}</td>
                <td>{u.successCount}</td>
                <td>{u.failureCount}</td>
                <td>{u.lastUsedAt ?? t("acc.usage.never")}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section className="acc-section">
        <h2>{t("acc.gameAgent.heading")}</h2>
        <p className="acc-hint">{t("acc.gameAgent.warning")}</p>
        <div className="acc-form-row">
          <input
            type="text"
            placeholder={t("acc.gameAgent.modelPlaceholder")}
            value={gameAgentModel}
            onChange={(e) => setGameAgentModel(e.target.value)}
            disabled={gameAgentRunning}
          />
        </div>
        <textarea
          rows={3}
          value={gameAgentPrompt}
          onChange={(e) => setGameAgentPrompt(e.target.value)}
          disabled={gameAgentRunning}
        />
        <p>
          {t("acc.gameAgent.statusLabel")}{" "}
          {gameAgentRunning ? t("acc.gameAgent.statusRunning") : t("acc.gameAgent.statusStopped")}{" "}
          {gameAgentRunning ? (
            <button onClick={() => handleStopGameAgent()}>{t("acc.gameAgent.stop")}</button>
          ) : (
            <button disabled={gameAgentBusy} onClick={() => handleStartGameAgent()}>
              {gameAgentBusy ? t("acc.gameAgent.starting") : t("acc.gameAgent.start")}
            </button>
          )}
        </p>
      </section>

      <section className="acc-section">
        <h2>{t("acc.recording.heading")}</h2>
        <p className="acc-hint">{t("acc.recording.hint")}</p>
        <div className="acc-form-row">
          <input
            type="text"
            placeholder={t("acc.recording.sessionNamePlaceholder")}
            value={recordingSession}
            onChange={(e) => setRecordingSession(e.target.value)}
            disabled={recording}
          />
          <input
            type="text"
            placeholder={t("acc.recording.outputDirectoryPlaceholder")}
            value={recordingOutputDir}
            onChange={(e) => setRecordingOutputDir(e.target.value)}
            disabled={recording}
          />
        </div>
        <p>
          {t("acc.recording.statusLabel")}{" "}
          {recording ? t("acc.recording.statusRecording") : t("acc.recording.statusStopped")}{" "}
          {recording ? (
            <button onClick={() => handleStopRecording()}>{t("acc.recording.stop")}</button>
          ) : (
            <button disabled={recordingBusy || !recordingOutputDir.trim()} onClick={() => handleStartRecording()}>
              {recordingBusy ? t("acc.recording.starting") : t("acc.recording.startRecording")}
            </button>
          )}
        </p>
      </section>
    </div>
  );
}
