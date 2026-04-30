//! Session 上下文 Bundle — 贯穿数据流终点的结构化 fragment 容器。
//!
//! Bundle 统一承载一个 session 在某个 phase / turn 下产出的所有 `ContextFragment`，
//! 按 scope 过滤提供给下游消费者（PiAgent F1 / title generator / summarizer / bridge replay /
//! audit bus）。Bundle 本身不做渲染决策；具体的系统 prompt 结构由 PiAgent connector
//! 通过 `render_section(scope, &[slot])` 按 slot 白名单组装。
//!
//! 设计意图详见
//! `.trellis/tasks/04-29-session-context-builder-unification/prd.md`（"数据结构"章节）
//! 和 `.trellis/tasks/04-30-session-pipeline-architecture-refactor/target-architecture.md`
//! §C4（bootstrap/turn_delta 双字段）。

use uuid::Uuid;

use crate::context_injection::{ContextFragment, FragmentScope, MergeStrategy};

/// 一个 session 在某个 phase / turn 的上下文 fragment 集合。
///
/// - `bundle_id` 用于审计总线追踪 "同一次 build 产出的 fragment 同属一次 Bundle"。
/// - `phase_tag` 是对 application 层 `ContextBuildPhase` 的弱引用（SPI 不依赖 application 类型）。
/// - `bootstrap_fragments` 在组装期（`build_session_context_bundle`）产出，整个 session
///   生命周期内基本不变；已按 slot 去重合并。
/// - `turn_delta` 每轮 prompt 重建，承载运行期 Hook 注入的 per-turn 增量。Inspector
///   可同时看到 bootstrap 与 turn_delta 两类 fragment，便于审计"静态上下文 vs 动态补充"。
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
    /// 运行期 per-turn 追加的 fragment（Hook bundle_delta 回灌 / 热更新注入）。
    pub turn_delta: Vec<ContextFragment>,
}

impl SessionContextBundle {
    /// 新建一个空 Bundle（bootstrap_fragments + turn_delta 均为空）。
    pub fn new(session_id: Uuid, phase_tag: impl Into<String>) -> Self {
        Self {
            bundle_id: Uuid::new_v4(),
            session_id,
            phase_tag: phase_tag.into(),
            created_at_ms: now_millis_u64(),
            bootstrap_fragments: Vec::new(),
            turn_delta: Vec::new(),
        }
    }

    /// 返回 bootstrap + turn_delta 合并后的 fragment 迭代器。
    ///
    /// bootstrap 在前、turn_delta 在后，便于调用方按物理位置识别来源。
    /// 需要区分来源时请直接访问 `bootstrap_fragments` / `turn_delta` 字段。
    pub fn iter_fragments(&self) -> impl Iterator<Item = &ContextFragment> {
        self.bootstrap_fragments
            .iter()
            .chain(self.turn_delta.iter())
    }

    /// 按 scope 过滤 fragment 迭代器（bootstrap + turn_delta，不消耗 Bundle）。
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
    /// 运行期 per-turn 增量请使用 `push_turn_delta` / `extend_turn_delta`。
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

    /// 向 `turn_delta` 追加一个 per-turn 增量 fragment。
    ///
    /// 与 `upsert_by_slot` 的 bootstrap 去重语义不同，`turn_delta` 保留多条同 slot 条目，
    /// 渲染时由 `render_section` 按 order 合并。设计意图：让 Hook 在同一轮内多次
    /// 回灌同一 slot（例如连续追加 `workflow_context` 提示）时不互相覆盖。
    pub fn push_turn_delta(&mut self, fragment: ContextFragment) {
        self.turn_delta.push(fragment);
    }

    /// 批量追加 per-turn 增量 fragment。
    pub fn extend_turn_delta(&mut self, fragments: impl IntoIterator<Item = ContextFragment>) {
        self.turn_delta.extend(fragments);
    }

    /// 按 scope 过滤、按 `slots` 白名单拼接 Markdown section。
    ///
    /// 规则：
    /// 1. 先按 scope 过滤（bootstrap + turn_delta 同规则处理）；
    /// 2. 再按 `slots` 白名单保留（保留顺序按 `slots` 参数的顺序，而不是 fragment 的 order）；
    /// 3. 同一 slot 内部合并 bootstrap + turn_delta 全部 fragment，按 `order` 升序、用 `\n\n` 拼接；
    /// 4. 空 content 的 fragment 跳过。
    ///
    /// 调用方通常是 PiAgent connector 的 `build_runtime_system_prompt`，通过指定
    /// `["task", "story", "project", ...]` 这样的 slot 白名单控制 section 内的排序。
    pub fn render_section(&self, scope: FragmentScope, slots: &[&str]) -> String {
        let mut sections: Vec<String> = Vec::with_capacity(slots.len());
        for slot_name in slots {
            let mut slot_fragments: Vec<&ContextFragment> = self
                .iter_fragments()
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
    fn turn_delta_appended_and_rendered_after_bootstrap() {
        let session = Uuid::new_v4();
        let mut bundle = SessionContextBundle::new(session, "task_start");
        bundle.push_raw(frag("task", 10, "bootstrap-task"));
        bundle.push_turn_delta(frag("task", 50, "runtime-task"));

        assert_eq!(bundle.bootstrap_fragments.len(), 1);
        assert_eq!(bundle.turn_delta.len(), 1);

        // render_section 按 order 合并两路 fragment：bootstrap(10) 在前、turn_delta(50) 在后
        let rendered = bundle.render_section(FragmentScope::RuntimeAgent, &["task"]);
        let bootstrap_pos = rendered.find("bootstrap-task").unwrap();
        let runtime_pos = rendered.find("runtime-task").unwrap();
        assert!(bootstrap_pos < runtime_pos);
    }

    #[test]
    fn turn_delta_extends_batch() {
        let session = Uuid::new_v4();
        let mut bundle = SessionContextBundle::new(session, "task_start");
        bundle.extend_turn_delta([
            frag("workflow_context", 100, "step-1"),
            frag("workflow_context", 110, "step-2"),
        ]);

        assert_eq!(bundle.turn_delta.len(), 2);
        // Bootstrap 保持空，turn_delta 按插入顺序保留（允许同 slot 多条）
        assert!(bundle.bootstrap_fragments.is_empty());
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

    #[test]
    fn filter_for_includes_turn_delta() {
        let session = Uuid::new_v4();
        let mut bundle = SessionContextBundle::new(session, "per_turn");
        bundle.push_raw(frag("task", 10, "bootstrap"));
        bundle.push_turn_delta(frag("task", 50, "runtime"));

        let all: Vec<_> = bundle.filter_for(FragmentScope::RuntimeAgent).collect();
        assert_eq!(all.len(), 2);
    }
}
