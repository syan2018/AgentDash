# Technical Design: Agent System Prompt 体系补全

## 变更概览

| 变更 | 类型 | 影响范围 |
|------|------|---------|
| R1: 重写 default_system_prompt.md | 纯文本 | connector prompt 文件 |
| R2: Environment Context Frame | 新 Rust module + preparation 改动 | runtime-session crate |
| R3: Companion 继承上下文 | request_assembler + companion composer 改动 | application crate |
| R4: Compaction handoff 提示 | compaction_context_frame 小改动 | runtime-session crate |

## R1: 重写 default_system_prompt.md

### 文件

`crates/agentdash-executor/src/connectors/pi_agent/prompts/default_system_prompt.md`

### 设计要点

- 身份行不绑定具体场景："You are a versatile AI agent built into AgentDash, capable of handling a wide range of tasks within the user's workspace."
- 保留 Core Principles 的 accuracy-first / minimal-footprint / respect-conventions 语义，但去掉 "code"、"file"、"compile" 等词
- 新增 Action Safety section，对齐 Claude Code 的 "Executing actions with care" 模式：可逆性判断 → blast radius → 确认
- 新增 Tool Usage section：先读后写、专用工具优先、并行调用、结果验证
- 新增 Output Style：简洁、一句话进度、不 emoji、跟随用户语言
- Communication section 精简，合并 Progress Updates 到 Output Style 中

### 不做的事

- 不在 base prompt 写具体工具名（这些由 capability_state_delta frame 动态注入）
- 不写 "coding agent" 相关指令（由各 preset 的 agent_system_prompt 叠加）
- 不写环境信息模板（由 R2 的 environment frame 承担）

## R2: Environment Context Frame

### 新文件

`crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs`

### 数据来源（preparation 阶段可用）

| 字段 | 来源 |
|------|------|
| date | `chrono::Utc::now()` + `chrono::Local::now()`（preparation 已用 `chrono::Utc::now()` 取 started_at_ms） |
| platform | `std::env::consts::OS` + `std::env::consts::ARCH` |
| model_id | `context.session.executor_config.model_id` |
| executor | `context.session.executor_config.executor` |
| working_directory | `context.session.working_directory` |
| connector_type | `deps.connector.connector_type()` |

### rendered_text 格式

```markdown
## Environment

- Date: 2026-07-01 (UTC)
- Platform: windows x86_64
- Model: claude-sonnet-4-20250514
- Executor: PI_AGENT
- Working directory: /workspace/project
```

### Frame 元数据

```rust
kind: "environment"
delivery_channel: "connector_context"
message_role: "system"
// delivery routing (via delivery_phase_for_kind / delivery_order_for_kind):
phase: SessionPolicy  // 需要在 hooks/mod.rs 注册
order: 15             // identity(10) 之后、system_guidelines(20) 之前
model_channel: System
cache_policy: SessionDigest  // 每 session 变一次
```

### 需要修改的文件

1. **新文件** `session/environment_context_frame.rs` — struct + ContextFramePayload impl
2. `session/launch/preparation.rs` — 在 `identity_frames` 和 `guidelines_frame` 之间构建 environment frame
3. `session/mod.rs` — 新增 mod 声明
4. `agentdash-spi/src/hooks/mod.rs` — 在 `delivery_order_for_kind` / `delivery_phase_for_kind` / `model_channel_for_kind` 中注册 `"environment"` kind

### model_channel 注册位置

`crates/agentdash-spi/src/hooks/mod.rs` 中的 `model_channel_for_kind` 和 `delivery_phase_for_kind` 等 match 分支需要新增 `"environment"` → `System` 和 `SessionPolicy`。

## R3: Companion 子代理上下文继承

### 方案

不改 `CompanionParentFacts.parent_context_bundle`（那条路径是整个 bundle 的完整传递，太重）。改为在 companion dispatch 阶段，从父级 session 提取一个轻量的 **`CompanionInheritedSummary`**，作为独立的 context fragment 注入子代理的 `compose_fragments`。

### 数据结构

```rust
// 新增 in frame_construction/request_assembler.rs 或独立文件
pub(crate) struct CompanionInheritedSummary {
    pub user_preferences: Vec<String>,         // 父级 user preferences（原样传递）
    pub environment_facts: String,             // 父级环境摘要（date/platform/model 一行）
    pub parent_assignment_hint: Option<String>, // 父级 assignment context 的 slot labels 摘要
}
```

### 注入方式

在 `CompanionChildDispatchService::dispatch_child`（`companion/tools.rs`）中，resolve 父级 capability state 后额外 resolve `CompanionInheritedSummary`，然后将其渲染为一个 `ContextFragment`：
- slot: `"inherited_parent_context"`
- content: 渲染为简洁 markdown（≤500 tokens）

这个 fragment 随 `dispatch_prompt` 一起进入子代理的 `compose_fragments`，最终被 `build_assignment_context_frame` 拾取并注入 Assignment 通道。

### 需要修改的文件

1. `crates/agentdash-application/src/companion/tools.rs` — dispatch_child 中增加 inherited summary 构建
2. `crates/agentdash-application/src/frame_construction/request_assembler.rs` — 新增 `resolve_companion_inherited_summary` 方法
3. `crates/agentdash-spi` 或 `agentdash-application-ports` — 可能需要扩展 `CompanionParentFactsProvider` trait 以获取 user_preferences

### 不做的事

- 不改 `parent_context_bundle` 流程（保持 None）
- 不传递父级对话历史
- 不传递父级 memory frame 内容（子代理有自己的 memory 发现）

## R4: Compaction Handoff 提示

### 文件

`crates/agentdash-application-runtime-session/src/session/compaction_context_frame.rs`

### 改动

在 `rendered_text()` 方法中，在 metadata 行和 summary body 之间插入固定 handoff 提示：

```rust
// 现有: lines.push(String::new()); + lines.push(self.summary.clone());
// 改为:
lines.push(String::new());
lines.push("以下是之前对话的压缩摘要，用于延续工作上下文。摘要中的路径、函数名等具体信息可能已过时，请在执行前验证。".to_string());
lines.push(String::new());
lines.push(self.summary.clone());
```

### 影响

- compaction_context_frame 的单测需要更新（rendered_text 格式变了）
- 不影响其他 frame 类型

## 兼容性与风险

| 风险 | 缓解 |
|------|------|
| 现有 preset agent 的 system_prompt 假设 base prompt 含 coding 措辞 | 检查所有 seed preset 的 agent_system_prompt，确保不依赖 base 中的 coding 指令 |
| environment frame 增加 system prompt token 消耗 | ~50 tokens，可忽略 |
| companion inherited summary 可能含敏感 user_preferences | 内容来自同一 project 的 settings，权限一致 |
| compaction handoff 提示可能干扰中文/英文混合场景 | 提示用中文写（与现有 compaction prompt 语言一致） |

## 实现顺序

1. R1 (default_system_prompt.md) — 零依赖，可独立提交
2. R4 (compaction handoff) — 一行改动，可与 R1 同提交
3. R2 (environment frame) — 需要新文件 + spi 注册
4. R3 (companion inherited) — 依赖 R2 的环境信息渲染逻辑复用
