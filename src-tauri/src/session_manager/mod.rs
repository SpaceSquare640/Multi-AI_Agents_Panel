//! Session Manager: independent sessions vs. group chats.
//! Design: `Multi-AI Agent Panel Document/04 Agents & Orchestration/Session Types.md`
//!
//! This module holds the mechanical, deterministic half of Group Chat:
//! who speaks next. The other half — conflict-round handoff, meeting
//! summarization, and the Error Code Registry E6xxx codes — lives in
//! `orchestrator`, which calls back into this module for turn order.

use crate::storage::Agent;

/// Decides who speaks next in a Group Chat.
///
/// Default order is round-robin over `members` (already ordered by join
/// time — see `storage::agents_for_session`), tracked by `cursor`. An
/// `@mention` speaks out of turn *without* consuming a rotation slot —
/// per `Session Types.md`: "該角色插隊優先發言，發言完畢後回到原本輪流順序" —
/// so the cursor is only returned unchanged when a mention is honored.
///
/// Returns `None` if `members` is empty (nobody to speak).
pub fn decide_next_speaker(members: &[String], cursor: usize, mention: Option<&str>) -> Option<(String, usize)> {
    if members.is_empty() {
        return None;
    }
    if let Some(mentioned_id) = mention {
        if members.iter().any(|m| m == mentioned_id) {
            return Some((mentioned_id.to_string(), cursor));
        }
        // Mentioned someone who isn't actually a member of this session —
        // fall through to normal rotation rather than erroring, since a
        // typo'd @mention shouldn't stall the whole meeting.
    }
    let index = cursor % members.len();
    Some((members[index].clone(), (cursor + 1) % members.len()))
}

/// Parses a leading/embedded `@AgentName` token out of a message and
/// resolves it to that agent's id, matching case-insensitively against
/// `members`' names. Only agents actually in `members` can be mentioned —
/// `@` text that doesn't match any member name is not treated as a
/// mention (see `decide_next_speaker`'s fallback).
///
/// Per `Session Types.md`: multi-word names are matched by taking the
/// longest `@`-prefixed run of words that matches a member's full name;
/// this keeps `@Lead Architect what do you think` working without needing
/// quotes or underscores in the name.
pub fn parse_mention(content: &str, members: &[Agent]) -> Option<String> {
    let at_pos = content.find('@')?;
    let after_at = &content[at_pos + 1..];
    let lower_after = after_at.to_lowercase();

    members
        .iter()
        .filter(|agent| lower_after.starts_with(&agent.name.to_lowercase()))
        .max_by_key(|agent| agent.name.len())
        .map(|agent| agent.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, name: &str) -> Agent {
        Agent {
            id: id.to_string(),
            name: name.to_string(),
            role_template: None,
            system_prompt: None,
            provider_kind: "cloud".to_string(),
            provider_name: "anthropic".to_string(),
            model: "claude".to_string(),
            pinned_provider_key_id: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn round_robins_through_members_in_order() {
        let members = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let (speaker1, cursor1) = decide_next_speaker(&members, 0, None).unwrap();
        assert_eq!(speaker1, "a");
        let (speaker2, cursor2) = decide_next_speaker(&members, cursor1, None).unwrap();
        assert_eq!(speaker2, "b");
        let (speaker3, cursor3) = decide_next_speaker(&members, cursor2, None).unwrap();
        assert_eq!(speaker3, "c");
        let (speaker4, _) = decide_next_speaker(&members, cursor3, None).unwrap();
        assert_eq!(speaker4, "a", "rotation wraps back to the start");
    }

    #[test]
    fn mention_speaks_out_of_turn_without_consuming_the_rotation_slot() {
        let members = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        // Cursor is at "b"'s turn; mentioning "c" should let c speak now...
        let (speaker, cursor_after_mention) = decide_next_speaker(&members, 1, Some("c")).unwrap();
        assert_eq!(speaker, "c");
        assert_eq!(cursor_after_mention, 1, "mention must not advance the rotation cursor");
        // ...and normal rotation resumes exactly where it left off, at "b".
        let (next_speaker, _) = decide_next_speaker(&members, cursor_after_mention, None).unwrap();
        assert_eq!(next_speaker, "b");
    }

    #[test]
    fn mentioning_a_non_member_falls_back_to_normal_rotation() {
        let members = vec!["a".to_string(), "b".to_string()];
        let (speaker, cursor) = decide_next_speaker(&members, 0, Some("not-a-member")).unwrap();
        assert_eq!(speaker, "a");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn no_members_means_nobody_speaks() {
        assert!(decide_next_speaker(&[], 0, None).is_none());
    }

    #[test]
    fn parses_a_single_word_mention() {
        let members = vec![agent("1", "Planner"), agent("2", "Reviewer")];
        assert_eq!(parse_mention("@Reviewer what do you think?", &members), Some("2".to_string()));
    }

    #[test]
    fn parses_a_multi_word_mention_by_preferring_the_longest_match() {
        let members = vec![agent("1", "Lead"), agent("2", "Lead Architect")];
        assert_eq!(
            parse_mention("@Lead Architect can you review this?", &members),
            Some("2".to_string()),
            "the longer, more specific name should win over a shorter prefix match"
        );
    }

    #[test]
    fn is_case_insensitive() {
        let members = vec![agent("1", "Product Lead")];
        assert_eq!(parse_mention("hey @product lead, thoughts?", &members), Some("1".to_string()));
    }

    #[test]
    fn no_at_symbol_means_no_mention() {
        let members = vec![agent("1", "Reviewer")];
        assert_eq!(parse_mention("what do you all think?", &members), None);
    }

    #[test]
    fn at_symbol_with_no_matching_member_is_not_a_mention() {
        let members = vec![agent("1", "Reviewer")];
        assert_eq!(parse_mention("email me at @someone-else", &members), None);
    }
}
