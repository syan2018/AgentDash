//! Session 上下文 Bundle — 贯穿数据流终点的结构化 fragment 容器。
//!
//! Bundle 统一承载一个 session 在某个 phase / turn 下产出的所有 `ContextFragment`，
//! 按 scope 过滤提供给下游消费者（PiAgent F1 / title generator / summarizer / bridge replay /
//! audit bus）。Bundle 本身不做渲染决策；Agent 可见内容由 application 层的
//! ContextFrame builder 按 slot 白名单组装。
//!
//! 设计意图详见
//! `.trellis/tasks/04-29-session-context-builder-unification/prd.md`（"数据结构"章节）
//! 和 `.trellis/tasks/04-30-session-pipeline-architecture-refactor/target-architecture.md`
//! §C4（Bundle 主数据面）。

use uuid::Uuid;

use crate::context::injection::{ContextFragment, FragmentScope, MergeStrategy};

/// 一个 session 在某个 phase / turn 的上下文 fragment 集合。
///
/// - `bundle_id` 用于审计总线追踪 "同一次 build 产出的 fragment 同属一次 Bundle"。
/// - `phase_tag` 是对 application 层 `ContextBuildPhase` 的弱引用（SPI 不依赖 application 类型）。
/// - `bootstrap_fragments` 在组装期（`build_session_context_bundle`）产出，整个 session
///   生命周期内基本不变；已按 slot 去重合并。
#[derive(Debug, Clone)]
pub struct SessionContextBundle {
    pub bundle_id: Uuid,
    pub session_id: Uuid,
    /// application 层 `ContextBuildPhase` 的字符串标签（"project_agent" / "task_start" 等）。
    /// 保持 String 形态以避免 SPI 反向依赖 application。
    pub phase_tag: String,
    pub created_at_ms: u64,
    /// 组装期产出的 fragment（语义相对稳定，跨 turn 复用）。
    pub bootstrap_fragments: Vec<ContextFragment>,
}

impl SessionContextBundle {
    /// 新建一个空 Bundle（仅包含 bootstrap_fragments）。
    pub fn new(session_id: Uuid, phase_tag: impl Into<String>) -> Self {
        Self {
            bundle_id: Uuid::new_v4(),
            session_id,
            phase_tag: phase_tag.into(),
            created_at_ms: now_millis_u64(),
            bootstrap_fragments: Vec::new(),
        }
    }

    /// 返回 bootstrap fragment 迭代器。
    pub fn iter_fragments(&self) -> impl Iterator<Item = &ContextFragment> {
        self.bootstrap_fragments.iter()
    }

    /// 按 scope 过滤 fragment 迭代器。
    pub fn filter_for(&self, scope: FragmentScope) -> impl Iterator<Item = &ContextFragment> {
        self.iter_fragments()
            .filter(move |fragment| fragment.scope.contains(scope))
    }

    /// 按 slot 唯一性合并一个 fragment 到 `bootstrap_fragments`。
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
    ///
    /// 运行期 Hook 注入审计不再挂在 Bundle；请使用 session runtime 的独立注入存储。
    pub fn upsert_by_slot(&mut self, fragment: ContextFragment) {
        if let Some(existing) = self
            .bootstrap_fragments
            .iter_mut()
            .find(|f| f.slot == fragment.slot)
        {
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
            self.bootstrap_fragments.push(fragment);
        }
    }

    /// 批量合并多个 fragment 到 `bootstrap_fragments`（逐个走 `upsert_by_slot`）。
    pub fn merge(&mut self, others: impl IntoIterator<Item = ContextFragment>) {
        for fragment in others {
            self.upsert_by_slot(fragment);
        }
    }

    /// 不做合并地追加 fragment 到 `bootstrap_fragments` —— 用于需要保留多条同 slot 条目的场景。
    pub fn push_raw(&mut self, fragment: ContextFragment) {
        self.bootstrap_fragments.push(fragment);
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
    use crate::context::injection::FragmentScopeSet;

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

        assert_eq!(bundle.bootstrap_fragments.len(), 1);
        let merged = &bundle.bootstrap_fragments[0];
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

        assert_eq!(bundle.bootstrap_fragments.len(), 1);
        assert_eq!(bundle.bootstrap_fragments[0].content, "second");
        assert_eq!(bundle.bootstrap_fragments[0].order, 5);
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

    #[test]
    fn filter_for_reads_bootstrap_fragments() {
        let session = Uuid::new_v4();
        let mut bundle = SessionContextBundle::new(session, "per_turn");
        bundle.push_raw(frag("task", 10, "bootstrap"));
        bundle.push_raw(frag("task", 50, "runtime"));

        let all: Vec<_> = bundle.filter_for(FragmentScope::RuntimeAgent).collect();
        assert_eq!(all.len(), 2);
    }
}
