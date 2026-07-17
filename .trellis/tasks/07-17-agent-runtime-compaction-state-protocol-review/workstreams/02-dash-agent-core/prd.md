# W2 — Dash Agent & AgentCore

## Depends On

- W1 Contracts & Crate Skeleton

## Ownership

- current `agentdash-agent`
- target `agentdash-agent-core`
- Dash Agent/Core tests

W2 独占 `agentdash-agent` → Dash Agent/AgentCore 的物理 move；只从
`agentdash-agent-types` 迁出 Core/Dash-owned 类型，不删除该 legacy crate。W8 在所有
消费者切换后独占删除。

## Goal

建立平台中立的 Dash Agent 中层与纯 AgentCore。Dash Agent 的 `AgentSession` 全部状态由
有序 history 唯一维护和重建；command/effect/queue 位于 Session 外；Core 只执行显式
input/context/tool/provider loop。

## Exit Criteria

- `AgentSessionState = fold(AgentHistory)` 有 replay/property test；
- fresh create 通过 `InitialContextInstalled` history contribution 原子安装 package，
  首个普通 input 使用独立 history entry；
- `DashAgentCommit` 原子提交 effect settlement、history/head/change/continuation；
- history tree、fork、context/compaction 与 lifecycle 可独立测试；
- AgentCore 无 Runtime/Product/Infrastructure/vendor 依赖；
- Dash Agent 无 Managed Runtime/Host/Application 依赖；
- `agentdash-agent-types` 按 owner 迁空；
- manual/automatic A/B/C 测试通过。
