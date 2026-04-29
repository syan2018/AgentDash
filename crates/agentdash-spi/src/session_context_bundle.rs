//! Session 上下文 Bundle — 贯穿数据流终点的结构化 fragment 容器。
//!
//! Bundle 统一承载一个 session 在某个 phase / turn 下产出的所有 `ContextFragment`，
//! 按 scope 过滤提供给下游消费者（PiAgent F1 / title generator / summarizer / bridge replay /
//! audit bus）。Bundle 本身不做渲染决策；具体的系统 prompt 结构由 PiAgent connector
//! 通过 `render_section(scope, &[slot])` 按 slot 白名单组装。
//!
//! 设计意图详见
//! `.trellis/tasks/04-29-session-context-builder-unification/prd.md`（"数据结构"章节）。

use uuid::Uuid;

use crate::context_injection::{ContextFragment, FragmentScope, MergeStrategy};

/// 一个 session 在某个 phase / turn 的上下文 fragment 集合。
///
/// - `bundle_id` 用于审计总线追踪 "同一次 build 产出的 fragment 同属一次 Bundle"。
/// - `phase_tag` 是对 application 层 `ContextBuildPhase` 的弱引用（SPI 不依赖 application 类型）。
/// - `fragments` 在任意时刻都已按 slot 去重合并完毕，调用方拿到就可以直接消费。
#[derive(Debug, Clone)]
pub struct SessionContextBundle {
    pub bundle_id: Uuid,
    pub session_id: Uuid,
    /// application 层 `ContextBuildPhase` 的字符串标签（"project_agent" / "task_start" 等）。
    /// 保持 String 形态以避免 SPI 反向依赖 application。
    pub phase_tag: String,
    pub created_at_ms: u64,
    pub fragments: Vec<ContextFragment>,
}

impl SessionContextBundle {
    /// 新建一个空 Bundle。
    pub fn new(session_id: Uuid, phase_tag: impl Into<String>) -> Self {
        Self {
            bundle_id: Uuid::new_v4(),
            session_id,
            phase_tag: phase_tag.into(),
            created_at_ms: now_millis_u64(),
            fragments: Vec::new(),
        }
    }

    /// 按 scope 过滤 fragment 迭代器（不消耗 Bundle）。
    pub fn filter_for(&self, scope: FragmentScope) -> impl Iterator<Item = &ContextFragment> {
        self.fragments
            .iter()
            .filter(move |fragment| fragment.scope.contains(scope))
    }

    /// 按 slot 唯一性合并一个 fragment。
    ///
    /// 行为：
    /// - 若 slot 首次出现 → 直接插入；
    /// - 若 slot 已存在 → 按新 fragment 的 `MergeStrategy` 决定：
    ///   - `Append`：在已有 fragment 之后追加（content 用 `\n\n` 连接、scope 与 source 取并集 / 合并）；
    ///   - `Override`：保留 label/source 信息但整体替换 content、order 取最小、scope 并集。
    ///
    /// 注意：这里的"slot 唯一"指"同一 slot 不会被多个来源重复塞同样的占位内容"，
    /// 承接去重语义。如果调用方需要严格 append 语义（例如 `workflow_context` 多个 binding），
    /// 应使用不同 order 的独立 slot 或 `push_raw` 接口保留多个条目。
    pub fn upsert_by_slot(&mut self, fragment: ContextFragment) {
        if let Some(existing) = self.fragments.iter_mut().find(|f| f.slot == fragment.slot) {
            match fragment.strategy {
                MergeStrategy::Append => {
                    if !existing.content.trim().is_empty() && !fragment.content.trim().is_empty() {
                        existing.content.push_str("\n\n");
                    }
                    existing.content.push_str(&fragment.content);
                    existing.scope |= fragment.scope;
                    if !fragment.source.is_empty() && existing.source != fragment.source {
                        existing.source = format!("{}+{}", existing.source, fragment.source);
                    }
                }
                MergeStrategy::Override => {
                    existing.content = fragment.content;
                    existing.label = fragment.label;
                    existing.source = fragment.source;
                    existing.order = existing.order.min(fragment.order);
                    existing.strategy = MergeStrategy::Override;
                    existing.scope |= fragment.scope;
                }
            }
        } else {
            self.fragments.push(fragment);
        }
    }

    /// 批量合并多个 fragment（逐个走 `upsert_by_slot`）。
    pub fn merge(&mut self, others: impl IntoIterator<Item = ContextFragment>) {
        for fragment in others {
            self.upsert_by_slot(fragment);
        }
    }

    /// 不做合并地追加 fragment —— 用于需要保留多条同 slot 条目的场景（例如 workflow 多 binding）。
    pub fn push_raw(&mut self, fragment: ContextFragment) {
        self.fragments.push(fragment);
    }

    /// 按 scope 过滤、按 `slots` 白名单拼接 Markdown section。
    ///
    /// 规则：
    /// 1. 先按 scope 过滤；
    /// 2. 再按 `slots` 白名单保留（保留顺序按 `slots` 参数的顺序，而不是 fragment 的 order）；
    /// 3. 同一 slot 内部多 fragment 按 `order` 升序、用 `\n\n` 拼接；
    /// 4. 空 content 的 fragment 跳过。
    ///
    /// 调用方通常是 PiAgent connector 的 `build_runtime_system_prompt`，通过指定
    /// `["task", "story", "project", ...]` 这样的 slot 白名单控制 section 内的排序。
    pub fn render_section(&self, scope: FragmentScope, slots: &[&str]) -> String {
        let mut sections: Vec<String> = Vec::with_capacity(slots.len());
        for slot_name in slots {
            let mut slot_fragments: Vec<&ContextFragment> = self
                .fragments
                .iter()
                .filter(|fragment| fragment.scope.contains(scope))
                .filter(|fragment| fragment.slot == *slot_name)
                .collect();
            slot_fragments.sort_by_key(|fragment| fragment.order);
            let merged = slot_fragments
                .into_iter()
                .map(|fragment| fragment.content.clone())
                .filter(|content| !content.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            if !merged.trim().is_empty() {
                sections.push(merged);
            }
        }
        sections.join("\n\n")
    }
}

fn now_millis_u64() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dur| dur.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_injection::FragmentScopeSet;

    fn frag(slot: &str, order: i32, content: &str) -> ContextFragment {
        ContextFragment {
            slot: slot.to_string(),
            label: format!("label_{slot}"),
            order,
            strategy: MergeStrategy::Append,
            scope: ContextFragment::default_scope(),
            source: "test".to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn upsert_append_merges_same_slot_content() {
        let session = Uuid::new_v4();
        let mut bundle = SessionContextBundle::new(session, "task_start");
        bundle.upsert_by_slot(frag("task", 10, "alpha"));
        bundle.upsert_by_slot(frag("task", 20, "beta"));

        assert_eq!(bundle.fragments.len(), 1);
        let merged = &bundle.fragments[0];
        assert!(merged.content.contains("alpha"));
        assert!(merged.content.contains("beta"));
    }

    #[test]
    fn upsert_override_replaces_content() {
        let session = Uuid::new_v4();
        let mut bundle = SessionContextBundle::new(session, "task_start");
        bundle.upsert_by_slot(frag("instruction", 10, "first"));
        let mut second = frag("instruction", 5, "second");
        second.strategy = MergeStrategy::Override;
        bundle.upsert_by_slot(second);

        assert_eq!(bundle.fragments.len(), 1);
        assert_eq!(bundle.fragments[0].content, "second");
        assert_eq!(bundle.fragments[0].order, 5);
    }

    #[test]
    fn render_section_orders_within_slot_by_order_field() {
        let session = Uuid::new_v4();
        let mut bundle = SessionContextBundle::new(session, "story_owner");
        bundle.push_raw(frag("workflow_context", 20, "second"));
        bundle.push_raw(frag("workflow_context", 10, "first"));
        bundle.push_raw(frag("task", 5, "task-body"));

        let rendered =
            bundle.render_section(FragmentScope::RuntimeAgent, &["task", "workflow_context"]);
        let task_pos = rendered.find("task-body").unwrap();
        let first_pos = rendered.find("first").unwrap();
        let second_pos = rendered.find("second").unwrap();
        assert!(task_pos < first_pos);
        assert!(first_pos < second_pos);
    }

    #[test]
    fn filter_for_respects_scope() {
        let session = Uuid::new_v4();
        let mut bundle = SessionContextBundle::new(session, "title_gen");

        let mut runtime_only = frag("task", 10, "runtime-only");
        runtime_only.scope = FragmentScopeSet::only(FragmentScope::RuntimeAgent);
        let mut title_only = frag("story", 10, "title-only");
        title_only.scope = FragmentScopeSet::only(FragmentScope::TitleGen);

        bundle.push_raw(runtime_only);
        bundle.push_raw(title_only);

        let title_visible: Vec<_> = bundle.filter_for(FragmentScope::TitleGen).collect();
        assert_eq!(title_visible.len(), 1);
        assert_eq!(title_visible[0].content, "title-only");
    }
}
