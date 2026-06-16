# W6 MCP + Capability

## 状态

pending

## 依赖

- W4 done

## 目标

收口 Story / Task MCP 与 capability，使 agent-facing 工具走 Run-scoped Task command 和 SubjectExecution projection，并保留默认开放的 policy hook。

## 输入

- W4 backend command / read model。
- `.trellis/spec/backend/capability/tool-capability-pipeline.md`
- `.trellis/spec/backend/permission/grant-lifecycle.md`
- `.trellis/spec/backend/permission/policy-engine.md`
- `crates/agentdash-mcp/src/servers/story.rs`
- `crates/agentdash-mcp/src/servers/task.rs`
- `crates/agentdash-spi/src/platform/tool_capability.rs`
- `crates/agentdash-application/src/capability/resolver.rs`
- `crates/agentdash-domain/src/companion/skills/companion-system/SKILL.md`

## 范围

- Story MCP 通过 Story-bound run 的 Task command 创建计划项，或查询 Story Task projection。
- Task MCP 状态推进只接受计划态。
- artifact 上报不写 Task facts，改走 Lifecycle / SubjectExecution 关联产物。
- Task management、collaboration、workflow capability 调用统一 command / read model。
- 为 create / update / assign / review / done 预留 policy check，默认开放。
- Story projection read / update 保留 Story scope capability check。

## 范围边界

- 该节点只预留稳定 policy hook 并保持默认开放，原因是 permission system convergence review 会独立收束完整策略。
- MCP 工具统一调用 Run-scoped Task command，原因是 agent-facing 入口需要和 API / UI 观察同一事实源。

## 验收

- Story / Task MCP 写入口走 Run-scoped Task command。
- Task MCP 拒绝旧 TaskStatus。
- artifact 不再写入 Task facts。
- Task / Story 相关入口存在稳定 policy hook。
- 默认行为不阻塞预研开发。

## 产出记录

- 待填写。

## 风险与交接

- W8 需要 MCP 旧状态、artifact 和 dispatch surface 搜索结果。
