# ContextFrame 全链路投递与 Bundle 收束

## 目标

将 `ContextFrame` 从"ContextFrame builder 产出 → prompt_pipeline 持久化 → hook notice 队列消费"的单一投递路径，升级为 Connector 可直接消费的全链路主线。同时把 `SessionContextBundle` 从公共消费面退出，只保留为 compose 期内部中间工具。

## 动机与前序

前序任务 `05-09-contextframe-injection-path-consolidation` 已将 Agent 可见上下文统一到 `ContextFrame` taxonomy（identity / mission_context / capability / pending_action / auto_resume / compaction_summary 等），但架构仍残留两条旧路径：

1. **rendered_system_prompt**：只承载 Identity，但通用 connector（executor_session / codex_bridge）将其作为唯一上下文注入点——ContextFrame 的动态内容对这些 connector 完全不可达。
2. **SessionContextBundle**：作为 fragment 数据源喂给 ContextFrame builder（`mission_context_frame.rs` 调 `bundle.render_section`），本身并不直接被 Agent 消费。实际数据流已经是 `contribute_* → Bundle → ContextFrame → Agent`，Bundle 退化为中间缓存层。

本任务让 Connector 自行决定消费策略，彻底消除 `rendered_system_prompt` 和 Bundle 的公共消费面残留。

## 核心设计原则

- **Connector 自行决定消费策略**：prompt_pipeline 不替 connector 做投递决策
- **ContextFrame 是 Agent 可见上下文的唯一投递单位**：Identity 也是一个 ContextFrame
- **Bundle 退化为可选的 compose 期中间工具**：不进入 connector 消费面

## 变更清单

### Phase 1: Connector 消费接口下沉

1. **`ExecutionTurnFrame` 新增 `context_frames: Vec<ContextFrame>`**
   - SPI 层 `ExecutionTurnFrame` 移除旧 `context_bundle` 字段，新增 `context_frames`
   - `AgentConnector` trait 移除 `update_session_context_bundle` 方法

2. **Identity 改为 `ContextFrame(kind=identity)`**
   - 新增 `identity_context_frame.rs` 构建器（resolve base/agent prompt、mode、effective prompt）
   - 新增 `ContextFrameSection::Identity` 变体
   - 删除 `rendered_system_prompt` 字段
   - 删除 `system_prompt_assembler.rs` 模块

3. **prompt_pipeline 统一收集并填充 `context_frames`**
   - 在调 `connector.prompt()` 前将以下 frame 收集到 `context.turn.context_frames`：
     - identity frame
     - owner bootstrap frames（capability state + mission context）
     - pending transition frames
     - hook turn-start 队列 frame
     - pending action frame
   - 新增 `dedupe_context_frames` 去重

4. **各 Connector 从 `context_frames` 消费**
   - **PiAgent**：取 identity frame 调 `set_system_prompt()`；其余 frame re-enqueue HookTurnStartNotice 供后续 `transform_context` 消费
   - **executor_session / codex_bridge**：新增 `context_frame_render.rs`（`render_context_frames_to_text` / `compose_prompt_text`），将所有 frame 拼成文本并与 user_text 组合

### Phase 2: Bundle 退化与审计路径迁移

5. **SessionContextBundle 从公共消费面退出**
   - 移除 `turn_delta` 和 `rendered_system_prompt` 字段
   - Bundle 保留为 compose 期 fragment 汇聚工具

6. **turn_delta 审计路径迁移**
   - 新增 `TurnExecution.runtime_injection_fragments` 存储运行期 hook 注入片段
   - 新增 `context_audit_bundle_id` / `context_audit_session_id` 审计字段
   - `hook_delegate` 写入审计直接使用 turn execution 上的新字段

7. **mission_context_frame 解耦 Bundle**
   - `build_mission_context_frame` 改为接收 `phase_tag: Option<&str>` + `&[ContextFragment]`
   - 不再依赖 `bundle.render_section`

### 前端对齐

8. **identity section 渲染支持**
   - `contextFrame.ts` 新增 `IdentitySection` 类型与 parser
   - `SectionRenderers.tsx` 新增 `IdentityBody` 组件
   - `ContextFrameCard.test.tsx` 新增 identity frame 测试

## 影响面

| 层 | 文件 | 改动 |
|---|---|---|
| SPI | `hooks/mod.rs` | `ContextFrameSection::Identity` |
| SPI | `connector/mod.rs` | `context_frames` 替换 `context_bundle`；移除 `update_session_context_bundle` |
| SPI | `context/bundle.rs` | 移除 `turn_delta`、`rendered_system_prompt` |
| Application | `identity_context_frame.rs` | 新建 |
| Application | `mission_context_frame.rs` | 解耦 Bundle |
| Application | `prompt_pipeline.rs` | 收集 frames、去重、填充 |
| Application | `hub_support.rs` | `TurnExecution` 新增审计字段 |
| Application | `hook_delegate.rs` | 审计写入迁移 |
| Application | `hub/runtime_context_transition.rs` | 返回 `Vec<ContextFrame>` |
| Application | `system_prompt_assembler.rs` | 删除 |
| Executor | `context_frame_render.rs` | 新建 |
| Executor | `executor_session.rs` | 使用 `compose_prompt_text` |
| Executor | `codex_bridge.rs` | 使用 `compose_prompt_text` |
| Executor | `pi_agent/connector.rs` | identity frame 消费 + re-enqueue |
| Frontend | `contextFrame.ts` | identity section |
| Frontend | `SectionRenderers.tsx` | identity 渲染 |

## 验证记录

- `cargo check` ✅
- `cargo test -p agentdash-application -p agentdash-executor --lib` ✅（331 + 35 全通过）
- `pnpm --dir frontend typecheck` ✅
- `pnpm --dir frontend test -- ContextFrameCard.test.tsx` ✅（8 tests pass）

## 关联

- 前序任务：`05-09-contextframe-injection-path-consolidation`
- 父任务：`05-09-context-frame-consolidation`
- Plan 文件：`contextframe_delivery_refactor_84971d9d.plan.md`
