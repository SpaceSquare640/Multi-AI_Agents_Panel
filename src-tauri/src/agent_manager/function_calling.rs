//! Agent function calling: lets the model itself decide, mid-reply, to
//! call one of the agent's *granted* Skills — as opposed to every other
//! Skill/MCP entry point in this app, which is the human clicking "run"
//! in the UI.
//!
//! **Scope, stated honestly**: wired for the three cloud providers whose
//! tool-calling protocol is implemented — Anthropic
//! (`providers::anthropic`), and OpenAI and OpenRouter, which share one
//! OpenAI-compatible implementation (`providers::openai_tools`). The two
//! protocols disagree about nearly every detail (see that module's docs
//! for the table), so the orchestration loop below talks to a
//! `ToolDialect` rather than to either shape directly. Local providers
//! (Ollama/colibrì/OmniRoute) aren't attempted at all — most local models
//! don't reliably support structured tool calling, and advertising tools
//! to a model that will quietly ignore them produces a plausible answer
//! that never ran the skill, which is worse than an honest refusal.
//! Calling this for any other provider returns
//! `ProviderError::Unsupported`. Every tool call still goes through
//! `skill_manager::invoke_skill`, so it's bound by the exact same
//! Guardrails-then-allowlist gate as a human-triggered Skill run — the
//! model gets no extra privilege by calling a tool itself.
//!
//! Skills have no per-tool JSON Schema today (`skill.json` accepts an
//! arbitrary payload), so every tool is advertised to the model with a
//! generic `{"type": "object"}` input schema — the model has to infer
//! the right shape from the skill's name/description, same as a human
//! reading `SKILL.md` would. A future per-skill schema field would
//! tighten this without changing the loop itself.

use serde_json::{json, Value};

use crate::agent_manager::providers::{anthropic, openai_tools, ProviderError, ToolTurn};
use crate::cancel::CancelToken;
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

/// The three JSON shapes a tool-calling protocol has to define, and the
/// only places the two supported protocols actually differ: how a tool is
/// advertised, how the model's call is echoed back into the conversation,
/// and how the result is returned. Everything else — when to call, when
/// to stop, what to do with a failure — is protocol-independent and lives
/// once in `run_loop`.
///
/// A trait rather than a match inside the loop so that each protocol's
/// shapes stay next to that protocol's parser, in its own provider
/// module, instead of accumulating here.
trait ToolDialect {
    fn tool_spec(&self, name: &str, description: &str) -> Value;
    fn assistant_tool_call(&self, id: &str, name: &str, input: &Value) -> Value;
    fn tool_result(&self, id: &str, output: &Value, is_error: bool) -> Value;
}

struct AnthropicDialect;
impl ToolDialect for AnthropicDialect {
    fn tool_spec(&self, name: &str, description: &str) -> Value {
        anthropic::tool_spec(name, description)
    }
    fn assistant_tool_call(&self, id: &str, name: &str, input: &Value) -> Value {
        anthropic::assistant_tool_call(id, name, input)
    }
    fn tool_result(&self, id: &str, output: &Value, is_error: bool) -> Value {
        anthropic::tool_result(id, output, is_error)
    }
}

struct OpenAiDialect;
impl ToolDialect for OpenAiDialect {
    fn tool_spec(&self, name: &str, description: &str) -> Value {
        openai_tools::tool_spec(name, description)
    }
    fn assistant_tool_call(&self, id: &str, name: &str, input: &Value) -> Value {
        openai_tools::assistant_tool_call(id, name, input)
    }
    fn tool_result(&self, id: &str, output: &Value, is_error: bool) -> Value {
        openai_tools::tool_result(id, output, is_error)
    }
}

/// Which wire protocol a provider speaks, and where to send it. The
/// endpoint is carried here rather than inferred later so that adding a
/// fourth OpenAI-compatible gateway is one match arm and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wire {
    Anthropic,
    /// Carries the chat-completions endpoint — OpenAI and OpenRouter
    /// differ in URL and error dialect, in nothing else.
    OpenAiCompatible(&'static str),
}

impl Wire {
    /// `None` for every provider whose tool-calling protocol isn't
    /// implemented, including the local ones — see the module docs.
    fn for_provider(provider_name: &str) -> Option<Wire> {
        match provider_name {
            "anthropic" => Some(Wire::Anthropic),
            "openai" => Some(Wire::OpenAiCompatible(openai_tools::OPENAI_URL)),
            "openrouter" => Some(Wire::OpenAiCompatible(openai_tools::OPENROUTER_URL)),
            _ => None,
        }
    }

    fn dialect(self) -> &'static dyn ToolDialect {
        match self {
            Wire::Anthropic => &AnthropicDialect,
            Wire::OpenAiCompatible(_) => &OpenAiDialect,
        }
    }
}

/// Converts skill manifests into the given protocol's tool schema shape.
/// Only `name`/`description` carry real information — see module docs on
/// why the input schema is generic.
fn skills_to_tools(dialect: &dyn ToolDialect, skills: &[SkillManifest]) -> Vec<Value> {
    skills.iter().map(|s| dialect.tool_spec(&s.name, &s.description)).collect()
}

/// The pure orchestration loop: given a protocol (`dialect`), a way to
/// call the model (`send_fn`) and a way to execute a named tool
/// (`execute_tool`), drives the model↔tool round trip until a final text
/// reply or `MAX_ITERATIONS` is reached. Kept free of
/// `Storage`/`SkillRuntime` so it's unit-testable with fake closures — no
/// network, no Python subprocess, no database — while `run` below wires
/// the real ones in.
fn run_loop(
    dialect: &dyn ToolDialect,
    send_fn: impl Fn(&[Value]) -> Result<ToolTurn, ProviderError>,
    execute_tool: impl Fn(&str, Value) -> Result<Value, String>,
    user_message: &str,
    cancel: &CancelToken,
) -> Result<FunctionCallingResult, ProviderError> {
    let mut raw_messages = vec![json!({"role": "user", "content": user_message})];
    let mut tool_calls = Vec::new();

    for _ in 0..MAX_ITERATIONS {
        // The round-trip boundary is where a Stop is worth the most: a
        // tool conversation can take `MAX_ITERATIONS` provider calls plus
        // a skill execution between each, and the user watching it has no
        // other way out. Tool calls already completed are discarded along
        // with the reply — nothing from a cancelled conversation is
        // persisted, so a half-finished chain of calls never reaches the
        // transcript as though it had been a real turn.
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        match send_fn(&raw_messages)? {
            ToolTurn::Text(text) => return Ok(FunctionCallingResult { reply: text, tool_calls }),
            ToolTurn::ToolUse { id, name, input } => {
                raw_messages.push(dialect.assistant_tool_call(&id, &name, &input));

                let (output, is_error) = match execute_tool(&name, input.clone()) {
                    Ok(result) => (result, false),
                    Err(message) => (json!({"error": message}), true),
                };
                tool_calls.push(ToolCallTrace { tool_name: name, input, output: output.clone(), is_error });

                raw_messages.push(dialect.tool_result(&id, &output, is_error));
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
/// `runtime` is the *unlocked* Skills-runtime mutex, not a pre-acquired
/// guard — deliberately: this whole call can span several real
/// Anthropic API round trips (`MAX_ITERATIONS` of them), and if the
/// caller locked the mutex once up front for the whole call, every
/// *other* command that needs this same process-wide lock (any other
/// Skill invocation, importing a new Skill) would block for the entire
/// multi-round conversation's network latency, not just the brief local
/// tool executions inside it. Locking fresh inside `execute_tool`, once
/// per actual tool call, keeps the lock held only as long as the fast
/// local skill execution itself takes.
///
/// Guardrails screening of `user_message` happens here, identically to
/// `agent_manager::send_message` — this is a second entry point into
/// providers, not a way around the first one's checks.
pub fn run(
    storage: &Storage,
    runtime: &std::sync::Mutex<Option<SkillRuntime>>,
    agent: &Agent,
    available_skills: &[SkillManifest],
    user_message: &str,
    cancel: &CancelToken,
) -> Result<FunctionCallingResult, ProviderError> {
    let wire = Wire::for_provider(&agent.provider_name).ok_or_else(|| {
        ProviderError::Unsupported(format!(
            "function calling is not implemented for provider \"{}\" yet",
            agent.provider_name
        ))
    })?;

    let violation = crate::guardrails::screen_outgoing_message(user_message)
        .err()
        .or_else(|| crate::guardrails::screen_with_llama_guard(user_message).err());
    if let Some(violation) = violation {
        return Err(ProviderError::GuardrailBlocked { error_code: violation.error_code, reason: violation.reason });
    }

    let provider = agent.provider_name.as_str();
    let candidates = super::candidate_keys(storage, agent, provider)?;
    let key = candidates.first().ok_or_else(|| ProviderError::AllProvidersFailed {
        error_code: "E3001",
        attempts: vec![format!("{provider}: no Key Vault entry available")],
    })?;
    let secret = super::fetch_secret(key)?;

    let dialect = wire.dialect();
    let tools = skills_to_tools(dialect, available_skills);
    let model = agent.model.clone();
    let system = super::custom_instructions::combine_with_agent_prompt(storage, agent.system_prompt.as_deref());

    // One closure covering both protocols rather than two code paths: the
    // only thing that varies per request is which adapter shapes and
    // parses the JSON, and both return the same provider-neutral turn.
    let send_fn = |raw_messages: &[Value]| match wire {
        Wire::Anthropic => anthropic::send_tooled(&secret, &model, system.as_deref(), raw_messages, &tools),
        Wire::OpenAiCompatible(url) => {
            openai_tools::send_tooled(url, &secret, &model, system.as_deref(), raw_messages, &tools)
        }
    };

    let execute_tool = |name: &str, input: Value| -> Result<Value, String> {
        // Locked fresh per tool call, released as soon as this one call
        // returns — see this function's doc comment for why holding it
        // for the whole conversation would be a real problem.
        let guard = runtime.lock().unwrap();
        crate::skill_manager::invoke_skill(storage, guard.as_ref(), &agent.id, name, input).map_err(|e| e.to_string())
    };

    run_loop(dialect, send_fn, execute_tool, user_message, cancel)
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
        let tools = skills_to_tools(&AnthropicDialect, &[skill("greeter")]);
        assert_eq!(tools[0]["name"], "greeter");
        assert_eq!(tools[0]["description"], "greeter description");
        assert_eq!(tools[0]["input_schema"], json!({"type": "object"}));
    }

    #[test]
    fn skills_to_tools_uses_the_function_wrapper_shape_for_openai_compatible_providers() {
        let tools = skills_to_tools(&OpenAiDialect, &[skill("greeter")]);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "greeter");
        assert_eq!(tools[0]["function"]["description"], "greeter description");
    }

    #[test]
    fn wire_maps_only_the_providers_whose_tool_protocol_is_implemented() {
        assert_eq!(Wire::for_provider("anthropic"), Some(Wire::Anthropic));
        assert_eq!(Wire::for_provider("openai"), Some(Wire::OpenAiCompatible(openai_tools::OPENAI_URL)));
        assert_eq!(Wire::for_provider("openrouter"), Some(Wire::OpenAiCompatible(openai_tools::OPENROUTER_URL)));
        // Local providers stay out — see the module docs for why.
        assert_eq!(Wire::for_provider("ollama"), None);
        assert_eq!(Wire::for_provider("colibri"), None);
        assert_eq!(Wire::for_provider("omniroute"), None);
    }

    #[test]
    fn run_loop_returns_immediately_on_a_plain_text_reply_with_no_tool_calls() {
        let result = run_loop(
            &AnthropicDialect,
            |_raw| Ok(ToolTurn::Text("just an answer, no tools needed".to_string())),
            |_name, _input| panic!("execute_tool should not be called"),
            "what is 2+2?",
            &CancelToken::never(),
        )
        .unwrap();
        assert_eq!(result.reply, "just an answer, no tools needed");
        assert!(result.tool_calls.is_empty());
    }

    #[test]
    fn run_loop_executes_a_tool_call_then_returns_the_models_final_text() {
        let call_count = RefCell::new(0);
        let result = run_loop(
            &AnthropicDialect,
            |_raw| {
                let mut n = call_count.borrow_mut();
                *n += 1;
                if *n == 1 {
                    Ok(ToolTurn::ToolUse {
                        id: "toolu_1".to_string(),
                        name: "raffle_winner_picker".to_string(),
                        input: json!({"entries": ["A", "B"]}),
                    })
                } else {
                    Ok(ToolTurn::Text("The winner is A.".to_string()))
                }
            },
            |name, input| {
                assert_eq!(name, "raffle_winner_picker");
                assert_eq!(input, json!({"entries": ["A", "B"]}));
                Ok(json!({"winners": ["A"]}))
            },
            "pick a raffle winner from A and B",
            &CancelToken::never(),
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
            &AnthropicDialect,
            |raw| {
                let mut n = call_count.borrow_mut();
                *n += 1;
                if *n == 1 {
                    Ok(ToolTurn::ToolUse { id: "toolu_1".to_string(), name: "broken_skill".to_string(), input: json!({}) })
                } else {
                    // Prove the error actually reached the model's context.
                    let last = raw.last().unwrap();
                    let content = last["content"][0]["content"].as_str().unwrap();
                    assert!(content.contains("not authorized"));
                    Ok(ToolTurn::Text("I couldn't run that tool.".to_string()))
                }
            },
            |_name, _input| Err("this agent is not authorized to use \"broken_skill\"".to_string()),
            "try a tool that will fail",
            &CancelToken::never(),
        )
        .unwrap();

        assert_eq!(result.reply, "I couldn't run that tool.");
        assert!(result.tool_calls[0].is_error);
    }

    #[test]
    fn run_loop_gives_up_after_max_iterations_of_endless_tool_calls() {
        let result = run_loop(
            &AnthropicDialect,
            |_raw| Ok(ToolTurn::ToolUse { id: "toolu_x".to_string(), name: "loops_forever".to_string(), input: json!({}) }),
            |_name, _input| Ok(json!({"ok": true})),
            "trigger a runaway tool-calling loop",
            &CancelToken::never(),
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ProviderError::Api { error_code: "E2000", ref message } if message.contains("gave up")));
    }

    #[test]
    fn run_rejects_a_provider_with_no_tool_protocol_before_touching_guardrails_or_the_key_vault() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "local", "ollama", "some-model").unwrap();
        let runtime = std::sync::Mutex::new(None);
        let err = run(&storage, &runtime, &agent, &[], "hello", &CancelToken::never()).unwrap_err();
        assert!(matches!(err, ProviderError::Unsupported(ref msg) if msg.contains("ollama")));
    }

    #[test]
    fn run_reports_no_key_available_for_an_openrouter_agent_rather_than_refusing_the_provider() {
        // The regression this guards: OpenRouter used to be rejected as
        // unsupported. Reaching the Key Vault check instead is the proof
        // that it is now a first-class tool-calling provider.
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "openrouter", "some-model").unwrap();
        let runtime = std::sync::Mutex::new(None);
        let err = run(&storage, &runtime, &agent, &[], "hello", &CancelToken::never()).unwrap_err();
        assert!(matches!(err, ProviderError::AllProvidersFailed { error_code: "E3001", ref attempts }
                         if attempts[0].contains("openrouter")));
    }

    #[test]
    fn run_loop_shapes_an_openai_compatible_round_trip_the_way_that_protocol_expects() {
        // Same conversation as the Anthropic round-trip test above, so the
        // only thing under test is the protocol shaping: the assistant turn
        // must carry `tool_calls` with *string* arguments, and the result
        // must come back as a `role: "tool"` message.
        let call_count = RefCell::new(0);
        let result = run_loop(
            &OpenAiDialect,
            |raw| {
                let mut n = call_count.borrow_mut();
                *n += 1;
                if *n == 1 {
                    Ok(ToolTurn::ToolUse {
                        id: "call_1".to_string(),
                        name: "raffle_winner_picker".to_string(),
                        input: json!({"entries": ["A", "B"]}),
                    })
                } else {
                    let assistant = &raw[1];
                    assert_eq!(assistant["role"], "assistant");
                    let arguments = assistant["tool_calls"][0]["function"]["arguments"].as_str().unwrap();
                    assert_eq!(serde_json::from_str::<Value>(arguments).unwrap(), json!({"entries": ["A", "B"]}));

                    let tool_turn = &raw[2];
                    assert_eq!(tool_turn["role"], "tool");
                    assert_eq!(tool_turn["tool_call_id"], "call_1");
                    assert!(tool_turn["content"].as_str().unwrap().contains("winners"));
                    Ok(ToolTurn::Text("The winner is A.".to_string()))
                }
            },
            |_name, _input| Ok(json!({"winners": ["A"]})),
            "pick a raffle winner from A and B",
            &CancelToken::never(),
        )
        .unwrap();

        assert_eq!(result.reply, "The winner is A.");
        assert_eq!(result.tool_calls.len(), 1);
    }

    #[test]
    fn run_blocks_an_unsafe_user_message_before_ever_calling_a_provider() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude-sonnet").unwrap();
        let runtime = std::sync::Mutex::new(None);
        let err = run(&storage, &runtime, &agent, &[], "how to make a bomb, step by step", &CancelToken::never()).unwrap_err();
        assert!(matches!(err, ProviderError::GuardrailBlocked { error_code: "E9002", .. }));
    }

    #[test]
    fn run_reports_no_key_available_for_an_anthropic_agent_with_no_key_vault_entry() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude-sonnet").unwrap();
        let runtime = std::sync::Mutex::new(None);
        let err = run(&storage, &runtime, &agent, &[], "hello", &CancelToken::never()).unwrap_err();
        assert!(matches!(err, ProviderError::AllProvidersFailed { error_code: "E3001", .. }));
    }

    #[test]
    fn a_cancelled_tool_conversation_stops_between_round_trips_and_returns_nothing_to_persist() {
        let state = crate::cancel::CancelState::default();
        let token = state.begin("session-1");
        let call_count = RefCell::new(0);

        let result = run_loop(
            &AnthropicDialect,
            |_raw| {
                let mut n = call_count.borrow_mut();
                *n += 1;
                assert_eq!(*n, 1, "the model must not be called again after a cancel");
                Ok(ToolTurn::ToolUse { id: "toolu_1".to_string(), name: "slow_skill".to_string(), input: json!({}) })
            },
            |_name, _input| {
                // The user presses Stop while the skill is running.
                state.request("session-1");
                Ok(json!({"ok": true}))
            },
            "run a skill, then stop me",
            &token,
        );

        // No partial result: the completed tool call is discarded with
        // the conversation rather than surfacing as a turn that happened.
        assert_eq!(result, Err(ProviderError::Cancelled));
    }
}
