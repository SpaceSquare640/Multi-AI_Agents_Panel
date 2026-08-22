//! Long-term memory: notes an agent has saved that survive across
//! sessions, unlike `storage::Message`, which is scoped to one
//! `session_id` and gone once that session's history isn't loaded
//! anymore. Beta-gap queue item 6/8 (see the vault's Backlog.md).
//!
//! **Scope, stated honestly**: relevance ranking here is keyword
//! overlap (case-insensitive token intersection), not semantic
//! embedding search. `turbovec`-backed semantic search
//! (`ml_engine::vector_index`) already exists in this codebase for
//! granted-file search, but wiring memory notes through the same
//! embedding pipeline is a real follow-up, not something to fake with a
//! numpy call this pass doesn't make. Keyword overlap is a real,
//! testable, honest MVP: it finds memories that share words with the
//! current message, which is a meaningful signal even without
//! embeddings, just a cruder one.
//!
//! Memories are written explicitly (`storage::add_agent_memory`, e.g.
//! from a "remember this" UI action), not auto-summarized from every
//! conversation turn — auto-summarization needs its own design
//! (when to summarize, how to avoid drowning genuinely important notes
//! in low-value ones) and is left for a future pass rather than done
//! halfway here.

use std::collections::HashSet;

use crate::agent_manager::providers::ChatMessage;
use crate::storage::{AgentMemory, Storage};

/// How many of an agent's most relevant memories to inject as context
/// per message — a small, fixed cap so this can't quietly balloon a
/// request's token count as an agent accumulates hundreds of memories.
pub const MAX_INJECTED_MEMORIES: usize = 5;

fn tokenize(text: &str) -> HashSet<String> {
    text.to_lowercase().split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string()).filter(|w| !w.is_empty()).collect()
}

/// Ranks `memories` by token overlap with `query`, descending, and
/// returns at most `top_k`. Ties keep the memories' original (creation)
/// order — Rust's sort is stable, so this doesn't need its own
/// tie-breaking logic. A memory with zero overlapping tokens is still
/// included if there's room (`top_k` not yet filled by more relevant
/// ones) — the alternative (excluding it entirely) would make an agent
/// with only a couple of memories effectively unable to recall them for
/// any oddly-phrased query, which is a worse failure mode than
/// occasionally injecting a weakly related note.
pub fn rank_relevant<'a>(memories: &'a [AgentMemory], query: &str, top_k: usize) -> Vec<&'a AgentMemory> {
    let query_tokens = tokenize(query);
    let mut scored: Vec<(usize, usize, &AgentMemory)> = memories
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let overlap = tokenize(&m.content).intersection(&query_tokens).count();
            (i, overlap, m)
        })
        .collect();
    // Stable sort descending by score: reverse comparison on score only,
    // so ties fall back to the original enumerate() order (ascending i).
    scored.sort_by_key(|(_, overlap, _)| std::cmp::Reverse(*overlap));
    scored.into_iter().take(top_k).map(|(_, _, m)| m).collect()
}

/// Builds the `ChatMessage`s to prepend before the user's actual message:
/// one `system`-role message listing the top `MAX_INJECTED_MEMORIES`
/// memories relevant to `user_message`, or nothing at all if the agent
/// has no memories yet (an agent with zero memories should look
/// identical to how it behaved before this feature existed, not carry
/// an empty "no memories" preamble on every single message).
pub fn context_messages(storage: &Storage, agent_id: &str, user_message: &str) -> Vec<ChatMessage> {
    let memories = storage.list_agent_memories(agent_id).unwrap_or_default();
    if memories.is_empty() {
        return Vec::new();
    }

    let relevant = rank_relevant(&memories, user_message, MAX_INJECTED_MEMORIES);
    if relevant.is_empty() {
        return Vec::new();
    }

    let bullets: Vec<String> = relevant.iter().map(|m| format!("- {}", m.content)).collect();
    vec![ChatMessage {
        role: "system".to_string(),
        content: format!("Relevant memories from earlier conversations with this user:\n{}", bullets.join("\n")),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(id: &str, content: &str) -> AgentMemory {
        AgentMemory { id: id.to_string(), agent_id: "agent-1".to_string(), content: content.to_string(), created_at: "2026-01-01T00:00:00Z".to_string() }
    }

    #[test]
    fn ranks_the_memory_with_more_shared_words_first() {
        let memories = vec![
            memory("1", "the user's favorite color is blue"),
            memory("2", "the user prefers terse replies and dislikes emoji"),
        ];
        let ranked = rank_relevant(&memories, "how does the user like replies formatted?", 5);
        assert_eq!(ranked[0].id, "2");
    }

    #[test]
    fn is_case_insensitive_and_ignores_punctuation() {
        let memories = vec![memory("1", "User's Favorite Language: Rust!")];
        let ranked = rank_relevant(&memories, "what is the user's favorite language?", 5);
        assert_eq!(ranked.len(), 1);
    }

    #[test]
    fn respects_top_k_even_when_more_memories_exist() {
        let memories: Vec<AgentMemory> = (0..10).map(|i| memory(&i.to_string(), "shared word overlap")).collect();
        let ranked = rank_relevant(&memories, "shared word overlap query", 3);
        assert_eq!(ranked.len(), 3);
    }

    #[test]
    fn ties_keep_original_creation_order() {
        let memories = vec![memory("first", "zzz"), memory("second", "zzz")];
        let ranked = rank_relevant(&memories, "totally unrelated query", 5);
        assert_eq!(ranked[0].id, "first");
        assert_eq!(ranked[1].id, "second");
    }

    #[test]
    fn context_messages_is_empty_for_an_agent_with_no_memories() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude").unwrap();
        assert!(context_messages(&storage, &agent.id, "hello").is_empty());
    }

    #[test]
    fn context_messages_includes_a_system_message_listing_relevant_memories() {
        let storage = Storage::open_in_memory().unwrap();
        let agent = storage.create_agent("Test", None, None, "cloud", "anthropic", "claude").unwrap();
        storage.add_agent_memory(&agent.id, "the user's project is called Multi-AI Agents Panel").unwrap();

        let messages = context_messages(&storage, &agent.id, "what is the user's project called?");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "system");
        assert!(messages[0].content.contains("Multi-AI Agents Panel"));
    }
}
