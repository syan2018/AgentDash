# workflow 禁止终止后 Agent 复读排查与修复

> Spun out of [04-21-acp-stream-e2e-workflow-termination](../04-21-acp-stream-e2e-workflow-termination/)

## Goal

用户观察到：session 被 workflow `stop_gate_checks_pending` 阻止终止后，
下一轮 Agent 喜欢**复读**上一轮的内容。怀疑会话恢复（prompt 重建）注入了重复历史。

## Hypothesis（待验证）

- `build_restored_session_messages_from_events` 在 stop_gate 场景下去重 key 失效，
  产出重复消息条目。
  文件：[session/continuation.rs:204](../../../crates/agentdash-application/src/session/continuation.rs#L204)
- stop_gate rhai hook 注入的 `constraint` / `inject` 内容被当作历史持久化，下一轮 prompt
  携带它后 Agent 复述。
- `resolve_session_prompt_lifecycle` 在 has_live_runtime=false（stop 后）+
  supports_repository_restore=true 的组合下，错误地把完整历史 + 注入一起带回。
  文件：[routine/executor.rs:425-429](../../../crates/agentdash-application/src/routine/executor.rs#L425-L429)

## Review Findings（2026-04-21，不改代码的前置排查）

### 1) 主通道 vs auto-resume 有 7 个字段漂移

**主通道**：`POST /sessions/:id/prompt` → [prompt_session](../../../crates/agentdash-api/src/routes/acp_sessions.rs#L930)
→ `augment_prompt_request_for_owner` → `req.identity = current_user` → `hub.start_prompt`。

**Auto-resume**：[turn_processor.rs:232-237](../../../crates/agentdash-application/src/session/turn_processor.rs#L232-L237)
判到 `BeforeStop.decision == "continue"`，200ms 后 [hub.rs:646 schedule_hook_auto_resume](../../../crates/agentdash-application/src/session/hub.rs#L646)
直接调 `hub.start_prompt(sid, PromptSessionRequest::from_user_input(AUTO_RESUME_PROMPT))`，
**完全跳过 augment**。

| 字段 | 主通道 | auto-resume | 风险 |
|---|---|---|---|
| mcp_servers | augment 注入 agent preset 的 MCP | Vec::new() | 严重 — Agent 失去工具 |
| vfs | augment 装配 workspace + canvas mounts | None（走 hub.default_vfs） | canvas 挂载可能缺失 |
| flow_capabilities | augment 按 workflow baseline | None | 严重 — 流程约束丢 |
| system_context | augment 生成 owner / 续跑 system | None | 严重 — owner 上下文裸奔 |
| bootstrap_action | augment 按 lifecycle 决定 | None | is_owner_bootstrap 永远 false |
| identity | handler 赋 current_user | None | 审计 / trace 漂移 |
| effective_capability_keys | augment 解析 | None | hook runtime capability 追踪错位 |

### 2) 不是"重复注入相同字串"的 dedup bug

- `restored_messages` dedup key 基于 `turn_id + entry_index`（[continuation.rs:429](../../../crates/agentdash-application/src/session/continuation.rs#L429)），
  同一轮内不会产生重复条目。
- stop_gate rhai hook 的 `inject` 内容**不会落 DB**，只在
  [hook_delegate.rs:350 build_hook_injection_message](../../../crates/agentdash-application/src/session/hook_delegate.rs#L350)
  每轮 `UserPromptSubmit` 时重新拼一次直接喂给 LLM。
- AUTO_RESUME_PROMPT 作为**用户消息**被持久化（prompt_pipeline.rs:317）。

### 3) 复读真实根因（分析判断）

LLM 在 auto-resume 轮看到的上下文：
```
[restored] user:  <上一轮用户 prompt>
[restored] agent: <上一轮 assistant 完整输出>           ← 原样保留
[hook-inj] ## 必须遵守的流程约束: Workflow stop gate... 不要重复总结前文
[new-user] [系统自动续跑] 上一轮执行结束但 workflow stop gate 仍未满足...
```

根因是**通道漂移的合力**，不是单点 bug：

- **主因**：auto-resume 跳过 augment → 丢失 owner 约束 / 工具链 / flow_capabilities，
  LLM 失去"工作流聚焦"背景，退化到通用 assistant 模式，倾向于 recap。
- **次因**：AUTO_RESUME_PROMPT 文案"上一轮执行结束，请继续完成..."主动诱导 LLM 去 review
  上一轮；人类"接着做"和机器"上一轮执行结束了"语气差距很大。
- **加乘**：`stop_gate_checks_pending.rhai` 注入的"不要重复总结前文"与 AUTO_RESUME_PROMPT
  的"上一轮执行结束"相互打架；LLM 的折中通常就是先复述再继续。

### 4) `has_live_runtime` 在 auto-resume 时大概率是 true

[prompt_pipeline.rs:33](../../../crates/agentdash-application/src/session/prompt_pipeline.rs#L33)
捕获 `had_existing_runtime`，`resolve_session_prompt_lifecycle` 走 `Plain` 分支 →
`restored_session_state = None`。此时 executor 本地 follow-up 持有上轮历史，
**不是 `build_restored_session_messages_from_events` 参与了复读**，所以最初的
hypothesis 1/3 优先级下调。

## Revised Plan

1. **修漂移**（PR1）：让 auto-resume 走同一条 augment 路径。两种实现：
   - A: 把 `schedule_hook_auto_resume` 挪到 API 层（不可行：没有 HTTP request 上下文）。
   - B（**推荐**）：抽公共函数 `build_augmented_prompt_request_for_auto_resume(session_id) -> PromptSessionRequest`，
     与 API handler 共享 augment 逻辑；identity 可复用 `session_meta.created_by` 或加专用的
     `Identity::System`。
2. **改 AUTO_RESUME_PROMPT 文案**（PR1 附带）：从"上一轮执行结束..."改为更中性的"继续处理当前
  workflow step"，不主动引导 recap。
3. **fail-lock 测试**（PR2）：
   - Rust 集成：构造 session + owner binding + stop_gate 配置，手工触发 auto-resume，
     断言发给 connector 的 `ExecutionContext` 带有 mcp_servers / flow_capabilities / system_context。
   - 另加一条断言 `req.identity.is_some()`。
4. **解决冲突信号**（PR3，可选）：stop_gate inject 与 AUTO_RESUME_PROMPT 语义协同，
  或者干脆把 stop_gate 作为 system_context 的一部分而不是 hook injection。

## Implementation Summary（2026-04-21 完成）

采用方案 B 的变种：用 **依赖注入** 而不是移动代码。

### 新增 / 改动

- **[augmenter.rs](../../../crates/agentdash-application/src/session/augmenter.rs)**（新）：
  定义 `PromptRequestAugmenter` trait，契约为"把裸 PromptSessionRequest 增强成与主通道
  一致的请求"。文档里点明了漂移风险与触发场景。
- **[hub.rs](../../../crates/agentdash-application/src/session/hub.rs)**：
  - SessionHub 加 `prompt_augmenter: Arc<RwLock<Option<SharedPromptRequestAugmenter>>>` 字段
    + `set_prompt_augmenter()` / `current_prompt_augmenter()`（延迟注入模式，与
    `terminal_callback` 一致的风格）。
  - `schedule_hook_auto_resume` 改为：**先从 hub 取 augmenter，augment 之后再 start_prompt**；
    augmenter 未注入时打 warn（非生产路径兜底）。
- **[hook_messages.rs](../../../crates/agentdash-application/src/session/hook_messages.rs)**：
  AUTO_RESUME_PROMPT 从 `[系统自动续跑] 上一轮执行结束但 workflow stop gate 仍未满足...`
  改为 `继续推进当前 workflow step，直接执行未完成的动作或补齐证据。不要重复总结已发生的内容。`
  去掉主动诱导 recap 的关键词。
- **[bootstrap/prompt_augmenter.rs](../../../crates/agentdash-api/src/bootstrap/prompt_augmenter.rs)**（新）：
  API 层 `AppStatePromptAugmenter` 包 `Arc<AppState>`，trait impl 直接委托给
  `augment_prompt_request_for_owner`（改为 `pub(crate)`）。
- **[app_state.rs](../../../crates/agentdash-api/src/app_state.rs)**：
  AppState Arc 封好之后立即调 `session_hub.set_prompt_augmenter(...)` 注入。
- **[routes/acp_sessions.rs](../../../crates/agentdash-api/src/routes/acp_sessions.rs)**：
  `augment_prompt_request_for_owner` 暴露为 `pub(crate)`。

### Fail-lock 测试

[hub.rs tests](../../../crates/agentdash-application/src/session/hub.rs#L2147)：

- `schedule_hook_auto_resume_routes_through_augmenter`：SpyAugmenter 断言 auto-resume
  时 augmenter 被**恰好调用一次**，且看到的是 `AUTO_RESUME_PROMPT` + 空 mcp_servers（裸
  请求），augmenter 负责补齐。未来若有人把 augment 链路删掉或短路，这条直接 fail。
- `auto_resume_prompt_does_not_induce_recap`：AUTO_RESUME_PROMPT 文案审计，禁出现
  "上一轮执行结束 / 请总结 / 请回顾 / 请汇报" 等 recap 触发词。

两条都已绿。预存 2 条 compaction 失败（`build_restored_session_messages_applies_latest_compaction_checkpoint`
/ `build_continuation_system_context_uses_compacted_projection`）与本任务无关，stash
验证过是 pre-existing。

### 未做 / 继续跟进

- identity 漂移：auto-resume 依旧是 `None`。本任务未引入新的 identity 来源（需先决定是
  `session_meta.last_user` 还是 `Identity::System` 这个产品方向），挪到后续任务。
- stop_gate 与 AUTO_RESUME_PROMPT 语义协同（原计划 PR3）暂不做，等实际观测文案调整后复读
  是否还在。

## Plan

1. 写 Rust 集成测试 fail-lock：构造一串事件（用户消息 → agent chunks → stop_gate inject →
  turn_interrupted → 新 user 消息），调用 `build_restored_session_messages_from_events` 和
  整条 prompt 重建链，断言没有重复条目。必要时给出重复样例 snapshot。
2. 定位根因（日志 / 单步 / diff 三者之一）。
3. 修复 + 让测试转绿。

> 已被 Review Findings 取代。见上方 **Revised Plan**。

## Out of Scope

- E2E 端到端（无真实 executor，无法直接测 Agent 输出是否复读）
- stop_gate 策略本身的设计（只修恢复注入的 bug，不改 hook 语义）

## Blocked by

- 等 sister 任务 `04-21-acp-stream-e2e-workflow-termination` 的 debug inject 端点
  着陆后，这里可以顺带加一条 E2E：session stop_gate 阻塞时，DOM 不应出现重复 agent
  消息（从事件总线层验证，不依赖真实 executor）。
