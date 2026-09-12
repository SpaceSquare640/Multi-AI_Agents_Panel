import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import InstallGuidance from "./InstallGuidance";
import { Icon } from "./shell/Icons";
import {
  CLOUD_PROVIDERS,
  type CuratedModel,
  type McpServer,
  type ModelRecommendation,
  type OllamaModel,
  type OpenRouterModel,
  type ProviderKeyView,
} from "./types";
import "./styles/screens/models.css";

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

/** Maps llmfit's fit level onto the design's two verdict tints. Anything
 *  that is not an unambiguous fit is drawn as the cautious one — reading
 *  "tight" optimistically is the expensive way to be wrong. */
function verdictFit(fitLevel: string): "good" | "tight" {
  return /^(excellent|good|fits)/i.test(fitLevel.trim()) ? "good" : "tight";
}

/** Models — API keys, cloud catalogues, local Ollama models, hardware fit
 *  and MCP servers. Rebuilt against the v2 design.
 *
 *  The design's sidebar for this screen lists Agents, and hangs Role
 *  templates and New agent off it. Those live in Chat's sidebar in this
 *  app, and moving them changes where agents are created rather than how
 *  they look — so the sidebar is left alone here and the question travels
 *  with Chat, the screen that would lose them. Same reasoning the rail
 *  split used: decide it in the step that makes the decision real.
 *
 *  Providers become cards, which is the design's arrangement and a better
 *  fit than a flat key table: a provider is a thing with a state, and
 *  several keys can belong to one. The keys stay in a table inside the
 *  card's detail — they are rows with the same five fields.
 *
 *  Not ported: the design's fallback-order list (fallback is configured
 *  per agent, in Chat) and its hardware-fit grid of VRAM/RAM figures
 *  (llmfit returns per-model verdicts, not machine specs). */
export default function AIControlCenter({
  onOpenUsage,
  onOpenManual,
}: {
  onOpenUsage: () => void;
  onOpenManual: () => void;
}) {
  const { t } = useTranslation();
  const [keys, setKeys] = useState<ProviderKeyView[]>([]);
  const [modelProvider, setModelProvider] = useState<string>("openrouter");
  const [curatedModels, setCuratedModels] = useState<CuratedModel[]>([]);
  const [ollamaRunning, setOllamaRunning] = useState<boolean | null>(null);
  const [ollamaInstalled, setOllamaInstalled] = useState<OllamaModel[]>([]);
  const [ollamaCurated, setOllamaCurated] = useState<CuratedModel[]>([]);
  const [pullingModel, setPullingModel] = useState<string | null>(null);
  const [pullProgress, setPullProgress] = useState<{ status: string; percent: number | null } | null>(null);
  const [ollamaModelsEnvHint, setOllamaModelsEnvHint] = useState<string | null | undefined>(undefined);
  const [suggestedOllamaModelsDir, setSuggestedOllamaModelsDir] = useState<string | null>(null);
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

  // MCP (Model Context Protocol) servers — see mcp_manager module docs.
  // Per-agent authorization to actually call a server's tools is
  // granted from Chat.tsx's agent header (mirrors Skills grant chips),
  // not here; this section only manages the server list itself.
  const [mcpServers, setMcpServers] = useState<McpServer[]>([]);
  const [newMcpName, setNewMcpName] = useState("");
  const [newMcpCommand, setNewMcpCommand] = useState("");
  const [newMcpArgs, setNewMcpArgs] = useState("");
  const [mcpBusy, setMcpBusy] = useState(false);

  // Hardware fit advisor — "which local model fits my machine", placed
  // next to Local Models since that's the only place its recommendation
  // is actually acted on (install the model it points at).
  const [hardwareRecommendations, setHardwareRecommendations] = useState<ModelRecommendation[] | null>(null);
  const [loadingHardwareRecommendations, setLoadingHardwareRecommendations] = useState(false);

  async function refreshKeys() {
    setKeys(await invoke<ProviderKeyView[]>("list_provider_keys"));
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

  async function handleRecommendLocalModels() {
    setError(null);
    setLoadingHardwareRecommendations(true);
    try {
      setHardwareRecommendations(await invoke<ModelRecommendation[]>("recommend_local_models", { limit: 8 }));
    } catch (err) {
      setError(String(err));
    } finally {
      setLoadingHardwareRecommendations(false);
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

  async function refreshMcpServers() {
    setMcpServers(await invoke<McpServer[]>("list_mcp_servers"));
  }

  async function handleAddMcpServer(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setMcpBusy(true);
    try {
      const args = newMcpArgs
        .split(/\s+/)
        .map((a) => a.trim())
        .filter((a) => a.length > 0);
      await invoke("add_mcp_server", { name: newMcpName, command: newMcpCommand, args });
      setNewMcpName("");
      setNewMcpCommand("");
      setNewMcpArgs("");
      await refreshMcpServers();
    } catch (err) {
      setError(String(err));
    } finally {
      setMcpBusy(false);
    }
  }

  async function handleDeleteMcpServer(id: string) {
    setError(null);
    try {
      await invoke("delete_mcp_server", { id });
      await refreshMcpServers();
    } catch (err) {
      setError(String(err));
    }
  }

  useEffect(() => {
    refreshKeys().catch((e) => setError(String(e)));
    refreshOllama().catch((e) => setError(String(e)));
    invoke<CuratedModel[]>("list_curated_models", { provider: "ollama" })
      .then(setOllamaCurated)
      .catch((e) => setError(String(e)));
    invoke<string | null>("ollama_models_env_hint")
      .then(setOllamaModelsEnvHint)
      .catch((e) => setError(String(e)));
    invoke<string>("suggest_ollama_models_dir")
      .then(setSuggestedOllamaModelsDir)
      .catch((e) => setError(String(e)));
    refreshMcpServers().catch((e) => setError(String(e)));
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

  const notYetInstalled = ollamaCurated.filter(
    (m) => !ollamaInstalled.some((installed) => installed.name === m.id),
  );

  /* One card per provider. Cloud providers come from the fixed list;
     Ollama is added as the local one, which is why it is not in it. */
  const keysByProvider = new Map<string, ProviderKeyView[]>();
  for (const k of keys) {
    keysByProvider.set(k.provider, [...(keysByProvider.get(k.provider) ?? []), k]);
  }

  return (
    <>
      <div className="workspace-header">
        <span className="workspace-title">{t("acc.title")}</span>
        <div className="workspace-actions">
          <button
            className="btn btn-ghost btn-sm"
            type="button"
            onClick={() => {
              refreshKeys().catch((e) => setError(String(e)));
              refreshOllama().catch((e) => setError(String(e)));
              refreshMcpServers().catch((e) => setError(String(e)));
            }}
          >
            {t("acc.localModels.refresh")}
          </button>
        </div>
      </div>

      <div className="workspace-body">
        <div className="pane pane-wide">
          {error && (
            <div className="callout" data-kind="danger" role="alert">
              <div className="callout-body">{error}</div>
            </div>
          )}

          {/* ---------- providers ---------- */}
          <section>
            <div className="section-head">
              <h2>{t("acc.providers")}</h2>
            </div>

            <div className="card provider">
              <span className="provider-mark" data-where="local">
                <Icon name="local" size="sm" />
              </span>
              <div className="provider-body">
                <div className="provider-name">Ollama</div>
                <div className="provider-sub">
                  {ollamaRunning === null
                    ? t("acc.localModels.statusChecking")
                    : ollamaRunning
                      ? t("acc.providerSub.ollamaRunning", { count: ollamaInstalled.length })
                      : t("acc.localModels.statusNotRunning")}
                </div>
              </div>
              <div className="provider-actions">
                <span
                  className="badge"
                  data-kind={ollamaRunning === null ? "idle" : ollamaRunning ? "running" : "failed"}
                >
                  {ollamaRunning === null
                    ? t("acc.localModels.statusChecking")
                    : ollamaRunning
                      ? t("acc.providerBadge.reachable")
                      : t("acc.providerBadge.unreachable")}
                </span>
              </div>
            </div>

            {CLOUD_PROVIDERS.map((provider) => {
              const providerKeys = keysByProvider.get(provider) ?? [];
              return (
                <div className="card provider" key={provider}>
                  <span className="provider-mark" data-where="cloud">
                    <Icon name="key" size="sm" />
                  </span>
                  <div className="provider-body">
                    <div className="provider-name">{provider}</div>
                    <div className="provider-sub">
                      {providerKeys.length === 0
                        ? t("acc.providerSub.noKey")
                        : t("acc.providerSub.keyCount", { count: providerKeys.length })}
                    </div>
                  </div>
                  {/* The badge says a key exists; it does not say the key
                      works. Nothing here validates a key against its
                      provider, so "key present" is the strongest claim the
                      data actually supports — the design's "Key valid" and
                      "Key rejected" would both be guesses. */}
                  <div className="provider-actions">
                    <span className="badge" data-kind={providerKeys.length > 0 ? "running" : "idle"}>
                      {providerKeys.length > 0
                        ? t("acc.providerBadge.keyPresent")
                        : t("acc.providerBadge.notConfigured")}
                    </span>
                  </div>
                  {providerKeys.length > 0 && (
                    <div className="provider-detail">
                      <div className="acc-table-wrap">
                        <table className="table">
                          <thead>
                            <tr>
                              <th>{t("acc.apiKeys.tableLabel")}</th>
                              <th>{t("acc.apiKeys.tableModelHint")}</th>
                              <th>{t("acc.apiKeys.tableKey")}</th>
                              <th>{t("acc.apiKeys.tableLastUsed")}</th>
                              <th />
                            </tr>
                          </thead>
                          <tbody>
                            {providerKeys.map((k) => (
                              <tr key={k.id}>
                                <td>{k.label ?? "—"}</td>
                                <td>{k.modelHint ?? "—"}</td>
                                <td className="mono">{k.maskedSecret}</td>
                                <td>{k.lastUsedAt ?? t("acc.apiKeys.never")}</td>
                                <td>
                                  <button
                                    className="btn btn-ghost btn-sm"
                                    type="button"
                                    onClick={() => handleDeleteKey(k.id)}
                                  >
                                    {t("acc.apiKeys.delete")}
                                  </button>
                                </td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </section>

          {/* ---------- adding keys ---------- */}
          <section>
            <div className="section-head">
              <h2>{t("acc.apiKeys.heading")}</h2>
            </div>

            <div className="card card-pad">
              <form onSubmit={handleAddSingle}>
                <h3>{t("acc.apiKeys.addOne")}</h3>
                <div className="acc-form-grid">
                  <select className="select" value={singleProvider} onChange={(e) => setSingleProvider(e.target.value)}>
                    {CLOUD_PROVIDERS.map((p) => (
                      <option key={p} value={p}>
                        {p}
                      </option>
                    ))}
                  </select>
                  <input
                    className="input"
                    type="password"
                    placeholder={t("acc.apiKeys.apiKeyPlaceholder")}
                    value={singleSecret}
                    onChange={(e) => setSingleSecret(e.target.value)}
                    required
                  />
                  <input
                    className="input"
                    type="text"
                    placeholder={t("acc.apiKeys.labelOptionalPlaceholder")}
                    value={singleLabel}
                    onChange={(e) => setSingleLabel(e.target.value)}
                  />
                  <input
                    className="input"
                    type="text"
                    placeholder={t("acc.apiKeys.modelHintOptionalPlaceholder")}
                    value={singleModelHint}
                    onChange={(e) => setSingleModelHint(e.target.value)}
                  />
                  <button className="btn btn-primary" type="submit">
                    {t("acc.apiKeys.add")}
                  </button>
                </div>
              </form>
            </div>

            <div className="card card-pad">
              <form onSubmit={handleBatchAdd}>
                <h3>{t("acc.apiKeys.addInBulk")}</h3>
                <p className="field-hint">
                  {t("acc.apiKeys.bulkHintBeforeCode")}
                  <code className="mono">provider,secret,label,modelHint</code>
                  {t("acc.apiKeys.bulkHintAfterCode")}
                </p>
                <textarea
                  className="textarea acc-batch"
                  rows={5}
                  placeholder={"openrouter,sk-or-v1-...,Ling-3.0-flash (free),inclusionai/ling-3.0-flash:free"}
                  value={batchText}
                  onChange={(e) => setBatchText(e.target.value)}
                />
                <div className="acc-actions">
                  <button className="btn btn-secondary btn-sm" type="submit">
                    {t("acc.apiKeys.importAll")}
                  </button>
                </div>
              </form>
            </div>

            <div className="card card-pad">
              <h3>{t("acc.apiKeys.importFromFiles")}</h3>
              <p className="field-hint">{t("acc.apiKeys.importFromFilesHint")}</p>
              <div className="acc-form-grid">
                <select
                  className="select"
                  value={fileImportProvider}
                  onChange={(e) => setFileImportProvider(e.target.value)}
                >
                  {CLOUD_PROVIDERS.map((p) => (
                    <option key={p} value={p}>
                      {p}
                    </option>
                  ))}
                </select>
                <button
                  className="btn btn-secondary"
                  type="button"
                  disabled={fileImportBusy}
                  onClick={() => handleImportFromFiles()}
                >
                  {fileImportBusy ? t("acc.apiKeys.importing") : t("acc.apiKeys.chooseFiles")}
                </button>
              </div>
            </div>
          </section>

          {/* ---------- cloud catalogue ---------- */}
          <section>
            <div className="section-head">
              <h2>{t("acc.cloudModels.heading")}</h2>
            </div>
            <div className="card card-pad">
              <div className="acc-form-grid">
                <select className="select" value={modelProvider} onChange={(e) => setModelProvider(e.target.value)}>
                  {CLOUD_PROVIDERS.map((p) => (
                    <option key={p} value={p}>
                      {p}
                    </option>
                  ))}
                </select>
                {modelProvider === "openrouter" && (
                  <>
                    <input
                      className="input"
                      type="search"
                      placeholder={t("acc.cloudModels.searchPlaceholder")}
                      value={openRouterQuery}
                      onChange={(e) => setOpenRouterQuery(e.target.value)}
                    />
                    <button
                      className="btn btn-secondary"
                      type="button"
                      disabled={openRouterLoading}
                      onClick={() => refreshOpenRouterModels(true).catch((e) => setError(String(e)))}
                    >
                      {openRouterLoading
                        ? t("acc.cloudModels.refreshing")
                        : t("acc.cloudModels.refreshFromOpenRouter")}
                    </button>
                  </>
                )}
              </div>

              {modelProvider === "openrouter" && !openRouterLive && (
                <div className="callout" data-kind="warning">
                  <Icon name="alert" />
                  <div className="callout-body">{t("acc.cloudModels.liveCatalogUnavailable")}</div>
                </div>
              )}

              <div className="acc-model-rows">
                {modelProvider === "openrouter"
                  ? openRouterModels
                      .filter((m) => {
                        const q = openRouterQuery.trim().toLowerCase();
                        return !q || m.id.toLowerCase().includes(q) || m.name.toLowerCase().includes(q);
                      })
                      .map((m) => (
                        <div className="row row-tall" key={m.id}>
                          <span className="name">
                            <span className="mono">{m.id}</span>
                            <span className="row-sub">
                              {m.name}
                              {(m.promptPricePerMillion !== null || m.completionPricePerMillion !== null) &&
                                " · " +
                                  t("acc.cloudModels.pricing", {
                                    promptPrice: m.promptPricePerMillion?.toFixed(2) ?? "?",
                                    completionPrice: m.completionPricePerMillion?.toFixed(2) ?? "?",
                                  })}
                            </span>
                          </span>
                        </div>
                      ))
                  : curatedModels.map((m) => (
                      <div className="row row-tall" key={m.id}>
                        <span className="name">
                          <span className="mono">{m.id}</span>
                          <span className="row-sub">{m.label}</span>
                        </span>
                      </div>
                    ))}
              </div>
            </div>
          </section>

          {/* ---------- local models ---------- */}
          <section>
            <div className="section-head">
              <h2>{t("acc.localModels.heading")}</h2>
            </div>

            {ollamaRunning === false && (
              <div className="card card-pad">
                <InstallGuidance command="winget install Ollama.Ollama" url="https://ollama.com/download" />
              </div>
            )}

            {ollamaRunning && (
              <div className="card card-pad">
                <h3>{t("acc.localModels.installedHeading")}</h3>
                <div className="acc-table-wrap">
                  <table className="table">
                    <thead>
                      <tr>
                        <th>{t("acc.localModels.tableModel")}</th>
                        <th>{t("acc.localModels.tableSize")}</th>
                        <th />
                      </tr>
                    </thead>
                    <tbody>
                      {ollamaInstalled.length === 0 && (
                        <tr>
                          <td colSpan={3}>{t("acc.localModels.noModelsInstalled")}</td>
                        </tr>
                      )}
                      {ollamaInstalled.map((m) => (
                        <tr key={m.name}>
                          <td className="mono">{m.name}</td>
                          <td className="mono">{formatBytes(m.size)}</td>
                          <td>
                            <button
                              className="btn btn-ghost btn-sm"
                              type="button"
                              onClick={() => handleDeleteOllamaModel(m.name)}
                            >
                              {t("acc.localModels.remove")}
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>

                <h3>{t("acc.localModels.availableHeading")}</h3>
                <div className="acc-model-rows">
                  {notYetInstalled.map((m) => (
                    <div className="row row-tall" key={m.id}>
                      <span className="name">
                        <span className="mono">{m.id}</span>
                        <span className="row-sub">{m.label}</span>
                      </span>
                      <button
                        className="btn btn-secondary btn-sm"
                        type="button"
                        disabled={pullingModel !== null}
                        onClick={() => handlePullModel(m.id)}
                      >
                        {pullingModel === m.id
                          ? pullProgress?.percent !== null && pullProgress?.percent !== undefined
                            ? `${pullProgress.percent.toFixed(0)}%`
                            : (pullProgress?.status ?? t("acc.localModels.installing"))
                          : t("acc.localModels.install")}
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            )}

            <details className="card card-pad disclosure">
              <summary>{t("acc.localModels.storageHintSummary")}</summary>
              <p className="field-hint">{t("acc.localModels.storageHintIntro", { endpoint: "localhost:11434" })}</p>
              {ollamaModelsEnvHint === undefined ? null : ollamaModelsEnvHint ? (
                <p className="field-hint">{t("acc.localModels.storageHintEnvSet", { path: ollamaModelsEnvHint })}</p>
              ) : (
                <p className="field-hint">{t("acc.localModels.storageHintEnvUnset")}</p>
              )}
              {suggestedOllamaModelsDir && <p className="field-hint">{t("acc.localModels.storageHintSuggested")}</p>}
              <ul className="field-hint acc-hint-list">
                <li>
                  {t("acc.localModels.storageHintWindows", {
                    command: `setx OLLAMA_MODELS "${suggestedOllamaModelsDir ?? "C:\\path\\to\\folder"}"`,
                  })}
                </li>
                <li>
                  {t("acc.localModels.storageHintUnix", {
                    command: `export OLLAMA_MODELS=${suggestedOllamaModelsDir ?? "/path/to/folder"}`,
                  })}
                </li>
              </ul>
              <p className="field-hint">{t("acc.localModels.storageHintRestart")}</p>
            </details>
          </section>

          {/* ---------- hardware fit ---------- */}
          <section>
            <div className="section-head">
              <h2>{t("acc.localModels.hardwareFitHeading")}</h2>
            </div>
            <div className="card card-pad">
              <p className="field-hint">{t("acc.localModels.hardwareFitHint")}</p>
              <div className="acc-actions">
                <button
                  className="btn btn-secondary btn-sm"
                  type="button"
                  disabled={loadingHardwareRecommendations}
                  onClick={() => void handleRecommendLocalModels()}
                >
                  {loadingHardwareRecommendations
                    ? t("acc.localModels.hardwareFitLoading")
                    : t("acc.localModels.hardwareFitButton")}
                </button>
              </div>
              {hardwareRecommendations?.map((m) => (
                <div className="verdict" data-fit={verdictFit(m.fitLevel)} key={m.name}>
                  <span className="model">
                    {m.name} ({m.parameterCount})
                  </span>
                  <span className="size">{m.bestQuantization}</span>
                  <span className="size">{m.estimatedTokensPerSecond.toFixed(1)} tok/s</span>
                  <span className="badge" data-kind={verdictFit(m.fitLevel) === "good" ? "succeeded" : "attention"}>
                    {m.fitLevel}
                  </span>
                </div>
              ))}
            </div>
          </section>

          {/* ---------- MCP ---------- */}
          <section>
            <div className="section-head">
              <h2>{t("acc.mcp.heading")}</h2>
            </div>
            <div className="card card-pad">
              <p className="field-hint">{t("acc.mcp.hint")}</p>
              <p className="field-hint">
                {t("acc.mcp.examplesHint")}{" "}
                <button className="btn btn-ghost btn-sm" type="button" onClick={onOpenManual}>
                  {t("acc.mcp.examplesHintLink")}
                </button>
              </p>
              <form onSubmit={handleAddMcpServer}>
                <div className="acc-form-grid">
                  <input
                    className="input"
                    type="text"
                    placeholder={t("acc.mcp.namePlaceholder")}
                    value={newMcpName}
                    onChange={(e) => setNewMcpName(e.target.value)}
                    required
                  />
                  <input
                    className="input"
                    type="text"
                    placeholder={t("acc.mcp.commandPlaceholder")}
                    value={newMcpCommand}
                    onChange={(e) => setNewMcpCommand(e.target.value)}
                    required
                  />
                  <input
                    className="input"
                    type="text"
                    placeholder={t("acc.mcp.argsPlaceholder")}
                    value={newMcpArgs}
                    onChange={(e) => setNewMcpArgs(e.target.value)}
                  />
                  <button className="btn btn-primary" type="submit" disabled={mcpBusy}>
                    {t("acc.mcp.add")}
                  </button>
                </div>
              </form>
              <div className="acc-table-wrap">
                <table className="table">
                  <thead>
                    <tr>
                      <th>{t("acc.mcp.tableName")}</th>
                      <th>{t("acc.mcp.tableCommand")}</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {mcpServers.length === 0 && (
                      <tr>
                        <td colSpan={3}>{t("acc.mcp.noneYet")}</td>
                      </tr>
                    )}
                    {mcpServers.map((s) => (
                      <tr key={s.id}>
                        <td>{s.name}</td>
                        <td className="mono">
                          {s.command} {s.args.join(" ")}
                        </td>
                        <td>
                          <button
                            className="btn btn-ghost btn-sm"
                            type="button"
                            onClick={() => handleDeleteMcpServer(s.id)}
                          >
                            {t("acc.mcp.delete")}
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          </section>

          <p className="pane-intro">
            {t("acc.usage.movedHint")}{" "}
            <button className="btn btn-ghost btn-sm" type="button" onClick={onOpenUsage}>
              {t("acc.usage.openLink")}
            </button>
          </p>
        </div>
      </div>
    </>
  );
}
