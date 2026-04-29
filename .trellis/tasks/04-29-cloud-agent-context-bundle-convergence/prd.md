# 统一云端 Agent 上下文注入收口

## Goal

在已引入 `SessionContextBundle` 的基础上，专注云端 Agent 主路径，把上下文注入从“Bundle + prompt resource + Hook user message + 多份 slot 白名单”的半收口状态，推进到清晰的单主数据面：静态/半静态 owner context 统一进入 `SessionContextBundle`，PiAgent 以 bundle 为唯一业务上下文来源；动态 Hook 注入明确分为“Bundle fragment”和“turn side effect”；Context Inspector 能反映云端 Agent 实际看到的上下文变化。

## What I Already Know

* 最近三笔提交已经完成 `SessionContextBundle` / `Contribution` / `FragmentScope` 的底座，并把 PiAgent `build_runtime_system_prompt` 的业务上下文读取切到了 `context.context_bundle`。
* 用户已明确本轮不再优先照顾 Relay 协议和本机执行器路径。Relay / local executor 可以作为后续废弃或单独兼容任务，不阻塞云端 Agent 收口。
* 当前仍存在四个未闭环点：
  * Task owner bootstrap 会把 bundle 渲染成 prompt resource block prepend 到用户消息，同时 bundle 又进入 PiAgent system prompt，存在重复注入和 title generator 污染风险。
  * `compose_lifecycle_node` 仍只产 VFS / capability / MCP / kickoff prompt，不产 `SessionContextBundle`。
  * Hook 动态注入仍直接以 `HookInjection` 渲染成 user message，`hooks/fragment_bridge.rs` 基本未接入运行时，审计总线也看不到动态 Hook fragment。
  * runtime slot 白名单分散在 application / PiAgent / Vibe Kanban 三处，已经出现 `story_context` 等差异。

## Requirements

* 云端 Agent 主路径中，owner/task/story/project/workflow 的业务上下文必须以 `SessionContextBundle` 为主数据源进入 PiAgent system prompt。
* Task owner bootstrap 不再把完整 task context 作为 prompt resource block 注入用户消息。用户 prompt 应保持用户原始输入和明确的任务执行指令，不携带重复的 owner context。
* Title generator 只消费原始用户文本摘要，不从增强后的 prompt blocks fallback text 中读取 owner context / task context resource。
* `compose_lifecycle_node` 必须产出最小可审计 bundle，覆盖 lifecycle step、workflow goal/instructions、constraints/runtime policy 等运行时需要的静态上下文。
* Hook 链路必须明确拆分：
  * 可重复展示的上下文类 injection 进入 `ContextFragment` / `SessionContextBundle`，带 `source` / `scope` / `slot` / `order`，并进入审计总线。
  * 会改变本轮控制流的 side effect，如 block、transform_message、pending action、BeforeStop steering，仍走现有 delegate 消息路径，但不伪装成 bundle 主上下文。
* Context Inspector 至少能看到 bootstrap、composer rebuild、lifecycle bundle、Hook context fragment 的审计事件。
* runtime-agent slot 白名单只保留一份共享定义，PiAgent 和 application 侧渲染复用同一来源。

## Acceptance Criteria

* [ ] Task owner bootstrap 的 PiAgent 请求中，同一段 task/story/project/instruction context 不会同时出现在 system prompt 与 user prompt blocks。
* [ ] 首轮 task session 自动标题生成不会包含 `## Task`、`## Story`、`## Project`、`## Instruction` 等 owner context 内容。
* [ ] `compose_lifecycle_node` 返回的 `PreparedSessionInputs.context_bundle` 非空，且含 workflow/lifecycle 相关 fragment。
* [ ] Hook `UserPromptSubmit` 产出的 context 类 injections 通过 `hook_injection_to_fragment` 或等价转换进入 bundle/audit；`companion_agents` 这类重复 slot 由 bundle 合并规则处理，不再依赖额外 user-message skip 白名单作为主去重机制。
* [ ] Context Inspector 对同一 session 能查询到 `session_bootstrap` / `composer_rebuild` / `hook:<trigger>` 类型事件。
* [ ] PiAgent runtime context slot 白名单由共享常量导出；新增 slot 只需改一处。
* [ ] 相关单元测试覆盖：bundle 去重、title gen 隔离、task bootstrap 不重复、lifecycle node bundle、Hook fragment 审计。
* [ ] `cargo test -p agentdash-spi session_context_bundle`、`cargo test -p agentdash-application context`、`cargo test -p agentdash-application hook_delegate`、`cargo test -p agentdash-executor runtime_system_prompt` 通过。

## Definition of Done

* 后端单元测试覆盖新增收口行为和关键回归场景。
* 不为 Relay / local executor 增加新协议字段或兼容层。
* 若删除或弱化 task prompt resource block，必须补一个测试证明 PiAgent 仍能从 bundle 获取 task context。
* 审计事件字段保持前端 Context Inspector 当前 DTO 可消费，必要时只做向后兼容扩展。
* PR 描述中明确指出 Relay / local executor 不在本轮范围内。

## Technical Approach

### Approach A: 云端主路径强收口（推荐）

保留 `SessionContextBundle` 作为静态/半静态上下文的唯一主数据面。清理云端 Agent 路径中把 bundle 重新渲染进 prompt blocks 的行为。Hook 动态注入不强行全部纳入 bootstrap bundle，而是在每轮 transform 阶段生成“turn bundle delta”或审计 fragment，再按需要渲染给当前 LLM 请求。

优点：
* 与用户当前方向一致，避免为废弃中的 Relay/local executor 支付复杂度。
* 修复 task bootstrap 重复注入和 title generator 污染。
* 保留 Hook side effect 的现有稳定语义，不一次性重构过大。

代价：
* Relay/local executor 在不做兼容前会继续不具备完整 bundle 语义，这被明确接受为 out of scope。
* Hook 仍有部分 user message 侧效应，不是 D2a 式完全统一。

### Approach B: TurnContextBundle 全量化

每一轮 LLM 请求前都生成一个完整 `SessionContextBundle`，Hook、pending action、workflow delta 全部转换为 fragment，然后 connector 只消费 bundle。

优点：
* 概念最干净。
* Inspector 与实际 LLM 输入高度一致。

代价：
* 改动大，会碰 agent loop、delegate、pending action、BeforeStop、compaction 等更多语义。
* 当前阶段容易把 Hook 副作用和上下文展示混成一个大迁移。

### Decision

采用 Approach A。当前项目处于预研期，但方向已经明确偏云端 Agent；最值得先做的是把云端主路径的重复/旁路去掉，让 bundle 成为 PiAgent 的唯一业务上下文来源。Hook 采用“context fragment 接 bundle/audit，side effect 保持 delegate”的中间态。

## Implementation Plan

### PR1: Task Bootstrap 与 Title Gen 隔离

* 移除或禁用 `compose_story_step` 产出的 task context resource block 在云端 PiAgent 主路径中的 prepend 行为。
* `build_task_owner_prompt_request` 在 `OwnerBootstrap` 时保留用户原始 prompt blocks，不再把完整 task context resource 插入前面。
* 调整 title generation 的输入来源，确保使用原始用户文本 block 的摘要，而不是增强后的全部 prompt fallback。
* 增加测试：
  * task bootstrap context 只在 `ExecutionContext.context_bundle` 中出现；
  * title prompt 不包含 owner context heading。

### PR2: Lifecycle Node Bundle

* 为 lifecycle AgentNode 增加 `contribute_lifecycle_context(...) -> Contribution`。
* `compose_lifecycle_node` 调 `build_session_context_bundle`，产出 `ContextBuildPhase::LifecycleNode` bundle。
* 该 bundle 至少包含：
  * active lifecycle key / step key / step title；
  * workflow goal / instructions；
  * constraints / ready ports / writable output ports；
  * runtime policy 或 complete node 提醒中属于静态上下文的部分。
* 对 bundle emit `AuditTrigger::ComposerRebuild` 或新增更精确的 `LifecycleActivation` trigger。

### PR3: Hook Context Fragment 接线

* 把 `hooks/fragment_bridge.rs` 从测试辅助变成运行时路径：
  * `UserPromptSubmit` 中的 context 类 injection 转成 `ContextFragment`；
  * 合并到本轮可见的 bundle delta 或至少 emit audit；
  * 只对需要作为即时 steering 的内容继续渲染 user message。
* 将 `HOOK_USER_MESSAGE_SKIP_SLOTS` 从主去重逻辑降级为过渡保护，目标是由 bundle slot 合并承担重复 slot 处理。
* 增加测试覆盖：
  * hook injection 产生审计事件；
  * `companion_agents` 不重复进入 user message；
  * custom hook slot 可见于 Inspector。

### PR4: Slot 白名单共享化

* 在共享 crate 或 `agentdash-spi` 中定义 `RUNTIME_AGENT_CONTEXT_SLOTS`。
* PiAgent、Vibe Kanban（如果保留测试）、application 侧临时渲染统一引用该定义。
* 删除重复常量，补测试证明关键 slot 如 `story_context`、`instruction`、`workflow_context` 均被渲染。

### PR5: Inspector 与测试补强

* 补 Context Inspector 查询层测试或 route DTO 测试，覆盖 `hook:<trigger>` 与 lifecycle bundle。
* 增加一个较高层的 cloud-agent session assembly 测试，验证 bootstrap request 到 `ExecutionContext` 的数据形状。

## Out of Scope

* Relay 协议新增 `context_bundle` 字段。
* 本机执行器 / local backend 的 bundle 消费适配。
* Vibe Kanban 的行为一致性修复，除非编译测试需要最小调整。
* D2a 激进方案：把 Hook 副作用、pending action、BeforeStop、compaction 全部改造成 fragment/effect 双轨模型。
* AGENTS.md / CLAUDE.md / MEMORY.md 自动发现加载。
* 删除所有历史 `SessionContextSnapshot` 查询构建器。只读 UI snapshot 路径可以继续存在。

## Technical Notes

* 当前核心文件：
  * `crates/agentdash-spi/src/session_context_bundle.rs`
  * `crates/agentdash-spi/src/context_injection.rs`
  * `crates/agentdash-application/src/context/builder.rs`
  * `crates/agentdash-application/src/session/assembler.rs`
  * `crates/agentdash-application/src/session/prompt_pipeline.rs`
  * `crates/agentdash-application/src/session/hook_delegate.rs`
  * `crates/agentdash-application/src/hooks/fragment_bridge.rs`
  * `crates/agentdash-application/src/context/audit.rs`
  * `crates/agentdash-executor/src/connectors/pi_agent/connector.rs`
* 用户已明确：Relay 协议和本机执行路径基本弃用，本任务不为多源适配牺牲云端主路径的清晰度。
* 相关前置任务：
  * `04-29-session-context-builder-unification`
  * `04-29-session-context-builder-d2a-exploration`
  * `04-29-agents-md-discovery-loading`

## Open Questions

* Hook context fragment 在本轮实现中是“只进审计 + 保留 user message 渲染”，还是同时进入一个 per-turn bundle delta 并让 PiAgent system prompt 每轮重建时消费？推荐先做后者的最小版，但如果风险偏大，PR3 可以先落审计与转换，再下一任务切消费。
