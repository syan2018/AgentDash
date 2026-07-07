# Companion 回流最小合同与 Channel 轻量对齐

## Goal

修复 Companion 工具、派发提示、skill 文档、运行时解析之间的信息传递闭环，让 Agent 获得最简有效信息集：知道何时调用 `companion_respond`、最小参数是什么、payload 应该长什么样、出错时如何修复。

Channel 在本任务中只作为未来通信模型的轻量词汇约束：沿用 `namespace`、`kind`、`source_ref`、`correlation_ref`、短 selector / alias 这些已经在 mailbox 和 companion 中出现的概念，但不提前实现全局 `agentdash-domain::channel` 模块，不迁移 mailbox source identity，不建立 channel broker 或持久化表。

## Background

Companion 当前的回流链路分散在多个模型可见入口：`build_companion_dispatch_prompt` 只拼自然语言回流要求，`CompanionRespondParams` 暴露 `request_id + payload`，payload schema 用 `anyOf` 固定少数内置 response 类型，companion skill 文档也手写 `request_id` 示例。运行时再通过 parent request gate、pending action、child result gate 三条路径尝试解析。

这导致几个问题：

- Agent 需要猜 `request_id` 究竟是 gate id、dispatch id 还是 pending action id。
- Tool schema 会拦截一些 runtime semantic validator 本来允许的自定义 payload。
- Dispatch prompt、tool description、Tool Schema Delta、skill 文档不是同一份合同的不同渲染。
- 错误反馈告诉模型“不匹配 dispatch_id”，却没有给出当前最小有效工具调用。
- 一些 Agent 无需关心、也不会使用的内部 GUID 被推向模型上下文，增加了误填概率。

## Evidence

- `crates/agentdash-application/src/companion/tools.rs:1779` 的 `CompanionRespondParams` 暴露 `request_id`，注释同时覆盖 gate id / dispatch id / pending action id。
- `crates/agentdash-application/src/companion/tools.rs:1864` 到 `:1882` 的 `companion_respond` 执行路径依次尝试 parent request gate、pending action、child result gate。
- `crates/agentdash-application/src/companion/tools.rs:2257` 的 response payload schema 使用 `anyOf` 固定内置 response 类型。
- `crates/agentdash-application/src/companion/tools.rs:2562` 的 `build_companion_dispatch_prompt` 只拼自然语言回流要求，没有从统一 reply instruction 生成最小工具调用。
- `crates/agentdash-application/src/companion/payload_types.rs:174` 的 `PayloadTypeRegistry::validate_response` 已经支持注册类型语义校验，未知 type 会放行。
- `crates/agentdash-domain/src/companion/skills/companion-system/SKILL.md:21` 和 `references/response-adoption.md:3` 仍要求 `request_id`。
- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs:48` 已有开放式 `MailboxSourceIdentity`，说明未来 Channel 词汇可以和 mailbox 对齐；但本任务不迁移该结构。

## Requirements

1. 在 companion application 层建立 `CompanionReplyContract`，作为内部事实：包含当前回复目标、owner refs、expected payload、adoption mode、以及模型可见的 `ModelReplyInstruction`。
2. `ModelReplyInstruction` 是唯一允许渲染到 Agent 上下文的回复合同视图，只包含工具名、最小参数、payload 必填字段、可选短 selector / alias 和修复提示。
3. `build_companion_dispatch_prompt` 改为从 `ModelReplyInstruction` 渲染最小 `companion_respond` 调用；常规 child dispatch 的最小调用只需要 `payload`，不要求复制 dispatch/gate/run/frame/session GUID。
4. `companion_respond` 的模型可见参数移除过载 `request_id`，改为 `payload + optional reply_to selector`。当当前上下文只有一个 active reply contract 时，`reply_to` 可以省略。
5. `companion_respond` 的 JSON Schema 只校验 envelope 和 `payload` 是 object；内置 payload type 的必填字段、request/response 匹配和业务校验交给 `PayloadTypeRegistry`。
6. Runtime 根据当前 active reply contract 或短 selector 选择单一路由，不再用一次工具调用同时尝试 parent gate、pending action、child result 三条路径。
7. Tool description、Tool Schema Delta、dispatch prompt、companion system skill、response-adoption reference 必须使用同一套最小回复合同词汇。
8. 错误反馈必须给出可修复信息：缺失字段、收到的 selector、当前可用 selector、以及由当前 `ModelReplyInstruction` 生成的最小有效 `companion_respond` 示例。
9. Channel 相关工作只保留轻量对齐：命名采用 `namespace/kind/alias/source_ref/correlation_ref` 这类已有词汇；不新增通用 actor/message/delivery-owner 类型，不迁移 `MailboxSourceIdentity`，不新增 channel persistence。
10. Bootstrap / runtime tool provider / frame construction 只装配依赖和注入 launch facts；回复合同创建、渲染、解析归属 companion application 模块。

## Scope Boundary

本任务聚焦 Companion 模型可见合同闭环。全局 channel event log、订阅/fan-out、外部 IM room、channel permission broker、AgentRun mailbox source identity 迁移、通用 `agentdash-domain::channel` 模块都不在首期范围内。

保留 Channel 方向的原因是后续会统一 agent2agent、agent2human、agent2system 通信；本任务只把 companion 做成未来可收束的形状，而不是先实现完整抽象。

数据库层仅在 reply contract 需要被写入既有 gate payload 且现有 JSON 结构不足时补 migration。默认不新增表。

## Acceptance Criteria

- [ ] Companion child dispatch 创建 gate、launch source、dispatch prompt 时使用同一份 `CompanionReplyContract` / `ModelReplyInstruction`。
- [ ] `build_companion_dispatch_prompt` 输出最小 Reply Instruction 与可直接调用的 `companion_respond` JSON envelope；常规 child dispatch 不要求复制内部 GUID。
- [ ] `companion_respond` 参数使用 `payload + optional reply_to selector`，当前单一回复目标时可省略 selector。
- [ ] `companion_respond` runtime 根据当前 reply contract 或 selector 单路解析目标，并给出结构化修复错误。
- [ ] 自定义 response payload object 不再被工具 JSON Schema 的 `anyOf` 阻断；内置 payload type 仍由 `PayloadTypeRegistry` 校验。
- [ ] Companion system skill、response-adoption reference、tool description、Tool Schema Delta、dispatch prompt 的说明一致，且只包含 Agent 完成工具调用所需的信息。
- [ ] 覆盖 prompt rendering、payload schema openness、selector omitted singleton、selector ambiguity、child-to-parent completion、parent request response 的 targeted tests。
- [ ] 规划和实现不引入通用 Channel domain module、mailbox source identity 迁移或 channel persistence。
