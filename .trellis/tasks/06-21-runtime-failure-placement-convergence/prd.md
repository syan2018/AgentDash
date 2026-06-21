# Runtime Failure 与 Placement 收敛

## Goal

验证并收敛 backend disconnect projection、MCP backend fallback、local backend identity 与 execution placement 行为，使执行期目标缺失、断线和 fallback 对用户可见且可诊断。

## Scope

- backend disconnect 对 running prompt/session/AgentRun 的 lost 或 terminal projection。
- session context 下 MCP backend target fallback 边界。
- standalone local backend id 来源。
- runtime-summary、session route、feed/stream 的可观察一致性。

## Open Decisions

- 已决策：执行期 backend disconnect / missing 不 fallback，必须暴露用户可见 lost 状态。
- 已决策：session context 下 MCP list/call 只绑定当前 session route/backend；setup/probe 才允许 discovery fallback。
- 已决策：standalone local backend id 必须显式消费 server ensure/claim 或 token-bound input。

## Acceptance Criteria

- [x] `design.md` 定义 execution placement failure 的产品语义。
- [ ] `work-items/index.md` 覆盖 D16、D17 及 standalone backend id 来源。
- [x] characterization task 验证当前 disconnect / MCP fallback / backend identity 行为。
- [ ] 后续实现任务不改变 setup/probe 与 session execution 的目标边界。
