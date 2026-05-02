//! Workflow Injection 渲染共享 helper（PR 5a）。
//!
//! 历史上 workflow `goal` / `instructions` / `context_bindings` 的 markdown 渲染
//! 散落在三处：
//! - `contribute_workflow_binding`（task 路径 / workflow_bindings.rs）
//! - `contribute_lifecycle_context`（lifecycle node 路径 / assembler.rs:1464+）
//! - `compose_companion_with_workflow`（companion+workflow 路径 / assembler.rs:1733+）
//!
//! 每次新增 `WorkflowInjectionSpec` 字段都要改三处；本模块收敛这部分纯渲染逻辑，
//! 三个调用点共用同一套文本模板。
//!
//! 仅包含**纯 markdown 拼接**，不产出 `ContextFragment`（fragment 包装由调用方按
//! 各自场景的 slot/label/order/source 决定）。

use agentdash_domain::workflow::{WorkflowContextBinding, WorkflowInjectionSpec};

use crate::vfs::{ResolveBindingsOutput, ResolvedBinding};

/// 渲染模式 —— 决定声明式 bindings 列表是否一并产出。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowInjectionMode {
    /// 仅渲染 `goal` + `instructions`。
    ///
    /// 对应 companion+workflow 路径：companion agent 只需要 workflow 的目标和说明，
    /// 不需要 bindings 清单（companion 本身不走 context binding 解析）。
    SummaryOnly,
    /// 渲染 `goal` + `instructions` + declarative `context_bindings` 列表。
    ///
    /// 对应 lifecycle node 路径：lifecycle agent 需要知道 workflow 声明了哪些
    /// context bindings（即便当前 node 没有主动解析它们），以便引用。
    Declarative,
}

/// 把 `WorkflowInjectionSpec` 渲染为整段 markdown 正文。
///
/// 若所有可选字段都为空，返回 `None`。
pub fn render_workflow_injection(
    injection: &WorkflowInjectionSpec,
    mode: WorkflowInjectionMode,
) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    if let Some(goal) = clean_text(injection.goal.as_deref()) {
        parts.push(format!("## Workflow Goal\n{goal}"));
    }
    if !injection.instructions.is_empty() {
        let lines: Vec<String> = injection
            .instructions
            .iter()
            .filter_map(|item| {
                let trimmed = item.trim();
                (!trimmed.is_empty()).then(|| format!("- {trimmed}"))
            })
            .collect();
        if !lines.is_empty() {
            parts.push(format!("## Workflow Instructions\n{}", lines.join("\n")));
        }
    }
    if matches!(mode, WorkflowInjectionMode::Declarative) && !injection.context_bindings.is_empty()
    {
        let lines = injection
            .context_bindings
            .iter()
            .map(render_declarative_binding_line)
            .collect::<Vec<_>>()
            .join("\n");
        parts.push(format!("## Workflow Context Bindings\n{lines}"));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

/// 渲染单条 resolved binding 的 section 正文（供 task 路径构造独立 fragment）。
///
/// 返回 None 表示该 binding 的正文为空（跳过 fragment 产出以避免垃圾条目）。
pub fn render_resolved_binding_section(binding: &ResolvedBinding) -> Option<String> {
    let body = binding.content.trim();
    if body.is_empty() {
        return None;
    }
    let heading = binding
        .title
        .as_deref()
        .and_then(|v| {
            let t = v.trim();
            if t.is_empty() { None } else { Some(t) }
        })
        .unwrap_or_else(|| {
            let reason = binding.reason.trim();
            if reason.is_empty() {
                binding.locator.as_str()
            } else {
                reason
            }
        });
    Some(format!(
        "## {}\n- locator: `{}`\n- reason: {}\n\n{}",
        heading, binding.locator, binding.reason, body
    ))
}

/// 渲染 resolved bindings 的 warnings 段（供 task 路径拼装 warning fragment）。
///
/// 返回 None 表示无 warnings。
pub fn render_resolved_binding_warnings(resolved: &ResolveBindingsOutput) -> Option<String> {
    if resolved.warnings.is_empty() {
        return None;
    }
    Some(format!(
        "## Workflow Binding Warnings\n{}",
        resolved
            .warnings
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

fn render_declarative_binding_line(binding: &WorkflowContextBinding) -> String {
    let title = binding
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(binding.locator.as_str());
    let required = if binding.required {
        "required"
    } else {
        "optional"
    };
    format!(
        "- `{}` ({required}) — {}: {}",
        binding.locator, title, binding.reason
    )
}

fn clean_text(input: Option<&str>) -> Option<&str> {
    input.and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::{WorkflowContextBinding, WorkflowInjectionSpec};

    fn spec_full() -> WorkflowInjectionSpec {
        WorkflowInjectionSpec {
            goal: Some("实现 x 能力".to_string()),
            instructions: vec!["先读 spec".to_string(), "再改代码".to_string()],
            context_bindings: vec![WorkflowContextBinding {
                locator: ".trellis/workflow.md".to_string(),
                reason: "workflow 规则".to_string(),
                required: true,
                title: Some("Workflow 规则".to_string()),
            }],
        }
    }

    #[test]
    fn summary_only_excludes_bindings() {
        let out =
            render_workflow_injection(&spec_full(), WorkflowInjectionMode::SummaryOnly).unwrap();
        assert!(out.contains("## Workflow Goal"));
        assert!(out.contains("## Workflow Instructions"));
        assert!(!out.contains("## Workflow Context Bindings"));
    }

    #[test]
    fn declarative_includes_bindings_list() {
        let out =
            render_workflow_injection(&spec_full(), WorkflowInjectionMode::Declarative).unwrap();
        assert!(out.contains("## Workflow Context Bindings"));
        assert!(out.contains("`.trellis/workflow.md` (required) — Workflow 规则: workflow 规则"));
    }

    #[test]
    fn empty_spec_returns_none() {
        let out = render_workflow_injection(
            &WorkflowInjectionSpec::default(),
            WorkflowInjectionMode::Declarative,
        );
        assert!(out.is_none());
    }

    #[test]
    fn whitespace_only_goal_ignored() {
        let spec = WorkflowInjectionSpec {
            goal: Some("   ".to_string()),
            ..WorkflowInjectionSpec::default()
        };
        let out = render_workflow_injection(&spec, WorkflowInjectionMode::SummaryOnly);
        assert!(out.is_none());
    }

    #[test]
    fn resolved_binding_section_falls_back_to_reason() {
        let section = render_resolved_binding_section(&ResolvedBinding {
            locator: "a/b.md".to_string(),
            title: None,
            reason: "规则".to_string(),
            content: "body".to_string(),
        })
        .unwrap();
        assert!(section.starts_with("## 规则"));
        assert!(section.contains("locator: `a/b.md`"));
        assert!(section.ends_with("body"));
    }

    #[test]
    fn resolved_binding_section_empty_body_skipped() {
        let section = render_resolved_binding_section(&ResolvedBinding {
            locator: "a".to_string(),
            title: Some("t".to_string()),
            reason: "r".to_string(),
            content: "   ".to_string(),
        });
        assert!(section.is_none());
    }
}
