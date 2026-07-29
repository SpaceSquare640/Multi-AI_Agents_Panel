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

export interface Agent {
  id: string;
  name: string;
  roleTemplate: string | null;
  systemPrompt: string | null;
  /** "local" | "cloud" */
  providerKind: string;
  providerName: string;
  model: string;
  createdAt: string;
}

export interface Session {
  id: string;
  /** "independent" | "group" */
  kind: string;
  title: string;
  createdAt: string;
}

export interface Message {
  id: string;
  sessionId: string;
  agentId: string | null;
  /** "user" | "assistant" | "system" */
  role: string;
  content: string;
  createdAt: string;
}

export interface FileAccessGrant {
  id: string;
  agentId: string;
  folderPath: string;
  grantedAt: string;
}

export interface SkillManifest {
  name: string;
  description: string;
  entrypoint: string;
  version: string;
}

export interface SkillAccessGrant {
  id: string;
  agentId: string;
  skillName: string;
  grantedAt: string;
}

export interface RoleTemplate {
  id: string;
  name: string;
  description: string;
  systemPrompt: string;
  suggestedProviderKind: string | null;
  suggestedProviderName: string | null;
  suggestedModel: string | null;
  /** "default" | "custom" */
  source: string;
}
