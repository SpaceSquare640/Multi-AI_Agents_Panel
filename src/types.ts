// Mirrors the Rust structs in src-tauri/src/storage/mod.rs,
// src-tauri/src/commands.rs, src-tauri/src/agent_manager/curated_models.rs,
// and src-tauri/src/agent_manager/providers/ollama.rs. Field names are
// camelCase on both sides (Rust structs use #[serde(rename_all = "camelCase")]).

export interface ProviderKeyView {
  id: string;
  provider: string;
  label: string | null;
  modelHint: string | null;
  createdAt: string;
  lastUsedAt: string | null;
  maskedSecret: string;
}

export interface UsageSummary {
  providerKeyId: string;
  provider: string;
  label: string | null;
  successCount: number;
  failureCount: number;
  lastUsedAt: string | null;
}

export interface CuratedModel {
  id: string;
  label: string;
}

export interface OllamaModel {
  name: string;
  size: number | null;
  modifiedAt: string | null;
}

export const CLOUD_PROVIDERS = ["anthropic", "openai", "openrouter"] as const;
export type CloudProvider = (typeof CLOUD_PROVIDERS)[number];
