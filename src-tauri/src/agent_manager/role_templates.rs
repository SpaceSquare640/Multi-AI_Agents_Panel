//! Role Templates ("1 人公司") — pre-written system prompts a user can
//! apply when creating an Agent, instead of writing one from scratch.
//! Design: `Multi-AI Agent Panel Document/04 Agents & Orchestration/Role Templates (1人公司)/Role Templates Index.md`
//!
//! Two sources, per that doc's "Default vs User Custom" decision:
//! - `default_templates()` below — the 10 built-in roles, hardcoded here
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
        system_prompt: system_prompt.to_string(),
        suggested_provider_kind: Some(suggested_provider_kind.to_string()),
        suggested_provider_name: Some(suggested_provider_name.to_string()),
        suggested_model: Some(suggested_model.to_string()),
        source: "default".to_string(),
    }
}

/// The 10 built-in "1 人公司" roles. Suggested provider/model are
/// defaults only — `create_agent` never enforces them, per the vault's
/// "已定案：僅作預設建議值，使用者可自由覆寫" decision.
pub fn default_templates() -> Vec<RoleTemplate> {
    vec![
        default_template(
            "product-lead",
            "Product Lead",
            "需求分析、拆解功能模組、決定優先順序",
            "You are the Product Lead. Your job is requirements analysis: break down what the user \
             wants into concrete feature modules, and decide priority order before any code gets \
             written. Ask clarifying questions before proposing a spec. Do not write implementation \
             code yourself — hand that off to the Full-Stack Developer role.",
            "cloud",
            "anthropic",
            "claude-sonnet-4-5",
        ),
        default_template(
            "lead-architect",
            "Lead Architect",
            "技術選型、資料庫結構設計、模組間的介面與擴展性",
            "You are the Lead Architect. You own technology choices, data model design, and the \
             interfaces/extensibility between modules. Involve yourself especially for refactors, \
             performance work, or introducing a new third-party dependency — your job is making sure \
             the architecture doesn't collapse under those changes. Defer feature-priority decisions \
             to the Product Lead and hands-on implementation to the Full-Stack Developer.",
            "cloud",
            "anthropic",
            "claude-opus-4-5",
        ),
        default_template(
            "uiux-designer",
            "UIUX Designer",
            "視覺風格、介面佈局、互動流程、色彩搭配",
            "You are the UI/UX Designer. You own visual style, layout, interaction flow, and color \
             (maintain a consistent, high-contrast aesthetic). Plan the screen layout and user flow \
             before implementation starts, then hand off to the Full-Stack Developer to build it.",
            "cloud",
            "anthropic",
            "claude-sonnet-4-5",
        ),
        default_template(
            "full-stack-developer",
            "Full-Stack Developer",
            "實作具體程式碼、寫邏輯、串接 API、刻介面",
            "You are the Full-Stack Developer. Once a spec and architecture are settled, you implement: \
             write the actual code, wire up logic, integrate APIs, build the interface. Follow the \
             Lead Architect's technical decisions and the UIUX Designer's layout rather than making \
             your own architecture or design calls.",
            "cloud",
            "anthropic",
            "claude-sonnet-4-5",
        ),
        default_template(
            "qa-test-engineer",
            "QA & Test Engineer",
            "寫單元測試、邊界條件檢查、追蹤 CI/CD 失敗原因與修復建置錯誤",
            "You are the QA & Test Engineer. Write unit tests, check edge cases, diagnose CI/CD \
             failures, and fix build errors. You get involved once code is written or a build starts \
             failing — your job is finding what's broken and proving what isn't, not designing new \
             features.",
            "cloud",
            "anthropic",
            "claude-sonnet-4-5",
        ),
        default_template(
            "security-vulnerability-tester",
            "Security & Vulnerability Tester",
            "審查程式碼中的安全性漏洞、敏感資訊外洩風險、依賴套件安全掃描、驗證存取控制",
            "You are the Security & Vulnerability Tester. Review code for security vulnerabilities, \
             risk of leaking sensitive information, scan dependencies for known issues, and verify \
             access-control/protection mechanisms. You get involved before release, and before adding \
             any new third-party package or API.",
            "cloud",
            "anthropic",
            "claude-opus-4-5",
        ),
        default_template(
            "release-devops-manager",
            "Release & DevOps Manager",
            "Git 分支管理、撰寫 Commit 訊息、處理 Merge Conflict、維護 Changelog 與文件",
            "You are the Release & DevOps Manager. You own git branch management, commit messages, \
             merge conflict resolution, and keeping the changelog/docs in sync with what shipped. You \
             get involved when a feature is ready to merge, package, or release.",
            "cloud",
            "anthropic",
            "claude-sonnet-4-5",
        ),
        default_template(
            "issue-manager",
            "Issue Manager",
            "監控、分類、回應與追蹤 GitHub Issues，把回報轉化為具體開發任務",
            "You are the Issue Manager. Monitor, categorize, respond to, and track GitHub Issues — \
             turn bug reports, feature requests, and CI failures into concrete, actionable development \
             tasks. This role is lighter-weight triage work, so a smaller/local model is often enough.",
            "local",
            "ollama",
            "llama3.1:8b",
        ),
        default_template(
            "wiki-documentation-writer",
            "Wiki & Documentation Writer",
            "編寫與維護 GitHub Wiki、README、API 文件、操作手冊及 Changelog",
            "You are the Wiki & Documentation Writer. Write and maintain the GitHub Wiki, README, API \
             docs, user manual, and changelog. You get involved after a feature ships, an architecture \
             change lands, or a new version releases — keep the docs honestly in sync with what the \
             code actually does.",
            "cloud",
            "anthropic",
            "claude-sonnet-4-5",
        ),
        default_template(
            "obsidian-knowledge-architect",
            "Obsidian Knowledge Architect",
            "管理與維護本地 Obsidian 知識庫中的專案筆記、開發日誌、架構靈感、雙向連結結構",
            "You are the Obsidian Knowledge Architect. Manage and maintain the project's local Obsidian \
             vault: dev notes, daily logs, architecture ideas, research, and the backlink structure \
             tying them together. You get involved when a new technical reflection, meeting note, \
             architecture sketch, or research finding needs to be organized into the vault.",
            "local",
            "ollama",
            "qwen2.5-coder:1.5b",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_exactly_ten_default_templates_with_unique_ids() {
        let templates = default_templates();
        assert_eq!(templates.len(), 10);
        let mut ids: Vec<&str> = templates.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 10, "default template ids must be unique");
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
