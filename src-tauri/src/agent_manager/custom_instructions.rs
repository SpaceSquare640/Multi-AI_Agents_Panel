//! Global custom instructions: one freeform text block, set once, always
//! included as context on *every* call to *every* agent — the analogue
//! of Claude's own "Instructions for Claude" (standing language/style
//! preferences, workflow rules that apply regardless of which agent or
//! conversation). Distinct from `agent_manager::memory` in two ways:
//! it's global rather than per-agent, and it's unconditionally included
//! rather than filtered by relevance to the current message — a
//! standing instruction like "always reply in Traditional Chinese"
//! isn't something that should ever be silently dropped because it
//! didn't score high enough against a particular message's keywords.

use crate::agent_manager::providers::ChatMessage;
use crate::storage::Storage;

/// Returns a `system`-role `ChatMessage` carrying the global custom
/// instructions, or `None` if nothing has ever been set (an empty or
/// unset instructions block means "behave exactly as if this feature
/// didn't exist" — no empty system message ever gets inserted). For
/// callers that build a `Vec<ChatMessage>` history directly
/// (`commands::send_chat_message`, group-chat turn/summarizer
/// building) — insert this at the very front, ahead of the agent's own
/// `system_prompt`, so a standing global preference can't be
/// overridden by a role template's own instructions.
pub fn as_system_message(storage: &Storage) -> Option<ChatMessage> {
    let content = storage.get_custom_instructions().ok().flatten()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(ChatMessage { role: "system".to_string(), content: trimmed.to_string() })
}

/// For callers that build a single `system` string directly rather than
/// a `Vec<ChatMessage>` (`agent_manager::function_calling::run`, whose
/// Anthropic tool-calling request takes one top-level `system` field) —
/// combines the global instructions with the agent's own system prompt,
/// global first, joined by a blank line, so it reads as one coherent
/// system message rather than two concatenated fragments. Returns
/// `None` only when *both* are absent, matching the existing behavior
/// of omitting the `system` field entirely when there's nothing to say.
pub fn combine_with_agent_prompt(storage: &Storage, agent_system_prompt: Option<&str>) -> Option<String> {
    let global = storage.get_custom_instructions().ok().flatten().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let agent = agent_system_prompt.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);

    match (global, agent) {
        (Some(global), Some(agent)) => Some(format!("{global}\n\n{agent}")),
        (Some(global), None) => Some(global),
        (None, Some(agent)) => Some(agent),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_system_message_is_none_when_nothing_was_ever_set() {
        let storage = Storage::open_in_memory().unwrap();
        assert_eq!(as_system_message(&storage), None);
    }

    #[test]
    fn as_system_message_is_none_for_whitespace_only_content() {
        let storage = Storage::open_in_memory().unwrap();
        storage.set_custom_instructions("   \n  ").unwrap();
        assert_eq!(as_system_message(&storage), None);
    }

    #[test]
    fn as_system_message_carries_the_trimmed_content_as_a_system_message() {
        let storage = Storage::open_in_memory().unwrap();
        storage.set_custom_instructions("  always reply in Traditional Chinese  ").unwrap();
        let message = as_system_message(&storage).unwrap();
        assert_eq!(message.role, "system");
        assert_eq!(message.content, "always reply in Traditional Chinese");
    }

    #[test]
    fn combine_with_agent_prompt_is_none_when_both_are_absent() {
        let storage = Storage::open_in_memory().unwrap();
        assert_eq!(combine_with_agent_prompt(&storage, None), None);
    }

    #[test]
    fn combine_with_agent_prompt_uses_only_the_agent_prompt_when_global_is_unset() {
        let storage = Storage::open_in_memory().unwrap();
        assert_eq!(combine_with_agent_prompt(&storage, Some("You are a QA engineer.")), Some("You are a QA engineer.".to_string()));
    }

    #[test]
    fn combine_with_agent_prompt_uses_only_the_global_instructions_when_agent_has_none() {
        let storage = Storage::open_in_memory().unwrap();
        storage.set_custom_instructions("always reply in Traditional Chinese").unwrap();
        assert_eq!(combine_with_agent_prompt(&storage, None), Some("always reply in Traditional Chinese".to_string()));
    }

    #[test]
    fn combine_with_agent_prompt_puts_global_instructions_first() {
        let storage = Storage::open_in_memory().unwrap();
        storage.set_custom_instructions("always reply in Traditional Chinese").unwrap();
        let combined = combine_with_agent_prompt(&storage, Some("You are a QA engineer.")).unwrap();
        assert_eq!(combined, "always reply in Traditional Chinese\n\nYou are a QA engineer.");
    }
}
