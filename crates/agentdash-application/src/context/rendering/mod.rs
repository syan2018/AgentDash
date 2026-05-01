//! Context rendering helpers —— 跨 contributor / compose_* 共享的 markdown 渲染逻辑。
//!
//! 所有函数都是纯文本拼接，**不**产出 `ContextFragment`；fragment 包装（slot /
//! label / order / source / scope）由调用方按场景决定。集中这些 helper 的目的
//! 是消除同一 section 在多个产出点被独立重写带来的 drift 风险。

pub mod declared_sources;
pub mod workflow_injection;

pub use declared_sources::{
    display_source_label, fragment_label, fragment_slot, render_source_section, truncate_text,
};
pub use workflow_injection::{
    WorkflowInjectionMode, render_resolved_binding_section, render_resolved_binding_warnings,
    render_workflow_injection,
};
