//! Session ID 语义化类型别名。
//!
//! Model C 下的会话拓扑（见 `.trellis/spec/backend/story-task-runtime.md`）：
//!
//! ```text
//! Story session (root, durable)
//!   ├─ LifecycleRun                    # 1:1 挂在 Story session 上
//!   │   └─ LifecycleStepState          # step 可选绑定独立 child session
//!   └─ child session(s)                # companion / step 远程执行等
//! ```
//!
//! 这里提供三个**纯语义**别名，在类型签名层面区分"哪条 session 被引用":
//!
//! - [`SessionId`] —— 通用会话 id（任意 kind / 任意层级）。
//! - [`StorySessionId`] —— 对应 Story aggregate 的 root session（durable，1:1 绑定 Story）。
//! - [`ChildSessionId`] —— 挂在 Story session 下的子 session（companion / lifecycle step
//!   agent node / 其他派生会话）。
//!
//! ## 设计决策：为什么用 type alias 而不是 newtype
//!
//! - **零运行时代价**：别名与 `String` 完全同构，serde / sqlx / 跨 crate 传参无需 wrapper。
//! - **不破坏 wire format**：JSON / DB 列 / 前端消费字段保持字符串不变。
//! - **语义仍然传达**：字段与函数签名读作 `StorySessionId` 即能立刻看出语义归属。
//!
//! Newtype 会带来大量 `.0` / `Deref` 样板，但无法阻止随意从 `String` 构造——因为任何
//! 会话实际就是一个字符串 ID，没有"不变量"可守护。故此选择轻量的 type alias。
//!
//! ## 使用规则
//!
//! - Domain 层定义字段时优先使用语义化别名；无法判定语义时再退回 [`SessionId`]。
//! - API / DTO / JSON 字段命名不动（如 `parent_session_id: string` 保持）。
//! - 下游若需要 borrow，写 `&StorySessionId`；需要转所有权，写 `StorySessionId`。
//!
//! 相关规范：
//! - [`.trellis/spec/backend/story-task-runtime.md`] §2.2 Story session / §2.5 child
//!   session / §3 关系拓扑 / §4 label 规范。

/// 通用会话标识。
///
/// 语义上等价于 `String`，表达"某个 SessionHub session 的 ID"。在无法确定
/// 该 session 属于哪一层（root / child）时使用。
pub type SessionId = String;

/// Story 根 session 的 ID。
///
/// Model C 下 Story 与 Story session 1:1 绑定（见
/// `.trellis/spec/backend/story-task-runtime.md` §2.2）。`LifecycleRun` 挂在
/// 此 session 上，LifecycleRun.session_id 就是它。
///
/// 与 [`SessionId`] 物理类型相同，仅在签名层面表达"这是 Story 的 root session"。
pub type StorySessionId = SessionId;

/// Story 下派生的子 session ID（companion / lifecycle step 的独立 agent node 等）。
///
/// 派生 session 通过 `SessionBinding(owner_type=Story, label=...)` 关联回 Story
/// aggregate（见 `.trellis/spec/backend/story-task-runtime.md` §2.5 / §4）。
///
/// 与 [`SessionId`] 物理类型相同，仅在签名层面表达"这是 Story 下的 child session"。
pub type ChildSessionId = SessionId;
