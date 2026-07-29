//! Orchestrator: multi-agent coordination for Group Chats.
//! Design: `Multi-AI Agent Panel Document/04 Agents & Orchestration/Orchestration Design.md`,
//! `04 Agents & Orchestration/Session Types.md` (conflict resolution,
//! meeting summarization).
//!
//! This module implements the part of `Orchestration Design.md` that has
//! a real execution point today: Group Chat's loop safety-net and
//! meeting-end summarizer selection. It deliberately does **not**
//! implement the DAG task-decomposition Planner/Aggregator pipeline
//! described in that doc — that needs a task-graph engine that doesn't
//! exist yet (see Backlog). Building that scaffolding now, with nothing
//! to run through it, would be the same mistake `guardrails` explicitly
//! avoids: enforcement-shaped code with no real enforcement behind it.
//!
//! ## Why a turn cap instead of real disagreement detection
//! `Session Types.md` asks for "3 rounds of disagreement, then hand back
//! to the user." Detecting that two agents are *actually disagreeing* —
//! as opposed to just talking — needs real semantic understanding, which
//! a mechanical check can't honestly claim to do. Instead this enforces a
//! concrete, honest proxy: a hard cap on consecutive agent turns with no
//! user message in between (3 rounds × 2 sides = 6 turns). This still
//! satisfies the actual safety goal — no unbounded Agent↔Agent loop
//! without the user able to step in — without pretending to detect
//! disagreement it doesn't detect.

use crate::session_manager;
use crate::storage::{Agent, GroupSessionState};

/// See the module-level note above for why this is a turn cap and not
/// semantic disagreement detection: 3 rounds × 2 sides, per
/// `Session Types.md`'s conflict-resolution rule.
pub const MAX_CONSECUTIVE_AGENT_TURNS_WITHOUT_USER_INPUT: i64 = 6;

#[derive(Debug, PartialEq, Eq)]
pub enum GroupChatError {
    /// Error Code Registry E6001 — the loop safety-net tripped; a real
    /// user message is required before another agent turn can happen.
    TurnLimitReached,
    /// Error Code Registry E6002 — nobody to hand the turn to at all.
    NoMembers,
}

impl GroupChatError {
    pub fn error_code(&self) -> &'static str {
        match self {
            GroupChatError::TurnLimitReached => "E6001",
            GroupChatError::NoMembers => "E6002",
        }
    }
}

impl std::fmt::Display for GroupChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroupChatError::TurnLimitReached => write!(
                f,
                "{} this meeting has gone {} agent turns without user input — please weigh in before it continues",
                self.error_code(),
                MAX_CONSECUTIVE_AGENT_TURNS_WITHOUT_USER_INPUT
            ),
            GroupChatError::NoMembers => write!(f, "{} this Group Chat has no participating agents", self.error_code()),
        }
    }
}

/// Checks the loop safety-net and, if it's still safe to continue, picks
/// the next speaker (honoring an `@mention` if present). Does not itself
/// mutate `state` — callers persist the returned cursor via
/// `storage::save_group_session_state` once the turn actually happens, so
/// a call that never results in an agent reply doesn't advance anything.
pub fn plan_next_turn(
    state: &GroupSessionState,
    members: &[String],
    mention: Option<&str>,
) -> Result<(String, usize), GroupChatError> {
    if state.consecutive_agent_turns >= MAX_CONSECUTIVE_AGENT_TURNS_WITHOUT_USER_INPUT {
        return Err(GroupChatError::TurnLimitReached);
    }
    session_manager::decide_next_speaker(members, state.rotation_cursor as usize, mention)
        .ok_or(GroupChatError::NoMembers)
}

/// Picks who summarizes a Group Chat when it ends, per `Session Types.md`:
/// an explicit user choice wins if it names an actual member; otherwise
/// prefer a member built from the "Product Lead" role template; otherwise
/// the first agent who joined (`members` is join-ordered).
pub fn pick_summarizer<'a>(members: &'a [Agent], explicit: Option<&str>) -> Option<&'a Agent> {
    if let Some(explicit_id) = explicit {
        if let Some(found) = members.iter().find(|a| a.id == explicit_id) {
            return Some(found);
        }
    }
    members
        .iter()
        .find(|a| a.role_template.as_deref() == Some("Product Lead"))
        .or_else(|| members.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, role_template: Option<&str>) -> Agent {
        Agent {
            id: id.to_string(),
            name: id.to_string(),
            role_template: role_template.map(str::to_string),
            system_prompt: None,
            provider_kind: "cloud".to_string(),
            provider_name: "anthropic".to_string(),
            model: "claude".to_string(),
            pinned_provider_key_id: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn state(cursor: i64, turns: i64) -> GroupSessionState {
        GroupSessionState { session_id: "s1".to_string(), rotation_cursor: cursor, consecutive_agent_turns: turns }
    }

    #[test]
    fn plans_the_next_speaker_when_under_the_turn_cap() {
        let members = vec!["a".to_string(), "b".to_string()];
        let (speaker, cursor) = plan_next_turn(&state(0, 0), &members, None).unwrap();
        assert_eq!(speaker, "a");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn refuses_to_plan_another_turn_once_the_cap_is_hit() {
        let members = vec!["a".to_string(), "b".to_string()];
        let capped = state(0, MAX_CONSECUTIVE_AGENT_TURNS_WITHOUT_USER_INPUT);
        let err = plan_next_turn(&capped, &members, None).unwrap_err();
        assert_eq!(err, GroupChatError::TurnLimitReached);
        assert_eq!(err.error_code(), "E6001");
    }

    #[test]
    fn a_mention_still_works_right_up_to_the_cap() {
        let members = vec!["a".to_string(), "b".to_string()];
        let almost_capped = state(0, MAX_CONSECUTIVE_AGENT_TURNS_WITHOUT_USER_INPUT - 1);
        let (speaker, _) = plan_next_turn(&almost_capped, &members, Some("b")).unwrap();
        assert_eq!(speaker, "b");
    }

    #[test]
    fn reports_no_members_when_the_session_is_empty() {
        let err = plan_next_turn(&state(0, 0), &[], None).unwrap_err();
        assert_eq!(err, GroupChatError::NoMembers);
        assert_eq!(err.error_code(), "E6002");
    }

    #[test]
    fn explicit_summarizer_choice_wins_when_valid() {
        let members = vec![agent("1", Some("Product Lead")), agent("2", None)];
        let chosen = pick_summarizer(&members, Some("2")).unwrap();
        assert_eq!(chosen.id, "2");
    }

    #[test]
    fn an_invalid_explicit_choice_falls_back_to_product_lead() {
        let members = vec![agent("1", None), agent("2", Some("Product Lead"))];
        let chosen = pick_summarizer(&members, Some("not-a-member")).unwrap();
        assert_eq!(chosen.id, "2");
    }

    #[test]
    fn falls_back_to_the_first_joined_agent_when_no_product_lead_is_present() {
        let members = vec![agent("1", Some("Full-Stack Developer")), agent("2", Some("QA & Test Engineer"))];
        let chosen = pick_summarizer(&members, None).unwrap();
        assert_eq!(chosen.id, "1");
    }

    #[test]
    fn no_members_means_no_summarizer() {
        assert!(pick_summarizer(&[], None).is_none());
    }
}
