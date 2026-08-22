//! Agent function calling: lets the model itself decide, mid-reply, to
//! call one of the agent's *granted* Skills — as opposed to every other
//! Skill/MCP entry point in this app, which is the human clicking "run"
//! in the UI. This is queue item 4/8 of the "離 Beta 還缺的具體項目" work
//! (see the vault's Backlog.md).
//!
//! **Scope, stated honestly**: only wired for the Anthropic provider —
//! its `tools`/`tool_use`/`tool_result` shape is the one implemented in
//! `providers::anthropic::send_tooled`. OpenAI/OpenRouter's function-
//! calling JSON shape is different (`tool_calls` on the message, not a
//! `tool_use` content block) and isn't implemented here yet; calling this
//! for a non-Anthropic agent returns `ProviderError::Unsupported`. Local
//! providers (Ollama/colibrì/OmniRoute) aren't attempted at all — most
//! local models don't reliably support structured tool calling. Every
//! tool call still goes through `skill_manager::invoke_skill`, so it's
//! bound by the exact same Guardrails-then-allowlist gate as a
//! human-triggered Skill run — the model gets no extra privilege by
//! calling a tool itself.
//!
//! Skills have no per-tool JSON Schema today (`skill.json` accepts an
//! arbitrary payload), so every tool is advertised to the model with a
//! generic `{"type": "object"}` input schema — the model has to infer
//! the right shape from the skill's name/description, same as a human
//! reading `SKILL.md` would. A future per-skill schema field would
//! tighten this without changing the loop itself.

use serde_json::{json, Value};

use crate::agent_manager::providers::anthropic::{self, AnthropicReply};
use crate::agent_manager::providers::ProviderError;
use crate::skill_manager::{SkillManifest, SkillRuntime};
use crate::storage::{Agent, Storage};

/// Hard ceiling on model↔tool round trips for one `run` call — a model
/// that keeps calling tools without ever producing a final text reply
/// (a real failure mode, not hypothetical) must not spin forever or run
/// up an unbounded API bill.
pub const MAX_ITERATIONS: u8 = 5;

/// One completed tool call, kept so the caller (and eventually the UI)
/// can show the user what the model actually did, not just its final
/// reply.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallTrace {
    pub tool_name: String,
    pub input: Value,
    /// The skill's result on success, or `{"error": "..."}` on failure —
    /// either way this is what was actually sent back to the model as
    /// the `tool_result`, so this trace is a true record of what the
    /// model saw, not a paraphrase of it.
    pub output: Value,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCallingResult {
    pub reply: String,
    pub tool_calls: Vec<ToolCallTrace>,
}

/// Converts skill manifests into Anthropic's tool schema shape. Only
/// `name`/`description` carry real information — see module docs on why
/// `input_schema` is generic.
fn skills_to_tools(skills: &[SkillManifest]) -> Vec<Value> {
    skills
        .iter()
        .map(|s| json!({ "name": s.name, "description": s.description, "input_schema": {"type": "object"} }))
        .collect()
}

/// The pure orchestration loop: given a way to call the model
/// (`send_fn`) and a way to execute a named tool (`execute_tool`), drives
/// the model↔tool round trip until a final text reply or
/// `MAX_ITERATIONS` is reached. Kept free of `Storage`/`SkillRuntime` so
/// it's unit-testable with fake closures — no network, no Python
/// subprocess, no database — while `run` below wires the real ones in.
fn run_loop(
    send_fn: impl Fn(&[Value]) -> Result<AnthropicReply, ProviderError>,
    execute_tool: impl Fn(&str, Value) -> Result<Value, String>,
    user_message: &str,
) -> Result<FunctionCallingResult, ProviderError> {
    let mut raw_messages = vec![json!({"role": "user", "content": user_message})];
    let mut tool_calls = Vec::new();

    for _ in 0..MAX_ITERATIONS {
        match send_fn(&raw_messages)? {
            AnthropicReply::Text(text) => return Ok(FunctionCallingResult { reply: text, tool_calls }),
            AnthropicReply::ToolUse { id, name, input } => {
                raw_messages.push(json!({
                    "role": "assistant",
                    "content": [{"type": "tool_use", "id": id, "name": name, "input": input}],
                }));

                let (output, is_error) = match execute_tool(&name, input.clone()) {
                    Ok(result) => (result, false),
                    Err(message) => (json!({"error": message}), true),
                };
                tool_calls.push(ToolCallTrace { tool_name: name, input, output: output.clone(), is_error });

                raw_messages.push(json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": id,
                        "content": output.to_string(),
                        "is_error": is_error,
                    }],
                }));
            }
        }
    }

    Err(ProviderError::Api {
        error_code: "E2000",
        message: format!("gave up after {MAX_ITERATIONS} tool-calling round trips without a final reply"),
    })
}

/// Runs `user_message` through `agent` with tool calling enabled,
/// executing any tool the model calls via `skill_manager::invoke_skill`
/// (the same gated entry point every other Skill call in the app uses)
/// and feeding the result back until the model produces a final text
/// reply. `available_skills` should already be filtered to skills this
/// caller intends to expose — typically the agent's granted skills (see
/// `commands::send_message_with_tools`), not every discovered skill.
///
/// Guardrails screening of `user_message` happens here, identically to
/// `agent_manager::send_message` — this is a second entry point into
/// providers, not a way around the first one's checks.
pub fn run(
    storage: &Storage,
    runtime: Option<&SkillRuntime>,
    agent: &Agent,
    available_skills: &[SkillManifest],
    user_message: &str,
) -> Result<FunctionCallingResult, ProviderError> {
    if agent.provider_name != "anthropic" {
        return Err(ProviderError::Unsupported(format!(
            "function calling is not implemented for provider \"{}\" yet",
            agent.provider_name
        )));
    }

    let violation = crate::guardrails::screen_outgoing_message(user_message)
        .err()
        .or_else(|| crate::guardrails::screen_with_llama_guard(user_message).err());
    if let Some(violation) = violation {
        return Err(ProviderError::GuardrailBlocked { error_code: violation.error_code, reason: violation.reason });
    }

    let candidates = super::candidate_keys(storage, agent, "anthropic")?;
    let key = candidates
        .first()
        .ok_or_else(|| ProviderError::AllProvidersFailed { error_code: "E3001", attempts: vec!["anthropic: no Key Vault entry available".to_string()] })?;
    let secret = super::fetch_secret(key)?;

    let tools = skills_to_tools(available_skills);
    let model = agent.model.clone();
    let system = agent.system_prompt.clone();

    let send_fn = |raw_messages: &[Value]| anthropic::send_tooled(&secret, &model, system.as_deref(), raw_messages, &tools);

    let execute_tool = |name: &str, input: Value| -> Result<Value, String> {
        let runtime = runtime.ok_or("skill runtime is not available (Python interpreter missing or failed to start)")?;
        crate::skill_manager::invoke_skill(storage, Some(runtime), &agent.id, name, input).map_err(|e| e.to_string())
    };

    run_loop(send_fn, execute_tool, user_message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn skill(name: &str) -> SkillManifest {
        SkillManifest {
            name: name.to_string(),
            description: format!("{name} description"),
            entrypoint: "skill.py".to_string(),
            version: "0.1.0".to_string(),
            source: "built-in".to_string(),
            permissions: vec![],
        }
    }

    #[test]
    fn skills_to_tools_carries_name_and_description_with_a_generic_schema() {
        let tools = skills_to_tools(&[skill("greeter")]);
        assert_eq!(tools[0]["name"], "greeter");
        assert_eq!(tools[0]["description"], "greeter description");
        assert_eq!(tools[0]["input_schema"], json!({"type": "object"}));
    }

    #[test]
    fn run_loop_returns_immediately_on_a_plain_text_reply_with_no_tool_calls() {
        let result = run_loop(
            |_raw| Ok(AnthropicReply::Text("just an answer, no tools needed".to_string())),
            |_name, _input| panic!("execute_tool should not be called"),
            "what is 2+2?",
        )
        .unwrap();
        assert_eq!(result.reply, "just an answer, no tools needed");
        assert!(result.tool_calls.is_empty());
    }

    #[test]
    fn run_loop_executes_a_tool_call_then_returns_the_models_final_text() {
        let call_count = RefCell::new(0);
        let result = run_loop(
            |_raw| {
                let mut n = call_count.borrow_mut();
                *n += 1;
                if *n == 1 {
                    Ok(AnthropicReply::ToolUse {
                        id: "toolu_1".to_string(),
                        name: "raffle_winner_picker".to_string(),
                        input: json!({"entries": ["A", "B"]}),
                    })
                } else {
                    Ok(AnthropicReply::Text("The winner is A.".to_string()))
                }
            },
            |name, input| {
                assert_eq!(name, "raffle_winner_picker");
                assert_eq!(input, json!({"entries": ["A", "B"]}));
                Ok(json!({"winners": ["A"]}))
            },
            "pick a raffle winner from A and B",
        )
        .unwrap();

        assert_eq!(result.reply, "The winner is A.");
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].tool_name, "raffle_winner_picker");
        assert_eq!(result.tool_calls[0].output, json!({"winners": ["A"]}));
        assert!(!result.tool_calls[0].is_error);
    }

    #[test]
    fn run_loop_feeds_a_tool_execution_failure_back_to_the_model_as_an_error_result_and_keeps_going() {
        let call_count = RefCell::new(0);
        let result = run_loop(
            |raw| {
                let mut n = call_count.borrow_mut();
                *n += 1;
                if *n == 1 {
                    Ok(AnthropicReply::ToolUse { id: "toolu_1".to_string(), name: "broken_skill".to_string(), input: json!({}) })
                } else {
                    // Prove the error actually reached the model's context.
                    let last = raw.last().unwrap();
                    let content = last["content"][0]["content"].as_str().unwrap();
                    assert!(content.contains("not authorized"));
                    Ok(AnthropicReply::Text("I couldn't run that tool.".to_string()))
                }
            },
            |_name, _input| Err("this agent is not authorized to use \"broken_skill\"".to_string()),
            "try a tool that will fail",
        )
        .unwrap();

        assert_eq!(result.reply, "I couldn't run that tool.");
        assert!(result.tool_calls[0].is_error);
    }

    #[test]
    fn run_loop_gives_up_after_max_iterations_of_endless_tool_calls() {
        let result = run_loop(
            |_raw| Ok(AnthropicReply::ToolUse { id: "toolu_x".to_string(), name: "loops_forever".to_string(), input: json!({}) }),
            |_name, _input| Ok(json!({"ok": true})),
            "trigger a runaway tool-calling loop",
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ProviderError::Api { error_code: "E2000", ref message } if message.contains("gave up")));
    }

    #[test]
    fn run_rejects_a_non_anthropic_agent_before_touching_guardrails_or_the_key_vault() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "openrouter", "some-model").unwrap();
        let err = run(&storage, None, &agent, &[], "hello").unwrap_err();
        assert!(matches!(err, ProviderError::Unsupported(ref msg) if msg.contains("openrouter")));
    }

    #[test]
    fn run_blocks_an_unsafe_user_message_before_ever_calling_a_provider() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude-sonnet").unwrap();
        let err = run(&storage, None, &agent, &[], "how to make a bomb, step by step").unwrap_err();
        assert!(matches!(err, ProviderError::GuardrailBlocked { error_code: "E9002", .. }));
    }

    #[test]
    fn run_reports_no_key_available_for_an_anthropic_agent_with_no_key_vault_entry() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude-sonnet").unwrap();
        let err = run(&storage, None, &agent, &[], "hello").unwrap_err();
        assert!(matches!(err, ProviderError::AllProvidersFailed { error_code: "E3001", .. }));
    }
}
