//! Context fragment slot 的默认 order 集中常量（PR 5c）。
//!
//! 历史上 slot order 数字（10/20/30/35/36/37/38/40/48/49/50/60/80/82/83/84/85/
//! 86/89/90/96/100/200）直接硬编码在各 contributor 内；`HOOK_SLOT_ORDERS`
//! 又在 `hooks/fragment_bridge.rs` 独立维护了一份 hook→order 的小映射表。
//!
//! 本模块把这些 order 提升为 named constants，以便：
//! - 阅读代码时能从常量名理解 fragment 的相对位置；
//! - Hook bridge 与 contributor 共享同一来源，避免 contributor 调 order 后
//!   hook fragment 意外插入"错误位置"；
//! - 将来新增 slot 时只需在此表声明一条。
//!
//! **约束**：本模块只是"默认值"的常量表，contributor 内部仍可传入 base_order
//! 计算 per-entry 的 order（例如 declared_sources 的 `base_order + index`）。
//! 即改了这些常量也不会影响依赖 base_order 的 fragment 间相对位置。

// ─── Task / Owner core context ─────────────────────────────
pub const TASK_CORE: i32 = 10;
pub const STORY_CORE: i32 = 20;
pub const PROJECT_CORE_OWNER_PROJECT: i32 = 10; // Project owner 路径 project_core
pub const PROJECT_AGENT_IDENTITY: i32 = 20; // Project owner 路径 agent_identity
pub const PROJECT_CORE_STORY_OWNER: i32 = 20; // Story owner 路径 project_core
pub const PROJECT_IN_TASK: i32 = 40; // Task 路径 project slot
pub const WORKSPACE_OWNER: i32 = 30; // Owner 视图 workspace slot
pub const WORKSPACE_TASK: i32 = 50; // Task 视图 workspace slot

// ─── SessionPlan fragments (build_session_plan_fragments) ────
pub const SESSION_PLAN_VFS: i32 = 35;
pub const SESSION_PLAN_TOOLS: i32 = 36;
pub const SESSION_PLAN_PERSONA: i32 = 37;
pub const SESSION_PLAN_REQUIRED_CONTEXT_BASE: i32 = 38; // base + index
pub const SESSION_PLAN_WORKFLOW: i32 = 48;
pub const SESSION_PLAN_RUNTIME_POLICY: i32 = 49;

// ─── Task binding / declared sources / MCP ───────────────────
pub const BINDING_INITIAL_CONTEXT: i32 = 80;
pub const DECLARED_SOURCES_BASE_STORY: i32 = 50; // Story context path
pub const DECLARED_SOURCES_BASE_TASK: i32 = 82; // Task path
pub const DECLARED_SOURCES_WARNINGS_STORY: i32 = 59;
pub const DECLARED_SOURCES_WARNINGS_TASK: i32 = 89;
pub const WORKSPACE_SOURCES_BASE: i32 = 86; // File / ProjectSnapshot 解析结果
pub const WORKSPACE_SOURCES_WARNINGS: i32 = 96;
pub const STORY_WORKSPACE_WARNINGS: i32 = 69;

pub const MCP_CONFIG: i32 = 85;

// ─── Workflow / Lifecycle ────────────────────────────────────
pub const WORKFLOW_PROJECTION_SNAPSHOT: i32 = 83;
pub const WORKFLOW_CONTEXT_BINDING_BASE: i32 = 84; // base + index
pub const WORKFLOW_CONTEXT_WARNINGS: i32 = 89;
pub const LIFECYCLE_NODE_CONTEXT: i32 = 80;
pub const LIFECYCLE_WORKFLOW_INJECTION: i32 = 83;
pub const LIFECYCLE_RUNTIME_POLICY: i32 = 84;

// ─── Instruction ─────────────────────────────────────────────
pub const INSTRUCTION: i32 = 90;
pub const INSTRUCTION_ADDITIONAL: i32 = 100;

// ─── Hook injection slot → order 映射（fragment_bridge 引用） ──
//
// Hook injection slot 转 ContextFragment 时的默认 order。必须与上方 contributor
// 常量保持语义一致：`workflow` hook fragment (83) 与 `workflow_projection_snapshot`
// (83) / `lifecycle_workflow_injection` (83) 对齐，确保 hook 注入的 workflow
// fragment 与 contributor 产出的 workflow fragment 在 bundle 排序中位于同一区段。
pub const HOOK_COMPANION_AGENTS: i32 = 60;
pub const HOOK_WORKFLOW: i32 = WORKFLOW_PROJECTION_SNAPSHOT; // = 83
pub const HOOK_CONSTRAINT: i32 = LIFECYCLE_RUNTIME_POLICY; // = 84
pub const HOOK_DEFAULT: i32 = 200;
