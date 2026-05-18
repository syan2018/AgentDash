# Runtime 稳定性与执行准入小修

## Goal

从 `05-13-multica-local-runtime-concept-alignment` 的研究结论中抽取一组小粒度、可顺序提交的 runtime 稳定性优化。目标不是建立完整 runtime health / execution attempt 架构，而是先补齐当前 AgentDash 已有链路中的诊断语义、执行准入和 provider adapter 测试底座，让后续大任务有更稳的落点。

本任务应保持模块边界清晰，每个优化项都能独立验证、独立提交。

## Confirmed Facts

- `crates/agentdash-api/src/relay/registry.rs` 当前 `BackendRegistry::send_command` 会区分 backend 不在线、发送失败、命令超时，但错误仍是普通 `anyhow` 文本，调用方难以稳定分类。
- `crates/agentdash-application/src/routine/executor.rs` 当前 `RoutineExecution.completed` 表示 prompt 已成功派发到 session，不表示 Agent 真实执行完成；代码注释已经记录该语义差异。
- `RoutineExecutionStatus` 已包含 `Skipped`，但 Routine 触发准入尚未系统使用它表达 backend/workspace/agent 不可用。
- `05-13` 研究建议可先做 relay 错误分类、Routine admission skip、provider adapter 行为矩阵，而暂缓完整 runtime health / ExecutionAttempt / desktop 控制台。

## Requirements

- 细化 relay 命令错误分类，使 backend offline、发送失败、响应超时、响应通道关闭等场景具备稳定类型或稳定错误码，调用方不依赖字符串解析。
- Routine 触发前应做最小准入检查：Project/Agent/ProjectAgentLink/Workspace 配置无法解析时仍按失败处理；因可选本机 backend/workspace 绑定不可用而无法派发时，应记录 `skipped` 并保留可读原因。
- Routine 的 `completed` 语义不得在本任务中扩大为真实 Agent terminal；只允许补强注释、UI 文案或字段说明，避免误导。
- Provider adapter 行为矩阵先落到测试或测试计划中，覆盖 session id、usage、stderr/error 分类、timeout/cancel、resume/poisoned output 等关键维度；不重写 Backbone/session envelope。
- 每个子项都需要对应单元测试、集成测试或至少明确的不可自动化验证说明。

## Acceptance Criteria

- [ ] relay 命令错误分类有稳定 API/类型，并覆盖 backend offline、send failed、timeout、response channel closed。
- [ ] Routine 在 backend/workspace 不可用的准入场景下会产生 `skipped` execution，且 `failure_reason` 或等价字段记录中文可读原因。
- [ ] Routine 派发成功后的 `completed` 语义在代码注释/API DTO/前端展示中保持一致，不被解释为 Agent 已执行完成。
- [ ] Provider adapter 行为矩阵至少沉淀为测试文件、fixture 或 `implement.md` 中的可执行 checklist，并有一批自动化覆盖。
- [ ] 相关测试通过；若无法运行全量测试，需记录实际运行的验证命令和未覆盖风险。

## Out of Scope

- 不创建持久化 backend runtime health 表，不实现 offline sweeper。
- 不引入 ExecutionAttempt / MessageLog 数据模型。
- 不做 desktop local backend 控制台。
- 不引入 React Query 或前端 server-state 架构迁移。

## Notes

- 父任务：`05-13-multica-local-runtime-concept-alignment`。
- 建议按 `relay -> routine -> executor tests` 顺序提交。
