//! Role Templates ("1 人公司") — pre-written system prompts a user can
//! apply when creating an Agent, instead of writing one from scratch.
//!
//! Two sources ("Default vs User Custom"):
//! - `default_templates()` below — the 11 built-in roles, hardcoded here
//!   (not in Storage) so an app update can safely refresh their content
//!   without touching anything the user wrote.
//! - `storage::CustomRoleTemplate` — user-authored, persisted, survives
//!   app updates.
//!
//! Both get normalized into the same `RoleTemplate` shape so the frontend
//! (and `create_agent`) don't need to care which folder something came
//! from.

use serde::{Deserialize, Serialize};

use crate::storage::CustomRoleTemplate;

/// Every default role's system prompt, each in its own file under
/// `prompts/` and embedded at compile time.
///
/// They started as string literals in the list below, three or four lines
/// each, and that was the right place for them at that length. They are
/// now full role briefs — an order of magnitude longer, written as
/// Markdown with headings and lists — and that is where inlining stops
/// paying: the same text as a Rust literal is dozens of lines of escaped
/// newlines and `\` continuations, and getting one wrong still compiles.
/// It just drops a line break, or smuggles this file's indentation into
/// the prompt. That is not hypothetical; it happened on the first prompt
/// written this way, and only a test caught it.
///
/// `include_str!` embeds at compile time: no runtime file to find, nothing
/// extra to bundle with the installer. What it buys is that the prompts
/// are plain text — readable, diffable, and editable without thinking
/// about escaping at all.
mod prompt {
    pub const PRODUCT_LEAD: &str = include_str!("prompts/product-lead.md");
    pub const LEAD_ARCHITECT: &str = include_str!("prompts/lead-architect.md");
    pub const UIUX_DESIGNER: &str = include_str!("prompts/uiux-designer.md");
    pub const FULL_STACK_DEVELOPER: &str = include_str!("prompts/full-stack-developer.md");
    pub const QA_TEST_ENGINEER: &str = include_str!("prompts/qa-test-engineer.md");
    pub const SECURITY_VULNERABILITY_TESTER: &str = include_str!("prompts/security-vulnerability-tester.md");
    pub const RELEASE_DEVOPS_MANAGER: &str = include_str!("prompts/release-devops-manager.md");
    pub const ISSUE_MANAGER: &str = include_str!("prompts/issue-manager.md");
    pub const WIKI_DOCUMENTATION_WRITER: &str = include_str!("prompts/wiki-documentation-writer.md");
    pub const OBSIDIAN_KNOWLEDGE_ARCHITECT: &str = include_str!("prompts/obsidian-knowledge-architect.md");
    pub const DAILY_ASSISTANT: &str = include_str!("prompts/daily-assistant.md");
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub suggested_provider_kind: Option<String>,
    pub suggested_provider_name: Option<String>,
    pub suggested_model: Option<String>,
    /// "default" | "custom"
    pub source: String,
}

/// The on-disk shape of an exported/shared custom role template — same
/// fields as `RoleTemplate` minus `id`/`source` (both meaningless outside
/// this app's own database: importing always creates a fresh id, and an
/// imported template is by definition "custom" regardless of where the
/// exporter's copy came from).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleTemplateExport {
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub suggested_provider_kind: Option<String>,
    pub suggested_provider_name: Option<String>,
    pub suggested_model: Option<String>,
}

impl From<&RoleTemplate> for RoleTemplateExport {
    fn from(t: &RoleTemplate) -> Self {
        RoleTemplateExport {
            name: t.name.clone(),
            description: t.description.clone(),
            system_prompt: t.system_prompt.clone(),
            suggested_provider_kind: t.suggested_provider_kind.clone(),
            suggested_provider_name: t.suggested_provider_name.clone(),
            suggested_model: t.suggested_model.clone(),
        }
    }
}

impl From<CustomRoleTemplate> for RoleTemplate {
    fn from(t: CustomRoleTemplate) -> Self {
        RoleTemplate {
            id: t.id,
            name: t.name,
            description: t.description,
            system_prompt: t.system_prompt,
            suggested_provider_kind: t.suggested_provider_kind,
            suggested_provider_name: t.suggested_provider_name,
            suggested_model: t.suggested_model,
            source: "custom".to_string(),
        }
    }
}

fn default_template(
    id: &str,
    name: &str,
    description: &str,
    system_prompt: &str,
    suggested_provider_kind: &str,
    suggested_provider_name: &str,
    suggested_model: &str,
) -> RoleTemplate {
    RoleTemplate {
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        system_prompt: system_prompt.trim().to_string(),
        suggested_provider_kind: Some(suggested_provider_kind.to_string()),
        suggested_provider_name: Some(suggested_provider_name.to_string()),
        suggested_model: Some(suggested_model.to_string()),
        source: "default".to_string(),
    }
}

/// The built-in roles. Suggested provider/model are defaults only —
/// `create_agent` never enforces them ("已定案：僅作預設建議值，使用者
/// 可自由覆寫").
///
/// Ten of the eleven are the "1 人公司" software-team roles, written to
/// hand work to one another: the Product Lead settles scope before the
/// Architect settles structure before the Developer writes anything, and
/// QA, Security, Release, Issues and Docs pick it up from there. They
/// share a common spine — state your assumptions, show the plan and wait
/// for a go-ahead before producing, never delete unilaterally, stay
/// inside the paths you were given, hand off with the risks named.
///
/// `daily-assistant` is not one of them: a general-purpose assistant with
/// no place in that pipeline, kept in the same list because one entry
/// outside the pattern does not earn a grouping mechanism.
pub fn default_templates() -> Vec<RoleTemplate> {
    vec![
        default_template(
            "product-lead",
            "Product Lead",
            "需求分析、拆解功能模組、決定優先順序",
            prompt::PRODUCT_LEAD,
            "cloud",
            "anthropic",
            "claude-sonnet-4-5",
        ),
        default_template(
            "lead-architect",
            "Lead Architect",
            "技術選型、資料庫結構設計、模組間的介面與擴展性",
            prompt::LEAD_ARCHITECT,
            "cloud",
            "anthropic",
            "claude-opus-4-5",
        ),
        default_template(
            "uiux-designer",
            "UIUX Designer",
            "視覺風格、介面佈局、互動流程、色彩搭配",
            prompt::UIUX_DESIGNER,
            "cloud",
            "anthropic",
            "claude-sonnet-4-5",
        ),
        default_template(
            "full-stack-developer",
            "Full-Stack Developer",
            "實作具體程式碼、寫邏輯、串接 API、刻介面",
            prompt::FULL_STACK_DEVELOPER,
            "cloud",
            "anthropic",
            "claude-sonnet-4-5",
        ),
        default_template(
            "qa-test-engineer",
            "QA & Test Engineer",
            "單元測試、邊界條件檢查、CI/CD 失敗診斷與建置修復",
            prompt::QA_TEST_ENGINEER,
            "cloud",
            "anthropic",
            "claude-sonnet-4-5",
        ),
        default_template(
            "security-vulnerability-tester",
            "Security & Vulnerability Tester",
            "漏洞審查、敏感資訊外洩、依賴套件 CVE、認證與授權驗證",
            prompt::SECURITY_VULNERABILITY_TESTER,
            "cloud",
            "anthropic",
            "claude-opus-4-5",
        ),
        default_template(
            "release-devops-manager",
            "Release & DevOps Manager",
            "分支策略、commit 訊息、合併衝突、打包發布與部署",
            prompt::RELEASE_DEVOPS_MANAGER,
            "cloud",
            "anthropic",
            "claude-sonnet-4-5",
        ),
        default_template(
            "issue-manager",
            "Issue Manager",
            "標籤與優先級、問題摘要與回覆、根因分析、開發任務拆解",
            prompt::ISSUE_MANAGER,
            "cloud",
            "anthropic",
            "claude-sonnet-4-5",
        ),
        default_template(
            "wiki-documentation-writer",
            "Wiki & Documentation Writer",
            "README、API 文件、操作手冊、變更日誌的撰寫與維護",
            prompt::WIKI_DOCUMENTATION_WRITER,
            "cloud",
            "anthropic",
            "claude-sonnet-4-5",
        ),
        default_template(
            "obsidian-knowledge-architect",
            "Obsidian Knowledge Architect",
            "管理與維護本地 Obsidian 知識庫中的專案筆記、開發日誌、架構靈感、雙向連結結構",
            prompt::OBSIDIAN_KNOWLEDGE_ARCHITECT,
            "local",
            "ollama",
            "qwen2.5-coder:1.5b",
        ),
        // The one default written in Traditional Chinese, and deliberately
        // so: its first instruction is to reply in Traditional Chinese, and
        // a prompt that issues that instruction in another language is
        // asking the model to infer what it could simply demonstrate.
        // Supplied by the user from their own prompt library and kept
        // verbatim — it is their voice, not the app's. The other ten are in
        // English because the app ships in seven languages, and a prompt's
        // own language steers what the model answers in.
        default_template(
            "daily-assistant",
            "Daily Assistant",
            "日常生活助理：繁體中文、先講結論、不確定就說不確定",
            prompt::DAILY_ASSISTANT,
            "cloud",
            "anthropic",
            "claude-sonnet-4-5",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_exactly_eleven_default_templates_with_unique_ids() {
        let templates = default_templates();
        assert_eq!(templates.len(), 11);
        let mut ids: Vec<&str> = templates.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 11, "default template ids must be unique");
    }

    #[test]
    fn every_prompt_file_arrives_whole_and_trimmed() {
        // `include_str!` cannot mangle the text the way hand-escaping
        // did, so this no longer guards against lost line breaks. What it
        // guards against is a file gone empty, truncated, or replaced by
        // something that is no longer a role brief — all of which compile.
        for t in default_templates() {
            let prompt = &t.system_prompt;
            assert!(prompt.len() > 400, "{} looks truncated ({} bytes)", t.id, prompt.len());
            assert!(prompt.starts_with("# ") || prompt.starts_with("## "), "{} lost its heading", t.id);
            assert_eq!(prompt, prompt.trim(), "{} carries surrounding whitespace", t.id);
        }
    }

    #[test]
    fn the_ten_pipeline_roles_share_their_working_rules() {
        // The spine every "1 人公司" role is supposed to carry. A file
        // rewritten without one of these would still compile, still load,
        // and quietly drop a rule the user relies on — which is why they
        // are asserted rather than trusted.
        for t in default_templates().into_iter().filter(|t| t.id != "daily-assistant") {
            for rule in ["Assumption:", "explicit go-ahead", "second, explicit confirmation"] {
                assert!(t.system_prompt.contains(rule), "{} is missing: {rule}", t.id);
            }
        }
    }

    #[test]
    fn the_daily_assistant_is_the_one_prompt_in_chinese() {
        let daily = default_templates().into_iter().find(|t| t.id == "daily-assistant").unwrap();
        for heading in ["## 核心定位", "## 資料蒐集與來源", "## 誠實與品質紅線", "## 界限"] {
            assert!(daily.system_prompt.contains(heading), "missing section {heading}");
        }
    }

    #[test]
    fn every_default_template_has_a_non_empty_system_prompt() {
        for t in default_templates() {
            assert!(!t.system_prompt.trim().is_empty(), "{} has an empty system prompt", t.name);
            assert_eq!(t.source, "default");
        }
    }

    #[test]
    fn custom_template_converts_with_custom_source() {
        let custom = CustomRoleTemplate {
            id: "abc".to_string(),
            name: "My Role".to_string(),
            description: "desc".to_string(),
            system_prompt: "You are My Role.".to_string(),
            suggested_provider_kind: None,
            suggested_provider_name: None,
            suggested_model: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let converted: RoleTemplate = custom.into();
        assert_eq!(converted.source, "custom");
        assert_eq!(converted.name, "My Role");
    }

    #[test]
    fn export_drops_id_and_source_but_keeps_every_content_field() {
        let template = RoleTemplate {
            id: "some-id".to_string(),
            name: "Analyst".to_string(),
            description: "Looks at numbers".to_string(),
            system_prompt: "You are an analyst.".to_string(),
            suggested_provider_kind: Some("cloud".to_string()),
            suggested_provider_name: Some("anthropic".to_string()),
            suggested_model: Some("claude-sonnet".to_string()),
            source: "custom".to_string(),
        };
        let export = RoleTemplateExport::from(&template);
        let json = serde_json::to_string(&export).unwrap();
        assert!(!json.contains("some-id"), "exported JSON must not leak the source database's id");
        assert!(!json.contains("\"source\""), "exported JSON must not contain a source field");

        let round_tripped: RoleTemplateExport = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped.name, "Analyst");
        assert_eq!(round_tripped.suggested_model, Some("claude-sonnet".to_string()));
    }
}
