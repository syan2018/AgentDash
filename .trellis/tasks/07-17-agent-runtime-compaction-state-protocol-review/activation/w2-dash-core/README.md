# W2 Dash Agent / AgentCore S5 activation component

## Identity

- Frozen base: `fc26d3ffb951461d8e9214b6b4639b88c18d533d`
- Source branch: `codex/agent-runtime-s5-dash-activation`
- Source worktree: `F:\Projects\AgentDash-s5-dash-activation`
- Code tip: `ce469857`
- Component patch: `0001-refactor-agent-runtime-Dash-Core.patch`
- Patch SHA-256:
  `050933092652A05BC49F36102D4E48CD14A585A1C6127BE61F4C02B7C5886A15`
- Consumer manifest: `consumer-manifest.json`
- W8 repository input: `dash-repository-contract.json`
- Native deletion/consumer handoff: `native-owner-deletion-manifest.json`
- Apply proof: `apply-proof.json`

本目录冻结在 S4 revision 上的 W2 physical/API 输入。它不修改 migration、最终 workspace
删除或 Product production route，也不把局部组件冒充 `activation_ready`。

## Owned result

- `agentdash-agent-core` 只物理拥有显式 input/context/tool/provider/callback/cancel 到 output
  的纯执行 loop；它不持有 Agent、运行状态、队列、approval、runtime delegate 或 tool
  result cache。
- `agentdash-agent` 拥有 complete Agent 行为以及 Dash history、fork、lifecycle、
  compaction、command/effect/change 与 repository port，依赖方向唯一为 `Dash -> Core`。
- Native 只持有 Dash Complete Agent service 与 typed host callback materializer；legacy
  driver、journal/context projector、presentation/tool route 及其旧测试已从 owner crate
  物理删除。
- `agentdash-agent` 不 re-export Core；Application/Product 没有新增到 Core 的生产依赖。
- recording repository/store 只存在于 integration test 私有边界；生产 API 公开
  `DashAgentRepository`、`DashAgentRepositoryStore` 以及 repository/store 注入的
  create/open/fork seam。
- Complete Agent 的 effect receipt/inspection、source metadata 与 Dash repository
  create/CAS 由 `DashCompleteAtomicCommit` 一次提交；execute 先持久占用 effect identity，
  再用 Dash effect/history authority 跨重启 reconcile。
- apply/revoke 与 replay/open 都通过 durable source metadata 的同一 live surface
  materializer；commit response 丢失不会留下旧 binding generation，也不会让 revoke 后的
  callback 继续存活。
- repository `Cargo.lock` 精确恢复 frozen base；component patch 明确排除 lock，W8 从最终
  manifests 生成 activation lock，并与 migration、PostgreSQL adapter、production
  composition 在同一 staging revision 提交。

## Direct consumer proof

冻结 base 上 `agentdash-agent-types` 有 14 个 direct consumers。该组件移除 5 个：

1. `agentdash-agent`
2. `agentdash-infrastructure`
3. `agentdash-integration-native-agent`
4. `agentdash-contracts`
5. `agentdash-executor`

组件后剩余 9 个，逐项 final owner 见 `consumer-manifest.json` 与
`research/agent-types-hard-cut-consumer-inventory.md`。

## Atomic activation order

1. **W2 type move + Native owner deletion**：应用本目录 patch，建立最终 Dash/Core
   物理所有权、Complete Agent atomic store contract，并删除 Native legacy driver 与
   journal/context route。
2. **W7 callers**：Product/API/Application/Lifecycle/VFS 与 Infrastructure durable worker
   切到 Business Surface、Runtime Tool Broker、Host callbacks 与 Runtime snapshot/change。
3. **W8 composition/deletion**：实现 PostgreSQL atomic store，装配
   `native_complete_agent_registration`，迁空 protocol/ports/runtime-gateway/SPI legacy
   boundary，删除 `agentdash-agent-types`、workspace/lock 条目并应用唯一 migration。

这三个步骤必须进入同一 S5 staging set。中间 revision 不进入 production，也不增加
Application -> Core 反向依赖、facade、re-export 或兼容 reader。

## Verification

```powershell
cargo metadata --format-version 1 --no-deps
cargo tree -p agentdash-agent-core --edges normal
cargo test --locked -p agentdash-agent-core -p agentdash-agent -p agentdash-integration-native-agent
cargo test --locked -p agentdash-application-agentrun fork_
rg -n "NativeAgentDriver|NativeAgentRuntimeIntegration|project_native_core|native_runtime_profile" crates/agentdash-integration-native-agent
pnpm run test-support:guard
git diff --check -- . ':(exclude)*.patch'
```

组件 revision 必须满足：

- metadata direct consumer 数量 `14 -> 9`；
- `rg -n "transcode_protocol_owned|MemoryDashAgentRepository" crates` 无结果；
- Core source/Cargo 不依赖 Application、Domain、Runtime、Integration、protocol、SPI、
  repository 或 vendor；
- Native owner crate 中不再存在旧 driver registration；W8 在最终 staging revision
  装配唯一 Complete Agent registration，不能双注册。
- `git diff fc26d3ff..HEAD -- Cargo.lock` 为零，component patch 也不得出现
  `Cargo.lock`；W8 必须从最终 manifests 重新生成 activation lock。
