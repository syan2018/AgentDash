//! Session ID 语义化类型别名。
//!
//! 会话拓扑（见 `.trellis/spec/backend/story-task-runtime.md`）：
//!
//! - **Session** 降级为 runtime substrate（event log, debug replay），不再承载业务归属语义。
//! - **LifecycleRun** 通过 `LifecycleRunLink` 与 Story/Task/RoutineExecution 等业务对象关联。
//! - 下方类型别名仅为向后兼容保留；新代码优先使用 `String` / `Option<String>`。
//!
//! ## 使用规则
//!
//! - `SessionId` 和 `ChildSessionId` 仍可在 runtime session 上下文中使用。
//! - `StorySessionId` 已降级：LifecycleRun.session_id 现为 `Option<String>`（runtime association）。
//!   业务归属请通过 `LifecycleRunLink` 查询，不要依赖 session_id 推断 Story 归属。

/// 通用会话标识。
///
/// 语义上等价于 `String`，表达"某个 SessionHub session 的 ID"。
pub type SessionId = String;

/// Story runtime session 的 ID。
///
/// 历史别名，保留仅为向后兼容。新代码请直接使用 `String` / `Option<String>`。
/// Story 业务归属通过 `LifecycleRunLink(subject_kind=Story)` 查询。
#[deprecated(note = "use plain String; Story ownership is via LifecycleRunLink, not session_id")]
pub type StorySessionId = SessionId;

/// Story 下派生的子 session ID（companion / lifecycle step 的独立 agent node 等）。
///
/// 派生 session 通过 `SessionBinding(owner_type=Story, label=...)` 关联回 Story
/// aggregate，仅用于 runtime 追踪。
pub type ChildSessionId = SessionId;
