# Hook Execution Runtime

## Ownership

- Business Agent Surface 编译 `HookDefinition` / requirement 与 immutable `HookPlanSnapshot`。
- Runtime admission 将实际 Driver `HookProfile` 与计划求交为 `BoundHookPlan`。
- Managed Runtime 拥有 durable `HookRun`、decision、failure policy 与 effect outbox。
- Tool Broker 执行同步 before/after-tool policy；Driver adapter只materialize被绑定的 native hook route。
- Infrastructure 实现事务、lease、CAS 与 effect exactly-once。

## Contracts

- Hook capability 按 trigger/actions/semantic strength/failure policy声明，不使用 boolean。
- `Exact` required hook必须具备同步 decision channel；callback/steer只能声明 boundary/observed语义。
- actionful hook先 durable accept/start，再 terminal decision+effects原子提交。
- effect worker以owner/token/expiry claim；ack/release要求未过期同token。
- Driver generation、binding、thread/item correlation不一致时拒绝或fence。
- Codex native bridge materialize到平台隔离artifact，不修改用户项目hook配置作为业务事实源。

## Tests Required

- required exact vs observed admission matrix。
- HookRun/effect crash recovery、lease takeover、duplicate callback exactly-once。
- Native/Codex/remote reverse HostPort hook E2E。
