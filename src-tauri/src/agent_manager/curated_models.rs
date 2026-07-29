//! Curated model lists shown in the AI Control Center's model pickers.
//! These mirror the defaults documented in the Obsidian vault:
//! `Ollama Default Model List.md` and `OpenRouter Default Model List.md`.
//! They're just a starting point — the user can always type/pick a model
//! outside this list (see those docs' "已定案" sections).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CuratedModel {
    /// The id/tag to actually send to the provider.
    pub id: String,
    /// Human-readable label for the UI.
    pub label: String,
}

pub fn anthropic_models() -> Vec<CuratedModel> {
    [
        "claude-opus-4-5",
        "claude-sonnet-4-5",
        "claude-haiku-4-5",
    ]
    .into_iter()
    .map(|id| CuratedModel {
        id: id.to_string(),
        label: id.to_string(),
    })
    .collect()
}

pub fn openrouter_models() -> Vec<CuratedModel> {
    [
        ("openai/gpt-5.6-sol", "OpenAI: GPT-5.6 Sol"),
        ("openai/gpt-5.5", "OpenAI: GPT-5.5"),
        ("openai/gpt-5.6-luna", "OpenAI: GPT-5.6 Luna"),
        ("openai/gpt-5.4", "OpenAI: GPT-5.4"),
        ("openai/gpt-5.6-terra", "OpenAI: GPT-5.6 Terra"),
        ("openai/gpt-4.1-mini", "OpenAI: GPT-4.1 Mini"),
        ("openai/gpt-4.1", "OpenAI: GPT-4.1"),
        ("openai/gpt-4.1-nano", "OpenAI: GPT-4.1 Nano"),
        ("anthropic/claude-opus-4.8", "Anthropic: Claude Opus 4.8"),
        ("anthropic/claude-opus-4.7", "Anthropic: Claude Opus 4.7"),
        ("anthropic/claude-sonnet-5", "Anthropic: Claude Sonnet 5"),
        ("anthropic/claude-sonnet-4.6", "Anthropic: Claude Sonnet 4.6"),
        ("anthropic/claude-opus-4.6", "Anthropic: Claude Opus 4.6"),
        ("anthropic/claude-sonnet-4.5", "Anthropic: Claude Sonnet 4.5"),
        ("google/gemini-3-flash-preview", "Google: Gemini 3 Flash Preview"),
        ("google/gemini-2.5-flash-lite", "Google: Gemini 2.5 Flash Lite"),
        ("google/gemini-2.5-flash", "Google: Gemini 2.5 Flash"),
        ("google/gemini-3.1-flash-lite", "Google: Gemini 3.1 Flash Lite"),
        ("google/gemini-3.5-flash", "Google: Gemini 3.5 Flash"),
        ("google/gemini-3.1-pro-preview", "Google: Gemini 3.1 Pro Preview"),
        ("google/gemini-3.6-flash", "Google: Gemini 3.6 Flash"),
        ("google/gemini-2.5-pro", "Google: Gemini 2.5 Pro"),
        ("google/gemini-3.5-flash-lite", "Google: Gemini 3.5 Flash Lite"),
        ("x-ai/grok-4.20", "xAI: Grok 4.20"),
        ("x-ai/grok-4.3", "xAI: Grok 4.3"),
        ("xiaomi/mimo-v2.5", "Xiaomi: MiMo V2.5"),
        ("xiaomi/mimo-v2.5-pro", "Xiaomi: MiMo V2.5 Pro"),
        ("deepseek/deepseek-v4-flash", "DeepSeek: DeepSeek V4 Flash"),
        ("deepseek/deepseek-v4-pro", "DeepSeek: DeepSeek V4 Pro"),
        ("z-ai/glm-5.2", "Z.ai: GLM 5.2"),
        ("minimax/minimax-m3", "MiniMax: MiniMax M3"),
        ("moonshotai/kimi-k3", "Moonshot AI: Kimi K3"),
        ("meta-llama/llama-4-maverick", "Meta: Llama 4 Maverick"),
        ("meta-llama/llama-4-scout", "Meta: Llama 4 Scout"),
    ]
    .into_iter()
    .map(|(id, label)| CuratedModel {
        id: id.to_string(),
        label: label.to_string(),
    })
    .collect()
}

pub fn ollama_models() -> Vec<CuratedModel> {
    [
        ("gpt-oss:20b", "GPT-OSS 20B"),
        ("llama3.1:8b", "Llama 3.1 8B"),
        ("gpt-oss:120b", "GPT-OSS 120B"),
        ("qwen3:32b", "Qwen 3 32B"),
        ("gemma3:27b", "Gemma 3 27B"),
        ("qwen3.5:9b", "Qwen 3.5 9B"),
        ("llama3.3:70b", "Llama 3.3 70B"),
        ("deepseek-v3.2", "DeepSeek V3.2"),
        ("llama4:scout", "Llama 4 Scout 17B"),
        ("mistral-small", "Mistral Small 3.1 24B"),
        ("kimi-k2", "Kimi K2"),
        ("qwen2.5-coder:1.5b", "Qwen 2.5 Coder 1.5B"),
    ]
    .into_iter()
    .map(|(id, label)| CuratedModel {
        id: id.to_string(),
        label: label.to_string(),
    })
    .collect()
}
